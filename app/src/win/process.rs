//! 1 つのドライブの取り込み: `UTRAN.CSV` を探す → サイズ確認 → 送る (一時的な失敗はやり直す)
//! → 401 なら資格情報を聞き直して 1 回だけ送り直す → ログと通知。

use std::collections::HashSet;
use std::fs;
use std::sync::{Arc, Mutex};
use std::thread;

use fuel_usb_core::drives;
use fuel_usb_core::message::{self, Notice};
use fuel_usb_core::outcome::Outcome;
use fuel_usb_core::retry;
use fuel_usb_core::utran;
use fuel_usb_core::{config::Config, PROBE_ATTEMPTS, PROBE_INTERVAL};
use reqwest::blocking::Client;

use super::credential::{self, Credential};
use super::{log, notify, upload};

pub struct Context {
    client: Client,
    config: Config,
    /// 処理中のドライブ (同じドライブを同時に 2 回送らない)
    busy: Mutex<HashSet<char>>,
    /// 資格情報のダイアログは 1 つずつ
    prompt_lock: Mutex<()>,
}

impl Context {
    pub fn new(config: Config) -> Result<Self, ()> {
        Ok(Self {
            client: upload::client().map_err(|_| ())?,
            config,
            busy: Mutex::new(HashSet::new()),
            prompt_lock: Mutex::new(()),
        })
    }
}

/// worker thread でドライブを処理する。`manual` は「今すぐ確認」(探すのは 1 回、見つからなくても通知)。
pub fn spawn(ctx: &Arc<Context>, drive: char, manual: bool) {
    {
        let mut busy = ctx.busy.lock().unwrap_or_else(|e| e.into_inner());
        if !busy.insert(drive) {
            return;
        }
    }
    let ctx = Arc::clone(ctx);
    thread::spawn(move || {
        let outcome = handle(&ctx, drive, manual);
        if let Some(o) = outcome {
            log::outcome(Some(drive), &o);
            notify::show(&message::notice(Some(drive), &o));
        }
        ctx.busy
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&drive);
    });
}

/// 結果を返す。`None` は通知もログもしない (自動の検知で `UTRAN.CSV` の無い USB だったとき)。
fn handle(ctx: &Context, drive: char, manual: bool) -> Option<Outcome> {
    let root = drives::root(drive);
    let attempts = if manual { 1 } else { PROBE_ATTEMPTS };
    let mut path = None;
    for i in 0..attempts {
        if let Ok(Some(p)) = utran::find_in(root.as_ref()) {
            path = Some(p);
            break;
        }
        if i + 1 < attempts {
            thread::sleep(PROBE_INTERVAL);
        }
    }
    let Some(path) = path else {
        return manual.then_some(Outcome::NotFound);
    };

    match fs::metadata(&path) {
        Ok(m) => {
            if let Err(bytes) = utran::check_size(m.len()) {
                return Some(Outcome::FileTooLarge { bytes });
            }
        }
        Err(_) => return Some(Outcome::ReadFailed),
    }
    let Ok(body) = fs::read(&path) else {
        return Some(Outcome::ReadFailed);
    };
    if let Err(bytes) = utran::check_size(body.len() as u64) {
        return Some(Outcome::FileTooLarge { bytes });
    }

    let cred = match credential::read() {
        Some(c) => c,
        None => match prompt_credentials(ctx, None) {
            Some(c) => c,
            None => return Some(Outcome::NoCredential),
        },
    };

    let outcome = send_with_retry(ctx, &cred, &body);
    if outcome != Outcome::Unauthorized {
        return Some(outcome);
    }
    log::outcome(Some(drive), &outcome);
    match reauth(ctx, &cred) {
        Some(fresh) => Some(send_with_retry(ctx, &fresh, &body)),
        None => Some(Outcome::Unauthorized),
    }
}

fn send_with_retry(ctx: &Context, cred: &Credential, body: &[u8]) -> Outcome {
    retry::run(
        || upload::send(&ctx.client, &ctx.config.endpoint, cred, body),
        thread::sleep,
    )
}

/// ダイアログを出して保存する (呼ぶ側が `prompt_lock` を持つ)。
fn prompt_and_save(reason: Option<&str>, prefill_user: Option<&str>) -> Option<Credential> {
    let Some(c) = credential::prompt(reason, prefill_user) else {
        log::event("credential_cancelled");
        return None;
    };
    if credential::write(&c) {
        log::event("credential_saved");
        notify::show(&Notice {
            title: "給油データ".into(),
            body: "資格情報を保存しました".into(),
        });
    } else {
        log::event("credential_save_failed");
        notify::show(&Notice {
            title: "給油データ".into(),
            body: "資格情報を保存できませんでした (今回だけ使います)".into(),
        });
    }
    Some(c)
}

/// 「資格情報を設定」・初回起動・未設定のまま USB が挿されたとき。
pub fn prompt_credentials(ctx: &Context, reason: Option<&str>) -> Option<Credential> {
    let _guard = ctx.prompt_lock.lock().unwrap_or_else(|e| e.into_inner());
    let prefill = credential::read().map(|c| c.username);
    prompt_and_save(reason, prefill.as_deref())
}

/// 401 を受けたとき。待っている間に別の thread が新しい資格情報を保存していれば、聞かずにそれを使う。
fn reauth(ctx: &Context, used: &Credential) -> Option<Credential> {
    let _guard = ctx.prompt_lock.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(current) = credential::read() {
        if current != *used {
            return Some(current);
        }
    }
    prompt_and_save(
        Some("資格情報が正しくありません。管理者から受け取った資格情報を入力し直してください"),
        Some(&used.username),
    )
}
