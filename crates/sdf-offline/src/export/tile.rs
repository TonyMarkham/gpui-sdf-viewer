use crate::{OfflineError, OfflineResult, TileKind, utilities::sha256_hex};

// ---------------------------------------------------------------------------------------------- //

use serde::{Deserialize, Serialize};
use std::path::Path;

// ---------------------------------------------------------------------------------------------- //

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ExportTile {
    pub(crate) path: String,
    pub(crate) payload: String,
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) kind: TileKind,
    pub(crate) source_sha256: String,
    pub(crate) payload_sha256: String,
}

impl ExportTile {
    pub(crate) fn reload(&self, dir: &Path) -> OfflineResult<Vec<u8>> {
        let path = dir.join(&self.payload);
        let data = std::fs::read(&path)
            .map_err(|e| OfflineError::io(format!("read `{}`: {e}", path.display())))?;

        let digest = sha256_hex(&data);
        if digest != self.payload_sha256 {
            return Err(OfflineError::manifest(format!(
                "tile payload `{}` hash {digest} does not match manifest {}",
                path.display(),
                self.payload_sha256
            )));
        }

        Ok(data)
    }
}
