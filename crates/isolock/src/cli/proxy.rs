//! Declarative proxy argument groups; clifacet owns parsing and separators.

#[clifacet::group]
#[command(display_name = "isolock")]
pub struct Exec {
    /// The child command and its arguments; everything after COMMAND is passed through.
    #[arg(
        required = true,
        trailing_var_arg = true,
        allow_hyphen_values = true,
        value_name = "COMMAND"
    )]
    pub(super) arguments: Vec<String>,
}

#[clifacet::group]
#[command(display_name = "isolock")]
pub struct Ingest {
    #[clifacet(long, value_parser = ["cargo", "pypi", "npm", "conda"])]
    pub(super) registry: Option<String>,
    pub(super) packages: Vec<String>,
}
