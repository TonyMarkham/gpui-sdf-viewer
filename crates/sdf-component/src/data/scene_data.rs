use crate::{
    Level, SceneTile, SdfError, SdfResult,
    data::{FORMAT_TAG, FORMAT_VERSION, level::levels, scene_tile::tiles},
    utilities::field_u32,
};

use serde_json::Value;
use soul_attributes::soul;
use std::path::{Path, PathBuf};

/// The resolved data behind a data scene: the layer's tile-grid geometry, its
/// mip-chain shape, and every tile with its verified-against payload. All of
/// it comes from the manifest entry; nothing is inferred from file sizes.
#[derive(Clone, Debug)]
pub struct SceneData {
    pub manifest: PathBuf,
    pub layer: String,
    pub grid: u32,
    pub levels: Vec<Level>,
    pub tiles: Vec<SceneTile>,
}

impl SceneData {
    /// The byte length of one real tile's payload: the sum of the declared
    /// mip sizes.
    pub fn tile_payload_len(&self) -> u64 {
        self.levels.iter().map(|level| u64::from(level.bytes)).sum()
    }

    /// The layer's mip level count.
    pub fn level_count(&self) -> u32 {
        self.levels.len() as u32
    }

    /// Parses and validates the VS-20 export manifest at `manifest_path` and
    /// resolves the layer named `layer_name` against the manifest's directory.
    #[soul(id = "concept.sdf-scene-contract", step = "manifest load")]
    pub fn load(manifest_path: &Path, layer_name: &str) -> SdfResult<Self> {
        let raw = std::fs::read_to_string(manifest_path).map_err(|error| {
            SdfError::data(&format!(
                "manifest `{}` could not be read: {error}",
                manifest_path.display()
            ))
        })?;
        let manifest: Value = serde_json::from_str(&raw).map_err(|error| {
            SdfError::data(&format!(
                "manifest `{}` is not valid JSON: {error}",
                manifest_path.display()
            ))
        })?;

        let format = manifest
            .get("format")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if format != FORMAT_TAG {
            return Err(SdfError::data(&format!(
                "manifest `{}` declares format `{format}`, expected `{FORMAT_TAG}`",
                manifest_path.display()
            )));
        }
        let version = manifest.get("version").and_then(Value::as_u64);
        if version != Some(FORMAT_VERSION) {
            return Err(SdfError::data(&format!(
                "manifest `{}` declares version {version:?}, expected {FORMAT_VERSION}",
                manifest_path.display()
            )));
        }

        let layers = manifest.get("layers").and_then(Value::as_array);
        let entry = layers.and_then(|layers| {
            layers
                .iter()
                .find(|layer| layer.get("name").and_then(Value::as_str) == Some(layer_name))
        });
        let Some(entry) = entry else {
            return Err(SdfError::data(&format!(
                "manifest `{}` has no layer named `{layer_name}`",
                manifest_path.display()
            )));
        };

        let size = field_u32(entry, "size")?;
        let levels = levels(entry)?;
        let tile_edge = levels.first().map(|level| level.size).unwrap_or_default();
        if size % tile_edge != 0 {
            return Err(SdfError::data(&format!(
                "layer `{layer_name}`: field size {size} is not a multiple of the tile edge {tile_edge}"
            )));
        }
        let grid = size / tile_edge;

        let tiles = tiles(entry, grid, layer_name, manifest_path.parent())?;

        Ok(Self {
            manifest: manifest_path.to_path_buf(),
            layer: String::from(layer_name),
            grid,
            levels,
            tiles,
        })
    }
}
