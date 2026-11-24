//! The reviewed workspace model: what configuration *means*, independent of where it came from.
//!
//! These types are deserialized from the layered configuration sources by
//! `infrastructure::config`, and consumed by use cases through
//! `application::ports::ConfigurationProvider`. Nothing here knows about Figment, argv,
//! environment variables, or the current directory.

use isolock_plan::profile::{Profile, PROFILE_SCHEMA_VERSION};

/// The only configuration schema this build accepts.
pub const SCHEMA_VERSION: u32 = PROFILE_SCHEMA_VERSION;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Every validated knob a reviewed workspace may declare.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Settings {
    pub schema_version: u32,
    pub storage: Storage,
    pub profiles: BTreeMap<String, Profile>,
    pub gc: GarbageCollection,
    pub proxy: ProxySettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            storage: Storage::default(),
            profiles: BTreeMap::new(),
            gc: GarbageCollection::default(),
            proxy: ProxySettings::default(),
        }
    }
}

/// Where reviewed artifacts live and how they travel between workspaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Storage {
    pub cas_dir: PathBuf,
    pub transport: Transport,
    pub source: Option<String>,
    pub reference: String,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            cas_dir: ".isolock/cas".into(),
            transport: Transport::Directory,
            source: None,
            reference: "isolock/v1".into(),
        }
    }
}

/// The reviewed artifact transport selected by configuration.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Transport {
    #[default]
    Directory,
    Git,
}

/// Collection roots that keep reachable blobs alive.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct GarbageCollection {
    pub roots: Vec<String>,
}

/// How the loopback interceptor behaves when a request is not in the locked plan.
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ProxyMode {
    Connected,
    Ci,
    #[default]
    LocalAirlock,
}

impl<'de> Deserialize<'de> for ProxyMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        match value.to_ascii_lowercase().as_str() {
            "connected" => Ok(Self::Connected),
            "ci" => Ok(Self::Ci),
            "local-airlock" | "localairlock" => Ok(Self::LocalAirlock),
            _ => Err(serde::de::Error::custom(
                "mode must be connected, ci, or local-airlock",
            )),
        }
    }
}

impl From<ProxyMode> for isolock_proxy::ProxyConfig {
    fn from(mode: ProxyMode) -> Self {
        let mode = match mode {
            ProxyMode::Connected => isolock_core::AirlockMode::Connected,
            ProxyMode::Ci => isolock_core::AirlockMode::Ci,
            ProxyMode::LocalAirlock => isolock_core::AirlockMode::LocalAirlock,
        };
        Self::new("127.0.0.1", 0, mode)
    }
}

/// Proxy paths, mode, and the reviewed resource bounds.
///
/// `limits` is the proxy crate's own value type: it is already a pure, validated model with no
/// I/O, so the domain reuses it instead of duplicating the bounds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct ProxySettings {
    pub plan: PathBuf,
    pub cargo_home: PathBuf,
    pub mode: ProxyMode,
    pub port: u16,
    pub limits: isolock_proxy::ProxyLimits,
}

impl Default for ProxySettings {
    fn default() -> Self {
        Self {
            plan: ".isolock/plan.json".into(),
            cargo_home: ".isolock/cargo-home".into(),
            mode: ProxyMode::LocalAirlock,
            port: 0,
            limits: isolock_proxy::ProxyLimits::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_are_the_reviewed_offline_posture() {
        let settings = Settings::default();
        assert_eq!(settings.schema_version, SCHEMA_VERSION);
        assert_eq!(settings.storage.transport, Transport::Directory);
        assert_eq!(settings.storage.reference, "isolock/v1");
        assert_eq!(settings.proxy.mode, ProxyMode::LocalAirlock);
        assert!(settings.profiles.is_empty());
    }

    #[test]
    fn proxy_mode_accepts_legacy_spellings_and_rejects_anything_else() {
        for (text, expected) in [
            ("connected", ProxyMode::Connected),
            ("CI", ProxyMode::Ci),
            ("local-airlock", ProxyMode::LocalAirlock),
            ("localairlock", ProxyMode::LocalAirlock),
        ] {
            let value: ProxyMode = serde_json::from_value(serde_json::Value::String(text.into()))
                .expect("reviewed mode spelling");
            assert_eq!(value, expected, "{text}");
        }
        assert!(
            serde_json::from_value::<ProxyMode>(serde_json::Value::String("open".into())).is_err()
        );
    }

    #[test]
    fn storage_and_proxy_tables_reject_unknown_keys() {
        assert!(toml::from_str::<Storage>("[storage]\nsource-dir='x'").is_err());
        assert!(toml::from_str::<Settings>("[proxy]\nbogus=1").is_err());
    }
}
