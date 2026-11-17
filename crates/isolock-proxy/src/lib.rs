#![forbid(unsafe_code)]
//! Hermetic interceptor proxy and local registry adapter for Isolock.

mod adapters;
mod server;

pub use adapters::{
    CargoAdapter, CondaAdapter, NpmAdapter, PypiAdapter, RegistryAdapter, RegistryContext,
    RegistryRouteAction,
};
pub use server::{ProxyHandle, ProxyServer};

use isolock_core::AirlockMode;
pub use rattler_conda_types::{ChannelInfo, PackageRecord, RepoData};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
pub use tame_index::krate::{IndexKrate, IndexVersion};

/// Canonical HTTP Content-Encoding header value for Brotli compression.
pub const CONTENT_ENCODING_BROTLI: &str = "br";

/// Canonical MIME content type for PEP 691 PyPI JSON simple API.
pub const MIME_PYPI_SIMPLE_JSON: &str = "application/vnd.pypi.simple.v1+json";

/// Canonical MIME content type for standard JSON registry responses (Conda, NPM, Cargo).
pub const MIME_APPLICATION_JSON: &str = "application/json";

/// Configuration for the local hermetic proxy listener.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub bind_host: String,
    pub port: u16,
    pub mode: AirlockMode,
}

impl ProxyConfig {
    /// Creates a configuration. Use port zero to request an OS-assigned port.
    #[must_use]
    pub fn new(bind_host: impl Into<String>, port: u16, mode: AirlockMode) -> Self {
        Self {
            bind_host: bind_host.into(),
            port,
            mode,
        }
    }
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self::new("127.0.0.1", 0, AirlockMode::LocalAirlock)
    }
}

/// Routing decision determined by the proxy for an intercepted package request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProxyDecision {
    /// Artifact present in CAS; serve hermetically.
    ServeCached,
    /// Connected mode: fetch artifact from upstream repository.
    FetchUpstream,
    /// Airlock / CI mode cache miss: fail closed immediately.
    FailClosedAirlockViolation,
}

/// Determines the proxy resolution path given the operational mode and cache presence.
#[must_use]
pub const fn evaluate_proxy_request(mode: AirlockMode, is_cached: bool) -> ProxyDecision {
    if is_cached {
        ProxyDecision::ServeCached
    } else {
        match mode {
            AirlockMode::Connected => ProxyDecision::FetchUpstream,
            AirlockMode::Ci | AirlockMode::LocalAirlock => {
                ProxyDecision::FailClosedAirlockViolation
            }
        }
    }
}

/// Compresses a slice of bytes using Brotli compression (default quality level 6).
///
/// # Errors
/// Returns an [`std::io::Error`] if compression fails.
pub fn compress_brotli(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut output = Vec::new();
    let mut writer = brotli::CompressorWriter::new(&mut output, 4096, 6, 22);
    writer.write_all(data)?;
    writer.flush()?;
    drop(writer);
    Ok(output)
}

/// Decompresses a slice of Brotli-compressed bytes.
///
/// # Errors
/// Returns an [`std::io::Error`] if decompression fails or payload is invalid.
pub fn decompress_brotli(compressed: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut reader = brotli::Decompressor::new(compressed, 4096);
    let mut output = Vec::new();
    reader.read_to_end(&mut output)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_decisions() {
        assert_eq!(
            evaluate_proxy_request(AirlockMode::LocalAirlock, true),
            ProxyDecision::ServeCached
        );
        assert_eq!(
            evaluate_proxy_request(AirlockMode::LocalAirlock, false),
            ProxyDecision::FailClosedAirlockViolation
        );
        assert_eq!(
            evaluate_proxy_request(AirlockMode::Ci, false),
            ProxyDecision::FailClosedAirlockViolation
        );
        assert_eq!(
            evaluate_proxy_request(AirlockMode::Connected, false),
            ProxyDecision::FetchUpstream
        );
    }

    #[test]
    fn test_proxy_config_serde_roundtrip() {
        let config = ProxyConfig::new("127.0.0.1", 8080, AirlockMode::LocalAirlock);
        let serialized = serde_json::to_string(&config).expect("serialization succeeds");
        let deserialized: ProxyConfig =
            serde_json::from_str(&serialized).expect("deserialization succeeds");
        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_registry_mime_types_are_json() {
        assert!(MIME_PYPI_SIMPLE_JSON.contains("json"));
        assert_eq!(MIME_APPLICATION_JSON, "application/json");
        assert_ne!(MIME_PYPI_SIMPLE_JSON, "text/html");
        assert_ne!(MIME_APPLICATION_JSON, "text/html");
    }

    #[test]
    fn test_brotli_roundtrip_compression() {
        let original_json =
            br#"{"packages":{"fastapi":{"version":"0.115.0","requires_python":">=3.8"}}}"#;
        let compressed = compress_brotli(original_json).expect("compression succeeds");
        assert_eq!(CONTENT_ENCODING_BROTLI, "br");
        let decompressed = decompress_brotli(&compressed).expect("decompression succeeds");
        assert_eq!(decompressed, original_json);
    }
}
