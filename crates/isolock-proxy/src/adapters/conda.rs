use super::{
    decoded_path, encode_segment, is_registry, valid_filename, RegistryAdapter, RegistryContext,
    RegistryRouteAction,
};
use crate::MIME_APPLICATION_JSON;
use isolock_core::Registry;
use isolock_plan::{PlannedArtifact, PlannedArtifactMetadata};
use rattler_conda_types::{package::IndexJson, PackageRecord, RepoData};
use serde_json::{json, Map, Value};
/// Conda channel adapter for repodata and local package downloads.
#[derive(Debug, Default, Clone, Copy)]
pub struct CondaAdapter;

impl RegistryAdapter for CondaAdapter {
    fn ecosystem(&self) -> Registry {
        Registry::Conda
    }

    fn handle_route(
        &self,
        _method: &str,
        path: &str,
        context: &RegistryContext<'_>,
    ) -> RegistryRouteAction {
        let path = match decoded_path(path) {
            Ok(path) => path,
            Err(error) => return RegistryRouteAction::BadRequest(error),
        };
        let Some(channel_path) = path.strip_prefix("/conda/channel/") else {
            return RegistryRouteAction::NotFound;
        };
        let components = channel_path.split('/').collect::<Vec<_>>();
        if components.len() < 2 {
            return RegistryRouteAction::NotFound;
        }
        let subdir = components[components.len() - 2];
        let resource = components[components.len() - 1];
        let channel = &components[..components.len() - 2];
        if !valid_filename(subdir)
            || channel
                .iter()
                .any(|component| !valid_filename(component) || matches!(*component, "." | ".."))
        {
            return RegistryRouteAction::BadRequest("invalid Conda channel path".into());
        }
        let channel = channel
            .iter()
            .map(|component| encode_segment(component))
            .collect::<Vec<_>>()
            .join("/");
        match resource {
            "repodata.json" | "current_repodata.json" => self.repodata(&channel, subdir, context),
            filename
                if valid_filename(filename)
                    && (filename.ends_with(".conda") || filename.ends_with(".tar.bz2")) =>
            {
                self.download(subdir, filename, context)
            }
            _ => RegistryRouteAction::NotFound,
        }
    }
}

impl CondaAdapter {
    fn repodata(
        &self,
        channel: &str,
        subdir: &str,
        context: &RegistryContext<'_>,
    ) -> RegistryRouteAction {
        let mut legacy = Map::new();
        let mut conda = Map::new();

        for (artifact, metadata) in context.plan.artifacts_with_metadata() {
            if !is_registry(artifact, Registry::Conda) {
                continue;
            }
            let filename = match conda_filename(artifact, metadata) {
                Ok(filename) => filename,
                Err(error) => return RegistryRouteAction::InvalidMetadata(error),
            };
            let package_subdir = conda_subdir(metadata).unwrap_or_else(|| "noarch".into());
            if package_subdir != subdir {
                continue;
            }
            let Some(bytes) = context.cas.get(&artifact.digest) else {
                return RegistryRouteAction::CacheMiss { filename };
            };
            let record = match conda_record(artifact, metadata, &filename, &package_subdir, bytes) {
                Ok(record) => record,
                Err(error) => return RegistryRouteAction::InvalidMetadata(error),
            };
            let record_value = match serde_json::to_value(record) {
                Ok(record) => record,
                Err(error) => return RegistryRouteAction::InvalidMetadata(error.to_string()),
            };
            if filename.ends_with(".conda") {
                conda.insert(filename, record_value);
            } else {
                legacy.insert(filename, record_value);
            }
        }

        let subdir = encode_segment(subdir);
        let base_url = if channel.is_empty() {
            format!("{}/conda/channel/{subdir}/", context.base_url)
        } else {
            format!("{}/conda/channel/{channel}/{subdir}/", context.base_url)
        };
        let payload = json!({
            "info": {
                "subdir": subdir,
                "base_url": base_url,
            },
            "packages": legacy,
            "packages.conda": conda,
            "repodata_version": 2,
        });
        match serde_json::from_value::<RepoData>(payload) {
            Ok(repodata) => match serde_json::to_vec(&repodata) {
                Ok(body) => RegistryRouteAction::ServeIndex {
                    content_type: MIME_APPLICATION_JSON,
                    body,
                },
                Err(error) => RegistryRouteAction::InvalidMetadata(error.to_string()),
            },
            Err(error) => RegistryRouteAction::InvalidMetadata(format!(
                "cannot construct Conda repodata: {error}"
            )),
        }
    }

    fn download(
        &self,
        subdir: &str,
        filename: &str,
        context: &RegistryContext<'_>,
    ) -> RegistryRouteAction {
        for (artifact, metadata) in context.plan.artifacts_with_metadata() {
            if !is_registry(artifact, Registry::Conda)
                || conda_filename(artifact, metadata).ok().as_deref() != Some(filename)
                || conda_subdir(metadata).as_deref().unwrap_or("noarch") != subdir
            {
                continue;
            }
            return if context.cas.contains(&artifact.digest) {
                RegistryRouteAction::DownloadArtifact {
                    digest: artifact.digest.clone(),
                    filename: filename.to_owned(),
                }
            } else {
                RegistryRouteAction::CacheMiss {
                    filename: filename.to_owned(),
                }
            };
        }
        RegistryRouteAction::CacheMiss {
            filename: filename.to_owned(),
        }
    }
}

fn conda_filename(
    artifact: &PlannedArtifact,
    metadata: &PlannedArtifactMetadata,
) -> Result<String, String> {
    let filename = metadata
        .filename
        .clone()
        .unwrap_or_else(|| format!("{}-{}-0.conda", artifact.name, artifact.version));
    if !valid_filename(&filename)
        || !(filename.ends_with(".conda") || filename.ends_with(".tar.bz2"))
    {
        return Err(format!("invalid Conda archive filename {filename:?}"));
    }
    Ok(filename)
}

fn conda_subdir(metadata: &PlannedArtifactMetadata) -> Option<String> {
    metadata.subdir.clone().or_else(|| {
        let value = metadata.registry_metadata.as_ref()?;
        value
            .get("subdir")
            .or_else(|| value.get("package_record")?.get("subdir"))
            .or_else(|| value.get("info/index.json")?.get("subdir"))?
            .as_str()
            .map(str::to_owned)
    })
}

fn conda_record(
    artifact: &PlannedArtifact,
    metadata: &PlannedArtifactMetadata,
    filename: &str,
    subdir: &str,
    bytes: &[u8],
) -> Result<PackageRecord, String> {
    if let Some(record) = metadata
        .registry_metadata
        .as_ref()
        .and_then(|value| value.get("package_record"))
        .and_then(|value| serde_json::from_value::<PackageRecord>(value.clone()).ok())
    {
        return finalize_record(record, artifact, subdir, bytes.len());
    }

    let metadata_value = metadata
        .registry_metadata
        .as_ref()
        .and_then(index_json_value);
    let mut index_value =
        metadata_value.unwrap_or_else(|| fallback_index_json(artifact, filename, subdir));
    let object = index_value
        .as_object_mut()
        .ok_or_else(|| "Conda info/index.json must be a JSON object".to_owned())?;
    object.insert("subdir".into(), Value::String(subdir.to_owned()));
    let index: IndexJson = serde_json::from_value(index_value)
        .map_err(|error| format!("invalid Conda info/index.json metadata: {error}"))?;
    let record = PackageRecord::from_index_json(index, Some(bytes.len() as u64), None, None)
        .map_err(|error| format!("invalid Conda package platform metadata: {error}"))?;
    finalize_record(record, artifact, subdir, bytes.len())
}

fn index_json_value(value: &Value) -> Option<Value> {
    let value = value
        .get("info/index.json")
        .or_else(|| value.get("index.json"))
        .or_else(|| value.get("index"))
        .or_else(|| value.get("info"))
        .unwrap_or(value);
    if value.get("name").is_some() && value.get("version").is_some() {
        Some(value.clone())
    } else {
        None
    }
}

fn fallback_index_json(artifact: &PlannedArtifact, filename: &str, subdir: &str) -> Value {
    let parsed = rattler_conda_types::package::DistArchiveIdentifier::try_from_filename(filename);
    let (name, version, build) = parsed.map_or_else(
        || {
            (
                artifact.name.clone(),
                artifact.version.clone(),
                "0".to_owned(),
            )
        },
        |identifier| {
            (
                identifier.identifier.name,
                identifier.identifier.version,
                identifier.identifier.build_string,
            )
        },
    );
    let build_number = build
        .rsplit('_')
        .next()
        .and_then(|number| number.parse::<u64>().ok())
        .unwrap_or_default();
    json!({
        "name": name,
        "version": version,
        "build": build,
        "build_number": build_number,
        "subdir": subdir,
        "depends": [],
    })
}

fn finalize_record(
    record: PackageRecord,
    artifact: &PlannedArtifact,
    subdir: &str,
    size: usize,
) -> Result<PackageRecord, String> {
    let mut value = serde_json::to_value(record).map_err(|error| error.to_string())?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "Conda package record did not serialize as an object".to_owned())?;
    object.insert(
        "sha256".into(),
        Value::String(artifact.digest.as_str().to_owned()),
    );
    object.insert("size".into(), json!(size));
    object.insert("subdir".into(), Value::String(subdir.to_owned()));
    serde_json::from_value(value)
        .map_err(|error| format!("invalid generated Conda package record: {error}"))
}
