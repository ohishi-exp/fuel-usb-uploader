//! Windows 側の配線: トレイとメッセージループ (main thread)、USB の監視 (controller thread)、
//! ドライブごとの取り込み (worker thread)。

mod credential;
mod drives;
mod log;
mod notify;
mod process;
mod tray;
mod upload;

use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::thread;

use fuel_usb_core::config::{self, Config};
use fuel_usb_core::drives::Watcher;
use fuel_usb_core::message::Notice;
use fuel_usb_core::POLL_INTERVAL;
use windows::core::w;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Threading::CreateMutexW;

/// トレイのメニューから controller への指示。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    SetCredentials,
    CheckNow,
}

pub fn run() {
    if already_running() {
        return;
    }
    log::event("start");

    let config = match config::load(
        std::env::var_os("APPDATA")
            .as_deref()
            .map(std::path::Path::new),
    ) {
        Ok(c) => c,
        Err(e) => {
            log::event("config_error");
            notify::show(&Notice {
                title: "給油データ".into(),
                body: format!("{e}。既定の送り先を使います"),
            });
            Config::default()
        }
    };

    let (tx, rx) = mpsc::channel::<Command>();
    let ctx = match process::Context::new(config) {
        Ok(c) => Arc::new(c),
        Err(()) => {
            log::event("client_error");
            return;
        }
    };
    thread::spawn(move || controller(ctx, rx));

    tray::run(tx);
    log::event("exit");
}

/// 同じログオン セッションで 2 つ目が起動されたら何もせず終わる。
fn already_running() -> bool {
    // ハンドルは閉じない (プロセスの終わりまで mutex を残す。HANDLE に Drop は無い)
    let _handle = unsafe { CreateMutexW(None, false, w!("Local\\fuel-usb-uploader")) };
    (unsafe { GetLastError() }) == ERROR_ALREADY_EXISTS
}

/// 2 秒おきにリムーバブルドライブを見て、新しいドライブを worker に渡す。メニューの指示もここで受ける。
fn controller(ctx: Arc<process::Context>, rx: mpsc::Receiver<Command>) {
    // 初回起動で資格情報が無ければ聞く
    if credential::read().is_none() {
        process::prompt_credentials(&ctx, None);
    }

    let mut watcher = Watcher::new();
    loop {
        match rx.recv_timeout(POLL_INTERVAL) {
            Ok(Command::CheckNow) => {
                let all = drives::removable();
                if all.is_empty() {
                    notify::show(&Notice {
                        title: "給油データ".into(),
                        body: "USB メモリが見つかりませんでした".into(),
                    });
                }
                for d in all {
                    process::spawn(&ctx, d, true);
                }
            }
            Ok(Command::SetCredentials) => {
                let ctx = Arc::clone(&ctx);
                thread::spawn(move || process::prompt_credentials(&ctx, None));
            }
            Err(RecvTimeoutError::Timeout) => {
                for d in watcher.update(drives::removable()) {
                    process::spawn(&ctx, d, false);
                }
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}
