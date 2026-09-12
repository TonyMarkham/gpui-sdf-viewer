use crate::{ExportMip, ExportTile};

// ---------------------------------------------------------------------------------------------- //

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------------------------- //

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ExportLayer {
    pub(crate) name: String,
    pub(crate) tile_prefix: String,
    pub(crate) size: u32,
    pub(crate) source_manifest_sha256: String,
    pub(crate) mips: Vec<ExportMip>,
    pub(crate) tiles: Vec<ExportTile>,
}
