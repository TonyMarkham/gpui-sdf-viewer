use crate::{
    Config, Export as SdfExport, ExportManifest, FieldMap, OfflineError, OfflineResult, Progress,
    TileKind, TilePayload, field::grid, utilities::sha256_hex,
};

#[cfg(test)]
mod mip0_spec;
#[cfg(test)]
mod stub_spec;

// ---------------------------------------------------------------------------------------------- //

// ---------------------------------------------------------------------------------------------- //

use soul_attributes::soul;
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------------------------- //

const STEP: &str = "field";

#[soul(id = "concept.game-data-pipeline", step = "field: dds → r8 payloads")]
pub fn run(config: &Config, progress: &mut dyn FnMut(Progress)) -> OfflineResult<()> {
    config.check()?;
    let routes = export_routes(config)?;
    let fields = convert_fields(config, &routes, progress)?;

    let export = SdfExport::new(
        config.paths()?.sdf_dir,
        config.sdf.export.manifest.clone(),
        source_manifest_sha256(config)?,
        &routes,
        &fields,
    )?;
    export.write(&fields, progress)?;

    round_trip(config, &fields, progress)?;

    Ok(())
}

// ---------------------------------------------------------------------------------------------- //

pub(crate) fn export_routes(config: &Config) -> OfflineResult<Vec<(String, String)>> {
    let mut routes: Vec<(String, String)> = config
        .sdf
        .tiles
        .iter()
        .map(|(name, prefix)| (name.clone(), prefix.clone()))
        .collect();

    for (name, prefix) in &config.sdf.export.extra {
        if config.sdf.tiles.contains_key(name) {
            return Err(OfflineError::toml(format!(
                "`[sdf.export.extra]` name `{name}` collides with a `[sdf.tiles]` key"
            )));
        }
        routes.push((name.clone(), prefix.clone()));
    }

    Ok(routes)
}

fn convert_fields(
    config: &Config,
    routes: &[(String, String)],
    progress: &mut dyn FnMut(Progress),
) -> OfflineResult<FieldMap> {
    let mut fields = BTreeMap::new();
    for (_, prefix) in routes {
        if fields.contains_key(prefix) {
            continue;
        }
        let converted = convert_field(config, prefix, progress)?;
        fields.insert(prefix.clone(), converted);
    }

    Ok(fields)
}

fn convert_field(
    config: &Config,
    prefix: &str,
    progress: &mut dyn FnMut(Progress),
) -> OfflineResult<(u32, Vec<TilePayload>)> {
    let records = grid::layer_records(config, prefix)?;

    let mut seen = vec![false; (grid::TILE_GRID_EDGE * grid::TILE_GRID_EDGE) as usize];
    let mut tiles = Vec::new();
    for record in &records {
        let Some((x, y)) = grid::tile_index(&record.path, prefix)? else {
            progress(Progress::layer(
                STEP,
                prefix,
                format!("ignoring non-tiled `{}`", record.path),
            ));
            continue;
        };
        seen[(y * grid::TILE_GRID_EDGE + x) as usize] = true;
        tiles.push((x, y, record));
    }
    if tiles.is_empty() {
        return Err(OfflineError::manifest(format!(
            "manifest has no `{prefix}*{}` tiles; run `extract` first",
            grid::DDS_EXTENSION
        )));
    }
    grid::require_complete(&seen, prefix)?;

    let mut chain: Option<u32> = None;
    let mut converted = Vec::with_capacity(tiles.len());
    for (x, y, record) in &tiles {
        let path = grid::tile_path(config, record)?;
        let (data, info) = grid::load_tile(config, record)?;
        let kind = match (info.width, info.height) {
            (grid::TILE_EDGE, grid::TILE_EDGE) => TileKind::Mip0,
            (grid::STUB_EDGE, grid::STUB_EDGE) => TileKind::Stub,
            (width, height) => {
                return Err(OfflineError::dds(format!(
                    "`{}` tile is {width}x{height}; expected {}² mip0 or the {}² empty stub",
                    path.display(),
                    grid::TILE_EDGE,
                    grid::STUB_EDGE
                )));
            }
        };

        let (edge, expected) = match kind {
            TileKind::Mip0 => {
                match chain {
                    None => {
                        if info.mip_count == 0
                            || info.mip_count > grid::mip_count_max(grid::TILE_EDGE)
                        {
                            return Err(OfflineError::manifest(format!(
                                "`{}` declares {} mips; 1..={} is the valid range for {}² tiles",
                                path.display(),
                                info.mip_count,
                                grid::mip_count_max(grid::TILE_EDGE),
                                grid::TILE_EDGE
                            )));
                        }
                        chain = Some(info.mip_count);
                    }
                    Some(expected_count) if expected_count != info.mip_count => {
                        return Err(OfflineError::manifest(format!(
                            "layer `{prefix}`: `{}` declares {} mips, {expected_count} expected",
                            path.display(),
                            info.mip_count
                        )));
                    }
                    Some(_) => {}
                }
                (
                    grid::TILE_EDGE,
                    grid::chain_len(grid::TILE_EDGE, info.mip_count),
                )
            }
            TileKind::Stub => {
                if info.mip_count == 0 || info.mip_count > grid::mip_count_max(grid::STUB_EDGE) {
                    return Err(OfflineError::manifest(format!(
                        "`{}` declares {} mips; 1..={} is the valid range for {}² stubs",
                        path.display(),
                        info.mip_count,
                        grid::mip_count_max(grid::STUB_EDGE),
                        grid::STUB_EDGE
                    )));
                }
                (
                    grid::STUB_EDGE,
                    grid::chain_len(grid::STUB_EDGE, info.mip_count),
                )
            }
        };

        let payload = data.get(config.extract.dds_header..).ok_or_else(|| {
            OfflineError::dds(format!(
                "`{}` truncated: {} bytes, header is {}",
                path.display(),
                data.len(),
                config.extract.dds_header
            ))
        })?;
        if payload.len() != expected {
            return Err(OfflineError::dds(format!(
                "`{}` carries {} payload bytes; the declared {}-mip {}² chain is {expected}",
                path.display(),
                payload.len(),
                info.mip_count,
                edge
            )));
        }
        if kind == TileKind::Stub {
            grid::stub_constant(&data, config.extract.dds_header, &path)?;
        }

        converted.push(TilePayload::convert(
            &record.path,
            &record.sha256,
            *x,
            *y,
            kind,
            payload.to_vec(),
        ));

        if converted.len() % crate::run::PROGRESS_EVERY == 0 {
            progress(Progress::batch(
                STEP,
                prefix,
                format!("converted {} tiles…", converted.len()),
            ));
        }
    }

    let mip_count = chain.ok_or_else(|| {
        OfflineError::manifest(format!(
            "layer `{prefix}` has no {}² mip0 tiles; the chain cannot be read from stubs",
            grid::TILE_EDGE
        ))
    })?;

    progress(Progress::layer(
        STEP,
        prefix,
        format!("layer converted: {} tiles", converted.len()),
    ));

    Ok((mip_count, converted))
}

#[soul(id = "concept.game-data-pipeline", step = "export round trip")]
fn round_trip(
    config: &Config,
    fields: &FieldMap,
    progress: &mut dyn FnMut(Progress),
) -> OfflineResult<()> {
    let manifest_path = config.paths()?.sdf_dir.join(&config.sdf.export.manifest);
    let manifest = ExportManifest::load(&manifest_path)?;
    let dir = manifest_path.parent().ok_or_else(|| {
        OfflineError::manifest(format!(
            "export manifest `{}` has no parent directory",
            manifest_path.display()
        ))
    })?;

    let mut tiles_checked = 0;
    for entry in &manifest.layers {
        let Some((mip_count, payloads)) = fields.get(&entry.tile_prefix) else {
            return Err(OfflineError::manifest(format!(
                "manifest layer `{}` maps to prefix `{}`, which has no converted field",
                entry.name, entry.tile_prefix
            )));
        };
        if entry.size != grid::TILE_GRID_EDGE * grid::TILE_EDGE {
            return Err(OfflineError::manifest(format!(
                "manifest layer `{}` size {} is not the {}×{} grid",
                entry.name,
                entry.size,
                grid::TILE_GRID_EDGE,
                grid::TILE_EDGE
            )));
        }
        for mip in &entry.mips {
            if mip.size != grid::TILE_EDGE >> mip.level {
                return Err(OfflineError::manifest(format!(
                    "manifest layer `{}` mip {}: size {} is not {} >> {}",
                    entry.name,
                    mip.level,
                    mip.size,
                    grid::TILE_EDGE,
                    mip.level
                )));
            }
        }
        if entry.mips.len() != *mip_count as usize {
            return Err(OfflineError::manifest(format!(
                "manifest layer `{}` declares {} mips, {mip_count} validated",
                entry.name,
                entry.mips.len()
            )));
        }
        if entry.tiles.len() != payloads.len() {
            return Err(OfflineError::manifest(format!(
                "manifest layer `{}` lists {} tile(s), {} converted",
                entry.name,
                entry.tiles.len(),
                payloads.len()
            )));
        }

        let mip_bytes: usize = entry.mips.iter().map(|mip| mip.bytes as usize).sum();
        for (tile, payload) in entry.tiles.iter().zip(payloads) {
            if tile.payload != payload.payload {
                return Err(OfflineError::manifest(format!(
                    "manifest layer `{}`: tile `{}` does not match converted `{}`",
                    entry.name, tile.payload, payload.payload
                )));
            }
            if tile.kind == TileKind::Mip0 && payload.bytes.len() != mip_bytes {
                return Err(OfflineError::manifest(format!(
                    "manifest layer `{}`: `{}` is {} bytes, the mip sizes sum to {mip_bytes}",
                    entry.name,
                    tile.payload,
                    payload.bytes.len()
                )));
            }

            let disk = tile.reload(dir)?;
            if disk.as_slice() != payload.bytes.as_slice() {
                let offset = disk
                    .iter()
                    .zip(payload.bytes.iter())
                    .position(|(disk_byte, memory_byte)| disk_byte != memory_byte);
                match offset {
                    Some(offset) => {
                        return Err(OfflineError::manifest(format!(
                            "round-trip `{}` tile `{}`: byte {offset} is {} on disk, {} in memory",
                            entry.name, tile.payload, disk[offset], payload.bytes[offset]
                        )));
                    }
                    None => {
                        return Err(OfflineError::manifest(format!(
                            "round-trip `{}` tile `{}`: payloads differ in length",
                            entry.name, tile.payload
                        )));
                    }
                }
            }
            tiles_checked += 1;
        }
        progress(Progress::layer(
            STEP,
            &entry.name,
            format!(
                "{}: {} tile(s) round-trip ok",
                entry.name,
                entry.tiles.len()
            ),
        ));
    }

    progress(Progress::milestone(
        STEP,
        format!(
            "round-trip: {tiles_checked} tile payload(s) verified across {} layer(s)",
            manifest.layers.len()
        ),
    ));

    Ok(())
}

fn source_manifest_sha256(config: &Config) -> OfflineResult<String> {
    let path = grid::extract_manifest_path(config)?;
    let raw = std::fs::read(&path)
        .map_err(|e| OfflineError::io(format!("read `{}`: {e}", path.display())))?;
    Ok(sha256_hex(&raw))
}

// ---------------------------------------------------------------------------------------------- //

#[cfg(test)]
mod tests {
    use super::mip0_spec::Mip0Spec;
    use super::stub_spec::StubSpec;
    use super::{convert_field, export_routes, round_trip};

    use crate::{
        Config, Export as SdfExport, ExportManifest, FieldMap, OfflineError, OfflineResult,
        TileKind, TilePayload,
        config::{Composite, Extract, Paths},
        config::{export::Export as ExportConfig, sdf::Sdf},
        field::grid,
        utilities::sha256_hex,
    };

    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    // ------------------------------------------------------------------------------------------ //

    const TEST_DIR_NAME: &str = "sdf-offline-field-convert";
    const TEST_PREFIX: &str = "cd_worldmap_test_sdf";
    const TEST_EXTRACT_DIR: &str = "extract";
    const TEST_MANIFEST_NAME: &str = "manifest.json";
    const TEST_PAZ_FILE: &str = "0.paz";
    const TEST_PAYLOAD_EXTENSION: &str = ".r8";
    const TEST_HEADER_SIZE: usize = 128;
    const TEST_HEADER_SIZE_OFFSET: usize = 4;
    const TEST_HEIGHT_OFFSET: usize = 12;
    const TEST_WIDTH_OFFSET: usize = 16;
    const TEST_MIP_COUNT_OFFSET: usize = 28;
    const TEST_BIT_COUNT_OFFSET: usize = 88;
    const TEST_R8_BIT_COUNT: u32 = 8;
    const TEST_MIP_COUNT: u32 = 4;
    const TEST_MIP0_VALUE: u8 = 0xA7;
    const TEST_STUB_VALUE: u8 = 0x55;
    const TEST_OTHER_VALUE: u8 = 0x2A;
    const TEST_EXPORT_DIR: &str = "fields";
    const TEST_SOURCE_SHA256: &str = "source-sha256";
    const TEST_LAYER_NAME: &str = "coast";
    const TEST_EXTRACT_PATH: &str = "cd_worldmap_test_sdf_0_0.dds";
    const TEST_STUB_EXTRACT_PATH: &str = "cd_worldmap_test_sdf_0_1.dds";
    const TEST_WRONG_PAYLOAD_NAME: &str = "cd_worldmap_test_sdf_9_9.r8";
    const TEST_WRONG_LAYER_SIZE: u32 = 4096;
    const TEST_WRONG_MIP_SIZE: u32 = 256;
    const TEST_TILE_COUNT: usize = 2;
    const TEST_SCENARIO_SUCCESS: &str = "success";
    const TEST_SCENARIO_MIP_COUNT_MISMATCH: &str = "mip-count-mismatch";
    const TEST_SCENARIO_STUB_MIP_RANGE: &str = "stub-mip-range";
    const TEST_SCENARIO_PAYLOAD_LENGTH: &str = "payload-length";
    const TEST_SCENARIO_ALL_STUBS: &str = "all-stubs";
    const TEST_SCENARIO_NON_CONSTANT_STUB: &str = "non-constant-stub";
    const TEST_SCENARIO_ROUTE_COLLISION: &str = "route-collision";
    const TEST_SCENARIO_ROUND_TRIP_OK: &str = "round-trip-ok";
    const TEST_SCENARIO_ROUND_TRIP_LAYER_SIZE: &str = "round-trip-layer-size";
    const TEST_SCENARIO_ROUND_TRIP_MIP_SIZE: &str = "round-trip-mip-size";
    const TEST_SCENARIO_ROUND_TRIP_MIP_COUNT: &str = "round-trip-mip-count";
    const TEST_SCENARIO_ROUND_TRIP_TILE_COUNT: &str = "round-trip-tile-count";
    const TEST_SCENARIO_ROUND_TRIP_PAYLOAD_NAME: &str = "round-trip-payload-name";

    // ------------------------------------------------------------------------------------------ //

    #[test]
    fn convert_field_reads_the_chain_from_mip0_tiles() -> OfflineResult<()> {
        scenario(
            TEST_SCENARIO_SUCCESS,
            &[Mip0Spec {
                x: 0,
                y: 0,
                mip_count: TEST_MIP_COUNT,
                payload_len: None,
            }],
            &[],
            assert_converted_chain,
        )
    }

    #[test]
    fn convert_field_rejects_inconsistent_mip_counts() -> OfflineResult<()> {
        scenario(
            TEST_SCENARIO_MIP_COUNT_MISMATCH,
            &[
                Mip0Spec {
                    x: 0,
                    y: 0,
                    mip_count: TEST_MIP_COUNT,
                    payload_len: None,
                },
                Mip0Spec {
                    x: 1,
                    y: 0,
                    mip_count: TEST_MIP_COUNT - 1,
                    payload_len: None,
                },
            ],
            &[],
            assert_mip_count_mismatch,
        )
    }

    #[test]
    fn convert_field_rejects_stub_mip_counts_out_of_range() -> OfflineResult<()> {
        scenario(
            TEST_SCENARIO_STUB_MIP_RANGE,
            &[Mip0Spec {
                x: 0,
                y: 0,
                mip_count: TEST_MIP_COUNT,
                payload_len: None,
            }],
            &[StubSpec {
                x: 0,
                y: 1,
                mip_count: TEST_MIP_COUNT + 1,
                constant: true,
            }],
            assert_stub_mip_range,
        )
    }

    #[test]
    fn convert_field_rejects_payload_lengths_outside_the_chain() -> OfflineResult<()> {
        scenario(
            TEST_SCENARIO_PAYLOAD_LENGTH,
            &[Mip0Spec {
                x: 0,
                y: 0,
                mip_count: TEST_MIP_COUNT,
                payload_len: Some(grid::chain_len(grid::TILE_EDGE, TEST_MIP_COUNT) - 1),
            }],
            &[],
            assert_payload_length,
        )
    }

    #[test]
    fn convert_field_rejects_layers_without_mip0_tiles() -> OfflineResult<()> {
        scenario(TEST_SCENARIO_ALL_STUBS, &[], &[], assert_all_stubs)
    }

    #[test]
    fn convert_field_rejects_non_constant_stubs() -> OfflineResult<()> {
        scenario(
            TEST_SCENARIO_NON_CONSTANT_STUB,
            &[Mip0Spec {
                x: 0,
                y: 0,
                mip_count: TEST_MIP_COUNT,
                payload_len: None,
            }],
            &[StubSpec {
                x: 0,
                y: 1,
                mip_count: TEST_MIP_COUNT,
                constant: false,
            }],
            assert_non_constant_stub,
        )
    }

    #[test]
    fn export_routes_rejects_a_name_colliding_with_a_tiles_key() -> OfflineResult<()> {
        let mut config = base_config(&scenario_dir(TEST_SCENARIO_ROUTE_COLLISION));
        config
            .sdf
            .tiles
            .insert(String::from(TEST_LAYER_NAME), String::from(TEST_PREFIX));
        config
            .sdf
            .export
            .extra
            .insert(String::from(TEST_LAYER_NAME), String::from(TEST_PREFIX));

        match export_routes(&config) {
            Err(e) => ensure(
                format!("{e}").contains("collides with a `[sdf.tiles]` key"),
                "the colliding export route name produced an unexpected error",
            ),
            Ok(_) => Err(OfflineError::manifest(String::from(
                "a colliding export route name passed export_routes",
            ))),
        }
    }

    #[test]
    fn round_trip_verifies_a_well_formed_export() -> OfflineResult<()> {
        let dir = scenario_dir(TEST_SCENARIO_ROUND_TRIP_OK);
        let result = round_trip_fixture(&dir)
            .and_then(|(config, fields)| round_trip(&config, &fields, &mut |_| {}));
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    #[test]
    fn round_trip_rejects_a_layer_size_outside_the_grid() -> OfflineResult<()> {
        round_trip_scenario(
            TEST_SCENARIO_ROUND_TRIP_LAYER_SIZE,
            mutate_layer_size,
            &format!(
                "size {} is not the {}×{} grid",
                TEST_WRONG_LAYER_SIZE,
                grid::TILE_GRID_EDGE,
                grid::TILE_EDGE
            ),
        )
    }

    #[test]
    fn round_trip_rejects_a_mip_size_outside_the_chain() -> OfflineResult<()> {
        round_trip_scenario(
            TEST_SCENARIO_ROUND_TRIP_MIP_SIZE,
            mutate_mip_size,
            &format!(
                "mip {}: size {} is not {} >> {}",
                0,
                TEST_WRONG_MIP_SIZE,
                grid::TILE_EDGE,
                0
            ),
        )
    }

    #[test]
    fn round_trip_rejects_a_mip_count_outside_the_validated_chain() -> OfflineResult<()> {
        round_trip_scenario(
            TEST_SCENARIO_ROUND_TRIP_MIP_COUNT,
            mutate_mip_count,
            &format!(
                "declares {} mips, {TEST_MIP_COUNT} validated",
                TEST_MIP_COUNT - 1
            ),
        )
    }

    #[test]
    fn round_trip_rejects_a_tile_count_outside_the_converted_grid() -> OfflineResult<()> {
        round_trip_scenario(
            TEST_SCENARIO_ROUND_TRIP_TILE_COUNT,
            mutate_tile_count,
            &format!(
                "lists {} tile(s), {} converted",
                TEST_TILE_COUNT - 1,
                TEST_TILE_COUNT
            ),
        )
    }

    #[test]
    fn round_trip_rejects_a_tile_outside_the_converted_payloads() -> OfflineResult<()> {
        round_trip_scenario(
            TEST_SCENARIO_ROUND_TRIP_PAYLOAD_NAME,
            mutate_payload_name,
            &format!("`{TEST_WRONG_PAYLOAD_NAME}` does not match converted"),
        )
    }

    // ------------------------------------------------------------------------------------------ //

    fn assert_converted_chain(config: &Config) -> OfflineResult<()> {
        let (mip_count, payloads) = convert_field(config, TEST_PREFIX, &mut |_| {})?;
        ensure(
            mip_count == TEST_MIP_COUNT,
            "the chain was not read from the mip0 tile",
        )?;
        ensure(
            payloads.len() == (grid::TILE_GRID_EDGE * grid::TILE_GRID_EDGE) as usize,
            "the full grid was not converted",
        )?;

        let mip0 = payloads
            .iter()
            .find(|tile| tile.kind == TileKind::Mip0)
            .ok_or_else(|| OfflineError::manifest(String::from("no mip0 payload was converted")))?;
        ensure(
            mip0.bytes.len() == grid::chain_len(grid::TILE_EDGE, TEST_MIP_COUNT),
            "the mip0 payload is not the declared chain",
        )?;
        ensure(
            mip0.bytes.iter().all(|&byte| byte == TEST_MIP0_VALUE),
            "the mip0 payload bytes were altered",
        )?;
        ensure(
            mip0.payload == format!("{TEST_PREFIX}_0_0{TEST_PAYLOAD_EXTENSION}"),
            "the payload name does not follow the tile name",
        )?;

        let stubs: Vec<&TilePayload> = payloads
            .iter()
            .filter(|tile| tile.kind == TileKind::Stub)
            .collect();
        ensure(
            stubs.len() == (grid::TILE_GRID_EDGE * grid::TILE_GRID_EDGE - 1) as usize,
            "the stub count does not match the grid",
        )?;
        let stub = stubs
            .first()
            .ok_or_else(|| OfflineError::manifest(String::from("no stub payload was converted")))?;
        ensure(
            stub.bytes.len() == grid::chain_len(grid::STUB_EDGE, TEST_MIP_COUNT),
            "the stub payload is not the stub chain",
        )?;
        ensure(
            stub.bytes.iter().all(|&byte| byte == TEST_STUB_VALUE),
            "the stub payload bytes were altered",
        )
    }

    fn assert_mip_count_mismatch(config: &Config) -> OfflineResult<()> {
        match convert_field(config, TEST_PREFIX, &mut |_| {}) {
            Err(e) => ensure(
                format!("{e}").contains(&format!(
                    "declares {} mips, {TEST_MIP_COUNT} expected",
                    TEST_MIP_COUNT - 1
                )),
                "the mip-count mismatch produced an unexpected error",
            ),
            Ok(_) => Err(OfflineError::manifest(String::from(
                "inconsistent mip counts passed convert_field",
            ))),
        }
    }

    fn assert_stub_mip_range(config: &Config) -> OfflineResult<()> {
        match convert_field(config, TEST_PREFIX, &mut |_| {}) {
            Err(e) => ensure(
                format!("{e}").contains(&format!(
                    "1..={} is the valid range for {}² stubs",
                    grid::mip_count_max(grid::STUB_EDGE),
                    grid::STUB_EDGE
                )),
                "the out-of-range stub mip count produced an unexpected error",
            ),
            Ok(_) => Err(OfflineError::manifest(String::from(
                "an out-of-range stub mip count passed convert_field",
            ))),
        }
    }

    fn assert_payload_length(config: &Config) -> OfflineResult<()> {
        match convert_field(config, TEST_PREFIX, &mut |_| {}) {
            Err(e) => ensure(
                format!("{e}").contains(&format!(
                    "carries {} payload bytes; the declared {TEST_MIP_COUNT}-mip {}² chain is {}",
                    grid::chain_len(grid::TILE_EDGE, TEST_MIP_COUNT) - 1,
                    grid::TILE_EDGE,
                    grid::chain_len(grid::TILE_EDGE, TEST_MIP_COUNT)
                )),
                "the payload length mismatch produced an unexpected error",
            ),
            Ok(_) => Err(OfflineError::manifest(String::from(
                "a payload length mismatch passed convert_field",
            ))),
        }
    }

    fn assert_all_stubs(config: &Config) -> OfflineResult<()> {
        match convert_field(config, TEST_PREFIX, &mut |_| {}) {
            Err(e) => ensure(
                format!("{e}").contains(&format!("has no {}² mip0 tiles", grid::TILE_EDGE)),
                "the all-stub layer produced an unexpected error",
            ),
            Ok(_) => Err(OfflineError::manifest(String::from(
                "a layer without mip0 tiles passed convert_field",
            ))),
        }
    }

    fn assert_non_constant_stub(config: &Config) -> OfflineResult<()> {
        match convert_field(config, TEST_PREFIX, &mut |_| {}) {
            Err(e) => ensure(
                format!("{e}").contains("stub payload is not constant"),
                "the non-constant stub produced an unexpected error",
            ),
            Ok(_) => Err(OfflineError::manifest(String::from(
                "a non-constant stub passed convert_field",
            ))),
        }
    }

    // ------------------------------------------------------------------------------------------ //

    fn scenario(
        name: &str,
        mip0: &[Mip0Spec],
        stubs: &[StubSpec],
        check: fn(&Config) -> OfflineResult<()>,
    ) -> OfflineResult<()> {
        let dir = scenario_dir(name);
        let result = scaffold(&dir, mip0, stubs).and_then(|config| check(&config));
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    fn scenario_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{TEST_DIR_NAME}-{name}-{}", std::process::id()))
    }

    fn scaffold(dir: &Path, mip0: &[Mip0Spec], stubs: &[StubSpec]) -> OfflineResult<Config> {
        let mut config = base_config(dir);
        config.extract = Extract {
            dir: String::from(TEST_EXTRACT_DIR),
            manifest: String::from(TEST_MANIFEST_NAME),
            packs: Vec::new(),
            includes: Vec::new(),
            dds_header: TEST_HEADER_SIZE,
        };

        let extract_dir = dir.join(TEST_EXTRACT_DIR);
        std::fs::create_dir_all(&extract_dir)
            .map_err(|e| OfflineError::io(format!("create `{}`: {e}", extract_dir.display())))?;

        let mut records = Vec::new();
        for y in 0..grid::TILE_GRID_EDGE {
            for x in 0..grid::TILE_GRID_EDGE {
                let (width, height, mip_count, payload) = tile_bytes(mip0, stubs, x, y);
                let name = format!("{TEST_PREFIX}_{x}_{y}.dds");
                let file = dds_bytes(width, height, mip_count, &payload);
                let path = extract_dir.join(&name);
                std::fs::write(&path, &file)
                    .map_err(|e| OfflineError::io(format!("write `{}`: {e}", path.display())))?;
                records.push(crate::ManifestRecord {
                    path: name,
                    paz_file: String::from(TEST_PAZ_FILE),
                    offset: 0,
                    comp_size: 0,
                    orig_size: file.len() as u32,
                    flags: 0,
                    sha256: sha256_hex(&file),
                });
            }
        }

        let manifest_path = extract_dir.join(TEST_MANIFEST_NAME);
        let json = serde_json::to_string(&records)
            .map_err(|e| OfflineError::json(format!("serialize the test extract manifest: {e}")))?;
        std::fs::write(&manifest_path, json)
            .map_err(|e| OfflineError::io(format!("write `{}`: {e}", manifest_path.display())))?;

        Ok(config)
    }

    fn base_config(data_dir: &Path) -> Config {
        Config {
            paths: Paths {
                game_root: String::new(),
                data_dir: data_dir.display().to_string(),
            },
            extract: Extract {
                dir: String::new(),
                manifest: String::new(),
                packs: Vec::new(),
                includes: Vec::new(),
                dds_header: 0,
            },
            sdf: Sdf {
                tiles: BTreeMap::new(),
                export: ExportConfig {
                    dir: String::new(),
                    manifest: String::new(),
                    extra: BTreeMap::new(),
                },
            },
            composite: Composite { layers: Vec::new() },
        }
    }

    fn tile_bytes(
        mip0: &[Mip0Spec],
        stubs: &[StubSpec],
        x: u32,
        y: u32,
    ) -> (u32, u32, u32, Vec<u8>) {
        if let Some(spec) = mip0.iter().find(|spec| spec.x == x && spec.y == y) {
            let len = spec
                .payload_len
                .unwrap_or_else(|| grid::chain_len(grid::TILE_EDGE, spec.mip_count));
            return (
                grid::TILE_EDGE,
                grid::TILE_EDGE,
                spec.mip_count,
                vec![TEST_MIP0_VALUE; len],
            );
        }

        let (mip_count, constant) = match stubs.iter().find(|spec| spec.x == x && spec.y == y) {
            Some(spec) => (spec.mip_count, spec.constant),
            None => (TEST_MIP_COUNT, true),
        };
        let mut payload = vec![TEST_STUB_VALUE; grid::chain_len(grid::STUB_EDGE, mip_count)];
        if constant {
            return (grid::STUB_EDGE, grid::STUB_EDGE, mip_count, payload);
        }
        if let Some(byte) = payload.last_mut() {
            *byte = TEST_OTHER_VALUE;
        }
        (grid::STUB_EDGE, grid::STUB_EDGE, mip_count, payload)
    }

    fn dds_bytes(width: u32, height: u32, mip_count: u32, payload: &[u8]) -> Vec<u8> {
        let mut data = vec![0u8; TEST_HEADER_SIZE];
        data[0..crate::DDS_MAGIC.len()].copy_from_slice(&crate::DDS_MAGIC);
        data[TEST_HEADER_SIZE_OFFSET..TEST_HEADER_SIZE_OFFSET + crate::U32_SIZE]
            .copy_from_slice(&(TEST_HEADER_SIZE as u32).to_le_bytes());
        data[TEST_HEIGHT_OFFSET..TEST_HEIGHT_OFFSET + crate::U32_SIZE]
            .copy_from_slice(&height.to_le_bytes());
        data[TEST_WIDTH_OFFSET..TEST_WIDTH_OFFSET + crate::U32_SIZE]
            .copy_from_slice(&width.to_le_bytes());
        data[TEST_MIP_COUNT_OFFSET..TEST_MIP_COUNT_OFFSET + crate::U32_SIZE]
            .copy_from_slice(&mip_count.to_le_bytes());
        data[TEST_BIT_COUNT_OFFSET..TEST_BIT_COUNT_OFFSET + crate::U32_SIZE]
            .copy_from_slice(&TEST_R8_BIT_COUNT.to_le_bytes());
        data.extend_from_slice(payload);
        data
    }

    // ------------------------------------------------------------------------------------------ //

    fn round_trip_fixture(dir: &Path) -> OfflineResult<(Config, FieldMap)> {
        let mut config = base_config(dir);
        config.sdf.export.dir = String::from(TEST_EXPORT_DIR);
        config.sdf.export.manifest = String::from(TEST_MANIFEST_NAME);

        let tiles = vec![
            TilePayload::convert(
                TEST_EXTRACT_PATH,
                TEST_SOURCE_SHA256,
                0,
                0,
                TileKind::Mip0,
                vec![TEST_MIP0_VALUE; grid::chain_len(grid::TILE_EDGE, TEST_MIP_COUNT)],
            ),
            TilePayload::convert(
                TEST_STUB_EXTRACT_PATH,
                TEST_SOURCE_SHA256,
                0,
                1,
                TileKind::Stub,
                vec![TEST_STUB_VALUE; grid::chain_len(grid::STUB_EDGE, TEST_MIP_COUNT)],
            ),
        ];
        let mut fields = BTreeMap::new();
        fields.insert(String::from(TEST_PREFIX), (TEST_MIP_COUNT, tiles));

        let routes = [(String::from(TEST_LAYER_NAME), String::from(TEST_PREFIX))];
        let export = SdfExport::new(
            config.paths()?.sdf_dir,
            String::from(TEST_MANIFEST_NAME),
            String::from(TEST_SOURCE_SHA256),
            &routes,
            &fields,
        )?;
        export.write(&fields, &mut |_| {})?;

        Ok((config, fields))
    }

    fn round_trip_scenario(
        name: &str,
        mutate: fn(&mut ExportManifest) -> OfflineResult<()>,
        expected: &str,
    ) -> OfflineResult<()> {
        let dir = scenario_dir(name);
        let result = round_trip_fixture(&dir).and_then(|(config, fields)| {
            let manifest_path = config.paths()?.sdf_dir.join(&config.sdf.export.manifest);
            let mut manifest = ExportManifest::load(&manifest_path)?;
            mutate(&mut manifest)?;
            let json = serde_json::to_string(&manifest).map_err(|e| {
                OfflineError::json(format!("serialize the mutated export manifest: {e}"))
            })?;
            std::fs::write(&manifest_path, json).map_err(|e| {
                OfflineError::io(format!("write `{}`: {e}", manifest_path.display()))
            })?;

            match round_trip(&config, &fields, &mut |_| {}) {
                Err(e) => ensure(
                    format!("{e}").contains(expected),
                    "the mutated export manifest produced an unexpected error",
                ),
                Ok(_) => Err(OfflineError::manifest(String::from(
                    "the mutated export manifest passed round_trip",
                ))),
            }
        });
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    fn mutate_layer_size(manifest: &mut ExportManifest) -> OfflineResult<()> {
        let layer = manifest.layers.first_mut().ok_or_else(|| {
            OfflineError::manifest(String::from("the mutated manifest has no layers"))
        })?;
        layer.size = TEST_WRONG_LAYER_SIZE;
        Ok(())
    }

    fn mutate_mip_size(manifest: &mut ExportManifest) -> OfflineResult<()> {
        let layer = manifest.layers.first_mut().ok_or_else(|| {
            OfflineError::manifest(String::from("the mutated manifest has no layers"))
        })?;
        let mip = layer.mips.first_mut().ok_or_else(|| {
            OfflineError::manifest(String::from("the mutated manifest has no mips"))
        })?;
        mip.size = TEST_WRONG_MIP_SIZE;
        Ok(())
    }

    fn mutate_mip_count(manifest: &mut ExportManifest) -> OfflineResult<()> {
        let layer = manifest.layers.first_mut().ok_or_else(|| {
            OfflineError::manifest(String::from("the mutated manifest has no layers"))
        })?;
        if layer.mips.pop().is_none() {
            return Err(OfflineError::manifest(String::from(
                "the mutated manifest has no mips",
            )));
        }
        Ok(())
    }

    fn mutate_tile_count(manifest: &mut ExportManifest) -> OfflineResult<()> {
        let layer = manifest.layers.first_mut().ok_or_else(|| {
            OfflineError::manifest(String::from("the mutated manifest has no layers"))
        })?;
        if layer.tiles.pop().is_none() {
            return Err(OfflineError::manifest(String::from(
                "the mutated manifest has no tiles",
            )));
        }
        Ok(())
    }

    fn mutate_payload_name(manifest: &mut ExportManifest) -> OfflineResult<()> {
        let layer = manifest.layers.first_mut().ok_or_else(|| {
            OfflineError::manifest(String::from("the mutated manifest has no layers"))
        })?;
        let tile = layer.tiles.first_mut().ok_or_else(|| {
            OfflineError::manifest(String::from("the mutated manifest has no tiles"))
        })?;
        tile.payload = String::from(TEST_WRONG_PAYLOAD_NAME);
        Ok(())
    }

    // ------------------------------------------------------------------------------------------ //

    fn ensure(condition: bool, message: &str) -> OfflineResult<()> {
        if condition {
            Ok(())
        } else {
            Err(OfflineError::manifest(String::from(message)))
        }
    }
}
