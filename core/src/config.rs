//! 設定 (`%APPDATA%\fuel-usb-uploader\config.toml`)。
//!
//! ```toml
//! endpoint = "https://auth.ippoan.org/m/fuel-utran/uploads"
//! ```
//!
//! ファイルが無ければ既定値。資格情報はここに書かない (Credential Manager に置く)。

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{APP_NAME, DEFAULT_ENDPOINT};

/// 設定ファイルの名前。
pub const FILE_NAME: &str = "config.toml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// 送り先 URL (`https://` だけ)。
    pub endpoint: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            endpoint: DEFAULT_ENDPOINT.to_string(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    #[serde(default)]
    endpoint: Option<String>,
}

#[derive(Debug)]
pub enum ConfigError {
    Read(io::Error),
    Parse(String),
    /// 送り先が `https://` で始まらない (資格情報を平文で送らない)。
    NotHttps,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Read(e) => write!(f, "設定ファイルを読めません: {e}"),
            ConfigError::Parse(e) => write!(f, "設定ファイルの書式が正しくありません: {e}"),
            ConfigError::NotHttps => write!(f, "endpoint は https:// で始めてください"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// `%APPDATA%` を渡すと設定ファイルのパス。
pub fn path(appdata: &Path) -> PathBuf {
    appdata.join(APP_NAME).join(FILE_NAME)
}

/// 設定ファイルの中身を読む。書かれていない項目は既定値。
pub fn parse(text: &str) -> Result<Config, ConfigError> {
    let f: FileConfig = toml::from_str(text).map_err(|e| ConfigError::Parse(e.to_string()))?;
    let endpoint = match f.endpoint {
        Some(e) => e.trim().to_string(),
        None => DEFAULT_ENDPOINT.to_string(),
    };
    if !endpoint
        .get(..8)
        .is_some_and(|p| p.eq_ignore_ascii_case("https://"))
        || endpoint.len() <= 8
    {
        return Err(ConfigError::NotHttps);
    }
    Ok(Config { endpoint })
}

/// 設定を読む。`appdata` が無い・ファイルが無いときは既定値。
pub fn load(appdata: Option<&Path>) -> Result<Config, ConfigError> {
    let Some(appdata) = appdata else {
        return Ok(Config::default());
    };
    match fs::read_to_string(path(appdata)) {
        Ok(text) => parse(&text),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(ConfigError::Read(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_endpoint() {
        assert_eq!(
            Config::default().endpoint,
            "https://auth.ippoan.org/m/fuel-utran/uploads"
        );
        assert_eq!(parse("").unwrap(), Config::default());
    }

    #[test]
    fn override_endpoint() {
        let c =
            parse(r#"endpoint = " https://auth-staging.example/m/fuel-utran/uploads ""#).unwrap();
        assert_eq!(
            c.endpoint,
            "https://auth-staging.example/m/fuel-utran/uploads"
        );
    }

    #[test]
    fn rejects_http_and_garbage() {
        assert!(matches!(
            parse(r#"endpoint = "http://example/m""#),
            Err(ConfigError::NotHttps)
        ));
        assert!(matches!(
            parse(r#"endpoint = "https://""#),
            Err(ConfigError::NotHttps)
        ));
        assert!(matches!(parse("endpoint = "), Err(ConfigError::Parse(_))));
        assert!(matches!(
            parse(r#"password = "x""#),
            Err(ConfigError::Parse(_))
        ));
    }

    #[test]
    fn path_and_load() {
        let base =
            std::env::temp_dir().join(format!("fuel-usb-core-config-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        assert_eq!(
            path(&base),
            base.join("fuel-usb-uploader").join("config.toml")
        );
        assert_eq!(load(None).unwrap(), Config::default());
        assert_eq!(load(Some(&base)).unwrap(), Config::default());

        fs::create_dir_all(base.join(APP_NAME)).unwrap();
        fs::write(path(&base), "endpoint = \"https://x.example/up\"\n").unwrap();
        assert_eq!(load(Some(&base)).unwrap().endpoint, "https://x.example/up");
        fs::remove_dir_all(&base).unwrap();
    }
}
