use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------------------------- //

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ExportMip {
    pub(crate) level: u32,
    pub(crate) size: u32,
    pub(crate) bytes: u32,
}
