//! A nested declarative controller receives only the parsed configuration overrides.

use super::profiles::NoArguments;
use crate::config::Overrides;
use crate::profile_cli::{self, Configuration as Action, Operation};

#[clifacet::group(commands)]
#[command(
    display_name = "isolock",
    subcommand_required = true,
    arg_required_else_help = true
)]
pub struct Config {
    #[clifacet(extract)]
    overrides: Overrides,
}

impl From<&Overrides> for Overrides {
    fn from(overrides: &Overrides) -> Self {
        overrides.clone()
    }
}

#[clifacet::commands(group)]
impl Config {
    /// Validate the layered configuration.
    pub fn check(&self, #[clifacet(subcommand)] _options: NoArguments) -> Result<(), String> {
        profile_cli::run(
            &self.overrides,
            Operation::Config {
                command: Action::Check,
            },
        )
    }

    /// Show the effective configuration.
    pub fn show(&self, #[clifacet(subcommand)] _options: NoArguments) -> Result<(), String> {
        profile_cli::run(
            &self.overrides,
            Operation::Config {
                command: Action::Show,
            },
        )
    }

    /// Explain a configuration value and its source.
    pub fn explain(&self, #[clifacet(subcommand)] options: Explain) -> Result<(), String> {
        profile_cli::run(
            &self.overrides,
            Operation::Config {
                command: Action::Explain { path: options.path },
            },
        )
    }
}

#[clifacet::group]
#[command(display_name = "isolock")]
pub struct Explain {
    path: String,
}
