use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------------------------- //

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Paths {
    pub game_root: String,
    #[serde(default)]
    pub data_dir: String,
}
