//! トレイ アイコンとメニュー (「資格情報を設定」「今すぐ確認」「終了」)、main thread のメッセージループ。

use std::sync::mpsc::Sender;

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, PostThreadMessageW, TranslateMessage, MSG, WM_QUIT,
};

use super::{log, Command};

/// 32x32 のアイコン (青い丸に白いしずく形)。画像ファイルを持たずに描く。
fn icon() -> Option<Icon> {
    const N: u32 = 32;
    let mut rgba = Vec::with_capacity((N * N * 4) as usize);
    for y in 0..N {
        for x in 0..N {
            let (fx, fy) = (x as f32 + 0.5 - 16.0, y as f32 + 0.5 - 16.0);
            let in_circle = fx * fx + fy * fy <= 15.0 * 15.0;
            // しずく: 下半分は円、上半分は三角
            let drop = {
                let (dx, dy) = (fx, fy - 3.0);
                (dy >= 0.0 && dx * dx + dy * dy <= 7.0 * 7.0)
                    || ((-12.0..0.0).contains(&dy) && dx.abs() <= 7.0 * (1.0 + dy / 12.0))
            };
            let px = match (in_circle, drop) {
                (true, true) => [255, 255, 255, 255],
                (true, false) => [0x1f, 0x6f, 0xd0, 255],
                _ => [0, 0, 0, 0],
            };
            rgba.extend_from_slice(&px);
        }
    }
    Icon::from_rgba(rgba, N, N).ok()
}

/// トレイを出してメッセージループを回す。「終了」で戻る。
pub fn run(tx: Sender<Command>) {
    let menu = Menu::new();
    let set_cred = MenuItem::new("資格情報を設定", true, None);
    let check_now = MenuItem::new("今すぐ確認", true, None);
    let quit = MenuItem::new("終了", true, None);
    if menu
        .append_items(&[
            &set_cred,
            &check_now,
            &PredefinedMenuItem::separator(),
            &quit,
        ])
        .is_err()
    {
        log::event("tray_error");
        return;
    }

    let mut builder = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("給油データ USB アップローダー");
    if let Some(i) = icon() {
        builder = builder.with_icon(i);
    }
    // 落とすとトレイから消えるので、ループの終わりまで持つ
    let Ok(_tray) = builder.build() else {
        log::event("tray_error");
        return;
    };

    let main_thread = unsafe { GetCurrentThreadId() };
    let (set_id, check_id, quit_id) = (
        set_cred.id().clone(),
        check_now.id().clone(),
        quit.id().clone(),
    );
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        if e.id == set_id {
            let _ = tx.send(Command::SetCredentials);
        } else if e.id == check_id {
            let _ = tx.send(Command::CheckNow);
        } else if e.id == quit_id {
            let _ = unsafe { PostThreadMessageW(main_thread, WM_QUIT, WPARAM(0), LPARAM(0)) };
        }
    }));

    let mut msg = MSG::default();
    loop {
        let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        // 0 = WM_QUIT、-1 = エラー
        if r.0 <= 0 {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
