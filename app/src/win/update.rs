//! 最新版の確認と「今すぐ更新」。
//!
//! 確認は GitHub の `releases/latest` の `tag_name` だけを見る。更新は版から組み立てた URL の
//! MSI と `.sha256` を落とし (redirect は自分で辿り、行き先ごとに host を確かめる)、
//! `%TEMP%` に書く → 読み直して sha256 を確かめる → PowerShell で
//! 「このプロセスの終了を待つ → msiexec → 新しい exe を起動」を走らせる → アプリを終える。
//! 入れるのはメニューを押したときだけ (自動の確認は通知だけ)。

use std::fs;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use fuel_usb_core::message;
use fuel_usb_core::update::{self, Version};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, CONTENT_LENGTH, LOCATION};
use reqwest::redirect::Policy;
use ring::digest::{digest, SHA256};

use super::process::Context;
use super::{log, notify, tray};

/// コンソール窓を出さずに子プロセスを起動する。
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 自動の確認で通知済みの版 (同じ版は 2 度通知しない)。
static NOTIFIED: Mutex<Option<Version>> = Mutex::new(None);

/// 更新の処理中 (2 回押しても 1 つだけ走らせる)。
static INSTALLING: AtomicBool = AtomicBool::new(false);

fn client() -> Option<Client> {
    Client::builder()
        .timeout(update::DOWNLOAD_TIMEOUT)
        // redirect は自分で辿って行き先を確かめる
        .redirect(Policy::none())
        .https_only(true)
        .user_agent(concat!("fuel-usb-uploader/", env!("CARGO_PKG_VERSION")))
        .build()
        .ok()
}

/// 最新の版 (調べられなければ `None`)。
fn fetch_latest(client: &Client) -> Option<Version> {
    let resp = client
        .get(update::LATEST_URL)
        .header(ACCEPT, "application/vnd.github+json")
        .send()
        .ok()?;
    if resp.status().as_u16() != 200 {
        return None;
    }
    let mut body = String::new();
    resp.take(1024 * 1024).read_to_string(&mut body).ok()?;
    update::parse_latest(&body)
}

/// `url` を落とす (redirect は最大 `MAX_REDIRECTS` 回、行き先ごとに `allowed_download_url`。`limit` を超えたら打ち切る)。
fn download(client: &Client, url: &str, limit: u64) -> Option<Vec<u8>> {
    let mut url = url.to_string();
    for _ in 0..=update::MAX_REDIRECTS {
        if !update::allowed_download_url(&url) {
            return None;
        }
        let resp = client.get(&url).send().ok()?;
        let status = resp.status();
        if status.is_redirection() {
            let loc = resp.headers().get(LOCATION)?.to_str().ok()?;
            url = resp.url().join(loc).ok()?.to_string();
            continue;
        }
        if status.as_u16() != 200 {
            return None;
        }
        let declared = resp
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());
        if declared.is_some_and(|n| n > limit) {
            return None;
        }
        let mut buf = Vec::new();
        resp.take(limit + 1).read_to_end(&mut buf).ok()?;
        if buf.len() as u64 > limit {
            return None;
        }
        return Some(buf);
    }
    None
}

fn sha256_hex(data: &[u8]) -> String {
    digest(&SHA256, data)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn powershell() -> PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    PathBuf::from(root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe")
}

/// 落とす → 書く → 読み直して照合。照合できた MSI のパスを返す。
fn prepare(client: &Client, v: Version) -> Option<PathBuf> {
    let (msi_url, sha_url) = update::asset_urls(v);
    let sha_file = download(client, &sha_url, update::MAX_SHA256_BYTES)?;
    let sha_file = String::from_utf8_lossy(&sha_file);
    let msi = download(client, &msi_url, update::MAX_MSI_BYTES)?;

    let path = std::env::temp_dir().join(update::msi_name(v));
    fs::write(&path, &msi).ok()?;
    drop(msi);
    // 書いたものを読み直して確かめる (検証と実行の間に差し替えられる隙を狭める)
    let written = fs::read(&path).ok()?;
    if !update::check_sha256(&sha256_hex(&written), &sha_file) {
        let _ = fs::remove_file(&path);
        return None;
    }
    Some(path)
}

/// 入れ替えの PowerShell を起動する (このプロセスの終了を待ってから msiexec が走る)。
fn launch(msi: &Path) -> Option<()> {
    let exe = std::env::current_exe().ok()?;
    let script = update::install_script(std::process::id(), msi.to_str()?, exe.to_str()?);
    std::process::Command::new(powershell())
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .ok()
        .map(drop)
}

/// 最新版を確かめる。`manual` はトレイの「更新を確認」(結果を必ず通知し、新しければ入れる)。
/// 自動の確認は新しい版を 1 回だけ通知する。worker thread から呼ぶ (通信で待つため)。
pub fn check(ctx: &Context, manual: bool) {
    log::event("update_check");
    let current = Version::current();
    let Some(latest) = client().as_ref().and_then(fetch_latest) else {
        log::event("update_error");
        if manual {
            notify::show(&message::update_failed());
        }
        return;
    };
    if latest <= current {
        if manual {
            notify::show(&message::up_to_date(current));
        }
        return;
    }
    log::event("update_available");

    if !manual {
        let mut notified = NOTIFIED.lock().unwrap_or_else(|e| e.into_inner());
        if *notified != Some(latest) {
            *notified = Some(latest);
            notify::show(&message::update_available(latest));
        }
        return;
    }

    if INSTALLING.swap(true, Ordering::AcqRel) {
        return;
    }
    if ctx.sending() {
        notify::show(&message::update_busy());
        INSTALLING.store(false, Ordering::Release);
        return;
    }
    notify::show(&message::update_starting(latest));
    log::event("update_start");
    let Some(msi) = client().and_then(|c| prepare(&c, latest)) else {
        log::event("update_error");
        notify::show(&message::update_failed());
        INSTALLING.store(false, Ordering::Release);
        return;
    };
    // 落としている間に USB が挿されて送信が始まっていたら、終えずに止める
    if ctx.sending() {
        notify::show(&message::update_busy());
        INSTALLING.store(false, Ordering::Release);
        return;
    }
    if launch(&msi).is_some() {
        tray::quit();
    } else {
        log::event("update_error");
        notify::show(&message::update_failed());
        INSTALLING.store(false, Ordering::Release);
    }
}
