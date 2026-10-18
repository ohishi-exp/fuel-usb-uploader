//! Windows のトースト通知。
//!
//! 送り元の AppUserModelID を登録しないため、PowerShell の ID を借りる (通知の送り元に
//! 「Windows PowerShell」と出る)。通知に出す文は `fuel_usb_core::message` が組み立てる。

use fuel_usb_core::message::Notice;
use tauri_winrt_notification::Toast;

pub fn show(n: &Notice) {
    let r = Toast::new(Toast::POWERSHELL_APP_ID)
        .title(&n.title)
        .text1(&n.body)
        .show();
    if r.is_err() {
        super::log::event("toast_failed");
    }
}
