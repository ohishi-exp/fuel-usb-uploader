//! 1 回の取り込みの結果と、サーバーの応答の解釈。
//!
//! 応答の仕様 (auth-worker の machine-relay → rust-fuel-worker の `/machine/fuel-utran/uploads`):
//! - 200 `{"ok":true,"key":…,"inserted":N,"skipped":N,"latest_start":…}`
//! - 400 CSV が読めない (`{"ok":false,"error":"csv","line":N,"reason":…}` / `{"error":"empty"}` など)
//! - 401 資格情報の誤り (`{"error":"Unauthorized"}` + `WWW-Authenticate: Basic`)
//! - 413 `too_large` (5 MB 超)
//! - 503 `relay_unavailable` など (一時的)

use serde::Deserialize;

/// 一時的な失敗の種類 (やり直しの対象)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transient {
    /// HTTP 503
    Unavailable,
    /// 接続できない・timeout・応答が途中で切れた
    Network,
}

/// 1 つのドライブを処理した結果。前半はサーバーの応答から、後半は送る前にこちらで決まるもの。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// 200: 取り込めた
    Imported {
        inserted: u64,
        skipped: u64,
        latest_start: Option<String>,
    },
    /// 400: CSV の誤り
    BadCsv {
        error: String,
        line: Option<u64>,
        reason: Option<String>,
    },
    /// 401: 資格情報の誤り
    Unauthorized,
    /// 413: サーバーが大きすぎると言った
    TooLarge,
    /// 503・通信失敗: やり直しの対象
    Transient(Transient),
    /// それ以外の応答 (200 なのに本文が読めないときも含む)
    Unknown { status: u16 },

    /// 送る前に 5 MB を超えていた (送っていない)
    FileTooLarge { bytes: u64 },
    /// `UTRAN.CSV` が無かった
    NotFound,
    /// `UTRAN.CSV` を読めなかった
    ReadFailed,
    /// 資格情報が未設定 (送っていない)
    NoCredential,
}

#[derive(Deserialize)]
struct OkBody {
    ok: bool,
    inserted: u64,
    skipped: u64,
    #[serde(default)]
    latest_start: Option<String>,
}

#[derive(Deserialize, Default)]
struct ErrorBody {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    line: Option<u64>,
    #[serde(default)]
    reason: Option<String>,
}

/// HTTP の応答 (status と本文) を解釈する。
pub fn interpret(status: u16, body: &str) -> Outcome {
    match status {
        200 => match serde_json::from_str::<OkBody>(body) {
            Ok(b) if b.ok => Outcome::Imported {
                inserted: b.inserted,
                skipped: b.skipped,
                latest_start: b.latest_start,
            },
            _ => Outcome::Unknown { status },
        },
        400 => {
            let b: ErrorBody = serde_json::from_str(body).unwrap_or_default();
            Outcome::BadCsv {
                error: b.error.unwrap_or_else(|| "bad_request".into()),
                line: b.line,
                reason: b.reason,
            }
        }
        401 => Outcome::Unauthorized,
        413 => Outcome::TooLarge,
        503 => Outcome::Transient(Transient::Unavailable),
        _ => Outcome::Unknown { status },
    }
}

impl Outcome {
    /// 通信そのものが失敗したとき。
    pub fn network() -> Self {
        Outcome::Transient(Transient::Network)
    }

    /// やり直しの対象か。
    pub fn is_transient(&self) -> bool {
        matches!(self, Outcome::Transient(_))
    }

    /// ログに書く結果の種類 (英小文字の固定の語)。
    pub fn kind(&self) -> &'static str {
        match self {
            Outcome::Imported { .. } => "imported",
            Outcome::BadCsv { .. } => "bad_csv",
            Outcome::Unauthorized => "unauthorized",
            Outcome::TooLarge => "too_large",
            Outcome::Transient(Transient::Unavailable) => "unavailable",
            Outcome::Transient(Transient::Network) => "network",
            Outcome::Unknown { .. } => "unknown",
            Outcome::FileTooLarge { .. } => "file_too_large",
            Outcome::NotFound => "not_found",
            Outcome::ReadFailed => "read_failed",
            Outcome::NoCredential => "no_credential",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_200() {
        let body = r#"{"inserted":12,"key":"utran/x.csv","latest_start":"2026-10-01 08:00:00","ok":true,"skipped":3}"#;
        assert_eq!(
            interpret(200, body),
            Outcome::Imported {
                inserted: 12,
                skipped: 3,
                latest_start: Some("2026-10-01 08:00:00".into())
            }
        );
        let body =
            r#"{"inserted":0,"key":"utran/x.csv","latest_start":null,"ok":true,"skipped":5}"#;
        assert_eq!(
            interpret(200, body),
            Outcome::Imported {
                inserted: 0,
                skipped: 5,
                latest_start: None
            }
        );
    }

    #[test]
    fn unreadable_200_is_unknown() {
        assert_eq!(interpret(200, "<html>"), Outcome::Unknown { status: 200 });
        assert_eq!(
            interpret(200, r#"{"ok":false,"inserted":0,"skipped":0}"#),
            Outcome::Unknown { status: 200 }
        );
    }

    #[test]
    fn bad_csv_400() {
        assert_eq!(
            interpret(
                400,
                r#"{"ok":false,"error":"csv","line":7,"reason":"bad date"}"#
            ),
            Outcome::BadCsv {
                error: "csv".into(),
                line: Some(7),
                reason: Some("bad date".into())
            }
        );
        assert_eq!(
            interpret(400, r#"{"error":"empty","ok":false}"#),
            Outcome::BadCsv {
                error: "empty".into(),
                line: None,
                reason: None
            }
        );
        assert_eq!(
            interpret(400, "not json"),
            Outcome::BadCsv {
                error: "bad_request".into(),
                line: None,
                reason: None
            }
        );
    }

    #[test]
    fn other_statuses() {
        assert_eq!(
            interpret(401, r#"{"error":"Unauthorized"}"#),
            Outcome::Unauthorized
        );
        assert_eq!(
            interpret(413, r#"{"error":"too_large","ok":false}"#),
            Outcome::TooLarge
        );
        let t = interpret(503, r#"{"error":"relay_unavailable"}"#);
        assert_eq!(t, Outcome::Transient(Transient::Unavailable));
        assert!(t.is_transient());
        assert!(Outcome::network().is_transient());
        assert_eq!(interpret(500, ""), Outcome::Unknown { status: 500 });
        assert_eq!(interpret(404, ""), Outcome::Unknown { status: 404 });
        assert!(!Outcome::Unknown { status: 500 }.is_transient());
        assert!(!Outcome::Unauthorized.is_transient());
    }

    #[test]
    fn kinds_are_stable() {
        assert_eq!(Outcome::network().kind(), "network");
        assert_eq!(
            Outcome::Transient(Transient::Unavailable).kind(),
            "unavailable"
        );
        assert_eq!(Outcome::Unauthorized.kind(), "unauthorized");
        assert_eq!(Outcome::FileTooLarge { bytes: 1 }.kind(), "file_too_large");
    }
}
