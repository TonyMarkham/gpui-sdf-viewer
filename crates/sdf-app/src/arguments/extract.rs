use sdf_offline::{Config, OfflineError, OfflineResult, Progress, extract_run, field_run};

// ---------------------------------------------------------------------------------------------- //

use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------------------------- //

/// Loads (or initializes) the config, applies the flag overrides, validates
/// the install, and runs the pipeline with milestone progress printed.
pub(crate) fn run(
    game_root: Option<String>,
    data_dir: Option<String>,
    save_config: bool,
) -> OfflineResult<()> {
    run_in(&Config::config_path()?, game_root, data_dir, save_config)
}

/// The whole CLI surface against an injectable config path: load, apply the
/// flag overrides, optionally persist them, prepare, and run the pipeline.
pub(crate) fn run_in(
    config_path: &Path,
    game_root: Option<String>,
    data_dir: Option<String>,
    save_config: bool,
) -> OfflineResult<()> {
    let mut config = Config::load_from(config_path)?;
    apply_overrides(&mut config, game_root, data_dir);
    if save_config {
        config.save_to(config_path)?;
    }

    prepare(&config)?;
    execute(&config)
}

/// Flag overrides win over the loaded config; an absent flag keeps the
/// configured value.
pub(crate) fn apply_overrides(
    config: &mut Config,
    game_root: Option<String>,
    data_dir: Option<String>,
) {
    if let Some(root) = game_root {
        config.paths.game_root = root;
    }
    if let Some(dir) = data_dir {
        config.paths.data_dir = dir;
    }
}

/// The hard wiring before any pipeline I/O: config validation, the
/// empty-root guard, then install validation — in that order.
pub(crate) fn prepare(config: &Config) -> OfflineResult<()> {
    config.check()?;
    if config.paths.game_root.is_empty() {
        return Err(OfflineError::io(
            "no game folder is configured; pass --game-root or pick one in the GUI",
        ));
    }
    config.validate(&PathBuf::from(&config.paths.game_root))
}

fn execute(config: &Config) -> OfflineResult<()> {
    let mut forward = |event: Progress| {
        if event.milestone {
            println!("{}", event.message);
        }
    };

    extract_run(config, &mut forward)?;
    field_run(config, &mut forward)?;

    Ok(())
}
