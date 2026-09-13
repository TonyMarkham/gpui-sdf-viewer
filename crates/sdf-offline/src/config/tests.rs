use super::{Config, DEFAULT_TEMPLATE};

// ---------------------------------------------------------------------------------------------- //

use crate::config::{BandKind, CompositeBand, CompositeLayer};
use crate::tests::{ensure, ensure_message};
use crate::{OfflineError, OfflineResult};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------------------------- //

const TEST_DIR_NAME: &str = "sdf-offline-config";
const TEST_GAME_ROOT: &str = "E:/Games/Crimson Desert";
const TEST_DATA_DIR: &str = "C:/maps-data";
const TEST_RELATIVE_DATA_DIR: &str = "maps-data";
const TEST_HAND_TUNED_INCLUDE: &str = "cd_worldmap_test_sdf*";
const TEST_MISSING_ROOT: &str = "Q:/nowhere";
const TEST_PAMT_TABLE: &str = "0.pamt";
const TEST_COLLIDING_ROUTE: &str = "coast";
const TEST_UNKNOWN_COMPOSITE_LAYER: &str = "not_a_route";
const TEST_HAND_TUNED_COMPOSITE_COMMENT: &str = "# my composite, hands off";

// ---------------------------------------------------------------------------------------------- //

#[test]
fn the_shipped_template_matches_the_defaults() -> OfflineResult<()> {
    let from_template: Config = toml::from_str(DEFAULT_TEMPLATE)
        .map_err(|e| OfflineError::toml(format!("parse template: {e}")))?;
    ensure(
        from_template == Config::defaults(),
        "the shipped template must equal defaults()",
    )
}

#[test]
fn a_missing_file_yields_the_defaults() -> OfflineResult<()> {
    let path = test_path("missing");
    let _ = std::fs::remove_file(&path);

    match Config::load_from(&path) {
        Ok(config) => ensure(
            config == Config::defaults(),
            "a missing config file must yield the defaults",
        ),
        Err(_) => Err(OfflineError::toml("a missing config file must not error")),
    }
}

#[test]
fn load_and_save_round_trip_the_user_state() -> OfflineResult<()> {
    let path = test_path("round-trip");
    let _ = std::fs::remove_file(&path);

    let mut config = Config::load_from(&path)?;
    config.paths.game_root = String::from(TEST_GAME_ROOT);
    config.save_to(&path)?;

    let reloaded = Config::load_from(&path)?;
    ensure(
        reloaded.paths.game_root == TEST_GAME_ROOT,
        "the game root must survive a save/load round trip",
    )?;
    ensure(
        reloaded == config,
        "the saved document must deserialize back to the saved config",
    )?;

    let _ = std::fs::remove_file(&path);

    Ok(())
}

#[test]
fn a_gui_save_preserves_hand_tuned_sections_and_comments() -> OfflineResult<()> {
    let path = test_path("hand-tuned");
    std::fs::write(
        &path,
        format!(
            "# my tuning, hands off\n[extract]\n# pruned for testing\nincludes = [\"{TEST_HAND_TUNED_INCLUDE}\"]\ndds_header = 256\n\n[paths]\ngame_root = \"\"\n"
        ),
    )
    .map_err(|e| OfflineError::io(format!("write test config: {e}")))?;

    let mut config = Config::load_from(&path)?;
    config.paths.game_root = String::from(TEST_GAME_ROOT);
    config.save_to(&path)?;

    let saved = std::fs::read_to_string(&path)
        .map_err(|e| OfflineError::io(format!("read saved config: {e}")))?;
    ensure(
        saved.contains("# my tuning, hands off"),
        "the leading comment must survive the save",
    )?;
    ensure(
        saved.contains("# pruned for testing"),
        "the inline comment must survive the save",
    )?;
    ensure(
        saved.contains(TEST_HAND_TUNED_INCLUDE),
        "the hand-tuned include must survive the save",
    )?;
    ensure(
        saved.contains("dds_header = 256"),
        "the hand-tuned dds_header must survive the save",
    )?;
    ensure(
        saved.contains(&format!("game_root = \"{TEST_GAME_ROOT}\"")),
        "the game root must be written",
    )?;

    let reloaded = Config::load_from(&path)?;
    ensure(
        reloaded.extract.includes == vec![String::from(TEST_HAND_TUNED_INCLUDE)],
        "the hand-tuned includes must reload unchanged",
    )?;

    let _ = std::fs::remove_file(&path);

    Ok(())
}

#[test]
fn validate_rejects_an_install_missing_the_pamt_table() -> OfflineResult<()> {
    let config = Config::defaults();

    match config.validate(Path::new(TEST_MISSING_ROOT)) {
        Err(error) => ensure_message(
            &error,
            TEST_MISSING_ROOT,
            TEST_PAMT_TABLE,
            "the validation error must name the install and the missing table",
        ),
        Ok(_) => Err(OfflineError::toml(
            "an install without a pamt table passed validate",
        )),
    }
}

#[test]
fn validate_accepts_an_install_with_every_pack_present() -> OfflineResult<()> {
    let dir = test_dir("valid-install");
    let pack_dir = dir.join("0012");
    std::fs::create_dir_all(&pack_dir)
        .map_err(|e| OfflineError::io(format!("create pack dir: {e}")))?;
    std::fs::write(pack_dir.join(TEST_PAMT_TABLE), b"pamt")
        .map_err(|e| OfflineError::io(format!("write test pamt: {e}")))?;

    let config = Config::defaults();
    let result = config.validate(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn the_data_dir_override_resolves_every_output_directory() -> OfflineResult<()> {
    let mut config = Config::defaults();
    config.paths.data_dir = String::from(TEST_DATA_DIR);

    let paths = config.paths()?;
    ensure(
        paths.data_dir.as_os_str() == TEST_DATA_DIR,
        "the override must be used as the data directory",
    )?;
    ensure(
        paths.dds_dir == PathBuf::from(TEST_DATA_DIR).join("dds"),
        "the dds directory must sit under the override",
    )?;
    ensure(
        paths.sdf_dir == PathBuf::from(TEST_DATA_DIR).join("sdf"),
        "the sdf directory must sit under the override",
    )?;

    config.paths.data_dir = String::new();
    let paths = config.paths()?;
    ensure(
        paths.data_dir == paths.config_dir.join("data"),
        "an empty override must default to <config dir>/data",
    )?;

    config.paths.data_dir = String::from(TEST_RELATIVE_DATA_DIR);
    let paths = config.paths()?;
    ensure(
        paths.data_dir == paths.config_dir.join(TEST_RELATIVE_DATA_DIR),
        "a relative override must resolve against the config dir",
    )?;
    ensure(
        paths.sdf_dir == paths.config_dir.join(TEST_RELATIVE_DATA_DIR).join("sdf"),
        "the sdf directory must sit under the resolved relative override",
    )
}

#[test]
fn check_rejects_an_extra_name_colliding_with_a_tiles_key() -> OfflineResult<()> {
    let mut config = Config::defaults();
    config.sdf.export.extra.insert(
        String::from(TEST_COLLIDING_ROUTE),
        String::from("cd_worldmap_land_sdf"),
    );

    match config.check() {
        Err(error) => ensure_message(
            &error,
            "collides with a `[sdf.tiles]` key",
            TEST_COLLIDING_ROUTE,
            "the colliding export route name must be rejected",
        ),
        Ok(_) => Err(OfflineError::toml(
            "a colliding export route name passed check",
        )),
    }
}

#[test]
fn check_rejects_a_route_prefix_no_include_covers() -> OfflineResult<()> {
    let mut config = Config::defaults();
    config.extract.includes = vec![String::from("cd_worldmap_land_sdf*")];

    match config.check() {
        Err(error) => ensure_message(
            &error,
            "is not covered by any",
            "abyss_hex",
            "a route prefix outside the includes must be rejected",
        ),
        Ok(_) => Err(OfflineError::toml("an uncovered route prefix passed check")),
    }
}

#[test]
fn check_rejects_empty_includes() -> OfflineResult<()> {
    let mut config = Config::defaults();
    config.extract.includes.clear();

    match config.check() {
        Err(error) => ensure_message(
            &error,
            "includes` is empty",
            "",
            "empty includes must be rejected",
        ),
        Ok(_) => Err(OfflineError::toml("empty includes passed check")),
    }
}

#[test]
fn check_accepts_the_defaults() -> OfflineResult<()> {
    Config::defaults().check()
}

#[test]
fn the_default_composite_paints_the_water_up_to_the_shore() -> OfflineResult<()> {
    let layers = &Config::defaults().composite.layers;
    let names: Vec<&str> = layers.iter().map(|layer| layer.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["river", "coast", "road", "mountain", "road_wagon"],
        "the shipped default palette must paint water first, then the shore line, then the features"
    );

    // The water fill must cover every byte below the shore line — closing
    // both the sub-32 lake voids and the 108-122 shore moat.
    let water_windows: Vec<(f32, f32)> = layers[0]
        .bands
        .iter()
        .map(|band| (band.low, band.high))
        .collect();
    assert_eq!(
        water_windows,
        vec![(0.0, 32.0), (32.0, 122.0)],
        "the water layer must fill every byte below the shore transition"
    );

    // The shore edge reads the transition strip only, not the landmass.
    let shore = &layers[1].bands[0];
    ensure(
        shore.low == 122.0 && shore.high == 132.0,
        "the shore layer must band the 122..132 transition strip",
    )?;
    ensure(
        shore.kind == BandKind::Band,
        "the shore layer must use the hard band primitive",
    )
}

#[test]
fn check_rejects_a_composite_layer_no_route_names() -> OfflineResult<()> {
    let mut config = Config::defaults();
    config.composite.layers.push(CompositeLayer {
        name: String::from(TEST_UNKNOWN_COMPOSITE_LAYER),
        bands: vec![band(0.0, 255.0)],
    });

    match config.check() {
        Err(error) => ensure_message(
            &error,
            "is not an export route",
            TEST_UNKNOWN_COMPOSITE_LAYER,
            "an unknown composite layer must be rejected",
        ),
        Ok(_) => Err(OfflineError::toml(
            "an unknown composite layer passed check",
        )),
    }
}

#[test]
fn check_rejects_a_duplicate_composite_layer() -> OfflineResult<()> {
    let mut config = Config::defaults();
    // Layer 2 (road) renamed to coast duplicates layer 1.
    config.composite.layers[2].name = String::from(TEST_COLLIDING_ROUTE);

    match config.check() {
        Err(error) => ensure_message(
            &error,
            "is listed twice",
            TEST_COLLIDING_ROUTE,
            "a duplicated composite layer must be rejected",
        ),
        Ok(_) => Err(OfflineError::toml(
            "a duplicated composite layer passed check",
        )),
    }
}

#[test]
fn check_rejects_a_layer_without_bands() -> OfflineResult<()> {
    let mut config = Config::defaults();
    config.composite.layers[0].bands.clear();

    match config.check() {
        Err(error) => ensure_message(
            &error,
            "carries no bands",
            "river",
            "a layer without bands must be rejected",
        ),
        Ok(_) => Err(OfflineError::toml("a bandless layer passed check")),
    }
}

#[test]
fn check_rejects_a_band_low_above_high() -> OfflineResult<()> {
    let mut config = Config::defaults();
    config.composite.layers[0].bands[0].low = 200.0;
    config.composite.layers[0].bands[0].high = 40.0;

    match config.check() {
        Err(error) => ensure_message(
            &error,
            "is above high",
            "river",
            "an inverted band window must be rejected",
        ),
        Ok(_) => Err(OfflineError::toml("an inverted band window passed check")),
    }
}

#[test]
fn check_rejects_a_band_value_outside_the_byte_range() -> OfflineResult<()> {
    let mut config = Config::defaults();
    config.composite.layers[0].bands[0].high = 300.0;

    match config.check() {
        Err(error) => ensure_message(
            &error,
            "is outside 0..=255",
            "river",
            "a band edge beyond the byte range must be rejected",
        ),
        Ok(_) => Err(OfflineError::toml(
            "a band value outside the byte range passed check",
        )),
    }
}

#[test]
fn check_rejects_a_band_weight_outside_the_unit_range() -> OfflineResult<()> {
    let mut config = Config::defaults();
    config.composite.layers[2].bands[0].weight = 1.5;

    match config.check() {
        Err(error) => ensure_message(
            &error,
            "band weight 1.5 is outside 0..=1",
            "road",
            "a band weight outside 0..=1 must be rejected",
        ),
        Ok(_) => Err(OfflineError::toml(
            "a band weight outside 0..=1 passed check",
        )),
    }
}

#[test]
fn check_rejects_a_band_ink_component_outside_the_unit_range() -> OfflineResult<()> {
    let mut config = Config::defaults();
    config.composite.layers[1].bands[0].ink[1] = 1.4;

    match config.check() {
        Err(error) => ensure_message(
            &error,
            "band ink component 1 1.4 is outside 0..=1",
            "coast",
            "an ink component outside 0..=1 must be rejected",
        ),
        Ok(_) => Err(OfflineError::toml(
            "an ink component outside 0..=1 passed check",
        )),
    }
}

#[test]
fn check_accepts_an_empty_composite() -> OfflineResult<()> {
    let mut config = Config::defaults();
    config.composite.layers.clear();
    config.check()
}

#[test]
fn a_hand_tuned_composite_survives_a_gui_save() -> OfflineResult<()> {
    let path = test_path("hand-tuned-composite");
    std::fs::write(
        &path,
        format!(
            "{TEST_HAND_TUNED_COMPOSITE_COMMENT}\n[paths]\ngame_root = \"\"\n\n\
             [composite]\n# my stack, hands off\n[[composite.layer]]\nname = \"coast\"\n\
             bands = [{{ low = 120.0, high = 140.0, kind = \"band\", ink = [0.2, 0.3, 0.4], weight = 0.5 }}]\n"
        ),
    )
    .map_err(|e| OfflineError::io(format!("write test config: {e}")))?;

    let mut config = Config::load_from(&path)?;
    config.paths.game_root = String::from(TEST_GAME_ROOT);
    config.save_to(&path)?;

    let saved = std::fs::read_to_string(&path)
        .map_err(|e| OfflineError::io(format!("read saved config: {e}")))?;
    ensure(
        saved.contains(TEST_HAND_TUNED_COMPOSITE_COMMENT),
        "the hand-tuned leading comment must survive the save",
    )?;
    ensure(
        saved.contains("[[composite.layer]]") && saved.contains("# my stack, hands off"),
        "the hand-tuned composite layer must survive the save",
    )?;

    let reloaded = Config::load_from(&path)?;
    ensure(
        reloaded.composite.layers.len() == 1
            && reloaded.composite.layers[0].name == "coast"
            && reloaded.composite.layers[0].bands[0].low == 120.0,
        "the hand-tuned composite layer must reload with its band intact",
    )?;

    let _ = std::fs::remove_file(&path);

    Ok(())
}

// ---------------------------------------------------------------------------------------------- //

/// One full-strength gray band covering `low..high` — enough styling for a
/// layer to pass the structural checks.
fn band(low: f32, high: f32) -> CompositeBand {
    CompositeBand {
        low,
        high,
        kind: BandKind::Band,
        ink: [0.5, 0.5, 0.5],
        weight: 1.0,
    }
}

// ---------------------------------------------------------------------------------------------- //

fn test_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{TEST_DIR_NAME}-{name}-{}.toml",
        std::process::id()
    ))
}

fn test_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{TEST_DIR_NAME}-{name}-{}", std::process::id()))
}
