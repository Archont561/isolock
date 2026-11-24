//! `config check`, `config show`, and `config explain` as one validated-load workflow.

use crate::application::ports::{ConfigProvenance, ConfigurationProvider, UseCaseResult};

/// The rendered result of a configuration command, exactly as the operator sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigurationOutcome {
    /// The single line (or document) the CLI prints on standard output.
    pub text: String,
}

/// Validates the layered configuration and confirms the schema version.
///
/// # Errors
/// Returns a [`crate::domain::DomainError`] when any layer is invalid.
pub fn check_configuration<P: ConfigurationProvider>(
    provider: &P,
    root: &std::path::Path,
) -> UseCaseResult<ConfigurationOutcome> {
    let loaded = provider.load(root)?;
    Ok(ConfigurationOutcome {
        text: format!(
            "Configuration is valid (schema {}).",
            loaded.settings.schema_version
        ),
    })
}

/// Renders the effective configuration as the reviewed settings document.
///
/// # Errors
/// Returns a [`crate::domain::DomainError`] when any layer is invalid or cannot be rendered.
pub fn show_configuration<P: ConfigurationProvider>(
    provider: &P,
    root: &std::path::Path,
) -> UseCaseResult<ConfigurationOutcome> {
    let loaded = provider.load(root)?;
    Ok(ConfigurationOutcome {
        text: loaded.provenance.render()?,
    })
}

/// Renders one configuration path together with the layer that supplied it.
///
/// # Errors
/// Returns a [`crate::domain::DomainError`] when the layer is invalid, the path is unknown,
/// or the value has no provenance.
pub fn explain_configuration<P: ConfigurationProvider>(
    provider: &P,
    root: &std::path::Path,
    path: &str,
) -> UseCaseResult<ConfigurationOutcome> {
    let loaded = provider.load(root)?;
    Ok(ConfigurationOutcome {
        text: loaded.provenance.explain(path)?,
    })
}
