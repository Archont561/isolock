//! Argument groups for the profile workflows; clifacet owns parsing and aliases.

use std::path::PathBuf;

#[clifacet::group]
#[command(display_name = "isolock")]
pub struct NoArguments {}

#[clifacet::group]
#[command(display_name = "isolock")]
pub struct Plan {
    /// Check locked inputs and fail on drift.
    #[clifacet(long)]
    pub(crate) ci: bool,
    #[clifacet(long)]
    pub(crate) locked: bool,
    #[clifacet(long)]
    pub(crate) check: bool,
}

#[clifacet::group]
#[command(display_name = "isolock")]
pub struct Fetch {
    #[clifacet(long)]
    pub(crate) manifest: Option<String>,
}

#[clifacet::group]
#[command(display_name = "isolock", visible_alias = "bootstrap")]
pub struct Sync {
    #[clifacet(long)]
    pub(crate) locked: bool,
    #[clifacet(long)]
    pub(crate) offline: bool,
    #[clifacet(long)]
    pub(crate) manifest: Option<String>,
}

#[clifacet::group]
#[command(display_name = "isolock")]
pub struct Export {
    #[clifacet(long)]
    pub(crate) output: PathBuf,
}

#[clifacet::group]
#[command(display_name = "isolock")]
pub struct Gc {
    #[clifacet(long)]
    pub(crate) apply: bool,
    #[clifacet(long)]
    pub(crate) root: Vec<String>,
}
