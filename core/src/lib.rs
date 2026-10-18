//! fuel-usb-uploader の OS に依存しないロジック (Refs yhonda-ohishi/nginx#814)。
//!
//! Windows 専用の `app` crate (トレイ・USB の検知・送信・資格情報・トースト) から呼ばれ、
//! ここは Linux でも `cargo test` できる純粋な判断だけを持つ。
//!
//! - [`drives`] — ドライブ一覧の差分 (新しく挿されたドライブ)
//! - [`utran`] — ドライブ直下の `UTRAN.CSV` を探す・送る前のサイズ確認
//! - [`outcome`] — サーバーの応答の解釈
//! - [`retry`] — 一時的な失敗のやり直し方針
//! - [`message`] — 通知文とログ行
//! - [`config`] — 設定 (送り先 URL)
//! - [`credential`] — machine credential の形式の確認

pub mod config;
pub mod credential;
pub mod drives;
pub mod message;
pub mod outcome;
pub mod retry;
pub mod utran;

use std::time::Duration;

/// 送り先の既定 (auth-worker の machine-relay)。
pub const DEFAULT_ENDPOINT: &str = "https://auth.ippoan.org/m/fuel-utran/uploads";

/// Credential Manager の target 名。
pub const CREDENTIAL_TARGET: &str = "fuel-usb-uploader";

/// アプリの名前 (`%APPDATA%` / `%LOCALAPPDATA%` の下のフォルダ名にも使う)。
pub const APP_NAME: &str = "fuel-usb-uploader";

/// ドライブ一覧を取り直す間隔。
pub const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// 新しいドライブで `UTRAN.CSV` を探す回数と間隔 (マウント途中の対策。最大 10 秒)。
pub const PROBE_ATTEMPTS: u32 = 10;
pub const PROBE_INTERVAL: Duration = Duration::from_secs(1);

/// 1 回の送信の timeout。
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
