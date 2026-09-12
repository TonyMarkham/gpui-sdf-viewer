#[derive(clap::Subcommand)]
pub enum ConfigQuery {
    /// Print the resolved config.toml path.
    Path,
}
