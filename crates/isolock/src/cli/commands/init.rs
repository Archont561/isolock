//! The `init` controller: scaffolding arguments only, no behaviour.

#[clifacet::group]
#[command(display_name = "isolock")]
pub struct Init {
    /// Profile name the generated configuration declares.
    #[clifacet(long, default_value = "default")]
    pub(crate) profile: String,
    /// Platform the generated profile targets.
    #[clifacet(long, default_value = "linux-64")]
    pub(crate) platform: String,
    /// Replace files that already exist.
    #[clifacet(long)]
    pub(crate) force: bool,
}
