use crate::arguments::extract::{apply_overrides, prepare, run_in};

use sdf_offline::Config;

use std::path::{Path, PathBuf};

const TEST_DIR_NAME: &str = "sdf-app-extract";
const TEST_GAME_ROOT: &str = "E:/Games/Crimson Desert";
const TEST_PACK: &str = "0012";
const TEST_PAMT: &str = "0.pamt";
const TEST_HAND_COMMENT: &str = "# my tuning, hands off";
const TEST_HAND_ROOT: &str = "E:/Games/old-install";

/// A hand-tuned config whose includes satisfy `check` and whose game root
/// misses the pamt table: a flag-less run fails at `validate`.
const TEST_HAND_CONFIG: &str = r#"# my tuning, hands off
[paths]
game_root = "E:/Games/old-install"

[extract]
includes = [
    "cd_worldmap_land_sdf*",
    "cd_worldmap_road_sdf*",
    "cd_worldmap_road_wagon_sdf*",
    "cd_worldmap_mountain_sdf*",
    "cd_worldmap_abyss_hex_sdf*",
    "cd_worldmap_blur_height*",
]
"#;

/// Empty includes and an empty root: the `check` error must beat the guard.
const TEST_EMPTY_CONFIG: &str = r#"[paths]
game_root = ""

[extract]
includes = []
"#;

// ---------------------------------------------------------------------------------------------- //

fn test_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{TEST_DIR_NAME}-{tag}-{}.toml", std::process::id()))
}

fn test_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{TEST_DIR_NAME}-{tag}-{}", std::process::id()))
}

fn fixture_install(tag: &str) -> PathBuf {
    let dir = test_dir(tag);
    let pack = dir.join(TEST_PACK);
    let created = std::fs::create_dir_all(&pack);
    assert!(created.is_ok(), "create pack dir failed: {created:?}");
    let written = std::fs::write(pack.join(TEST_PAMT), b"pamt");
    assert!(written.is_ok(), "write pamt failed: {written:?}");
    dir
}

fn read(path: &Path) -> String {
    match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) => unreachable!("read `{}` failed: {error}", path.display()),
    }
}

// ---------------------------------------------------------------------------------------------- //

#[test]
fn flag_overrides_win_over_the_loaded_config() {
    let mut config = match Config::load_from(&test_path("overrides")) {
        Ok(config) => config,
        Err(error) => unreachable!("a missing config file must yield defaults: {error}"),
    };
    config.paths.game_root = String::from(TEST_HAND_ROOT);
    config.paths.data_dir = String::from("C:/loaded-data");

    apply_overrides(
        &mut config,
        Some(String::from(TEST_GAME_ROOT)),
        Some(String::from("C:/flag-data")),
    );
    assert_eq!(config.paths.game_root, TEST_GAME_ROOT);
    assert_eq!(config.paths.data_dir, "C:/flag-data");

    apply_overrides(&mut config, None, None);
    assert_eq!(
        config.paths.game_root, TEST_GAME_ROOT,
        "an absent flag must keep the applied value"
    );
    assert_eq!(config.paths.data_dir, "C:/flag-data");
}

#[test]
fn save_config_persists_the_overrides_before_the_pipeline_runs() {
    let path = test_path("persist");
    let written = std::fs::write(&path, TEST_HAND_CONFIG);
    assert!(written.is_ok(), "write test config failed: {written:?}");

    let install = fixture_install("persist");
    let result = run_in(&path, Some(install.display().to_string()), None, true);

    // The fixture pamt table is a stub: the pipeline fails, but the override
    // must already be on disk by then.
    assert!(
        result.is_err(),
        "the stub install must fail in the pipeline"
    );
    let reloaded = match Config::load_from(&path) {
        Ok(config) => config,
        Err(error) => unreachable!("reload the persisted config: {error}"),
    };
    assert_eq!(
        reloaded.paths.game_root,
        install.display().to_string(),
        "--save-config must persist the flag override"
    );
    let saved = read(&path);
    assert!(
        saved.contains(TEST_HAND_COMMENT),
        "the save must stay format-preserving, got: {saved}"
    );
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir_all(&install);
}

#[test]
fn overrides_do_not_persist_without_save_config() {
    let path = test_path("no-save");
    let written = std::fs::write(&path, TEST_HAND_CONFIG);
    assert!(written.is_ok(), "write test config failed: {written:?}");
    let before = read(&path);

    let install = fixture_install("no-save");
    let result = run_in(&path, Some(install.display().to_string()), None, false);

    assert!(
        result.is_err(),
        "the stub install must fail in the pipeline"
    );
    assert_eq!(
        read(&path),
        before,
        "without --save-config the config file must stay untouched"
    );
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir_all(&install);
}

#[test]
fn an_empty_game_root_is_a_named_error() {
    let path = test_path("empty-root");
    let _ = std::fs::remove_file(&path);

    let failure = match run_in(&path, None, None, false) {
        Err(error) => error.to_string(),
        Ok(()) => String::new(),
    };
    assert!(
        failure.contains("no game folder is configured"),
        "an empty game root must fail with the guard error, got: {failure}"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn config_check_fires_before_the_root_guard() {
    let path = test_path("check-first");
    let written = std::fs::write(&path, TEST_EMPTY_CONFIG);
    assert!(written.is_ok(), "write test config failed: {written:?}");

    let failure = match run_in(&path, None, None, false) {
        Err(error) => error.to_string(),
        Ok(()) => String::new(),
    };
    assert!(
        failure.contains("includes` is empty"),
        "check must run before the root guard and the pipeline, got: {failure}"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn prepare_validates_against_a_fixture_install() {
    let mut config = Config::defaults();
    config.paths.game_root = String::from(TEST_GAME_ROOT);

    let failure = match prepare(&config) {
        Err(error) => error.to_string(),
        Ok(()) => String::new(),
    };
    assert!(
        failure.contains(TEST_GAME_ROOT) && failure.contains(TEST_PAMT),
        "prepare must reject an install missing the pamt table, got: {failure}"
    );

    let install = fixture_install("prepare");
    config.paths.game_root = install.display().to_string();
    assert!(prepare(&config).is_ok(), "a fixture install passes prepare");
    let _ = std::fs::remove_dir_all(&install);
}
