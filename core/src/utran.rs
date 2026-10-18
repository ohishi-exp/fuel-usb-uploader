//! ドライブ直下の `UTRAN.CSV` を探す・送る前のサイズ確認。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// 探すファイル名 (大文字小文字は区別しない)。
pub const FILE_NAME: &str = "UTRAN.CSV";

/// サーバーの上限 (rust-fuel-worker の `MAX_BYTES`)。超えると 413 になるので送らない。
pub const MAX_BYTES: u64 = 5 * 1024 * 1024;

/// 名前が `UTRAN.CSV` か (大文字小文字を区別しない)。
pub fn is_utran(name: &str) -> bool {
    name.eq_ignore_ascii_case(FILE_NAME)
}

/// `dir` の**直下だけ**から `UTRAN.CSV` (ファイル) を探す。サブフォルダは見ない。
/// 見つからなければ `Ok(None)`。フォルダが読めなければ `Err` (マウント途中など)。
pub fn find_in(dir: &Path) -> io::Result<Option<PathBuf>> {
    let mut found: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if is_utran(&name) && entry.file_type()?.is_file() {
            found.push(entry.path());
        }
    }
    // 大文字小文字を区別するファイルシステムで複数あっても、毎回同じものを選ぶ
    found.sort();
    Ok(found.into_iter().next())
}

/// 送る前のサイズ確認。上限を超えたら `Err(size)`。
pub fn check_size(len: u64) -> Result<(), u64> {
    if len > MAX_BYTES {
        Err(len)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// テストごとの一時フォルダ (dev-dependency を増やさない)。
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static N: AtomicU32 = AtomicU32::new(0);
            let p = std::env::temp_dir().join(format!(
                "fuel-usb-core-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::SeqCst)
            ));
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn name_is_case_insensitive() {
        assert!(is_utran("UTRAN.CSV"));
        assert!(is_utran("utran.csv"));
        assert!(is_utran("Utran.Csv"));
        assert!(!is_utran("UTRAN.CSV.bak"));
        assert!(!is_utran("UTRAN.TXT"));
        assert!(!is_utran("XUTRAN.CSV"));
    }

    #[test]
    fn finds_file_at_root_only() {
        let d = TempDir::new();
        assert_eq!(find_in(&d.0).unwrap(), None);

        fs::create_dir(d.0.join("sub")).unwrap();
        fs::write(d.0.join("sub").join("UTRAN.CSV"), b"x").unwrap();
        assert_eq!(find_in(&d.0).unwrap(), None, "サブフォルダは見ない");

        fs::write(d.0.join("utran.csv"), b"x").unwrap();
        assert_eq!(find_in(&d.0).unwrap(), Some(d.0.join("utran.csv")));
    }

    #[test]
    fn directory_named_utran_is_ignored() {
        let d = TempDir::new();
        fs::create_dir(d.0.join("UTRAN.CSV")).unwrap();
        assert_eq!(find_in(&d.0).unwrap(), None);
    }

    #[test]
    fn missing_dir_is_error() {
        let d = TempDir::new();
        assert!(find_in(&d.0.join("nope")).is_err());
    }

    #[test]
    fn size_limit() {
        assert_eq!(MAX_BYTES, 5_242_880);
        assert_eq!(check_size(0), Ok(()));
        assert_eq!(check_size(MAX_BYTES), Ok(()));
        assert_eq!(check_size(MAX_BYTES + 1), Err(MAX_BYTES + 1));
    }
}
