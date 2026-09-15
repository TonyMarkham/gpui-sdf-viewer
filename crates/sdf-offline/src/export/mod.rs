pub(crate) mod field_map;
pub(crate) mod kind;
pub(crate) mod layer;
pub(crate) mod manifest;
pub(crate) mod mip;
pub(crate) mod tile;
pub(crate) mod tile_payload;

// ---------------------------------------------------------------------------------------------- //

use crate::{
    ExportLayer, ExportManifest, ExportMip, ExportTile, FieldMap, OfflineError, OfflineResult,
    Progress,
    constants::DISPLAY_SEPARATOR,
    field::grid::{TILE_EDGE, TILE_GRID_EDGE},
    utilities::sha256_hex,
};

// ---------------------------------------------------------------------------------------------- //

use soul_attributes::soul;
use std::path::PathBuf;

// ---------------------------------------------------------------------------------------------- //

pub(crate) struct Export {
    dir: PathBuf,
    manifest_name: String,
    manifest: ExportManifest,
}

impl Export {
    pub(crate) fn new(
        dir: PathBuf,
        manifest_name: String,
        source_manifest_sha256: String,
        routes: &[(String, String)],
        fields: &FieldMap,
    ) -> OfflineResult<Export> {
        let mut layers = Vec::with_capacity(routes.len());
        for (name, prefix) in routes {
            if layers.iter().any(|layer: &ExportLayer| &layer.name == name) {
                return Err(OfflineError::manifest(format!(
                    "duplicate export layer name `{name}`"
                )));
            }
            let Some((mip_count, tiles)) = fields.get(prefix) else {
                return Err(OfflineError::manifest(format!(
                    "layer `{name}` maps to prefix `{prefix}`, which has no converted field"
                )));
            };

            layers.push(ExportLayer {
                name: name.clone(),
                tile_prefix: String::from(prefix.as_str()),
                size: TILE_GRID_EDGE * TILE_EDGE,
                source_manifest_sha256: source_manifest_sha256.clone(),
                mips: (0..*mip_count)
                    .map(|mip| ExportMip {
                        level: mip,
                        size: mip_size(mip),
                        bytes: mip_bytes(mip),
                    })
                    .collect(),
                tiles: tiles
                    .iter()
                    .map(|tile| ExportTile {
                        path: tile.path.clone(),
                        payload: tile.payload.clone(),
                        x: tile.x,
                        y: tile.y,
                        kind: tile.kind,
                        source_sha256: tile.source_sha256.clone(),
                        payload_sha256: sha256_hex(&tile.bytes),
                    })
                    .collect(),
            });
        }

        Ok(Export {
            dir,
            manifest_name,
            manifest: ExportManifest::new(layers),
        })
    }

    #[soul(id = "concept.game-data-pipeline", step = "export write")]
    pub(crate) fn write(
        &self,
        fields: &FieldMap,
        progress: &mut dyn FnMut(Progress),
    ) -> OfflineResult<()> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| OfflineError::io(format!("create `{}`: {e}", self.dir.display())))?;

        for (prefix, (_, tiles)) in fields {
            let total: usize = tiles.iter().map(|tile| tile.bytes.len()).sum();
            for tile in tiles {
                let path = self.dir.join(&tile.payload);
                std::fs::write(&path, &tile.bytes)
                    .map_err(|e| OfflineError::io(format!("write `{}`: {e}", path.display())))?;
            }
            progress(Progress::milestone(
                STEP,
                format!("{prefix}: {} tile payload(s), {total} bytes", tiles.len()),
            ));
        }

        let manifest_path = self.dir.join(&self.manifest_name);
        let json = serde_json::to_string_pretty(&self.manifest).map_err(|e| {
            OfflineError::json(format!(
                "serialize export manifest `{}`: {e}",
                manifest_path.display()
            ))
        })?;
        std::fs::write(&manifest_path, json)
            .map_err(|e| OfflineError::io(format!("write `{}`: {e}", manifest_path.display())))?;
        progress(Progress::milestone(
            STEP,
            format!(
                "wrote `{}`: {} layers",
                display_path(&manifest_path),
                self.manifest.layers.len()
            ),
        ));

        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------- //

const STEP: &str = "export";

fn mip_size(mip: u32) -> u32 {
    TILE_EDGE >> mip
}

fn mip_bytes(mip: u32) -> u32 {
    let edge = TILE_EDGE >> mip;
    edge * edge
}

fn display_path(path: &std::path::Path) -> String {
    path.display()
        .to_string()
        .replace(std::path::MAIN_SEPARATOR, DISPLAY_SEPARATOR)
}

// ---------------------------------------------------------------------------------------------- //

#[cfg(test)]
mod tests {
    use crate::export::manifest::{FORMAT_TAG, FORMAT_VERSION};
    use crate::{
        Export as SdfExport, ExportManifest, FieldMap, OfflineError, OfflineResult, TileKind,
        TilePayload, field::grid, utilities::sha256_hex,
    };

    use std::collections::BTreeMap;
    use std::path::Path;

    // ------------------------------------------------------------------------------------------ //

    const TEST_DIR_NAME: &str = "sdf-offline-export";
    const TEST_MANIFEST_NAME: &str = "manifest.json";
    const TEST_LAYER_NAME: &str = "coast";
    const TEST_EXTRACT_PATH: &str = "ui/cd_worldmap_land_sdf_32768x32768_0_0.dds";
    const TEST_STUB_EXTRACT_PATH: &str = "ui/cd_worldmap_land_sdf_32768x32768_0_1.dds";
    const TEST_PAYLOAD_NAME: &str = "cd_worldmap_land_sdf_32768x32768_0_0.r8";
    const TEST_TILE_PREFIX: &str = "cd_worldmap_land_sdf";
    const TEST_SOURCE_SHA256: &str = "source-sha256";
    const TEST_DDS_SHA256: &str = "dds-sha256";
    const TEST_MIP0_BYTE: u8 = 0xA7;
    const TEST_STUB_BYTE: u8 = 0x55;

    #[test]
    fn round_trip_detects_corrupted_payload() -> OfflineResult<()> {
        let dir = std::env::temp_dir().join(format!("{TEST_DIR_NAME}-{}", std::process::id()));
        std::fs::create_dir_all(&dir)
            .map_err(|e| OfflineError::io(format!("create `{}`: {e}", dir.display())))?;

        let result = export_and_corrupt(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    fn export_and_corrupt(dir: &Path) -> OfflineResult<()> {
        let mip0 = vec![
            TEST_MIP0_BYTE;
            grid::chain_len(grid::TILE_EDGE, grid::mip_count_max(grid::TILE_EDGE))
        ];
        let stub = vec![
            TEST_STUB_BYTE;
            grid::chain_len(grid::STUB_EDGE, grid::mip_count_max(grid::STUB_EDGE))
        ];
        let tiles = vec![
            TilePayload::convert(
                TEST_EXTRACT_PATH,
                TEST_DDS_SHA256,
                0,
                0,
                TileKind::Mip0,
                mip0.clone(),
            ),
            TilePayload::convert(
                TEST_STUB_EXTRACT_PATH,
                TEST_DDS_SHA256,
                0,
                1,
                TileKind::Stub,
                stub.clone(),
            ),
        ];
        let mut fields: FieldMap = BTreeMap::new();
        fields.insert(
            String::from(TEST_TILE_PREFIX),
            (grid::mip_count_max(grid::TILE_EDGE), tiles),
        );
        let routes = vec![(
            String::from(TEST_LAYER_NAME),
            String::from(TEST_TILE_PREFIX),
        )];

        let export = SdfExport::new(
            dir.to_path_buf(),
            String::from(TEST_MANIFEST_NAME),
            String::from(TEST_SOURCE_SHA256),
            &routes,
            &fields,
        )?;
        export.write(&fields, &mut |_| {})?;

        let manifest = ExportManifest::load(&dir.join(TEST_MANIFEST_NAME))?;
        ensure(
            manifest.format == FORMAT_TAG,
            "manifest format tag mismatch",
        )?;
        ensure(
            manifest.version == FORMAT_VERSION,
            "manifest format version mismatch",
        )?;
        let entry = manifest
            .layers
            .first()
            .ok_or_else(|| OfflineError::manifest(String::from("export manifest has no layers")))?;
        ensure(
            entry.name == TEST_LAYER_NAME,
            "manifest layer name mismatch",
        )?;
        ensure(
            entry.size == grid::TILE_GRID_EDGE * grid::TILE_EDGE,
            "manifest layer size mismatch",
        )?;
        ensure(
            entry.mips[0].size == grid::TILE_EDGE,
            "manifest mip size mismatch",
        )?;
        ensure(
            entry.mips.len() == grid::mip_count_max(grid::TILE_EDGE) as usize,
            "manifest mip count mismatch",
        )?;
        ensure(entry.mips[0].level == 0, "manifest first mip mismatch")?;
        let mip_sum: usize = entry.mips.iter().map(|mip| mip.bytes as usize).sum();
        ensure(
            mip_sum == mip0.len(),
            "manifest mip sizes do not sum to the mip0 payload",
        )?;
        ensure(entry.tiles.len() == 2, "manifest tile count mismatch")?;
        let tile0 = entry
            .tiles
            .first()
            .ok_or_else(|| OfflineError::manifest(String::from("manifest layer has no tiles")))?;
        ensure(tile0.x == 0 && tile0.y == 0, "manifest tile slot mismatch")?;
        ensure(
            tile0.payload == TEST_PAYLOAD_NAME,
            "manifest tile payload name mismatch",
        )?;
        ensure(tile0.kind == TileKind::Mip0, "manifest tile kind mismatch")?;
        ensure(
            tile0.source_sha256 == TEST_DDS_SHA256,
            "manifest tile source sha256 mismatch",
        )?;
        ensure(
            tile0.payload_sha256 == sha256_hex(&mip0),
            "manifest tile payload sha256 mismatch",
        )?;

        let disk = tile0.reload(dir)?;
        ensure(disk == mip0, "round-trip tile payload mismatch")?;

        let stub_tile = entry.tiles.get(1).ok_or_else(|| {
            OfflineError::manifest(String::from("manifest layer is missing the stub tile"))
        })?;
        ensure(
            stub_tile.kind == TileKind::Stub,
            "manifest stub tile kind mismatch",
        )?;
        ensure(
            stub_tile.reload(dir)? == stub,
            "round-trip stub payload mismatch",
        )?;

        let payload_path = dir.join(&tile0.payload);
        corrupt(&payload_path)?;

        match tile0.reload(dir) {
            Err(_) => Ok(()),
            Ok(_) => Err(OfflineError::manifest(format!(
                "corrupted tile payload `{}` passed the round-trip check",
                payload_path.display()
            ))),
        }
    }

    fn corrupt(path: &Path) -> OfflineResult<()> {
        let mut data = std::fs::read(path)
            .map_err(|e| OfflineError::io(format!("read `{}`: {e}", path.display())))?;
        if let Some(byte) = data.first_mut() {
            *byte = byte.wrapping_add(1);
        }
        std::fs::write(path, data)
            .map_err(|e| OfflineError::io(format!("write `{}`: {e}", path.display())))
    }

    fn ensure(condition: bool, message: &str) -> OfflineResult<()> {
        if condition {
            Ok(())
        } else {
            Err(OfflineError::manifest(String::from(message)))
        }
    }
}
