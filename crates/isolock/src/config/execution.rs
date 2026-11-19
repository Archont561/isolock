//! Resolve legacy proxy commands through the same layered boundary as profiles.

use super::{absolute, load, Named, Overrides, Resolved};
use figment::providers::Serialized;
use figment::Figment;
use isolock_core::AirlockMode;
use isolock_proxy::{ProxyConfig, ProxyLimits};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Default, Serialize, clap::ValueEnum)]
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

impl From<ProxyMode> for AirlockMode {
    fn from(mode: ProxyMode) -> Self {
        match mode {
            ProxyMode::Connected => Self::Connected,
            ProxyMode::Ci => Self::Ci,
            ProxyMode::LocalAirlock => Self::LocalAirlock,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct ProxySettings {
    pub plan: PathBuf,
    pub cargo_home: PathBuf,
    pub mode: ProxyMode,
    pub port: u16,
    pub limits: ProxyLimits,
}

impl Default for ProxySettings {
    fn default() -> Self {
        Self {
            plan: ".isolock/plan.json".into(),
            cargo_home: ".isolock/cargo-home".into(),
            mode: ProxyMode::LocalAirlock,
            port: 0,
            limits: ProxyLimits::default(),
        }
    }
}

impl ProxySettings {
    pub(super) fn validate(&self) -> Result<(), String> {
        self.limits.validate().map_err(|error| error.to_string())?;
        if self.plan.as_os_str().is_empty() || self.cargo_home.as_os_str().is_empty() {
            return Err("proxy plan and Cargo home paths cannot be empty".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, clap::Args)]
pub struct ProxyOverrides {
    /// Local loopback listener port (zero chooses a free port).
    #[arg(long = "port", global = true)]
    pub port: Option<u16>,
    #[arg(long = "mode", global = true, value_enum, ignore_case = true)]
    pub mode: Option<ProxyMode>,
    /// Maximum concurrent proxy requests (1–256); extra requests receive 503.
    #[arg(long = "max-concurrency", global = true)]
    pub max_concurrency: Option<usize>,
}

pub(super) fn merge_flags(mut figment: Figment, flags: &ProxyOverrides) -> Result<Figment, String> {
    let values = [
        ("proxy.port", flags.port.map(serde_json::Value::from)),
        (
            "proxy.mode",
            flags
                .mode
                .map(serde_json::to_value)
                .transpose()
                .map_err(|_| "cannot serialize proxy mode")?,
        ),
        (
            "proxy.limits.max-concurrency",
            flags.max_concurrency.map(serde_json::Value::from),
        ),
    ];
    for (path, value) in values {
        if let Some(value) = value {
            figment = figment.merge(Named {
                provider: Serialized::default(path, value),
                name: "explicit CLI flag".into(),
            });
        }
    }
    Ok(figment)
}

/// Fully resolved execution inputs; no lower-level collaborator reads process configuration.
pub struct ExecutionConfig {
    pub plan: PathBuf,
    pub require_plan: bool,
    pub cas_dir: PathBuf,
    pub cargo_home: PathBuf,
    pub pixi_config: Option<PathBuf>,
    pub temporary_dir: PathBuf,
    pub inherited_no_proxy: String,
    pub proxy: ProxyConfig,
}

impl ExecutionConfig {
    pub fn load(overrides: &Overrides) -> Result<Self, String> {
        let root = std::env::current_dir().map_err(|_| "cannot resolve project directory")?;
        let resolved = load(&root, overrides)?;
        Ok(Self::from_resolved(&root, &resolved))
    }

    fn from_resolved(root: &Path, resolved: &Resolved) -> Self {
        let proxy = &resolved.settings.proxy;
        let require_plan = resolved
            .figment
            .find_metadata("proxy.plan")
            .is_some_and(|metadata| metadata.name != "compiled defaults");
        Self {
            plan: absolute(root, &proxy.plan),
            require_plan,
            cas_dir: absolute(root, &resolved.settings.storage.cas_dir),
            cargo_home: absolute(root, &proxy.cargo_home),
            pixi_config: std::env::var_os("PIXI_CONFIG_FILE")
                .map(|path| absolute(root, Path::new(&path))),
            temporary_dir: std::env::temp_dir(),
            inherited_no_proxy: std::env::var("NO_PROXY")
                .or_else(|_| std::env::var("no_proxy"))
                .unwrap_or_default(),
            proxy: ProxyConfig::new("127.0.0.1", proxy.port, proxy.mode.into())
                .with_limits(proxy.limits),
        }
    }
}
