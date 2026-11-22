//! Declarative argument groups attach metadata to controller methods.
//! Keep the application version label and bootstrap alias without intercepting argv.

use std::path::PathBuf;

#[clifacet::group]
#[command(display_name = "isolock")]
pub struct NoArguments {}

#[clifacet::group]
#[command(display_name = "isolock")]
pub struct Plan {
    /// Check locked inputs and fail on drift.
    #[clifacet(long)]
    pub(super) ci: bool,
    #[clifacet(long)]
    pub(super) locked: bool,
    #[clifacet(long)]
    pub(super) check: bool,
}

#[clifacet::group]
#[command(display_name = "isolock")]
pub struct Fetch {
    #[clifacet(long)]
    pub(super) manifest: Option<String>,
}

#[clifacet::group]
#[command(display_name = "isolock", visible_alias = "bootstrap")]
pub struct Sync {
    #[clifacet(long)]
    pub(super) locked: bool,
    #[clifacet(long)]
    pub(super) offline: bool,
    #[clifacet(long)]
    pub(super) manifest: Option<String>,
}

#[clifacet::group]
#[command(display_name = "isolock")]
pub struct Export {
    #[clifacet(long)]
    pub(super) output: PathBuf,
}

#[clifacet::group]
#[command(display_name = "isolock")]
pub struct Gc {
    #[clifacet(long)]
    pub(super) apply: bool,
    #[clifacet(long)]
    pub(super) root: Vec<String>,
}
