//! 更新の確認と入れ替えの純粋な部分: 版の比較、GitHub Release の応答の解釈、
//! ダウンロード先の制限、sha256 の照合、入れ替えの PowerShell の組み立て。
//!
//! 通信・ファイル・プロセスの起動は `app` 側 (Windows) が持つ。

use std::fmt;
use std::time::Duration;

/// 最新の Release を引く URL (repo は public なので認証なし)。
pub const LATEST_URL: &str =
    "https://api.github.com/repos/ohishi-exp/fuel-usb-uploader/releases/latest";

/// Release の asset の置き場 (版ごとに `v<x.y.z>/<名前>`)。
const DOWNLOAD_BASE: &str = "https://github.com/ohishi-exp/fuel-usb-uploader/releases/download";

/// ダウンロードで辿ってよい host (https だけ)。
const DOWNLOAD_HOSTS: [&str; 3] = [
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

/// 起動してから最初に自動で確かめるまでの時間。
pub const CHECK_DELAY: Duration = Duration::from_secs(60);

/// 自動で確かめる間隔。
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// ダウンロード 1 回の timeout (MSI は数 MB だが、遅い回線でも打ち切らないよう送信より長く)。
pub const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);

/// redirect を辿る最大回数。
pub const MAX_REDIRECTS: usize = 5;

/// MSI の上限 (これを超えたら打ち切る)。
pub const MAX_MSI_BYTES: u64 = 50 * 1024 * 1024;

/// `.sha256` の上限。
pub const MAX_SHA256_BYTES: u64 = 1024;

/// 版 (`major.minor.patch`)。pre-release は付けない運用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(pub u32, pub u32, pub u32);

impl Version {
    /// `0.1.2` / `v0.1.2` だけを受ける。
    pub fn parse(s: &str) -> Option<Self> {
        let mut it = s
            .strip_prefix('v')
            .unwrap_or(s)
            .split('.')
            .map(|p| p.parse::<u32>().ok());
        let v = Self(it.next()??, it.next()??, it.next()??);
        it.next().is_none().then_some(v)
    }

    /// このアプリの版 (workspace の版)。
    pub fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).expect("CARGO_PKG_VERSION は x.y.z")
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// `releases/latest` の応答から版を読む (`tag_name` だけを見る)。
pub fn parse_latest(json: &str) -> Option<Version> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    Version::parse(v.get("tag_name")?.as_str()?)
}

/// MSI のファイル名 (`fuel-usb-uploader-<x.y.z>-x64.msi`。release.yml が付ける名前)。
pub fn msi_name(v: Version) -> String {
    format!("fuel-usb-uploader-{v}-x64.msi")
}

/// MSI と `.sha256` の URL (版から組み立てる。応答の assets は読まない)。
pub fn asset_urls(v: Version) -> (String, String) {
    let msi = format!("{DOWNLOAD_BASE}/v{v}/{}", msi_name(v));
    let sha = format!("{msi}.sha256");
    (msi, sha)
}

/// ダウンロードで繋いでよい URL か (https で、host が GitHub の Release の 3 つのどれか。
/// userinfo・port 付きは受けない)。redirect の行き先ごとに確かめる。
pub fn allowed_download_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.contains(['@', ':']) {
        return false;
    }
    DOWNLOAD_HOSTS.iter().any(|h| host.eq_ignore_ascii_case(h))
}

/// 計算した sha256 (hex) と `.sha256` の中身 (先頭の 64 桁の hex。前後の空白・改行は無視) が合うか。
pub fn check_sha256(actual_hex: &str, sha256_file: &str) -> bool {
    let Some(expected) = sha256_file.split_whitespace().next() else {
        return false;
    };
    expected.len() == 64
        && expected.bytes().all(|b| b.is_ascii_hexdigit())
        && expected.eq_ignore_ascii_case(actual_hex)
}

/// PowerShell の単一引用符の文字列にする (`'` と、PowerShell が `'` と同じに扱う ‘ ’ ‚ ‛ を重ねる)。
fn ps_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}') {
            out.push(c);
        }
        out.push(c);
    }
    out.push('\'');
    out
}

/// 入れ替えの PowerShell (`-Command` に渡す 1 行):
/// 今のプロセス (`pid`) の終了を待つ → MSI を入れる (終わるまで待つ) → 新しい exe を起動する。
///
/// パスは単一引用符で包む (`%` も `$` も展開されない)。msiexec にはパスを `"` で包んで渡す
/// (`Start-Process -ArgumentList` は要素を引用せずに空白でつなぐため)。
/// UAC で「いいえ」を押しても、最後の起動で今の版が起動し直す。
pub fn install_script(pid: u32, msi: &str, exe: &str) -> String {
    format!(
        "Wait-Process -Id {pid} -ErrorAction SilentlyContinue; \
         Start-Process -FilePath msiexec.exe -ArgumentList '/i',{},'/passive','/norestart' -Wait; \
         Start-Process -FilePath {}",
        ps_quote(&format!("\"{msi}\"")),
        ps_quote(exe),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parse() {
        assert_eq!(Version::parse("v0.1.2"), Some(Version(0, 1, 2)));
        assert_eq!(Version::parse("1.20.3"), Some(Version(1, 20, 3)));
        for bad in [
            "",
            "v",
            "0.1",
            "0.1.2.3",
            "v0.1.x",
            "0.1.2-rc1",
            "vv0.1.2",
            " 0.1.2",
        ] {
            assert_eq!(Version::parse(bad), None, "{bad}");
        }
        assert_eq!(Version(0, 1, 2).to_string(), "0.1.2");
        assert_eq!(
            Version::current().to_string(),
            env!("CARGO_PKG_VERSION").to_string()
        );
    }

    #[test]
    fn version_order() {
        assert!(Version(0, 1, 2) > Version(0, 1, 1));
        assert!(Version(0, 2, 0) > Version(0, 1, 9));
        assert!(Version(1, 0, 0) > Version(0, 99, 99));
        assert!(Version(0, 1, 10) > Version(0, 1, 9));
        assert_eq!(Version(0, 1, 1), Version(0, 1, 1));
    }

    #[test]
    fn parse_latest_ok() {
        let json = r#"{"tag_name":"v0.1.2","name":"v0.1.2","assets":[{"name":"x"}]}"#;
        assert_eq!(parse_latest(json), Some(Version(0, 1, 2)));
    }

    #[test]
    fn parse_latest_bad() {
        for bad in [
            "",
            "not json",
            "[]",
            r#"{"name":"v0.1.2"}"#,
            r#"{"tag_name":12}"#,
            r#"{"tag_name":"latest"}"#,
            r#"{"tag_name":"v0.1.2-rc1"}"#,
        ] {
            assert_eq!(parse_latest(bad), None, "{bad}");
        }
    }

    #[test]
    fn asset_urls_from_version() {
        let (msi, sha) = asset_urls(Version(0, 1, 2));
        assert_eq!(
            msi,
            "https://github.com/ohishi-exp/fuel-usb-uploader/releases/download/v0.1.2/fuel-usb-uploader-0.1.2-x64.msi"
        );
        assert_eq!(sha, format!("{msi}.sha256"));
        assert!(allowed_download_url(&msi));
        assert!(allowed_download_url(&sha));
    }

    #[test]
    fn download_url_hosts() {
        for ok in [
            "https://github.com/ohishi-exp/fuel-usb-uploader/releases/download/v0.1.2/a.msi",
            "https://objects.githubusercontent.com/github-production-release-asset/1?x=y",
            "https://release-assets.githubusercontent.com/github-production-release-asset/1",
            "https://GitHub.com/a",
            "https://github.com",
        ] {
            assert!(allowed_download_url(ok), "{ok}");
        }
        for bad in [
            "http://github.com/a",
            "ftp://github.com/a",
            "https://evil.example/a",
            "https://github.com.evil.example/a",
            "https://evilgithub.com/a",
            "https://api.github.com/a",
            "https://raw.githubusercontent.com/a",
            "https://github.com@evil.example/a",
            "https://user@github.com/a",
            "https://github.com:8443/a",
            "github.com/a",
            "",
        ] {
            assert!(!allowed_download_url(bad), "{bad}");
        }
    }

    #[test]
    fn sha256_check() {
        let h = "3a7bd3e2360a3d29eea436fcfb7e44c735d117c42d1c1835420b6b9942dd4f1b";
        assert!(check_sha256(h, h));
        assert!(check_sha256(h, &h.to_uppercase()));
        assert!(check_sha256(&h.to_uppercase(), h));
        assert!(check_sha256(h, &format!("{h}\r\n")));
        assert!(check_sha256(h, &format!("  {h}\n")));
        assert!(check_sha256(
            h,
            &format!("{h}  fuel-usb-uploader-0.1.2-x64.msi\n")
        ));
        let other = "0".repeat(64);
        assert!(!check_sha256(h, &other));
        assert!(!check_sha256(h, ""));
        assert!(!check_sha256(h, &h[..63]));
        assert!(!check_sha256("", ""));
        let not_hex = format!("{}zz", &h[..62]);
        assert!(!check_sha256(&not_hex, &not_hex));
    }

    #[test]
    fn script_plain() {
        assert_eq!(
            install_script(
                1234,
                r"C:\Users\a\AppData\Local\Temp\fuel-usb-uploader-0.1.2-x64.msi",
                r"C:\Program Files\fuel-usb-uploader\fuel-usb-uploader.exe"
            ),
            "Wait-Process -Id 1234 -ErrorAction SilentlyContinue; \
             Start-Process -FilePath msiexec.exe -ArgumentList '/i',\
             '\"C:\\Users\\a\\AppData\\Local\\Temp\\fuel-usb-uploader-0.1.2-x64.msi\"',\
             '/passive','/norestart' -Wait; \
             Start-Process -FilePath 'C:\\Program Files\\fuel-usb-uploader\\fuel-usb-uploader.exe'"
        );
    }

    #[test]
    fn script_quotes_space_percent() {
        let s = install_script(
            7,
            r"C:\Users\O'Brien %USERNAME% $env:X\Temp\a.msi",
            "C:\\Users\\D\u{2019}Arc\\a b.exe",
        );
        assert!(
            s.contains(r#"'"C:\Users\O''Brien %USERNAME% $env:X\Temp\a.msi"'"#),
            "{s}"
        );
        assert!(
            s.ends_with("Start-Process -FilePath 'C:\\Users\\D\u{2019}\u{2019}Arc\\a b.exe'"),
            "{s}"
        );
        assert!(s.starts_with("Wait-Process -Id 7 "), "{s}");
    }
}
