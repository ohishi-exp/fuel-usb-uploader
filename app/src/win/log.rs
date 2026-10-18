//! `%LOCALAPPDATA%\fuel-usb-uploader\log.txt` への追記。
//!
//! 書くのは日時・ドライブ・結果 (件数・エラーの種類) だけ (行の形は `fuel_usb_core::message`)。
//! 資格情報はこのモジュールに渡らない。1 MB を超えたら `log.old.txt` に回す。

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use fuel_usb_core::message;
use fuel_usb_core::outcome::Outcome;
use fuel_usb_core::APP_NAME;

const MAX_LOG_BYTES: u64 = 1024 * 1024;

static LOCK: Mutex<()> = Mutex::new(());

fn dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join(APP_NAME))
}

fn now() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn append(line: &str) {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(dir) = dir() else { return };
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("log.txt");
    if fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        let _ = fs::rename(&path, dir.join("log.old.txt"));
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{line}");
    }
}

/// 1 つのドライブの結果。
pub fn outcome(drive: Option<char>, outcome: &Outcome) {
    append(&message::log_line(&now(), drive, outcome));
}

/// 結果を伴わないできごと。`event` は固定の英語の語だけを渡す。
pub fn event(event: &'static str) {
    append(&message::log_event(&now(), event));
}
