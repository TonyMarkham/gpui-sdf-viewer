// ---------------------------------------------------------------------------------------------- //

use std::path::PathBuf;

// ---------------------------------------------------------------------------------------------- //

/// The config surface resolved against the OS home: every output directory is
/// concrete, defaults applied, no I/O performed.
#[derive(Clone, Debug)]
pub struct ResolvedPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub dds_dir: PathBuf,
    pub sdf_dir: PathBuf,
    pub game_root: PathBuf,
}
