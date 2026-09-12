use crate::{SdfError, SdfResult, utilities::field_u32};

use serde_json::Value;
use std::path::{Path, PathBuf};

/// One grid tile of a data layer, resolved against the manifest's directory.
#[derive(Clone, Debug)]
pub struct SceneTile {
    pub payload: PathBuf,
    pub x: u32,
    pub y: u32,
    pub stub: bool,
    pub sha256: String,
}

/// Parses and structurally validates the layer's tile table: every grid slot
/// covered exactly once, coordinates inside the grid, and payload names
/// resolved beside the manifest.
pub(crate) fn tiles(
    entry: &Value,
    grid: u32,
    layer_name: &str,
    manifest_dir: Option<&Path>,
) -> SdfResult<Vec<SceneTile>> {
    let raw = entry
        .get("tiles")
        .and_then(Value::as_array)
        .ok_or_else(|| SdfError::data("manifest layer carries no `tiles` table"))?;

    let expected = match grid.checked_mul(grid) {
        Some(expected) => expected as usize,
        None => {
            return Err(SdfError::data(&format!(
                "layer `{layer_name}`: the {grid}×{grid} tile grid exceeds the supported range"
            )));
        }
    };
    if raw.len() != expected {
        return Err(SdfError::data(&format!(
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
                SdfError::data(&format!(
                    "layer `{layer_name}`: a tile entry is missing its `payload` name"
                ))
            })?;
        let x = field_u32(tile, "x")?;
        let y = field_u32(tile, "y")?;
        if x >= grid || y >= grid {
            return Err(SdfError::data(&format!(
                "layer `{layer_name}`: tile `{payload}` sits at ({x}, {y}), outside the {grid}×{grid} grid"
            )));
        }
        let slot = (y * grid + x) as usize;
        if covered[slot] {
            return Err(SdfError::data(&format!(
                "layer `{layer_name}`: grid slot ({x}, {y}) is claimed by more than one tile"
            )));
        }
        covered[slot] = true;
        let kind = tile.get("kind").and_then(Value::as_str).unwrap_or_default();
        let stub = match kind {
            "mip0" => false,
            "stub" => true,
            other => {
                return Err(SdfError::data(&format!(
                    "layer `{layer_name}`: tile `{payload}` has unknown kind `{other}`"
                )));
            }
        };
        let sha256 = tile
            .get("payload_sha256")
            .and_then(Value::as_str)
            .filter(|sha| sha.len() == 64 && sha.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .ok_or_else(|| {
                SdfError::data(&format!(
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
