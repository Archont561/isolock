//! The nested `config` controller: argument groups over configuration use cases.

use super::profiles::NoArguments;
use crate::application::use_cases::{
    check_configuration, explain_configuration, show_configuration,
};
use crate::cli::context::AppContext;
use crate::cli::error;
use crate::cli::view;
use clifacet::Failure;

#[clifacet::group(commands)]
#[command(
    display_name = "isolock",
    subcommand_required = true,
    arg_required_else_help = true
)]
pub struct Config {
    #[clifacet(extract)]
    context: AppContext,
}

#[clifacet::commands(group)]
impl Config {
    /// Validate the layered configuration.
    pub fn check(&self, #[clifacet(subcommand)] _options: NoArguments) -> Result<String, Failure> {
        let provider = self.context.configuration();
        check_configuration(&provider, self.context.root())
            .map(|outcome| view::configuration(&outcome))
            .map_err(error::execution)
    }

    /// Show the effective configuration.
    pub fn show(&self, #[clifacet(subcommand)] _options: NoArguments) -> Result<String, Failure> {
        let provider = self.context.configuration();
        show_configuration(&provider, self.context.root())
            .map(|outcome| view::configuration(&outcome))
            .map_err(error::execution)
    }

    /// Explain a configuration value and its source.
    pub fn explain(&self, #[clifacet(subcommand)] options: Explain) -> Result<String, Failure> {
        let provider = self.context.configuration();
        explain_configuration(&provider, self.context.root(), &options.path)
            .map(|outcome| view::configuration(&outcome))
            .map_err(error::execution)
    }
}

#[clifacet::group]
#[command(display_name = "isolock")]
pub struct Explain {
    pub(crate) path: String,
}
