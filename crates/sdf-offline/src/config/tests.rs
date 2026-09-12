use super::{Config, DEFAULT_TEMPLATE};

// ---------------------------------------------------------------------------------------------- //

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
