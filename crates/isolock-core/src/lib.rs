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

/// Parsed semantic version components (major.minor.patch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Semver {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Semver {
    /// Parses a semantic version string in "X.Y.Z" format.
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
