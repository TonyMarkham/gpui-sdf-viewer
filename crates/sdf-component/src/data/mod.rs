pub(crate) mod level;
pub(crate) mod scene_data;
pub(crate) mod scene_tile;

// ---------------------------------------------------------------------------------------------- //

/// Format tag of the VS-20 export manifest this component consumes.
pub(crate) const FORMAT_TAG: &str = "cd-map-sdf-field";

/// Format version of the VS-20 export manifest this component consumes.
pub(crate) const FORMAT_VERSION: u64 = 1;
