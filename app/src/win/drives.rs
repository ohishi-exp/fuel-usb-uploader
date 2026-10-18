//! リムーバブルドライブの一覧 (`GetLogicalDrives` + `GetDriveTypeW == DRIVE_REMOVABLE`)。

use fuel_usb_core::drives::{self, DriveSet};
use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
use windows::Win32::System::WindowsProgramming::DRIVE_REMOVABLE;

use super::credential::wide;

pub fn removable() -> DriveSet {
    let mask = unsafe { GetLogicalDrives() };
    drives::from_mask(mask)
        .into_iter()
        .filter(|&d| {
            let root = wide(&drives::root(d));
            unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) == DRIVE_REMOVABLE }
        })
        .collect()
}
