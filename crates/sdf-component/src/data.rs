use crate::error::{Error, Result};
use serde_json::Value;
use soul_attr::soul;
use std::path::{Path, PathBuf};

/// Format tag of the VS-20 export manifest this component consumes.
const FORMAT_TAG: &str = "cd-map-sdf-field";

/// Format version of the VS-20 export manifest this component consumes.
const FORMAT_VERSION: u64 = 1;

/// One entry of a layer's mip chain as declared by the manifest: the pixel
/// edge of the level inside a real tile's payload and its byte length.
#[derive(Clone, Debug)]
pub(crate) struct Level {
    pub size: u32,
    pub bytes: u32,
}

/// One grid tile of a data layer, resolved against the manifest's directory.
#[derive(Clone, Debug)]
pub(crate) struct SceneTile {
    pub payload: PathBuf,
    pub x: u32,
    pub y: u32,
    pub stub: bool,
    pub sha256: String,
}

/// The resolved data behind a data scene: the layer's tile-grid geometry, its
/// mip-chain shape, and every tile with its verified-against payload. All of
/// it comes from the manifest entry; nothing is inferred from file sizes.
#[derive(Clone, Debug)]
pub(crate) struct SceneData {
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
    pub fn load(manifest_path: &Path, layer_name: &str) -> Result<Self> {
        let raw = std::fs::read_to_string(manifest_path).map_err(|error| {
            Error::data(&format!(
                "manifest `{}` could not be read: {error}",
                manifest_path.display()
            ))
        })?;
        let manifest: Value = serde_json::from_str(&raw).map_err(|error| {
            Error::data(&format!(
                "manifest `{}` is not valid JSON: {error}",
                manifest_path.display()
            ))
        })?;

        let format = manifest
            .get("format")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if format != FORMAT_TAG {
            return Err(Error::data(&format!(
                "manifest `{}` declares format `{format}`, expected `{FORMAT_TAG}`",
                manifest_path.display()
            )));
        }
        let version = manifest.get("version").and_then(Value::as_u64);
        if version != Some(FORMAT_VERSION) {
            return Err(Error::data(&format!(
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
            return Err(Error::data(&format!(
                "manifest `{}` has no layer named `{layer_name}`",
                manifest_path.display()
            )));
        };

        let size = field_u32(entry, "size")?;
        let levels = levels(entry)?;
        let tile_edge = levels.first().map(|level| level.size).unwrap_or_default();
        if size % tile_edge != 0 {
            return Err(Error::data(&format!(
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

fn field_u32(entry: &Value, key: &str) -> Result<u32> {
    let value = entry.get(key).and_then(Value::as_u64).ok_or_else(|| {
        Error::data(&format!(
            "manifest layer is missing a valid `{key}` (u64) field"
        ))
    })?;
    u32::try_from(value)
        .map_err(|_| Error::data(&format!("manifest layer `{key}` {value} exceeds u32")))
}

/// Parses and structurally validates the layer's mip chain: levels ascend
/// from 0, every level is nonzero, each edge halves the previous one, and
/// each byte length matches its level's pixel count.
fn levels(entry: &Value) -> Result<Vec<Level>> {
    let raw = entry
        .get("mips")
        .and_then(Value::as_array)
        .filter(|mips| !mips.is_empty())
        .ok_or_else(|| Error::data("manifest layer carries no `mips` table"))?;

    let mut levels: Vec<Level> = Vec::with_capacity(raw.len());
    for (index, mip) in raw.iter().enumerate() {
        let level = field_u32(mip, "level")?;
        if level != index as u32 {
            return Err(Error::data(&format!(
                "manifest layer mip table must list levels 0.. in order, found level {level} at position {index}"
            )));
        }
        let size = field_u32(mip, "size")?;
        if size == 0 {
            return Err(Error::data(&format!(
                "manifest layer mip level {level} carries a zero-size level"
            )));
        }
        if let Some(previous) = levels.last()
            && size != previous.size / 2
        {
            return Err(Error::data(&format!(
                "manifest layer mip level {level} size {size} does not halve level {} size {}",
                level - 1,
                previous.size
            )));
        }
        let bytes = field_u32(mip, "bytes")?;
        let expected = size.checked_mul(size).ok_or_else(|| {
            Error::data(&format!(
                "manifest layer mip level {level} size {size} exceeds the supported range"
            ))
        })?;
        if bytes != expected {
            return Err(Error::data(&format!(
                "manifest layer mip level {level} declares {bytes} bytes for a {size}² level"
            )));
        }
        levels.push(Level { size, bytes });
    }

    Ok(levels)
}

/// Parses and structurally validates the layer's tile table: every grid slot
/// covered exactly once, coordinates inside the grid, and payload names
/// resolved beside the manifest.
fn tiles(
    entry: &Value,
    grid: u32,
    layer_name: &str,
    manifest_dir: Option<&Path>,
) -> Result<Vec<SceneTile>> {
    let raw = entry
        .get("tiles")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::data("manifest layer carries no `tiles` table"))?;

    let expected = match grid.checked_mul(grid) {
        Some(expected) => expected as usize,
        None => {
            return Err(Error::data(&format!(
                "layer `{layer_name}`: the {grid}×{grid} tile grid exceeds the supported range"
            )));
        }
    };
    if raw.len() != expected {
        return Err(Error::data(&format!(
            "layer `{layer_name}`: tile table has {} entries, the {grid}×{grid} grid needs {expected}",
            raw.len()
        )));
    }

    let base = manifest_dir.unwrap_or_else(|| Path::new("."));
    let mut tiles = Vec::with_capacity(raw.len());
    let mut covered = vec![false; expected];
    for tile in raw {
        let payload = tile
            .get("payload")
            .and_then(Value::as_str)
            .filter(|payload| !payload.is_empty())
            .ok_or_else(|| {
                Error::data(&format!(
                    "layer `{layer_name}`: a tile entry is missing its `payload` name"
                ))
            })?;
        let x = field_u32(tile, "x")?;
        let y = field_u32(tile, "y")?;
        if x >= grid || y >= grid {
            return Err(Error::data(&format!(
                "layer `{layer_name}`: tile `{payload}` sits at ({x}, {y}), outside the {grid}×{grid} grid"
            )));
        }
        let slot = (y * grid + x) as usize;
        if covered[slot] {
            return Err(Error::data(&format!(
                "layer `{layer_name}`: grid slot ({x}, {y}) is claimed by more than one tile"
            )));
        }
        covered[slot] = true;
        let kind = tile.get("kind").and_then(Value::as_str).unwrap_or_default();
        let stub = match kind {
            "mip0" => false,
            "stub" => true,
            other => {
                return Err(Error::data(&format!(
                    "layer `{layer_name}`: tile `{payload}` has unknown kind `{other}`"
                )));
            }
        };
        let sha256 = tile
            .get("payload_sha256")
            .and_then(Value::as_str)
            .filter(|sha| sha.len() == 64 && sha.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .ok_or_else(|| {
                Error::data(&format!(
                    "layer `{layer_name}`: tile `{payload}` is missing a valid `payload_sha256`"
                ))
            })?;

        tiles.push(SceneTile {
            payload: base.join(payload),
            x,
            y,
            stub,
            sha256: String::from(sha256),
        });
    }

    Ok(tiles)
}
