//! The reviewed, versioned input contract. No registry resolution or network access.

use isolock_cas::Sha256Digest;
use isolock_core::Registry;
use isolock_plan::profile::{canonical_digest, filename, public_source};
use isolock_plan::PlannedArtifactMetadata;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub(super) const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Manifest {
    pub schema_version: u32,
    pub artifacts: Vec<Artifact>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Artifact {
    pub registry: String,
    pub name: String,
    pub version: String,
    pub sha256: Sha256Digest,
    pub filename: String,
    pub source: String,
    #[serde(default)]
    pub metadata: PlannedArtifactMetadata,
    /// For a Cargo git dependency, the reviewed commit pin.
    ///
    /// Absent for every registry artifact, and serialised only when present so
    /// existing manifests keep their exact bytes and digest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<GitPin>,
}

/// The reviewed pin for one Cargo git dependency.
///
/// `revision` is what the reviewer wrote and is re-resolved on every ingest;
/// `commit` is the full commit that revision must resolve to. Requiring both
/// means a moving tag cannot silently change which tree is ingested.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct GitPin {
    /// Credential-free `https://` (or `file://` in tests) repository URL.
    pub repository: String,
    /// The reviewed revision, as written: a full or abbreviated commit, tag, or branch.
    pub revision: String,
    /// The full commit `revision` must resolve to.
    pub commit: String,
}

impl Manifest {
    pub fn read(path: &Path) -> Result<Self, String> {
        let mut bytes = Vec::new();
        File::open(path)
            .map_err(|_| "cannot read reviewed ingestion manifest")?
            .take(MAX_MANIFEST_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "cannot read reviewed ingestion manifest")?;
        if bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err("reviewed ingestion manifest exceeds 4 MiB".into());
        }
        let manifest: Self = serde_json::from_slice(&bytes)
            .map_err(|_| "invalid reviewed ingestion manifest schema")?;
        manifest.validate()?;
        // The immutable, pretty-printed audit object must fit the same read limit.
        let encoded = serde_json::to_vec_pretty(&manifest)
            .map_err(|_| "cannot encode reviewed ingestion manifest")?;
        if encoded.len() as u64 + 1 > MAX_MANIFEST_BYTES {
            return Err("encoded reviewed ingestion manifest exceeds 4 MiB".into());
        }
        Ok(manifest)
    }

    pub(super) fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("unsupported ingestion schema-version".into());
        }
        if self.artifacts.is_empty() {
            return Err("reviewed ingestion manifest contains no artifacts".into());
        }
        let mut identities = BTreeSet::new();
        let mut filenames = BTreeSet::new();
        for artifact in &self.artifacts {
            artifact.validate()?;
            if !identities.insert(artifact.key()?.as_str().to_owned())
                || !filenames.insert(&artifact.filename)
            {
                return Err("duplicate artifact identity or filename in reviewed manifest".into());
            }
        }
        Ok(())
    }

    pub fn select(self, registry: Option<&str>, packages: &[String]) -> Result<Self, String> {
        if !packages.is_empty() && registry.is_none() {
            return Err("package selection requires --registry".into());
        }
        let mut selected = BTreeSet::new();
        for package in packages {
            let (name, version) = package
                .split_once("==")
                .ok_or("expected a reviewed package selection NAME==VERSION")?;
            validate_name(registry.unwrap_or_default(), name)?;
            validate_version(version)?;
            selected.insert(package.clone());
        }
        let artifacts = self
            .artifacts
            .into_iter()
            .filter(|artifact| {
                registry.is_none_or(|value| value == artifact.registry)
                    && (selected.is_empty()
                        || selected.contains(&format!("{}=={}", artifact.name, artifact.version)))
            })
            .collect::<Vec<_>>();
        if artifacts.is_empty()
            || selected.iter().any(|package| {
                !artifacts
                    .iter()
                    .any(|artifact| *package == format!("{}=={}", artifact.name, artifact.version))
            })
        {
            return Err(
                "requested artifact is not pinned in the reviewed ingestion manifest".into(),
            );
        }
        Ok(Self {
            schema_version: self.schema_version,
            artifacts,
        })
    }

    pub fn digest(&self) -> Result<Sha256Digest, String> {
        canonical_digest(self)
    }
}

impl Artifact {
    fn validate(&self) -> Result<(), String> {
        if !matches!(
            self.registry.as_str(),
            "cargo" | "conda" | "pypi" | "npm" | "generic"
        ) {
            return Err("unsupported ingestion registry".into());
        }
        validate_name(&self.registry, &self.name)?;
        validate_version(&self.version)?;
        filename(&self.filename)?;
        public_source(&self.source)?;
        if let Some(pin) = &self.git {
            return self.validate_git(pin);
        }
        let authority = self
            .source
            .strip_prefix("https://")
            .and_then(|rest| rest.split('/').next())
            .unwrap_or_default();
        if authority.is_empty()
            || self
                .source
                .bytes()
                .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control() || byte == b'\\')
        {
            return Err("reviewed artifact sources must be public HTTPS URLs".into());
        }
        if self
            .metadata
            .filename
            .as_ref()
            .is_some_and(|name| name != &self.filename)
        {
            return Err("reviewed filename conflicts with registry metadata".into());
        }
        Ok(())
    }

    /// Validates a Cargo git dependency against its reviewed pin.
    ///
    /// A git artifact is fetched by `git`, not downloaded by URL, so the
    /// public-HTTPS download rules do not apply. What must hold instead is that
    /// the pin is a full lowercase commit, that the filename is derived from the
    /// crate name so a reviewer can predict it, and that `source` names the same
    /// repository the pin does.
    fn validate_git(&self, pin: &GitPin) -> Result<(), String> {
        if self.registry != "cargo" {
            return Err("reviewed git pins are only supported for Cargo artifacts".into());
        }
        super::git_dependency::validate(&pin.repository, &pin.revision)?;
        full_commit(&pin.commit)?;
        if self.filename != format!("{}-git.tar", self.name) {
            return Err("reviewed Cargo git dependency filename must be its crate name".into());
        }
        if self.source != pin.repository {
            return Err("reviewed Cargo git dependency source must be its repository".into());
        }
        Ok(())
    }

    pub fn key(&self) -> Result<Sha256Digest, String> {
        canonical_digest(&(&self.registry, &self.name, &self.version, &self.filename))
    }

    pub fn registry(&self) -> Option<Registry> {
        match self.registry.as_str() {
            "cargo" => Some(Registry::Cargo),
            "conda" => Some(Registry::Conda),
            "pypi" => Some(Registry::Pypi),
            "npm" => Some(Registry::Npm),
            _ => None,
        }
    }
}

/// Rejects anything that is not already a full lowercase commit SHA.
///
/// Revisions may be abbreviated or symbolic; the *pin* may not, because a pin
/// that has to be resolved to be compared is not a pin.
pub fn full_commit(value: &str) -> Result<String, String> {
    if !matches!(value.len(), 40 | 64)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("invalid resolved Git commit".into());
    }
    Ok(value.to_owned())
}

fn validate_name(registry: &str, name: &str) -> Result<(), String> {
    let valid = if registry == "npm" && name.starts_with('@') {
        name[1..]
            .split_once('/')
            .is_some_and(|(scope, name)| identifier(scope) && identifier(name))
    } else {
        identifier(name)
    };
    if !valid {
        return Err("invalid reviewed package name".into());
    }
    Ok(())
}

fn identifier(value: &str) -> bool {
    value
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

fn validate_version(value: &str) -> Result<(), String> {
    if !value
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_alphanumeric())
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
    {
        return Err("invalid reviewed package version".into());
    }
    Ok(())
}
