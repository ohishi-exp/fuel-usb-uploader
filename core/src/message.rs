//! 通知文 (日本語) とログ行。
//!
//! どちらにも資格情報 (username・password) は渡さない — 引数に無いので出しようがない。

use crate::outcome::{Outcome, Transient};
use crate::utran::MAX_BYTES;

/// トーストの 1 件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub body: String,
}

/// サーバーが返した `reason` を通知に載せるときの上限 (文字数)。
const REASON_MAX_CHARS: usize = 80;

fn mib(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

fn short(s: &str) -> String {
    let mut out: String = s.chars().take(REASON_MAX_CHARS).collect();
    if s.chars().count() > REASON_MAX_CHARS {
        out.push('…');
    }
    out
}

/// 結果の通知文。`drive` は処理したドライブ (`None` は特定のドライブに依らない通知)。
pub fn notice(drive: Option<char>, outcome: &Outcome) -> Notice {
    let title = match drive {
        Some(d) => format!("給油データ ({}:)", d.to_ascii_uppercase()),
        None => "給油データ".to_string(),
    };
    let body = match outcome {
        Outcome::Imported {
            inserted, skipped, ..
        } => format!("給油データを取り込みました: 追加 {inserted} 件 / 既存 {skipped} 件"),
        Outcome::BadCsv {
            error,
            line,
            reason,
        } => {
            let detail = match (line, reason) {
                (Some(l), Some(r)) => format!(" ({l} 行目: {})", short(r)),
                (Some(l), None) => format!(" ({l} 行目)"),
                (None, Some(r)) => format!(" ({})", short(r)),
                (None, None) if error == "empty" => " (データがありません)".to_string(),
                (None, None) => String::new(),
            };
            format!("UTRAN.CSV の内容を読み取れませんでした{detail}。ファイルを確認してください")
        }
        Outcome::Unauthorized => {
            "資格情報が正しくありません。トレイから再設定してください".to_string()
        }
        Outcome::TooLarge => format!(
            "UTRAN.CSV が大きすぎるため取り込めませんでした ({} まで)",
            mib(MAX_BYTES)
        ),
        Outcome::FileTooLarge { bytes } => format!(
            "UTRAN.CSV が大きすぎるため送りませんでした ({} / 上限 {})",
            mib(*bytes),
            mib(MAX_BYTES)
        ),
        Outcome::Transient(Transient::Unavailable) => {
            "サーバーが一時的に使えません。時間をおいてトレイの「今すぐ確認」からやり直してください"
                .to_string()
        }
        Outcome::Transient(Transient::Network) => {
            "サーバーに接続できませんでした。ネットワークを確認し、トレイの「今すぐ確認」からやり直してください"
                .to_string()
        }
        Outcome::Unknown { status } => {
            format!("取り込みに失敗しました (HTTP {status})。管理者に連絡してください")
        }
        Outcome::NotFound => "UTRAN.CSV が見つかりませんでした".to_string(),
        Outcome::ReadFailed => "UTRAN.CSV を読み込めませんでした".to_string(),
        Outcome::NoCredential => {
            "資格情報が未設定です。トレイの「資格情報を設定」から設定してください".to_string()
        }
    };
    Notice { title, body }
}

/// ログの 1 行 (改行なし)。日時・ドライブ・結果 (件数・エラーの種類) だけを書く。
pub fn log_line(timestamp: &str, drive: Option<char>, outcome: &Outcome) -> String {
    let drive = drive
        .map(|d| format!("{}:", d.to_ascii_uppercase()))
        .unwrap_or_else(|| "-".to_string());
    let detail = match outcome {
        Outcome::Imported {
            inserted, skipped, ..
        } => format!(" inserted={inserted} skipped={skipped}"),
        Outcome::BadCsv { error, line, .. } => match line {
            Some(l) => format!(" error={error} line={l}"),
            None => format!(" error={error}"),
        },
        Outcome::Unknown { status } => format!(" status={status}"),
        Outcome::FileTooLarge { bytes } => format!(" bytes={bytes}"),
        _ => String::new(),
    };
    format!("{timestamp}\t{drive}\t{}{detail}", outcome.kind())
}

/// 結果を伴わないできごと (起動・資格情報の保存など) のログ行。`event` は固定の語だけを渡す。
pub fn log_event(timestamp: &str, event: &str) -> String {
    format!("{timestamp}\t-\t{event}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imported_notice() {
        let n = notice(
            Some('e'),
            &Outcome::Imported {
                inserted: 12,
                skipped: 3,
                latest_start: None,
            },
        );
        assert_eq!(n.title, "給油データ (E:)");
        assert_eq!(n.body, "給油データを取り込みました: 追加 12 件 / 既存 3 件");
    }

    #[test]
    fn unauthorized_notice() {
        assert_eq!(
            notice(None, &Outcome::Unauthorized),
            Notice {
                title: "給油データ".into(),
                body: "資格情報が正しくありません。トレイから再設定してください".into()
            }
        );
    }

    #[test]
    fn bad_csv_notice_variants() {
        let b = |line, reason: Option<&str>, error: &str| {
            notice(
                Some('F'),
                &Outcome::BadCsv {
                    error: error.into(),
                    line,
                    reason: reason.map(Into::into),
                },
            )
            .body
        };
        assert_eq!(
            b(Some(7), Some("bad date"), "csv"),
            "UTRAN.CSV の内容を読み取れませんでした (7 行目: bad date)。ファイルを確認してください"
        );
        assert_eq!(
            b(Some(7), None, "csv"),
            "UTRAN.CSV の内容を読み取れませんでした (7 行目)。ファイルを確認してください"
        );
        assert_eq!(
            b(None, None, "empty"),
            "UTRAN.CSV の内容を読み取れませんでした (データがありません)。ファイルを確認してください"
        );
        assert_eq!(
            b(None, None, "bad_request"),
            "UTRAN.CSV の内容を読み取れませんでした。ファイルを確認してください"
        );
        let long = "あ".repeat(100);
        let body = b(None, Some(&long), "csv");
        assert!(body.contains(&format!("({}…)", "あ".repeat(80))));
    }

    #[test]
    fn size_notices() {
        assert_eq!(
            notice(
                None,
                &Outcome::FileTooLarge {
                    bytes: 6 * 1024 * 1024
                }
            )
            .body,
            "UTRAN.CSV が大きすぎるため送りませんでした (6.0 MB / 上限 5.0 MB)"
        );
        assert_eq!(
            notice(None, &Outcome::TooLarge).body,
            "UTRAN.CSV が大きすぎるため取り込めませんでした (5.0 MB まで)"
        );
    }

    #[test]
    fn every_outcome_has_text() {
        for o in [
            Outcome::network(),
            Outcome::Transient(Transient::Unavailable),
            Outcome::Unknown { status: 500 },
            Outcome::NotFound,
            Outcome::ReadFailed,
            Outcome::NoCredential,
        ] {
            assert!(!notice(Some('E'), &o).body.is_empty());
        }
        assert!(notice(None, &Outcome::Unknown { status: 502 })
            .body
            .contains("HTTP 502"));
    }

    #[test]
    fn log_lines() {
        let ts = "2026-10-18 12:00:00";
        assert_eq!(
            log_line(
                ts,
                Some('e'),
                &Outcome::Imported {
                    inserted: 12,
                    skipped: 3,
                    latest_start: Some("x".into())
                }
            ),
            "2026-10-18 12:00:00\tE:\timported inserted=12 skipped=3"
        );
        assert_eq!(
            log_line(
                ts,
                Some('E'),
                &Outcome::BadCsv {
                    error: "csv".into(),
                    line: Some(4),
                    reason: Some("理由はログに書かない".into())
                }
            ),
            "2026-10-18 12:00:00\tE:\tbad_csv error=csv line=4"
        );
        assert_eq!(
            log_line(ts, None, &Outcome::Unknown { status: 500 }),
            "2026-10-18 12:00:00\t-\tunknown status=500"
        );
        assert_eq!(
            log_line(ts, Some('G'), &Outcome::Unauthorized),
            "2026-10-18 12:00:00\tG:\tunauthorized"
        );
        assert_eq!(log_event(ts, "start"), "2026-10-18 12:00:00\t-\tstart");
    }
}
