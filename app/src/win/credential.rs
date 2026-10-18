//! machine credential の入力 (`CredUIPromptForWindowsCredentialsW`) と保存
//! (Credential Manager、target 名 `fuel-usb-uploader`)。
//!
//! パスワードは `Zeroizing` に入れ、使い終わった UTF-16 の作業領域も 0 で埋める。
//! ファイル・ログ・通知には出さない (`Debug` も伏せる)。

use std::ffi::c_void;
use std::fmt;
use std::ptr;

use fuel_usb_core::credential;
use fuel_usb_core::CREDENTIAL_TARGET;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{ERROR_SUCCESS, HWND};
use windows::Win32::Graphics::Gdi::HBITMAP;
use windows::Win32::Security::Credentials::{
    CredFree, CredPackAuthenticationBufferW, CredReadW, CredUIPromptForWindowsCredentialsW,
    CredUnPackAuthenticationBufferW, CredWriteW, CREDENTIALW, CREDUIWIN_GENERIC, CREDUI_INFOW,
    CRED_PACK_FLAGS, CRED_PACK_GENERIC_CREDENTIALS, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
};
use windows::Win32::System::Com::CoTaskMemFree;
use zeroize::{Zeroize, Zeroizing};

/// NUL 終端の UTF-16。
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[derive(Clone, PartialEq, Eq)]
pub struct Credential {
    pub username: String,
    pub password: Zeroizing<String>,
}

impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credential").finish_non_exhaustive()
    }
}

/// 保存済みの資格情報。無い・読めなければ `None`。
pub fn read() -> Option<Credential> {
    let target = wide(CREDENTIAL_TARGET);
    let mut p: *mut CREDENTIALW = ptr::null_mut();
    unsafe { CredReadW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None, &mut p) }.ok()?;
    let result = unsafe {
        let c = &*p;
        let username = if c.UserName.is_null() {
            None
        } else {
            c.UserName.to_string().ok()
        };
        let blob = if c.CredentialBlob.is_null() {
            &[][..]
        } else {
            std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize)
        };
        let password = Zeroizing::new(String::from_utf8(blob.to_vec()).ok());
        match (username, password.as_ref()) {
            (Some(u), Some(pw)) if !u.is_empty() && !pw.is_empty() => Some(Credential {
                username: u,
                password: Zeroizing::new(pw.clone()),
            }),
            _ => None,
        }
    };
    unsafe { CredFree(p as *const c_void) };
    result
}

/// 資格情報を保存する (同じ target 名を上書き)。
pub fn write(c: &Credential) -> bool {
    let target = wide(CREDENTIAL_TARGET);
    let user = wide(&c.username);
    let mut blob = Zeroizing::new(c.password.as_bytes().to_vec());
    let cred = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target.as_ptr() as *mut u16),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: PWSTR(user.as_ptr() as *mut u16),
        ..Default::default()
    };
    unsafe { CredWriteW(&cred, 0) }.is_ok()
}

/// UTF-16 の作業領域から NUL までを取り出し、前後の空白を落とす。
fn take(buf: &[u16]) -> Option<String> {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16(&buf[..end])
        .ok()
        .map(|s| s.trim().to_string())
}

/// 標準の資格情報ダイアログを 1 回出す。キャンセル・失敗は `None`。
fn prompt_once(message: &str, prefill_user: Option<&str>) -> Option<Credential> {
    let caption = wide("給油データ USB アップローダー");
    let text = wide(message);
    let info = CREDUI_INFOW {
        cbSize: std::mem::size_of::<CREDUI_INFOW>() as u32,
        hwndParent: HWND::default(),
        pszMessageText: PCWSTR(text.as_ptr()),
        pszCaptionText: PCWSTR(caption.as_ptr()),
        hbmBanner: HBITMAP::default(),
    };

    // ユーザー名だけを入れておく (パスワードは空)
    let mut inbuf: Vec<u8> = Vec::new();
    if let Some(u) = prefill_user {
        let u = wide(u);
        let empty = wide("");
        let mut size = 0u32;
        let _ = unsafe {
            CredPackAuthenticationBufferW(
                CRED_PACK_GENERIC_CREDENTIALS,
                PCWSTR(u.as_ptr()),
                PCWSTR(empty.as_ptr()),
                None,
                &mut size,
            )
        };
        inbuf = vec![0u8; size as usize];
        let packed = unsafe {
            CredPackAuthenticationBufferW(
                CRED_PACK_GENERIC_CREDENTIALS,
                PCWSTR(u.as_ptr()),
                PCWSTR(empty.as_ptr()),
                Some(inbuf.as_mut_ptr()),
                &mut size,
            )
        };
        if packed.is_err() {
            inbuf.clear();
        }
    }

    let mut package = 0u32;
    let mut out: *mut c_void = ptr::null_mut();
    let mut out_size = 0u32;
    let rc = unsafe {
        CredUIPromptForWindowsCredentialsW(
            Some(&info),
            0,
            &mut package,
            (!inbuf.is_empty()).then_some(inbuf.as_ptr() as *const c_void),
            inbuf.len() as u32,
            &mut out,
            &mut out_size,
            None,
            CREDUIWIN_GENERIC,
        )
    };
    if rc != ERROR_SUCCESS.0 || out.is_null() {
        return None;
    }

    let mut user = [0u16; 514];
    let mut pass = [0u16; 257];
    let mut user_len = user.len() as u32;
    let mut pass_len = pass.len() as u32;
    let unpacked = unsafe {
        CredUnPackAuthenticationBufferW(
            CRED_PACK_FLAGS(0),
            out,
            out_size,
            Some(PWSTR(user.as_mut_ptr())),
            &mut user_len,
            None,
            None,
            Some(PWSTR(pass.as_mut_ptr())),
            &mut pass_len,
        )
    };
    unsafe {
        ptr::write_bytes(out as *mut u8, 0, out_size as usize);
        CoTaskMemFree(Some(out as *const c_void));
    }
    let result = if unpacked.is_ok() {
        match (take(&user), take(&pass).map(Zeroizing::new)) {
            (Some(username), Some(password)) => Some(Credential { username, password }),
            _ => None,
        }
    } else {
        None
    };
    pass.zeroize();
    result
}

/// 形式が正しくなるまで (最大 3 回) ダイアログを出す。キャンセルは `None`。
pub fn prompt(reason: Option<&str>, prefill_user: Option<&str>) -> Option<Credential> {
    let mut message = reason.map(str::to_owned).unwrap_or_else(|| {
        "管理者から受け取った資格情報 (ユーザー名 mc_… とパスワード) を入力してください".into()
    });
    let mut prefill = prefill_user.map(str::to_owned);
    for _ in 0..3 {
        let c = prompt_once(&message, prefill.as_deref())?;
        match credential::validate(&c.username, &c.password) {
            Ok(()) => return Some(c),
            Err(e) => {
                message = format!("{e}。もう一度入力してください");
                prefill = Some(c.username.clone());
            }
        }
    }
    None
}
