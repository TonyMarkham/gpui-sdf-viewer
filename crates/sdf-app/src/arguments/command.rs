use crate::ConfigQuery;

#[derive(clap::Subcommand)]
pub enum Command {
    /// Headless extract + field run against the configured install.
    Extract {
        /// Override `paths.game_root` for this run.
        #[arg(long)]
        game_root: Option<String>,

        /// Override `paths.data_dir` for this run.
        #[arg(long)]
        data_dir: Option<String>,

        /// Persist the flag overrides into config.toml.
        #[arg(long, default_value_t = false)]
        save_config: bool,
    },

    /// Print a resolved support path.
    Config {
        #[command(subcommand)]
        query: ConfigQuery,
    },
}
