//! Cargo source replacement generation, without environment reads or proxy ownership.

use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum CargoConfigError {
    CreateHome { path: PathBuf, source: io::Error },
    Serialize(toml::ser::Error),
    Write { path: PathBuf, source: io::Error },
}
impl fmt::Display for CargoConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreateHome { path, source } => {
                write!(f, "cannot create Cargo home {}: {source}", path.display())
            }
            Self::Serialize(_) => f.write_str("cannot serialize Cargo registry config"),
            Self::Write { path, source } => {
                write!(f, "cannot write Cargo config {}: {source}", path.display())
            }
        }
    }
}
impl Error for CargoConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::CreateHome { source, .. } | Self::Write { source, .. } => Some(source),
            Self::Serialize(error) => Some(error),
        }
    }
}

pub fn prepare(home: &Path, origin: &str) -> Result<(), CargoConfigError> {
    let config = toml::Value::try_from(serde_json::json!({"source": {
        "crates-io": {"replace-with": "isolock"},
        "isolock": {"registry": format!("sparse+{origin}/cargo/")}
    }}))
    .map_err(CargoConfigError::Serialize)?;
    let contents = toml::to_string_pretty(&config).map_err(CargoConfigError::Serialize)?;
    fs::create_dir_all(home).map_err(|source| CargoConfigError::CreateHome {
        path: home.into(),
        source,
    })?;
    let path = home.join("config.toml");
    fs::write(&path, contents).map_err(|source| CargoConfigError::Write { path, source })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_contains_typed_local_sparse_source_and_escapes_strings() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("cargo home");
        let origin = "http://[::1]:4567";
        prepare(&home, origin).unwrap();
        let config: toml::Value =
            toml::from_str(&fs::read_to_string(home.join("config.toml")).unwrap()).unwrap();
        assert_eq!(
            config["source"]["crates-io"]["replace-with"].as_str(),
            Some("isolock")
        );
        assert_eq!(
            config["source"]["isolock"]["registry"].as_str(),
            Some("sparse+http://[::1]:4567/cargo/")
        );
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[test]
    fn failures_preserve_the_operation_and_path() {
        let temp = tempfile::tempdir().unwrap();
        let blocked = temp.path().join("not-a-directory");
        fs::write(&blocked, "blocked").unwrap();
        assert!(
            matches!(prepare(&blocked, "http://127.0.0.1:1"), Err(CargoConfigError::CreateHome { path, .. }) if path == blocked)
        );
        fs::create_dir(temp.path().join("config.toml")).unwrap();
        assert!(matches!(
            prepare(temp.path(), "http://127.0.0.1:1"),
            Err(CargoConfigError::Write { .. })
        ));
    }
}
