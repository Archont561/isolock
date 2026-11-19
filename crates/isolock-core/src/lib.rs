#![forbid(unsafe_code)]
//! Core types, errors, and operational contracts for Isolock.

use serde::{Deserialize, Serialize};

/// Exported version constant synchronized with Cargo package version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Returns the static version string.
#[must_use]
pub const fn version() -> &'static str {
    VERSION
}

/// Operational modes for dependency airlock resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AirlockMode {
    /// Workstation with full upstream network access.
    #[default]
    Connected,
    /// Read-only CI mode: verifies hermeticity without publishing.
    Ci,
    /// Air-gapped / sealed workstation: fails closed on any cache miss.
    LocalAirlock,
}

/// Supported package registry ecosystems.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Registry {
    Conda,
    Pypi,
    Npm,
    Cargo,
}

impl Registry {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Conda => "conda",
            Self::Pypi => "pypi",
            Self::Npm => "npm",
            Self::Cargo => "cargo",
        }
    }
}

/// Recognized package archive suffixes; filenames and URLs are case-sensitive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryArchive {
    Conda,
    CondaLegacy,
    Wheel,
    Npm,
    Cargo,
}

impl RegistryArchive {
    /// Recognizes exact lowercase suffixes without normalizing any part of the filename.
    /// Callers must independently validate path safety and locked filename equality.
    #[must_use]
    #[allow(
        clippy::case_sensitive_file_extension_comparisons,
        reason = "Registry filenames are byte-exact locked inputs, not case-insensitive filesystem guesses"
    )]
    pub fn from_filename(filename: &str) -> Option<Self> {
        if filename.ends_with(".conda") {
            Some(Self::Conda)
        } else if filename.ends_with(".tar.bz2") {
            Some(Self::CondaLegacy)
        } else if filename.ends_with(".whl") {
            Some(Self::Wheel)
        } else if filename.ends_with(".tgz") {
            Some(Self::Npm)
        } else if filename.ends_with(".crate") {
            Some(Self::Cargo)
        } else {
            None
        }
    }
}

/// Parsed semantic version components (major.minor.patch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Semver {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Semver {
    /// Parses a semantic version string in "X.Y.Z" format.
    ///
    /// # Errors
    /// Returns an error unless there are exactly three unsigned integer components.
    pub fn parse(s: &str) -> Result<Self, &'static str> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return Err("version must have exactly major, minor, and patch components");
        }
        let major = parts[0].parse().map_err(|_| "invalid major version")?;
        let minor = parts[1].parse().map_err(|_| "invalid minor version")?;
        let patch = parts[2].parse().map_err(|_| "invalid patch version")?;
        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

/// Validates whether a given version string conforms to valid semver.
#[must_use]
pub fn is_valid_semver(v: &str) -> bool {
    Semver::parse(v).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_version_and_registry() {
        assert!(!VERSION.is_empty());
        assert_eq!(version(), VERSION);
        assert!(is_valid_semver(VERSION));
        assert_eq!(Registry::Cargo.as_str(), "cargo");
    }
}

#[cfg(test)]
mod archive_tests {
    use super::RegistryArchive;

    #[test]
    fn registry_suffix_policy_is_exact_while_basename_case_is_preserved() {
        for (filename, kind) in [
            ("Project.conda", RegistryArchive::Conda),
            ("Project.tar.bz2", RegistryArchive::CondaLegacy),
            ("Project.whl", RegistryArchive::Wheel),
            ("Project.tgz", RegistryArchive::Npm),
            ("Project.crate", RegistryArchive::Cargo),
        ] {
            assert_eq!(RegistryArchive::from_filename(filename), Some(kind));
            assert_eq!(
                RegistryArchive::from_filename(&filename.to_uppercase()),
                None
            );
        }
        for invalid in ["x.wHl", "x.tar.Bz2", "x.whl?download=1", "x.whl.exe", "x"] {
            assert_eq!(RegistryArchive::from_filename(invalid), None);
        }
    }
}
