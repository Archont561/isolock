//! Registry-specific protocol adapters used by the shared loopback HTTP server.

mod cargo;
mod conda;
mod npm;
mod pypi;

use isolock_cas::{InMemoryCas, Sha256Digest};
use isolock_core::Registry;
use isolock_plan::AirlockPlan;
use std::sync::atomic::{AtomicBool, Ordering};

pub use cargo::CargoAdapter;
pub use conda::CondaAdapter;
pub use npm::NpmAdapter;
pub use pypi::PypiAdapter;

/// Request context shared with registry adapters while synthesizing a response.
pub struct RegistryContext<'a> {
    /// Immutable lock/airlock plan.
    pub plan: &'a AirlockPlan,
    /// Verified content-addressed blobs.
    pub cas: &'a InMemoryCas,
    /// Local origin used when registry metadata needs an absolute download URL.
    pub base_url: &'a str,
    pub(crate) cancelled: &'a AtomicBool,
}

impl RegistryContext<'_> {
    /// Adapters doing expensive work should check cancellation and return promptly.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// Response action returned by a registry adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryRouteAction {
    /// Synthesized registry metadata. The HTTP server may Brotli-compress it.
    ServeIndex {
        /// HTTP media type for the response.
        content_type: &'static str,
        /// Uncompressed response bytes.
        body: Vec<u8>,
    },
    /// The requested artifact is present in the plan and should be read from CAS.
    DownloadArtifact {
        /// Digest identifying the blob in CAS.
        digest: Sha256Digest,
        /// Filename advertised to the client.
        filename: String,
    },
    /// A planned artifact is not present in CAS; fail-closed modes must reject it.
    CacheMiss {
        /// Artifact filename, when known.
        filename: String,
    },
    /// The path is not part of this adapter's registry contract.
    NotFound,
    /// The request path or adapter metadata is malformed.
    BadRequest(String),
    /// Stored archive metadata could not be parsed or validated.
    InvalidMetadata(String),
}

/// Ordered collection of adapters. Registration is independent of listener setup.
#[derive(Default)]
pub struct AdapterRegistry {
    adapters: Vec<Box<dyn RegistryAdapter>>,
}

impl AdapterRegistry {
    /// Creates an empty registry; unmatched requests receive 404, never upstream fallback.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends an adapter. Earlier adapters have route precedence.
    pub fn register(&mut self, adapter: impl RegistryAdapter + 'static) -> &mut Self {
        self.adapters.push(Box::new(adapter));
        self
    }

    pub(crate) fn route(
        &self,
        method: &str,
        path: &str,
        context: &RegistryContext<'_>,
    ) -> RegistryRouteAction {
        self.adapters
            .iter()
            .map(|adapter| adapter.handle_route(method, path, context))
            .find(|action| *action != RegistryRouteAction::NotFound)
            .unwrap_or(RegistryRouteAction::NotFound)
    }
}

/// Supplies the standard Conda, PyPI, npm, and Cargo adapters in route order.
#[must_use]
pub fn default_adapters() -> AdapterRegistry {
    let mut registry = AdapterRegistry::new();
    registry
        .register(CondaAdapter)
        .register(PypiAdapter)
        .register(NpmAdapter)
        .register(CargoAdapter);
    registry
}

/// Unified contract for protocol adapters that synthesize registry responses.
///
/// Implementations must bound their work and cooperate with context cancellation.
/// Rust cannot forcibly stop an arbitrary adapter: grace-expired workers are counted
/// and detached, and explicit shutdown returns a timeout instead of blocking forever.
pub trait RegistryAdapter: Send + Sync {
    /// The package registry ecosystem handled by this adapter.
    fn ecosystem(&self) -> Registry;

    /// Routes and renders a request using the immutable plan and CAS snapshot.
    fn handle_route(
        &self,
        method: &str,
        path: &str,
        context: &RegistryContext<'_>,
    ) -> RegistryRouteAction;
}

/// Removes a query from a request target and percent-decodes its path.
pub(crate) fn decoded_path(path: &str) -> Result<String, String> {
    let path = path.split_once('?').map_or(path, |(path, _)| path);
    let path = if let Some(scheme_start) = path.find("://") {
        let authority_start = scheme_start + 3;
        match path[authority_start..].find('/') {
            Some(offset) => &path[authority_start + offset..],
            None => "/",
        }
    } else {
        path
    };

    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err("incomplete percent escape in request path".into());
            }
            let high = hex_value(bytes[index + 1])
                .ok_or_else(|| "invalid percent escape in request path".to_owned())?;
            let low = hex_value(bytes[index + 2])
                .ok_or_else(|| "invalid percent escape in request path".to_owned())?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| "request path is not valid UTF-8".into())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Encodes an individual URL path segment without changing its separators.
pub(crate) fn encode_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

/// Normalizes a PyPI project name as specified by PEP 503.
pub(crate) fn normalize_pypi_name(name: &str) -> String {
    let mut normalized = String::with_capacity(name.len());
    let mut separator = false;
    for character in name.chars() {
        if matches!(character, '-' | '_' | '.') {
            if !separator {
                normalized.push('-');
            }
            separator = true;
        } else {
            normalized.extend(character.to_lowercase());
            separator = false;
        }
    }
    normalized
}

/// Validates that a registry filename is a safe single path component.
pub(crate) fn valid_filename(filename: &str) -> bool {
    !filename.is_empty()
        && filename != "."
        && filename != ".."
        && !filename.contains('/')
        && !filename.contains('\\')
        && !filename.bytes().any(|byte| byte.is_ascii_control())
}

/// Checks whether an artifact belongs to an ecosystem.
pub(crate) fn is_registry(artifact: &isolock_plan::PlannedArtifact, ecosystem: Registry) -> bool {
    artifact.registry == ecosystem
}
