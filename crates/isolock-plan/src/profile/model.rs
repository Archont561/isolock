//! Versioned, environment-independent profile planning contracts.

use isolock_cas::Sha256Digest;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Component, Path};

pub const PROFILE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Provider {
    Pixi,
    #[default]
    Standalone,
    Composed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArchiveFormat {
    #[default]
    Raw,
    Tar,
    TarGz,
}

/// A reviewed, content-pinned seed executable or relocatable runtime prefix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct ToolArtifact {
    pub version: String,
    pub platform: String,
    pub digest: Sha256Digest,
    pub filename: String,
    pub source: String,
    #[serde(default)]
    pub format: ArchiveFormat,
    pub entry: Option<String>,
}

/// A reviewed pin for one Cargo git dependency.
///
/// `Cargo.lock` records a commit but never a content hash for a git package, so a
/// plan cannot derive an artifact digest from the lockfile alone. The reviewer
/// supplies the digest that `git archive <commit>` produces; because that output
/// is reproducible, the pin is verifiable independently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct CargoGitPin {
    /// The crate name as it appears in `Cargo.lock`.
    pub name: String,
    /// The crate version as it appears in `Cargo.lock`.
    pub version: String,
    /// Credential-free repository URL, which must equal the lockfile's git origin.
    pub repository: String,
    /// The full lowercase commit the lockfile resolved.
    pub commit: String,
    /// SHA-256 of `git archive` output for `commit`.
    pub digest: Sha256Digest,
}

impl CargoGitPin {
    /// The archive filename this pin is ingested and materialised under.
    #[must_use]
    pub fn filename(&self) -> String {
        format!("{}-git.tar", self.name)
    }
}

/// Every input affecting Cargo vendor derivation, independently of runtime provider.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct CargoPolicy {
    pub cargo_version: String,
    pub target: String,
    pub features: Vec<String>,
    /// Version one deliberately vendors the entire explicitly selected lockfile.
    pub all_features: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct PlatformTools {
    pub seeds: BTreeMap<String, ToolArtifact>,
    pub runtimes: BTreeMap<String, ToolArtifact>,
    pub cargo: Option<CargoPolicy>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Profile {
    pub provider: Provider,
    pub platforms: Vec<String>,
    pub environment: Option<String>,
    pub pixi_lock: Option<String>,
    pub pixi_manifest: Option<String>,
    pub cargo_lock: Option<String>,
    pub pypi_lock: Option<String>,
    pub javascript_lock: Option<String>,
    pub catalog: Option<String>,
    pub compose: Vec<String>,
    pub seeds: BTreeMap<String, ToolArtifact>,
    pub runtimes: BTreeMap<String, ToolArtifact>,
    pub cargo: CargoPolicy,
    /// Reviewed pins for git dependencies. A lockfile git package without one fails closed.
    pub cargo_git: Vec<CargoGitPin>,
    pub platform_tools: BTreeMap<String, PlatformTools>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct StorageIdentity {
    pub kind: String,
    pub repository: Option<String>,
    pub reference: Option<String>,
    pub commit: Option<String>,
}

impl Default for StorageIdentity {
    fn default() -> Self {
        Self {
            kind: "directory".into(),
            repository: None,
            reference: None,
            commit: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactKind {
    Seed,
    Runtime,
    Conda,
    Cargo,
    Wheel,
    Javascript,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Artifact {
    pub kind: ArtifactKind,
    pub name: String,
    pub version: String,
    pub platform: String,
    pub digest: Sha256Digest,
    pub filename: String,
    pub source: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
    pub integrity: Option<String>,
    #[serde(default)]
    pub destinations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct CatalogEntry {
    pub name: String,
    pub version: String,
    pub platform: String,
    pub digest: Sha256Digest,
    pub filename: String,
    pub source: String,
    pub integrity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct CargoDerivation {
    pub schema_version: u32,
    pub lock_digest: Sha256Digest,
    pub platform: String,
    pub policy: CargoPolicy,
    /// Reviewed git pins, so a derivation never covers two different commits.
    #[serde(default)]
    pub git: Vec<CargoGitPin>,
}

/// Immutable manifest. Operational paths, ports and concurrency are intentionally absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct ProfileManifest {
    pub schema_version: u32,
    pub profile: String,
    pub platform: String,
    pub configuration_digest: Sha256Digest,
    pub specification: Profile,
    pub storage: StorageIdentity,
    pub inputs: BTreeMap<String, Sha256Digest>,
    pub artifacts: Vec<Artifact>,
    pub cargo_payloads: BTreeMap<String, CargoDerivation>,
    pub constituents: BTreeMap<String, Sha256Digest>,
    pub materializers: BTreeMap<String, u32>,
}

impl ProfileManifest {
    /// Compute this manifest’s canonical content identity.
    ///
    /// # Errors
    /// Returns an error if the manifest cannot be serialized to canonical JSON.
    pub fn digest(&self) -> Result<Sha256Digest, String> {
        canonical_digest(self)
    }

    /// All source blobs that must be retained/fetched, including original lock inputs.
    pub fn reachable_blobs(&self) -> std::collections::BTreeSet<String> {
        self.inputs
            .values()
            .chain(self.artifacts.iter().map(|artifact| &artifact.digest))
            .map(|digest| digest.as_str().to_owned())
            .collect()
    }

    /// Validate schema, specification, immutable storage, paths, and recorded digests.
    ///
    /// # Errors
    /// Returns an error for unsupported versions/providers, unsafe provenance/paths, invalid
    /// derivation identities, or a configuration digest mismatch.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != PROFILE_SCHEMA_VERSION {
            return Err("unsupported profile manifest schema version".into());
        }
        self.specification.validate(&self.profile, &self.platform)?;
        if !matches!(self.storage.kind.as_str(), "directory" | "git") {
            return Err("unsupported storage provider".into());
        }
        if let Some(repository) = &self.storage.repository {
            public_source(repository)?;
        }
        if self.storage.kind == "git"
            && (self.storage.repository.is_none()
                || self.storage.reference.is_none()
                || self.storage.commit.as_ref().is_none_or(|commit| {
                    !matches!(commit.len(), 40 | 64)
                        || !commit.bytes().all(|byte| byte.is_ascii_hexdigit())
                }))
        {
            return Err("Git profile plans require an immutable storage commit".into());
        }
        for path in self.inputs.keys() {
            relative_path(path)?;
        }
        for artifact in &self.artifacts {
            filename(&artifact.filename)?;
            public_source(&artifact.source)?;
            for destination in &artifact.destinations {
                relative_path(destination)?;
                if !destination.starts_with("node_modules/") {
                    return Err("JavaScript installation must be inside node_modules".into());
                }
            }
        }
        for (key, derivation) in &self.cargo_payloads {
            if canonical_digest(derivation)?.as_str() != key {
                return Err("Cargo payload derivation digest mismatch".into());
            }
        }
        if self.materializers.values().any(|version| *version != 1) {
            return Err("unsupported materializer schema version".into());
        }
        if configuration_digest(self)? != self.configuration_digest {
            return Err("profile configuration digest mismatch".into());
        }
        Ok(())
    }
}

/// A completely resolved request. The planner never inspects process configuration.
pub struct PlanRequest<'a> {
    pub root: &'a Path,
    pub profile: &'a str,
    pub platform: &'a str,
    pub profiles: &'a BTreeMap<String, Profile>,
    pub storage: StorageIdentity,
}

/// Hash canonical, sorted JSON without depending on struct/map declaration order.
///
/// # Errors
/// Returns an error if the supplied value cannot be serialized as JSON.
pub fn canonical_digest(value: &impl Serialize) -> Result<Sha256Digest, String> {
    // serde_json's default map representation is sorted; normalize struct/map order alike.
    let value = serde_json::to_value(value).map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec(&value).map_err(|error| error.to_string())?;
    Ok(Sha256Digest::from_bytes(&bytes))
}

pub(super) fn configuration_digest(manifest: &ProfileManifest) -> Result<Sha256Digest, String> {
    canonical_digest(&(
        manifest.schema_version,
        &manifest.profile,
        &manifest.platform,
        &manifest.specification,
        &manifest.storage,
        &manifest.inputs,
        &manifest.artifacts,
        &manifest.cargo_payloads,
        &manifest.constituents,
        &manifest.materializers,
    ))
}

/// Validate a nonempty ASCII profile, platform, environment, or tool identifier.
///
/// # Errors
/// Returns an error for empty/dot identifiers or bytes outside letters, digits, `._-`.
pub fn identifier(value: &str) -> Result<(), String> {
    if value.is_empty()
        || matches!(value, "." | "..")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return Err("invalid profile, platform, environment or tool identifier".into());
    }
    Ok(())
}

/// Validate a portable single-component artifact basename.
///
/// # Errors
/// Returns an error for empty/dot names or bytes outside letters, digits, `._+-`.
pub fn filename(value: &str) -> Result<(), String> {
    if value.is_empty()
        || matches!(value, "." | "..")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
    {
        return Err("invalid artifact filename".into());
    }
    Ok(())
}

/// Validate a nonempty project-relative path without traversal.
///
/// # Errors
/// Returns an error for absolute paths, non-normal components, backslashes or control delimiters.
pub fn relative_path(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.contains(['\0', '\n', '\r', '\\'])
        || !Path::new(value)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err("expected a nonempty project-relative path without traversal".into());
    }
    Ok(())
}

/// Never persist credentials or signed query parameters as artifact provenance.
///
/// # Errors
/// Returns an error for empty sources, credentials in the authority, query/fragment
/// parameters, or NUL/newline/carriage-return delimiters.
pub fn public_source(value: &str) -> Result<(), String> {
    let authority = value
        .split("://")
        .nth(1)
        .unwrap_or(value)
        .split('/')
        .next()
        .unwrap_or_default();
    if value.is_empty() || value.contains(['\0', '\n', '\r', '?', '#']) || authority.contains('@') {
        return Err(
            "artifact/storage sources must not contain credentials or query parameters".into(),
        );
    }
    Ok(())
}

impl Profile {
    /// Resolve only the requested platform policy before normalizing/locking intent.
    ///
    /// # Errors
    /// Returns an error for unsupported selected platforms or undeclared/invalid platform-tools keys.
    pub fn resolve(&self, platform: &str) -> Result<Self, String> {
        if !self.platforms.iter().any(|value| value == platform) {
            return Err("profile does not support the selected platform".into());
        }
        for key in self.platform_tools.keys() {
            identifier(key)?;
            if !self.platforms.contains(key) {
                return Err("platform-tools contains an undeclared platform".into());
            }
        }
        let mut profile = self.clone();
        if let Some(tools) = self.platform_tools.get(platform) {
            profile.seeds.extend(tools.seeds.clone());
            profile.runtimes.extend(tools.runtimes.clone());
            if let Some(cargo) = &tools.cargo {
                profile.cargo = cargo.clone();
            }
        }
        profile.platform_tools.clear();
        profile.platforms = vec![platform.into()];
        profile.compose.sort();
        profile.compose.dedup();
        profile.cargo.features.sort();
        profile.cargo.features.dedup();
        Ok(profile)
    }

    /// Validate resolved profile policy and pinned seed/runtime declarations.
    ///
    /// # Errors
    /// Returns an error for unsafe identifiers/paths, incompatible platforms/tools, invalid
    /// provider composition, missing seeds/lock policy, or credential-bearing provenance.
    pub fn validate(&self, name: &str, platform: &str) -> Result<(), String> {
        if !self.platform_tools.is_empty() {
            return self.resolve(platform)?.validate(name, platform);
        }
        identifier(name)?;
        identifier(platform)?;
        if !self.platforms.iter().any(|candidate| candidate == platform) {
            return Err(format!(
                "profile {name} does not support platform {platform}"
            ));
        }
        for path in [
            &self.pixi_lock,
            &self.pixi_manifest,
            &self.cargo_lock,
            &self.pypi_lock,
            &self.javascript_lock,
            &self.catalog,
        ]
        .into_iter()
        .flatten()
        {
            relative_path(path)?;
        }
        for child in &self.compose {
            identifier(child)?;
        }
        self.validate_provider()?;
        if let Some(environment) = &self.environment {
            identifier(environment)?;
        }
        if self.cargo_lock.is_some()
            && (self.cargo.cargo_version.is_empty() || self.cargo.target.is_empty())
        {
            return Err(
                "Cargo profiles require explicit cargo-version and target derivation policy".into(),
            );
        }
        self.validate_tools(platform)
    }

    fn validate_provider(&self) -> Result<(), String> {
        if self.provider == Provider::Standalone
            && self.runtimes.contains_key("rust")
            && self.runtimes["rust"].format == ArchiveFormat::Raw
        {
            return Err(
                "Rust requires a relocatable prefix containing both bin/cargo and bin/rustc".into(),
            );
        }
        if self.provider == Provider::Composed {
            if self.compose.is_empty()
                || !self.seeds.is_empty()
                || !self.runtimes.is_empty()
                || self.pixi_lock.is_some()
                || self.cargo_lock.is_some()
                || self.pypi_lock.is_some()
                || self.javascript_lock.is_some()
            {
                return Err(
                    "composed profiles must only declare constituent profiles and platforms".into(),
                );
            }
        } else if !self.compose.is_empty() || !self.seeds.contains_key("isolock") {
            return Err(
                "leaf profiles require a verified isolock seed and cannot compose profiles".into(),
            );
        }
        if self.provider == Provider::Pixi {
            if self.environment.is_none()
                || self.pixi_lock.is_none()
                || self.pixi_manifest.is_none()
                || !self.seeds.contains_key("pixi")
                || !self.runtimes.is_empty()
            {
                return Err("Pixi profiles require environment, pixi-lock, pixi-manifest and seed Pixi; runtimes come from Conda".into());
            }
        } else if self.pixi_lock.is_some() || self.environment.is_some() {
            return Err("only Pixi profiles may select a Pixi environment".into());
        }
        Ok(())
    }

    fn validate_tools(&self, platform: &str) -> Result<(), String> {
        for (name, tool) in self.seeds.iter().chain(&self.runtimes) {
            identifier(name)?;
            filename(&tool.filename)?;
            public_source(&tool.source)?;
            if tool.platform != platform
                || tool.version.is_empty()
                || !tool
                    .version
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
            {
                return Err(format!(
                    "tool {name} has an incompatible platform or missing version"
                ));
            }
            if tool.format == ArchiveFormat::Raw && tool.entry.is_some() {
                return Err("raw tools cannot select an archive entry".into());
            }
            if let Some(entry) = &tool.entry {
                relative_path(entry)?;
            }
            if tool.format != ArchiveFormat::Raw
                && tool.entry.is_none()
                && self.seeds.contains_key(name)
            {
                return Err(format!(
                    "archived seed {name} requires an executable entry path"
                ));
            }
        }
        if self
            .seeds
            .keys()
            .any(|name| !matches!(name.as_str(), "isolock" | "pixi"))
            || self
                .runtimes
                .keys()
                .any(|name| !matches!(name.as_str(), "rust" | "python" | "node" | "bun"))
        {
            return Err("unsupported seed or runtime kind".into());
        }
        Ok(())
    }
}
