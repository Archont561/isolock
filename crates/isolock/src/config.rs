//! Configuration is loaded only at the CLI boundary. Values leave this module typed.

use figment::providers::{Env, Format, Serialized, Toml};
use figment::value::{Dict, Map};
use figment::{Figment, Metadata, Provider};
use isolock_plan::profile::{identifier, public_source, Profile, PROFILE_SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Settings {
    pub schema_version: u32,
    pub storage: Storage,
    pub profiles: BTreeMap<String, Profile>,
    pub gc: GarbageCollection,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: PROFILE_SCHEMA_VERSION,
            storage: Storage::default(),
            profiles: BTreeMap::new(),
            gc: GarbageCollection::default(),
        }
    }
}

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

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Transport {
    #[default]
    Directory,
    Git,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct GarbageCollection {
    pub roots: Vec<String>,
}

#[derive(Debug, Clone, Default, clap::Args)]
pub struct Overrides {
    /// Project configuration (relative to the working directory).
    #[arg(long, global = true, default_value = "isolock.toml")]
    pub config: PathBuf,
    #[arg(long, global = true)]
    pub user_config: Option<PathBuf>,
    #[arg(long, global = true)]
    pub profile: Option<String>,
    #[arg(long, global = true)]
    pub platform: Option<String>,
    #[arg(long, global = true)]
    pub cas_dir: Option<PathBuf>,
    #[arg(long, global = true)]
    pub pixi_environment: Option<String>,
    #[arg(long, global = true)]
    pub pixi_version: Option<String>,
    #[arg(long, global = true)]
    pub storage_ref: Option<String>,
}

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

pub struct Resolved {
    pub settings: Settings,
    pub figment: Figment,
}

pub fn load(root: &Path, overrides: &Overrides) -> Result<Resolved, String> {
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
    let config_path = if overrides.config.as_os_str().is_empty() {
        root.join("isolock.toml")
    } else {
        root.join(&overrides.config)
    };
    if config_path.exists() {
        figment = figment.merge(Named {
            provider: Toml::file(&config_path),
            name: "project configuration".into(),
        });
    } else if overrides.config != Path::new("isolock.toml")
        && !overrides.config.as_os_str().is_empty()
    {
        return Err("explicit project configuration file does not exist".into());
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
        "invalid configuration: unknown fields, invalid TOML or values outside the typed schema"
            .to_owned()
    })?;
    validate(&settings)?;
    Ok(Resolved { settings, figment })
}

fn validate_user_configuration(path: &Path) -> Result<(), String> {
    let value: serde_json::Value = Figment::from(Toml::file(path))
        .extract()
        .map_err(|_| "invalid user operational configuration".to_owned())?;
    let object = value
        .as_object()
        .ok_or("user operational configuration must be a table")?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "storage" | "gc"))
        || object
            .get("storage")
            .and_then(serde_json::Value::as_object)
            .is_some_and(|storage| storage.keys().any(|key| key != "cas-dir"))
    {
        return Err("user configuration may only set operational CAS paths and GC roots".into());
    }
    Ok(())
}

fn merge_environment(mut figment: Figment, profile: Option<&str>) -> Result<Figment, String> {
    let variables = std::env::vars()
        .map(|(key, _)| key)
        .collect::<std::collections::BTreeSet<_>>();
    let mut mapped = BTreeMap::new();
    for variable in variables {
        let path = match variable.as_str() {
            "ISOLOCK_CAS_DIR" => Some("storage.cas-dir".into()),
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
                return Err(
                    "multiple environment variables override the same configuration key".into(),
                );
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

fn profile_key(profile: Option<&str>, key: &str) -> Result<String, String> {
    let profile = profile.ok_or("profile-specific overrides require --profile")?;
    identifier(profile)?;
    Ok(format!("profiles.{profile}.{key}"))
}

fn merge_flags(mut figment: Figment, flags: &Overrides) -> Result<Figment, String> {
    let mut values: Vec<(String, String)> = Vec::new();
    if let Some(value) = &flags.cas_dir {
        values.push((
            "storage.cas-dir".into(),
            value.to_str().ok_or("CAS path is not UTF-8")?.into(),
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
    Ok(figment)
}

fn validate(settings: &Settings) -> Result<(), String> {
    if settings.schema_version != PROFILE_SCHEMA_VERSION {
        return Err("unsupported configuration schema-version".into());
    }
    if settings.storage.cas_dir.as_os_str().is_empty() {
        return Err("storage.cas-dir cannot be empty".into());
    }
    if let Some(source) = &settings.storage.source {
        public_source(source)?;
    }
    if settings.storage.reference.is_empty()
        || settings.storage.reference.starts_with('-')
        || settings.storage.reference.contains(['\0', '\n', '\r'])
    {
        return Err("invalid storage reference".into());
    }
    for root in &settings.gc.roots {
        identifier(root)?;
    }
    for (name, profile) in &settings.profiles {
        if profile.platforms.is_empty() {
            return Err(format!("profile {name} must declare platforms"));
        }
        for platform in &profile.platforms {
            profile.validate(name, platform)?;
        }
        for child in &profile.compose {
            if !settings.profiles.contains_key(child) {
                return Err(format!("profile {name} references an unknown constituent"));
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

fn validate_composition(
    name: &str,
    profiles: &BTreeMap<String, Profile>,
    active: &mut std::collections::BTreeSet<String>,
    complete: &mut std::collections::BTreeSet<String>,
) -> Result<(), String> {
    if complete.contains(name) {
        return Ok(());
    }
    if !active.insert(name.into()) {
        return Err("profile composition cycle".into());
    }
    for child in &profiles
        .get(name)
        .ok_or("unknown constituent profile")?
        .compose
    {
        validate_composition(child, profiles, active, complete)?;
    }
    active.remove(name);
    complete.insert(name.into());
    Ok(())
}

pub fn explain(resolved: &Resolved, path: &str) -> Result<String, String> {
    let value: serde_json::Value = resolved
        .figment
        .extract_inner(path)
        .map_err(|_| "unknown configuration path".to_owned())?;
    let metadata = resolved
        .figment
        .find_metadata(path)
        .ok_or("configuration value has no provenance")?;
    // Only validated, credential-free typed settings can reach rendering.
    Ok(format!(
        "{path} = {}\nsource: {}",
        serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?,
        metadata.name
    ))
}

pub fn absolute(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.into()
    } else {
        root.join(path)
    }
}
