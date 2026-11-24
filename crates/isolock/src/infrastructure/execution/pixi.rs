//! Pixi mirror rendering and temporary-file ownership, independent of the CLI process.

use std::error::Error;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

const KNOWN_HOSTS: &[&str] = &[
    "https://conda.anaconda.org",
    "https://repo.anaconda.com",
    "https://anaconda.org",
    "https://prefix.dev",
    "https://repo.prefix.dev",
];

#[derive(Debug)]
pub enum PixiConfigError {
    Read { path: PathBuf, source: io::Error },
    Parse(toml::de::Error),
    InvalidTable(&'static str),
    Serialize(toml::ser::Error),
    Temporary(io::Error),
    Write(io::Error),
}
impl fmt::Display for PixiConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never print raw TOML or parse diagnostics; user Pixi configs may contain credentials.
        match self {
            Self::Read { path, source } => {
                write!(f, "cannot read Pixi config {}: {source}", path.display())
            }
            Self::Parse(_) => f.write_str("invalid Pixi configuration TOML"),
            Self::InvalidTable(key) => write!(f, "Pixi config {key} must be a table"),
            Self::Serialize(_) => f.write_str("cannot serialize Pixi configuration"),
            Self::Temporary(source) => {
                write!(f, "cannot create temporary Pixi configuration: {source}")
            }
            Self::Write(source) => write!(f, "cannot write temporary Pixi configuration: {source}"),
        }
    }
}
impl Error for PixiConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read { source, .. } | Self::Temporary(source) | Self::Write(source) => {
                Some(source)
            }
            Self::Parse(source) => Some(source),
            Self::Serialize(source) => Some(source),
            Self::InvalidTable(_) => None,
        }
    }
}

pub struct PixiConfig {
    file: NamedTempFile,
}
impl PixiConfig {
    pub fn prepare(
        origin: &str,
        inherited: Option<&Path>,
        temporary_dir: &Path,
    ) -> Result<Self, PixiConfigError> {
        let original = inherited
            .map(|path| {
                fs::read_to_string(path).map_err(|source| PixiConfigError::Read {
                    path: path.into(),
                    source,
                })
            })
            .transpose()?;
        let contents = render(origin, original.as_deref())?;
        let mut file = tempfile::Builder::new()
            .prefix("isolock-pixi-config-")
            .suffix(".toml")
            .tempfile_in(temporary_dir)
            .map_err(PixiConfigError::Temporary)?;
        // NamedTempFile removes partial output on every write/flush/error path (private permissions).
        file.write_all(contents.as_bytes())
            .map_err(PixiConfigError::Write)?;
        file.flush().map_err(PixiConfigError::Write)?;
        Ok(Self { file })
    }
    pub fn path(&self) -> &Path {
        self.file.path()
    }
}

fn render(origin: &str, inherited: Option<&str>) -> Result<String, PixiConfigError> {
    let mut config = inherited
        .map(toml::from_str::<toml::Value>)
        .transpose()
        .map_err(PixiConfigError::Parse)?
        .unwrap_or_else(|| toml::Value::Table(toml::value::Table::new()));
    let table = config
        .as_table_mut()
        .ok_or(PixiConfigError::InvalidTable("root"))?;
    let mirrors = table
        .entry("mirrors".to_owned())
        .or_insert_with(|| toml::Value::Table(toml::value::Table::new()))
        .as_table_mut()
        .ok_or(PixiConfigError::InvalidTable("mirrors"))?;
    for upstream in mirrors.keys().cloned().collect::<Vec<_>>() {
        if let Some(local_url) = local_conda_mirror(origin, &upstream) {
            mirrors.insert(
                upstream,
                toml::Value::Array(vec![toml::Value::String(local_url)]),
            );
        }
    }
    for upstream in KNOWN_HOSTS {
        mirrors.insert(
            (*upstream).into(),
            toml::Value::Array(vec![toml::Value::String(format!(
                "{origin}/conda/channel/"
            ))]),
        );
    }
    toml::to_string_pretty(&config).map_err(PixiConfigError::Serialize)
}

fn local_conda_mirror(origin: &str, upstream: &str) -> Option<String> {
    let host = KNOWN_HOSTS.iter().copied().find(|host| {
        upstream == *host
            || upstream
                .strip_prefix(*host)
                .is_some_and(|suffix| suffix.starts_with('/'))
    })?;
    let channel = upstream
        .strip_prefix(host)
        .unwrap_or_default()
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(encode_path_segment)
        .collect::<Vec<_>>()
        .join("/");
    if channel.is_empty() {
        Some(format!("{origin}/conda/channel/"))
    } else {
        Some(format!("{origin}/conda/channel/{channel}/"))
    }
}

fn encode_path_segment(segment: &str) -> String {
    use std::fmt::Write as _;
    let mut encoded = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrors_preserve_options_and_encode_channel_segments() {
        let rendered = render("http://127.0.0.1:123", Some("tls-no-verify = false\n[mirrors]\n'https://conda.anaconda.org/conda forge' = ['https://old']\n'https://custom.example' = ['https://custom-mirror']\n")).unwrap();
        let value: toml::Value = toml::from_str(&rendered).unwrap();
        assert_eq!(value["tls-no-verify"].as_bool(), Some(false));
        assert_eq!(
            value["mirrors"]["https://conda.anaconda.org/conda forge"][0].as_str(),
            Some("http://127.0.0.1:123/conda/channel/conda%20forge/")
        );
        assert_eq!(
            value["mirrors"]["https://custom.example"][0].as_str(),
            Some("https://custom-mirror")
        );
        assert_eq!(
            local_conda_mirror(
                "http://local",
                "https://conda.anaconda.org.attacker/channel"
            ),
            None
        );
    }

    #[test]
    fn temporary_file_is_owned_and_original_config_is_unchanged() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("original.toml");
        fs::write(&original, "tls-no-verify = false\n").unwrap();
        let path = {
            let config =
                PixiConfig::prepare("http://127.0.0.1:3", Some(&original), temp.path()).unwrap();
            assert!(config.path().exists());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    fs::metadata(config.path()).unwrap().permissions().mode() & 0o077,
                    0
                );
            }
            config.path().to_owned()
        };
        assert!(!path.exists());
        assert_eq!(
            fs::read_to_string(original).unwrap(),
            "tls-no-verify = false\n"
        );
    }

    #[test]
    fn errors_are_structured_and_do_not_expose_config_contents() {
        assert!(matches!(
            render("http://local", Some("mirrors = 42")),
            Err(PixiConfigError::InvalidTable("mirrors"))
        ));
        let error = render("http://local", Some("secret = 'TOKEN\n")).unwrap_err();
        assert!(matches!(error, PixiConfigError::Parse(_)));
        assert!(!error.to_string().contains("TOKEN"));
        let temp = tempfile::tempdir().unwrap();
        assert!(matches!(
            PixiConfig::prepare(
                "http://local",
                Some(&temp.path().join("absent")),
                temp.path()
            ),
            Err(PixiConfigError::Read { .. })
        ));
        assert!(matches!(
            PixiConfig::prepare("http://local", None, &temp.path().join("absent")),
            Err(PixiConfigError::Temporary(_))
        ));
    }
}
