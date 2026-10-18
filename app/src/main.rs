//! fuel-usb-uploader: USB に挿した `UTRAN.CSV` を auth-worker の machine-relay へ送る Windows 常駐アプリ
//! (Refs yhonda-ohishi/nginx#814)。判断はすべて `fuel-usb-core` にあり、ここは Windows との接点だけ。

// コンソール窓を出さない (トレイ常駐)
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod win;

#[cfg(windows)]
fn main() {
    win::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("fuel-usb-uploader は Windows 専用です");
    std::process::exit(1);
}
