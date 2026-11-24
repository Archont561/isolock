//! Layered configuration loading, validation, and provenance: the only Figment boundary.
//!
//! Values leave this module typed as [`crate::domain::Settings`]; the provenance payload stays
//! adapter-internal so no use case can depend on Figment.

use crate::application::ports::{
    ConfigProvenance, ConfigurationProvider, LayeredOverrides, LoadedConfiguration,
    SessionSettings, UseCaseResult,
};
use crate::domain::configuration::SCHEMA_VERSION;
use crate::domain::{DomainError, ProxyMode, Settings};
use figment::providers::{Env, Format, Serialized, Toml};
use figment::value::{Dict, Map};
use figment::{Figment, Metadata, Provider};
use isolock_plan::profile::{identifier, public_source};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

struct Named<P> {
    provider: P,
    name: String,
}
impl<P: Provider> Provider for Named<P> {
    fn metadata(&self) -> Metadata {
        let mut metadata = self.provider.metadata();
        metadata.name = self.name.clone().into();
        metadata
    }
    fn data(&self) -> Result<Map<figment::Profile, Dict>, figment::Error> {
        self.provider.data()
    }
}

/// Source-aware rendering of one resolved configuration.
#[derive(Debug)]
pub struct Provenance {
    figment: Figment,
}

impl ConfigProvenance for Provenance {
    fn explain(&self, path: &str) -> UseCaseResult<String> {
        let value: serde_json::Value = self
            .figment
            .extract_inner(path)
            .map_err(|_| DomainError::new("unknown configuration path"))?;
        let metadata = self
            .figment
            .find_metadata(path)
            .ok_or_else(|| DomainError::new("configuration value has no provenance"))?;
        // Only validated, credential-free typed settings can reach rendering.
        Ok(format!(
            "{path} = {}\nsource: {}",
            serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?,
            metadata.name
        ))
    }

    fn render(&self) -> UseCaseResult<String> {
        self.figment
            .extract::<Settings>()
            .map_err(|error| DomainError::new(error.to_string()))
            .and_then(|settings| render_settings(&settings))
    }
}

/// Serializes the effective settings exactly as the review view prints them.
///
/// # Errors
/// Returns a [`DomainError`] when the settings cannot be serialized.
pub fn render_settings(settings: &Settings) -> UseCaseResult<String> {
    serde_json::to_string_pretty(settings).map_err(|error| DomainError::new(error.to_string()))
}

/// Reads layered configuration from the filesystem and environment for one project root.
#[derive(Debug, Clone, Default)]
pub struct FilesystemConfiguration {
    /// The overrides the CLI adapter extracted.
    pub overrides: LayeredOverrides,
}

impl FilesystemConfiguration {
    /// Bundles the override set the adapter extracted.
    #[must_use]
    pub fn new(overrides: LayeredOverrides) -> Self {
        Self { overrides }
    }
}

impl ConfigurationProvider for FilesystemConfiguration {
    type Provenance = Provenance;

    fn load(&self, root: &Path) -> UseCaseResult<LoadedConfiguration<Provenance>> {
        load(root, &self.overrides)
    }
}

/// Loads every layer for `root`, returning typed settings and provenance.
///
/// # Errors
/// Returns a [`DomainError`] when a layer is unreadable, an explicit project file is missing,
/// or the merged values violate the reviewed schema.
pub fn load(
    root: &Path,
    overrides: &LayeredOverrides,
) -> UseCaseResult<LoadedConfiguration<Provenance>> {
    let mut figment = Figment::from(Named {
        provider: Serialized::defaults(Settings::default()),
        name: "compiled defaults".into(),
    });
    let user_path = overrides
        .user_config
        .clone()
        .or_else(|| std::env::var_os("ISOLOCK_USER_CONFIG").map(PathBuf::from))
        .or_else(|| {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(|path| PathBuf::from(path).join("isolock/config.toml"))
        })
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|path| PathBuf::from(path).join(".config/isolock/config.toml"))
        });
    if let Some(path) = user_path {
        if path.exists() {
            validate_user_configuration(&path)?;
            figment = figment.merge(Named {
                provider: Toml::file(path),
                name: "user operational configuration".into(),
            });
        }
    }
    let config_path = if overrides.project_config.as_os_str().is_empty() {
        root.join("isolock.toml")
    } else {
        root.join(&overrides.project_config)
    };
    if config_path.exists() {
        figment = figment.merge(Named {
            provider: Toml::file(&config_path),
            name: "project configuration".into(),
        });
    } else if overrides.project_config != Path::new("isolock.toml")
        && !overrides.project_config.as_os_str().is_empty()
    {
        return Err(DomainError::new(
            "explicit project configuration file does not exist",
        ));
    }
    let local = root.join(".isolock/config.local.toml");
    if local.exists() {
        figment = figment.merge(Named {
            provider: Toml::file(local),
            name: "generated project-local configuration".into(),
        });
    }
    figment = merge_environment(figment, overrides.profile.as_deref())?;
    figment = merge_flags(figment, overrides)?;
    // Figment errors can include raw invalid values. Do not print those into CI diagnostics.
    let settings: Settings = figment.extract().map_err(|_| {
        DomainError::new("invalid configuration: unknown fields, invalid TOML or values outside the typed schema")
    })?;
    validate(&settings)?;
    Ok(LoadedConfiguration {
        settings,
        provenance: Provenance { figment },
    })
}

fn validate_user_configuration(path: &Path) -> UseCaseResult<()> {
    let value: serde_json::Value = Figment::from(Toml::file(path))
        .extract()
        .map_err(|_| DomainError::new("invalid user operational configuration"))?;
    let object = value
        .as_object()
        .ok_or_else(|| DomainError::new("user operational configuration must be a table"))?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "storage" | "gc" | "proxy"))
        || object
            .get("storage")
            .and_then(serde_json::Value::as_object)
            .is_some_and(|storage| storage.keys().any(|key| key != "cas-dir"))
        || object
            .get("proxy")
            .and_then(serde_json::Value::as_object)
            .is_some_and(|proxy| {
                proxy
                    .keys()
                    .any(|key| !matches!(key.as_str(), "port" | "limits" | "cargo-home"))
            })
    {
        return Err(DomainError::new(
            "user configuration may only set operational CAS paths, GC roots, and proxy paths/limits/port",
        ));
    }
    Ok(())
}

fn merge_environment(mut figment: Figment, profile: Option<&str>) -> UseCaseResult<Figment> {
    let variables = std::env::vars_os()
        .filter_map(|(key, _)| key.into_string().ok())
        .collect::<std::collections::BTreeSet<_>>();
    let mut mapped = BTreeMap::new();
    for variable in variables {
        let path = match variable.as_str() {
            "ISOLOCK_CAS_DIR" => Some("storage.cas-dir".into()),
            "ISOLOCK_PLAN" => Some("proxy.plan".into()),
            "ISOLOCK_MODE" => Some("proxy.mode".into()),
            "ISOLOCK_PROXY_PORT" => Some("proxy.port".into()),
            "ISOLOCK_CARGO_HOME" => Some("proxy.cargo-home".into()),
            "ISOLOCK_PROXY_MAX_CONCURRENCY" => Some("proxy.limits.max-concurrency".into()),
            "ISOLOCK_PROXY_HEADER_TIMEOUT_MS" => Some("proxy.limits.header-timeout-ms".into()),
            "ISOLOCK_PROXY_WRITE_TIMEOUT_MS" => Some("proxy.limits.write-timeout-ms".into()),
            "ISOLOCK_PROXY_SHUTDOWN_GRACE_MS" => Some("proxy.limits.shutdown-grace-ms".into()),
            "ISOLOCK_STORAGE_REF" => Some("storage.reference".into()),
            "ISOLOCK_PIXI_ENVIRONMENT" => Some(profile_key(profile, "environment")?),
            "ISOLOCK_PIXI_VERSION" => Some(profile_key(profile, "seeds.pixi.version")?),
            _ => variable.strip_prefix("ISOLOCK_CONFIG_").map(|key| {
                key.to_ascii_lowercase()
                    .replace("__", ".")
                    .replace('_', "-")
            }),
        };
        if let Some(path) = path {
            if mapped.insert(path.clone(), variable.clone()).is_some() {
                return Err(DomainError::new(
                    "multiple environment variables override the same configuration key",
                ));
            }
            let target = path.clone();
            let provider = Env::raw()
                .only(&[&variable])
                .map(move |_| target.clone().into());
            figment = figment.merge(Named {
                provider,
                name: variable,
            });
        }
    }
    Ok(figment)
}

fn profile_key(profile: Option<&str>, key: &str) -> UseCaseResult<String> {
    let profile =
        profile.ok_or_else(|| DomainError::new("profile-specific overrides require --profile"))?;
    identifier(profile)?;
    Ok(format!("profiles.{profile}.{key}"))
}

fn merge_flags(mut figment: Figment, flags: &LayeredOverrides) -> UseCaseResult<Figment> {
    let mut values: Vec<(String, String)> = Vec::new();
    if let Some(value) = &flags.cas_dir {
        values.push((
            "storage.cas-dir".into(),
            value
                .to_str()
                .ok_or_else(|| DomainError::new("CAS path is not UTF-8"))?
                .into(),
        ));
    }
    if let Some(value) = &flags.pixi_environment {
        values.push((
            profile_key(flags.profile.as_deref(), "environment")?,
            value.clone(),
        ));
    }
    if let Some(value) = &flags.pixi_version {
        values.push((
            profile_key(flags.profile.as_deref(), "seeds.pixi.version")?,
            value.clone(),
        ));
    }
    if let Some(value) = &flags.storage_ref {
        values.push(("storage.reference".into(), value.clone()));
    }
    for (path, value) in values {
        figment = figment.merge(Named {
            provider: Serialized::default(&path, value),
            name: "explicit CLI flag".into(),
        });
    }
    merge_proxy_flags(figment, flags)
}

fn merge_proxy_flags(mut figment: Figment, flags: &LayeredOverrides) -> UseCaseResult<Figment> {
    let values = [
        (
            "proxy.port",
            flags.proxy_port.map(serde_json::Value::from),
            "port",
        ),
        (
            "proxy.mode",
            flags
                .proxy_mode
                .map(serde_json::to_value)
                .transpose()
                .map_err(|_| DomainError::new("cannot serialize proxy mode"))?,
            "mode",
        ),
        (
            "proxy.limits.max-concurrency",
            flags.proxy_max_concurrency.map(serde_json::Value::from),
            "max-concurrency",
        ),
    ];
    for (path, value, _) in values {
        if let Some(value) = value {
            figment = figment.merge(Named {
                provider: Serialized::default(path, value),
                name: "explicit CLI flag".into(),
            });
        }
    }
    Ok(figment)
}

fn validate(settings: &Settings) -> UseCaseResult<()> {
    if settings.schema_version != SCHEMA_VERSION {
        return Err(DomainError::new("unsupported configuration schema-version"));
    }
    if settings.storage.cas_dir.as_os_str().is_empty() {
        return Err(DomainError::new("storage.cas-dir cannot be empty"));
    }
    if let Some(source) = &settings.storage.source {
        public_source(source)?;
    }
    if settings.storage.reference.is_empty()
        || settings.storage.reference.starts_with('-')
        || settings.storage.reference.contains(['\0', '\n', '\r'])
    {
        return Err(DomainError::new("invalid storage reference"));
    }
    validate_proxy(&settings.proxy)?;
    for root in &settings.gc.roots {
        identifier(root)?;
    }
    for (name, profile) in &settings.profiles {
        if profile.platforms.is_empty() {
            return Err(DomainError::new(format!(
                "profile {name} must declare platforms"
            )));
        }
        for platform in &profile.platforms {
            profile.validate(name, platform)?;
        }
        for child in &profile.compose {
            if !settings.profiles.contains_key(child) {
                return Err(DomainError::new(format!(
                    "profile {name} references an unknown constituent"
                )));
            }
        }
    }
    let mut complete = std::collections::BTreeSet::new();
    for name in settings.profiles.keys() {
        validate_composition(
            name,
            &settings.profiles,
            &mut std::collections::BTreeSet::new(),
            &mut complete,
        )?;
    }
    Ok(())
}

fn validate_proxy(proxy: &crate::domain::ProxySettings) -> UseCaseResult<()> {
    proxy
        .limits
        .validate()
        .map_err(|error| DomainError::new(error.to_string()))?;
    if proxy.plan.as_os_str().is_empty() || proxy.cargo_home.as_os_str().is_empty() {
        return Err(DomainError::new(
            "proxy plan and Cargo home paths cannot be empty",
        ));
    }
    Ok(())
}

fn validate_composition(
    name: &str,
    profiles: &BTreeMap<String, isolock_plan::profile::Profile>,
    active: &mut std::collections::BTreeSet<String>,
    complete: &mut std::collections::BTreeSet<String>,
) -> UseCaseResult<()> {
    if complete.contains(name) {
        return Ok(());
    }
    if !active.insert(name.into()) {
        return Err(DomainError::new("profile composition cycle"));
    }
    for child in &profiles
        .get(name)
        .ok_or_else(|| DomainError::new("unknown constituent profile"))?
        .compose
    {
        validate_composition(child, profiles, active, complete)?;
    }
    active.remove(name);
    complete.insert(name.into());
    Ok(())
}

/// Projects validated settings into the absolute paths an execution session needs.
///
/// # Errors
/// Returns a [`DomainError`] when configuration cannot be serialized for provenance lookup.
pub fn session(
    root: &Path,
    loaded: &LoadedConfiguration<Provenance>,
) -> UseCaseResult<SessionSettings> {
    let settings = &loaded.settings;
    let proxy = &settings.proxy;
    let require_plan = loaded
        .provenance
        .figment
        .find_metadata("proxy.plan")
        .is_some_and(|metadata| metadata.name != "compiled defaults");
    Ok(SessionSettings {
        plan: absolute(root, &proxy.plan),
        require_plan,
        cas_dir: absolute(root, &settings.storage.cas_dir),
        cargo_home: absolute(root, &proxy.cargo_home),
        pixi_config: std::env::var_os("PIXI_CONFIG_FILE")
            .map(|path| absolute(root, Path::new(&path))),
        temporary_dir: std::env::temp_dir(),
        inherited_no_proxy: std::env::var("NO_PROXY")
            .or_else(|_| std::env::var("no_proxy"))
            .unwrap_or_default(),
        proxy: proxy_config(proxy),
        profile: None,
    })
}

fn proxy_config(proxy: &crate::domain::ProxySettings) -> isolock_proxy::ProxyConfig {
    let mode = match proxy.mode {
        ProxyMode::Connected => isolock_core::AirlockMode::Connected,
        ProxyMode::Ci => isolock_core::AirlockMode::Ci,
        ProxyMode::LocalAirlock => isolock_core::AirlockMode::LocalAirlock,
    };
    isolock_proxy::ProxyConfig::new("127.0.0.1", proxy.port, mode).with_limits(proxy.limits)
}

/// Resolves `path` against `root` unless it is already absolute.
#[must_use]
pub fn absolute(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.into()
    } else {
        root.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::ConfigurationProvider;

    fn overrides(config: &str) -> LayeredOverrides {
        LayeredOverrides {
            project_config: PathBuf::from(config),
            ..LayeredOverrides::new()
        }
    }

    #[test]
    fn compiled_defaults_are_always_the_first_layer() {
        let root = tempfile::tempdir().expect("temp dir");
        let loaded = FilesystemConfiguration::new(LayeredOverrides::new())
            .load(root.path())
            .expect("defaults load");
        assert_eq!(loaded.settings.storage.reference, "isolock/v1");
        assert!(loaded.provenance.explain("storage.reference").is_ok());
    }

    #[test]
    fn an_explicit_missing_project_file_is_reported_rather_than_ignored() {
        let root = tempfile::tempdir().expect("temp dir");
        let error = FilesystemConfiguration::new(overrides("absent.toml"))
            .load(root.path())
            .expect_err("missing explicit config must fail");
        assert_eq!(
            error.to_string(),
            "explicit project configuration file does not exist"
        );
    }

    #[test]
    fn unknown_keys_and_bad_values_never_leak_raw_input_into_diagnostics() {
        let root = tempfile::tempdir().expect("temp dir");
        std::fs::write(root.path().join("isolock.toml"), "secret-token='hunter2'")
            .expect("write config");
        let error = FilesystemConfiguration::new(LayeredOverrides::new())
            .load(root.path())
            .expect_err("unknown key must fail");
        assert!(!error.to_string().contains("hunter2"));
        assert!(error.to_string().contains("invalid configuration"));
    }

    #[test]
    fn absolute_paths_survive_and_relative_ones_join_the_root() {
        let root = Path::new("/workspace");
        assert_eq!(
            absolute(root, Path::new("/etc/isolock")),
            PathBuf::from("/etc/isolock")
        );
        assert_eq!(
            absolute(root, Path::new(".isolock/cas")),
            PathBuf::from("/workspace/.isolock/cas")
        );
    }
}
