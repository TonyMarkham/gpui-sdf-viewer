use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------------------------- //

pub(crate) const DEFAULT_PACK: &str = "0012";
pub(crate) const DEFAULT_EXTRACT_DIR: &str = "dds";
pub(crate) const DEFAULT_MANIFEST_NAME: &str = "manifest.json";
pub(crate) const DEFAULT_DDS_HEADER: usize = 128;
pub(crate) const DEFAULT_INCLUDES: [&str; 6] = [
    "cd_worldmap_land_sdf*",
    "cd_worldmap_road_sdf*",
    "cd_worldmap_road_wagon_sdf*",
    "cd_worldmap_mountain_sdf*",
    "cd_worldmap_abyss_hex_sdf*",
    "cd_worldmap_blur_height*",
];

// ---------------------------------------------------------------------------------------------- //

/// The extraction surface: which packs to open, which entry families to keep,
/// and where the `.dds` tiles land under the data directory.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Extract {
    #[serde(default = "default_packs")]
    pub packs: Vec<String>,
    #[serde(default = "default_includes")]
    pub includes: Vec<String>,
    #[serde(default = "default_dir")]
    pub dir: String,
    #[serde(default = "default_manifest")]
    pub manifest: String,
    #[serde(default = "default_dds_header")]
    pub dds_header: usize,
}

impl Default for Extract {
    fn default() -> Self {
        Self {
            packs: default_packs(),
            includes: default_includes(),
            dir: default_dir(),
            manifest: default_manifest(),
            dds_header: default_dds_header(),
        }
    }
}

// ---------------------------------------------------------------------------------------------- //

fn default_packs() -> Vec<String> {
    vec![String::from(DEFAULT_PACK)]
}

fn default_includes() -> Vec<String> {
    DEFAULT_INCLUDES
        .iter()
        .map(|pattern| String::from(*pattern))
        .collect()
}

fn default_dir() -> String {
    String::from(DEFAULT_EXTRACT_DIR)
}

fn default_manifest() -> String {
    String::from(DEFAULT_MANIFEST_NAME)
}

fn default_dds_header() -> usize {
    DEFAULT_DDS_HEADER
}
