use crate::{
    Level, SceneTile, SdfError, SdfResult,
    data::{FORMAT_TAG, FORMAT_VERSION, level::levels, scene_tile::tiles},
    utilities::field_u32,
};

use serde_json::Value;
use soul_attributes::soul;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------------------------- //

/// One configured field of a data scene: the layer's name and its resolved
/// tiles. Geometry is shared across a scene's fields — one grid, one mip
/// chain — and lives on [`SceneData`].
#[derive(Clone, Debug)]
pub struct SceneField {
    pub layer: String,
    pub tiles: Vec<SceneTile>,
}

/// The resolved data behind a data scene: every configured layer with its
/// verified-against tiles, sharing one tile-grid geometry and one mip-chain
/// shape. The shared shape is what lets every field reuse the one
/// `field_coord(p)` decomposition. All of it comes from the manifest
/// entries; nothing is inferred from file sizes.
#[derive(Clone, Debug)]
pub struct SceneData {
    pub manifest: PathBuf,
    pub fields: Vec<SceneField>,
    pub grid: u32,
    pub levels: Vec<Level>,
}

impl SceneData {
    /// The byte length of one real tile's payload: the sum of the declared
    /// mip sizes.
    pub fn tile_payload_len(&self) -> u64 {
        self.levels.iter().map(|level| u64::from(level.bytes)).sum()
    }

    /// The fields' mip level count.
    pub fn level_count(&self) -> u32 {
        self.levels.len() as u32
    }

    /// Parses and validates the VS-20 export manifest at `manifest_path` and
    /// resolves every layer named in `layer_names` (paint order, bottom →
    /// top) against the manifest's directory. Every field must share the
    /// first field's geometry — grid, mip-level count, and, byte-exact, the
    /// whole mip table — so the shared decomposition stays valid.
    #[soul(id = "concept.sdf-scene-contract", step = "manifest load")]
    pub fn load(manifest_path: &Path, layer_names: &[String]) -> SdfResult<Self> {
        if layer_names.is_empty() {
            return Err(SdfError::data(
                "the scene's data directives resolve to no fields",
            ));
        }

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

        let mut fields = Vec::with_capacity(layer_names.len());
        let mut grid = 0;
        let mut shared_levels = Vec::new();
        let mut first = String::new();
        for layer_name in layer_names {
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
            let field_levels = levels(entry)?;
            let tile_edge = field_levels
                .first()
                .map(|level| level.size)
                .unwrap_or_default();
            if size % tile_edge != 0 {
                return Err(SdfError::data(&format!(
                    "layer `{layer_name}`: field size {size} is not a multiple of the tile edge {tile_edge}"
                )));
            }
            let field_grid = size / tile_edge;

            if fields.is_empty() {
                first = String::from(layer_name);
                grid = field_grid;
                shared_levels = field_levels;
            } else {
                if field_grid != grid {
                    return Err(SdfError::data(&format!(
                        "field \"{layer_name}\": grid {field_grid} differs from field \"{first}\": {grid}"
                    )));
                }
                if field_levels.len() != shared_levels.len() {
                    return Err(SdfError::data(&format!(
                        "field \"{layer_name}\": mip level count {} differs from field \"{first}\": {}",
                        field_levels.len(),
                        shared_levels.len()
                    )));
                }
                for (index, level) in field_levels.iter().enumerate() {
                    if level.size != shared_levels[index].size {
                        return Err(SdfError::data(&format!(
                            "field \"{layer_name}\": mip level {index} size {} differs from field \"{first}\": {}",
                            level.size, shared_levels[index].size
                        )));
                    }
                }
            }

            let tiles = tiles(entry, field_grid, layer_name, manifest_path.parent())?;
            fields.push(SceneField {
                layer: String::from(layer_name),
                tiles,
            });
        }

        Ok(Self {
            manifest: manifest_path.to_path_buf(),
            fields,
            grid,
            levels: shared_levels,
        })
    }
}
