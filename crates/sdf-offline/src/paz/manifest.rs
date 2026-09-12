use crate::{OfflineError, OfflineResult};

// ---------------------------------------------------------------------------------------------- //

use serde::{Deserialize, Serialize};
use std::path::Path;

// ---------------------------------------------------------------------------------------------- //

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ManifestRecord {
    pub(crate) path: String,
    pub(crate) paz_file: String,
    pub(crate) offset: u64,
    pub(crate) comp_size: u32,
    pub(crate) orig_size: u32,
    pub(crate) flags: u32,
    pub(crate) sha256: String,
}

impl ManifestRecord {
    pub(crate) fn load(path: &Path) -> OfflineResult<Vec<ManifestRecord>> {
        let data = std::fs::read_to_string(path)
            .map_err(|e| OfflineError::io(format!("read `{}`: {e}", path.display())))?;
        serde_json::from_str(&data).map_err(|e| {
            OfflineError::manifest(format!("parse manifest `{}`: {e}", path.display()))
        })
    }
}
