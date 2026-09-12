use crate::{
    Config, OfflineError, OfflineResult,
    config::{Extract, Paths, export::Export as ExportConfig, sdf::Sdf as SdfConfig},
    field::grid,
    run,
};

// ---------------------------------------------------------------------------------------------- //

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------------------------- //

const TEST_DIR_NAME: &str = "sdf-offline-run";
const TEST_PREFIX: &str = "cd_worldmap_test_sdf";
const TEST_HILL_PREFIX: &str = "cd_worldmap_test_hill";
const TEST_HILL_INCLUDE: &str = "cd_worldmap_test_hill*";
const TEST_HILL_ROUTE: &str = "hill";
const TEST_PACK: &str = "0012";
const TEST_PAZ_NAME: &str = "0.paz";
const TEST_INCLUDE: &str = "cd_worldmap_test_sdf*";
const TEST_MIP0_VALUE: u8 = 0xA7;
const TEST_STUB_VALUE: u8 = 0x55;
const TEST_DDS_HEADER: usize = 128;
const TEST_EXTRACT_DIR: &str = "dds";
const TEST_EXPORT_DIR: &str = "sdf";
const TEST_MANIFEST_NAME: &str = "manifest.json";
const TEST_ESCAPING_ENTRY: &str = "../cd_worldmap_test_sdf_evil.dds";

// ---------------------------------------------------------------------------------------------- //

pub(crate) fn ensure(condition: bool, message: &str) -> OfflineResult<()> {
    if condition {
        Ok(())
    } else {
        Err(OfflineError::manifest(String::from(message)))
    }
}

pub(crate) fn ensure_message(
    error: &OfflineError,
    first: &str,
    second: &str,
    failure: &str,
) -> OfflineResult<()> {
    let rendered = format!("{error}");
    ensure(
        rendered.contains(first) && rendered.contains(second),
        failure,
    )
}

// ---------------------------------------------------------------------------------------------- //

#[test]
fn extract_and_field_run_end_to_end_against_a_synthetic_install() -> OfflineResult<()> {
    let dir = test_dir("e2e")?;
    let result = (|| {
        let (install, work) = build_fixture(&dir)?;
        let config = fixture_config(&install, &work);
        config.validate(&install)?;

        let mut events = Vec::new();
        run::extract_run(&config, &mut |event| events.push(event))?;

        let manifest = std::fs::read(work.join(TEST_EXTRACT_DIR).join(TEST_MANIFEST_NAME))
            .map_err(|e| OfflineError::io(format!("read extract manifest: {e}")))?;
        let records: Vec<serde_json::Value> = serde_json::from_slice(&manifest)
            .map_err(|e| OfflineError::json(format!("parse extract manifest: {e}")))?;
        ensure(
            records.len() == 512,
            "every synthetic tile must be extracted",
        )?;

        run::field_run(&config, &mut |event| events.push(event))?;

        let export_manifest =
            std::fs::read_to_string(work.join(TEST_EXPORT_DIR).join(TEST_MANIFEST_NAME))
                .map_err(|e| OfflineError::io(format!("read export manifest: {e}")))?;
        ensure(
            export_manifest.contains("\"name\": \"coast\""),
            "the tiles route must export under its configured name",
        )?;

        ensure(
            events.iter().any(|event| {
                event.step == "extract" && event.milestone && event.message.contains("pack scanned")
            }),
            "the extract step must emit the pack-scanned milestone",
        )?;
        ensure(
            events.iter().any(|event| {
                event.step == "field"
                    && event.milestone
                    && event.message.contains("layer converted")
            }),
            "the field step must emit the layer-converted milestone",
        )?;
        ensure(
            events
                .iter()
                .any(|event| event.message.contains("round-trip: 512 tile payload(s)")),
            "the round-trip gate must verify every payload",
        )?;

        let first = std::fs::read(work.join(TEST_EXTRACT_DIR).join(TEST_MANIFEST_NAME))
            .map_err(|e| OfflineError::io(format!("re-read extract manifest: {e}")))?;
        run::extract_run(&config, &mut |_| {})?;
        let second = std::fs::read(work.join(TEST_EXTRACT_DIR).join(TEST_MANIFEST_NAME))
            .map_err(|e| OfflineError::io(format!("re-read extract manifest: {e}")))?;
        ensure(first == second, "re-running the extract must be idempotent")
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn pruning_an_include_and_renaming_a_route_change_the_next_run() -> OfflineResult<()> {
    let dir = test_dir("config-driven")?;
    let result = (|| {
        let (install, work) = build_fixture(&dir)?;
        let mut config = fixture_config(&install, &work);

        run::extract_run(&config, &mut |_| {})?;
        run::field_run(&config, &mut |_| {})?;
        let both = std::fs::read_to_string(work.join(TEST_EXPORT_DIR).join(TEST_MANIFEST_NAME))
            .map_err(|e| OfflineError::io(format!("read export manifest: {e}")))?;
        ensure(
            both.contains("\"name\": \"coast\"") && both.contains("\"name\": \"hill\""),
            "the full config must export both routes",
        )?;

        config.extract.includes = vec![String::from(TEST_INCLUDE)];
        config.sdf.export.extra.clear();
        run::extract_run(&config, &mut |_| {})?;
        run::field_run(&config, &mut |_| {})?;
        let pruned_extract = std::fs::read(work.join(TEST_EXTRACT_DIR).join(TEST_MANIFEST_NAME))
            .map_err(|e| OfflineError::io(format!("read extract manifest: {e}")))?;
        let records: Vec<serde_json::Value> = serde_json::from_slice(&pruned_extract)
            .map_err(|e| OfflineError::json(format!("parse extract manifest: {e}")))?;
        ensure(
            records.len() == 256,
            "a pruned include must shrink the next extract manifest",
        )?;
        let pruned_export =
            std::fs::read_to_string(work.join(TEST_EXPORT_DIR).join(TEST_MANIFEST_NAME))
                .map_err(|e| OfflineError::io(format!("read export manifest: {e}")))?;
        ensure(
            pruned_export.contains("\"name\": \"coast\"")
                && !pruned_export.contains("\"name\": \"hill\""),
            "a removed export route must drop its layer from the next manifest",
        )
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// A `0.pamt` entry whose path escapes the output directory (a crafted or
/// corrupt table) is a named error at the join, and nothing is written
/// outside the data directory.
#[test]
fn an_escaping_pamt_entry_is_rejected_and_writes_nothing() -> OfflineResult<()> {
    let dir = test_dir("escape")?;
    let result = (|| {
        let (install, work) = build_fixture(&dir)?;
        let config = fixture_config(&install, &work);

        let mip0 = tile_dds(
            grid::TILE_EDGE,
            grid::TILE_EDGE,
            1,
            &vec![TEST_MIP0_VALUE; grid::chain_len(grid::TILE_EDGE, 1)],
        );
        let stub = tile_dds(
            grid::STUB_EDGE,
            grid::STUB_EDGE,
            1,
            &vec![TEST_STUB_VALUE; grid::chain_len(grid::STUB_EDGE, 1)],
        );
        let pack_dir = install.join(TEST_PACK);
        let paz_path = pack_dir.join(TEST_PAZ_NAME);
        let entries = vec![
            (
                String::from("cd_worldmap_test_sdf_0_0.dds"),
                0u32,
                mip0.len() as u32,
                mip0.len() as u32,
            ),
            (
                String::from(TEST_ESCAPING_ENTRY),
                mip0.len() as u32,
                stub.len() as u32,
                stub.len() as u32,
            ),
        ];
        std::fs::write(&paz_path, [&mip0[..], &stub[..]].concat())
            .map_err(|e| OfflineError::io(format!("write test paz: {e}")))?;
        std::fs::write(
            pack_dir.join(crate::constants::PAMT_FILE_NAME),
            pamt_bytes(&entries),
        )
        .map_err(|e| OfflineError::io(format!("write test pamt: {e}")))?;

        let failure = match run::extract_run(&config, &mut |_| {}) {
            Err(error) => format!("{error}"),
            Ok(()) => String::new(),
        };
        ensure(
            failure.contains(".."),
            "the escaping entry must fail at the join naming the traversal, got: {failure}",
        )?;

        let inside = work
            .join(TEST_EXTRACT_DIR)
            .join("cd_worldmap_test_sdf_0_0.dds");
        ensure(inside.is_file(), "the contained entry must still extract")?;
        ensure(
            !work.join("cd_worldmap_test_sdf_evil.dds").exists(),
            "the escaped path must not be written outside the data directory",
        )
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

// ---------------------------------------------------------------------------------------------- //

fn test_dir(name: &str) -> OfflineResult<PathBuf> {
    let dir = std::env::temp_dir().join(format!("{TEST_DIR_NAME}-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)
        .map_err(|e| OfflineError::io(format!("create `{}`: {e}", dir.display())))?;
    Ok(dir)
}

fn fixture_config(install: &Path, work: &Path) -> Config {
    let mut config = Config::defaults();
    config.paths = Paths {
        game_root: install.display().to_string(),
        data_dir: work.display().to_string(),
    };
    config.extract = Extract {
        packs: vec![String::from(TEST_PACK)],
        includes: vec![String::from(TEST_INCLUDE), String::from(TEST_HILL_INCLUDE)],
        dir: String::from(TEST_EXTRACT_DIR),
        manifest: String::from(TEST_MANIFEST_NAME),
        dds_header: TEST_DDS_HEADER,
    };
    config.sdf = SdfConfig {
        tiles: BTreeMap::from([(String::from("coast"), String::from(TEST_PREFIX))]),
        export: ExportConfig {
            dir: String::from(TEST_EXPORT_DIR),
            manifest: String::from(TEST_MANIFEST_NAME),
            extra: BTreeMap::from([(
                String::from(TEST_HILL_ROUTE),
                String::from(TEST_HILL_PREFIX),
            )]),
        },
    };
    config
}

/// Builds a synthetic install: pack `0012` with a `0.pamt` table over a
/// `0.paz` holding one 512² mip0 tile and 255 constant 8² stubs — a full
/// 16×16 grid in the shape the real pack uses.
fn build_fixture(dir: &Path) -> OfflineResult<(PathBuf, PathBuf)> {
    let install = dir.join("install");
    let pack_dir = install.join(TEST_PACK);
    let work = dir.join("work");
    std::fs::create_dir_all(&pack_dir)
        .map_err(|e| OfflineError::io(format!("create pack dir: {e}")))?;
    std::fs::create_dir_all(&work)
        .map_err(|e| OfflineError::io(format!("create work dir: {e}")))?;

    let mip0 = tile_dds(
        grid::TILE_EDGE,
        grid::TILE_EDGE,
        1,
        &vec![TEST_MIP0_VALUE; grid::chain_len(grid::TILE_EDGE, 1)],
    );
    let stub = tile_dds(
        grid::STUB_EDGE,
        grid::STUB_EDGE,
        1,
        &vec![TEST_STUB_VALUE; grid::chain_len(grid::STUB_EDGE, 1)],
    );

    let mut paz = Vec::new();
    let mut entries: Vec<(String, u32, u32, u32)> = Vec::new();
    for prefix in [TEST_PREFIX, TEST_HILL_PREFIX] {
        for y in 0..grid::TILE_GRID_EDGE {
            for x in 0..grid::TILE_GRID_EDGE {
                let file = if x == 0 && y == 0 { &mip0 } else { &stub };
                let name = format!("{prefix}_{x}_{y}.dds");
                entries.push((name, paz.len() as u32, file.len() as u32, file.len() as u32));
                paz.extend_from_slice(file);
            }
        }
    }

    std::fs::write(pack_dir.join(TEST_PAZ_NAME), &paz)
        .map_err(|e| OfflineError::io(format!("write test paz: {e}")))?;
    std::fs::write(
        pack_dir.join(crate::constants::PAMT_FILE_NAME),
        pamt_bytes(&entries),
    )
    .map_err(|e| OfflineError::io(format!("write test pamt: {e}")))?;

    Ok((install, work))
}

fn tile_dds(width: u32, height: u32, mip_count: u32, payload: &[u8]) -> Vec<u8> {
    let mut data = vec![0u8; TEST_DDS_HEADER];
    data[0..crate::DDS_MAGIC.len()].copy_from_slice(&crate::DDS_MAGIC);
    data[4..8].copy_from_slice(&(TEST_DDS_HEADER as u32).to_le_bytes());
    data[12..16].copy_from_slice(&height.to_le_bytes());
    data[16..20].copy_from_slice(&width.to_le_bytes());
    data[28..32].copy_from_slice(&mip_count.to_le_bytes());
    data[88..92].copy_from_slice(&8u32.to_le_bytes());
    data.extend_from_slice(payload);
    data
}

fn pamt_bytes(entries: &[(String, u32, u32, u32)]) -> Vec<u8> {
    const FOLDER_RECORD: &[u8] = &[0, 0, 0, 0, 0];

    let mut data = Vec::new();
    data.extend_from_slice(b"0000");
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&[0u8; 8]);
    data.extend_from_slice(&[0u8; 8]);

    data.extend_from_slice(&(FOLDER_RECORD.len() as u32).to_le_bytes());
    data.extend_from_slice(FOLDER_RECORD);

    let mut node_refs = Vec::with_capacity(entries.len());
    let mut nodes = Vec::new();
    for (name, ..) in entries {
        node_refs.push(nodes.len() as u32);
        nodes.extend_from_slice(&u32::MAX.to_le_bytes());
        nodes.push(name.len() as u8);
        nodes.extend_from_slice(name.as_bytes());
    }
    data.extend_from_slice(&(nodes.len() as u32).to_le_bytes());
    data.extend_from_slice(&nodes);

    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&[0u8; 4]);

    for ((.., offset, comp, orig), node_ref) in entries.iter().zip(node_refs) {
        data.extend_from_slice(&node_ref.to_le_bytes());
        data.extend_from_slice(&offset.to_le_bytes());
        data.extend_from_slice(&comp.to_le_bytes());
        data.extend_from_slice(&orig.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
    }

    data
}
