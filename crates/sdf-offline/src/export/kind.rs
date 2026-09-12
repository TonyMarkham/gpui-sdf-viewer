use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------------------------- //

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TileKind {
    Mip0,
    Stub,
}
