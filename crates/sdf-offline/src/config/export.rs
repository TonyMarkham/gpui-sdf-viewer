use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------------------------- //

use std::collections::BTreeMap;

// ---------------------------------------------------------------------------------------------- //

pub(crate) const LAND_PREFIX: &str = "cd_worldmap_land_sdf";
pub(crate) const ROAD_PREFIX: &str = "cd_worldmap_road_sdf";
pub(crate) const MOUNTAIN_PREFIX: &str = "cd_worldmap_mountain_sdf";
pub(crate) const ABYSS_HEX_PREFIX: &str = "cd_worldmap_abyss_hex_sdf";
pub(crate) const ROAD_WAGON_PREFIX: &str = "cd_worldmap_road_wagon_sdf";
pub(crate) const BLUR_HEIGHT_PREFIX: &str = "cd_worldmap_blur_height";

pub(crate) const DEFAULT_EXPORT_DIR: &str = "sdf";
pub(crate) const DEFAULT_EXPORT_MANIFEST_NAME: &str = "manifest.json";
pub(crate) const ROAD_WAGON_ROUTE: &str = "road_wagon";
pub(crate) const BLUR_HEIGHT_ROUTE: &str = "blur_height";

// ---------------------------------------------------------------------------------------------- //

/// The VS-20 export surface: where `.r8` payloads and the export manifest land
/// under the data directory, plus fields without a recipe scene exported under
/// a given name.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Export {
    #[serde(default = "default_dir")]
    pub dir: String,
    #[serde(default = "default_manifest")]
    pub manifest: String,
    #[serde(default = "default_extra")]
    pub extra: BTreeMap<String, String>,
}

impl Default for Export {
    fn default() -> Self {
        Self {
            dir: default_dir(),
            manifest: default_manifest(),
            extra: default_extra(),
        }
    }
}

// ---------------------------------------------------------------------------------------------- //

fn default_dir() -> String {
    String::from(DEFAULT_EXPORT_DIR)
}

fn default_manifest() -> String {
    String::from(DEFAULT_EXPORT_MANIFEST_NAME)
}

fn default_extra() -> BTreeMap<String, String> {
    let mut extra = BTreeMap::new();
    extra.insert(
        String::from(ROAD_WAGON_ROUTE),
        String::from(ROAD_WAGON_PREFIX),
    );
    extra.insert(
        String::from(BLUR_HEIGHT_ROUTE),
        String::from(BLUR_HEIGHT_PREFIX),
    );
    extra
}
