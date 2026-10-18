//! machine credential の形式の確認 (auth-worker の `lib/machine-credential.ts` と同じ形)。
//!
//! - username: `mc_` + 小文字 16 桁の hex
//! - password: 24 文字の base64url (`A-Z a-z 0-9 - _`)
//!
//! 保存の前に確かめ、打ち間違いを 401 になる前に止める。エラーには入力そのものを載せない。

use std::fmt;

pub const PASSWORD_LEN: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialError {
    BadUsername,
    BadPassword,
}

impl fmt::Display for CredentialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CredentialError::BadUsername => {
                write!(f, "ユーザー名は mc_ と 16 桁の英数字 (0-9, a-f) です")
            }
            CredentialError::BadPassword => {
                write!(f, "パスワードは {PASSWORD_LEN} 文字です")
            }
        }
    }
}

impl std::error::Error for CredentialError {}

pub fn valid_username(username: &str) -> bool {
    username
        .strip_prefix("mc_")
        .is_some_and(|h| h.len() == 16 && h.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
}

pub fn valid_password(password: &str) -> bool {
    password.len() == PASSWORD_LEN
        && password
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// 入力された資格情報を確かめる (前後の空白は呼ぶ側で落とす)。
pub fn validate(username: &str, password: &str) -> Result<(), CredentialError> {
    if !valid_username(username) {
        return Err(CredentialError::BadUsername);
    }
    if !valid_password(password) {
        return Err(CredentialError::BadPassword);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER: &str = "mc_0123456789abcdef";
    const PASS: &str = "AbCdEfGhIjKlMnOpQrStUv-_";

    #[test]
    fn accepts_issued_shape() {
        assert_eq!(PASS.len(), 24);
        assert_eq!(validate(USER, PASS), Ok(()));
    }

    #[test]
    fn rejects_bad_username() {
        for u in [
            "",
            "mc_",
            "mc_0123456789abcde",
            "mc_0123456789abcdef0",
            "mc_0123456789ABCDEF",
            "MC_0123456789abcdef",
            "xx_0123456789abcdef",
            "mc_0123456789abcdeg",
        ] {
            assert_eq!(validate(u, PASS), Err(CredentialError::BadUsername), "{u}");
        }
    }

    #[test]
    fn rejects_bad_password() {
        for p in [
            "",
            "short",
            &"a".repeat(23),
            &"a".repeat(25),
            "AbCdEfGhIjKlMnOpQrStUv+/",
        ] {
            assert_eq!(validate(USER, p), Err(CredentialError::BadPassword));
        }
    }

    #[test]
    fn messages_do_not_echo_input() {
        let m = CredentialError::BadPassword.to_string();
        assert!(!m.contains(PASS));
        assert_eq!(m, "パスワードは 24 文字です");
    }
}
