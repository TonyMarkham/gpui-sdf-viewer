use super::offline_segments;
use crate::{
    constants::{
        STATUS_EXTRACTING, STATUS_EXTRACTION_FAILED, STATUS_GAME_DATA_MISSING,
        STATUS_GAME_FOLDER_BAD, STATUS_GAME_FOLDER_INVALID, STATUS_GAME_FOLDER_OK,
        STATUS_NO_GAME_FOLDER,
    },
    state::offline::OfflineStatus,
};

// ---------------------------------------------------------------------------------------------- //

const TEST_REJECTION: &str = "the install is missing `0012/0.pamt`";
const TEST_ROOT: &str = "E:/Games/Crimson Desert";
const TEST_PROGRESS: &str = "extract · pack scanned";
const TEST_FAILURE: &str = "pack `0012` could not be read";

// ---------------------------------------------------------------------------------------------- //

/// The offline status the render call site produces: every scenario builds
/// the snapshot with named fields, exactly as `OfflineState::status()` fills
/// them, so a swapped or dropped value cannot compile here either.
fn snapshot(
    game_root_valid: bool,
    data_present: bool,
    rejection: Option<&str>,
    game_root: &str,
    extraction_running: bool,
    progress_line: &str,
    extraction_failed: Option<&str>,
) -> OfflineStatus {
    OfflineStatus {
        game_root_valid,
        data_present,
        rejection: rejection.map(String::from),
        game_root: String::from(game_root),
        extraction_running,
        progress_line: String::from(progress_line),
        extraction_failed: extraction_failed.map(String::from),
    }
}

#[test]
fn an_empty_game_root_reports_the_missing_folder() {
    let segments = offline_segments(&snapshot(false, false, None, "", false, "", None));
    assert_eq!(
        segments,
        vec![String::from(STATUS_NO_GAME_FOLDER)],
        "the first run without a folder must name the missing game folder"
    );
}

#[test]
fn a_set_but_invalid_root_reports_the_invalid_folder() {
    let segments = offline_segments(&snapshot(false, false, None, TEST_ROOT, false, "", None));
    assert_eq!(
        segments,
        vec![String::from(STATUS_GAME_FOLDER_INVALID)],
        "a configured root failing validation must read as invalid"
    );
}

#[test]
fn a_valid_root_without_data_reports_missing_data_not_an_invalid_folder() {
    let segments = offline_segments(&snapshot(true, false, None, TEST_ROOT, false, "", None));
    assert_eq!(
        segments,
        vec![String::from(STATUS_GAME_DATA_MISSING)],
        "a valid pick that has not extracted yet must read as missing data, \
         never as an invalid folder"
    );
}

#[test]
fn a_rejection_names_the_reason() {
    let segments = offline_segments(&snapshot(
        false,
        false,
        Some(TEST_REJECTION),
        TEST_ROOT,
        false,
        "",
        None,
    ));
    assert_eq!(
        segments,
        vec![format!("{STATUS_GAME_FOLDER_BAD}{TEST_REJECTION}")],
        "a rejected pick must surface the validation reason"
    );
}

#[test]
fn a_valid_root_reports_ok_and_no_extraction_segments() {
    let segments = offline_segments(&snapshot(true, true, None, TEST_ROOT, false, "", None));
    assert_eq!(
        segments,
        vec![String::from(STATUS_GAME_FOLDER_OK)],
        "a ready root with no extraction must show only the ok segment"
    );
}

#[test]
fn a_running_extraction_appends_the_progress_line() {
    let segments = offline_segments(&snapshot(
        true,
        true,
        None,
        TEST_ROOT,
        true,
        TEST_PROGRESS,
        None,
    ));
    assert_eq!(
        segments,
        vec![
            String::from(STATUS_GAME_FOLDER_OK),
            format!("{STATUS_EXTRACTING}{TEST_PROGRESS}"),
        ],
        "extraction progress must follow the folder segment"
    );
}

#[test]
fn a_failed_extraction_appends_the_failure() {
    let segments = offline_segments(&snapshot(
        true,
        true,
        None,
        TEST_ROOT,
        false,
        "",
        Some(TEST_FAILURE),
    ));
    assert_eq!(
        segments,
        vec![
            String::from(STATUS_GAME_FOLDER_OK),
            format!("{STATUS_EXTRACTION_FAILED}{TEST_FAILURE}"),
        ],
        "an extraction failure must surface as its own segment"
    );
}

#[test]
fn a_failed_extraction_keeps_the_progress_segment_it_ran_with() {
    let segments = offline_segments(&snapshot(
        true,
        true,
        None,
        TEST_ROOT,
        true,
        TEST_PROGRESS,
        Some(TEST_FAILURE),
    ));
    assert_eq!(
        segments,
        vec![
            String::from(STATUS_GAME_FOLDER_OK),
            format!("{STATUS_EXTRACTING}{TEST_PROGRESS}"),
            format!("{STATUS_EXTRACTION_FAILED}{TEST_FAILURE}"),
        ],
        "running and failed extraction segments must keep their order"
    );
}

#[test]
fn every_status_field_reaches_the_segments() {
    // The two bools the old positional call site could transpose: a valid
    // root without data must read as missing data, and an invalid root with
    // data must read as invalid — the pair proves each bool lands where its
    // name says.
    let invalid_root_with_data =
        offline_segments(&snapshot(false, true, None, TEST_ROOT, false, "", None));
    assert_eq!(
        invalid_root_with_data,
        vec![String::from(STATUS_GAME_FOLDER_INVALID)],
        "data presence must not mask an invalid folder"
    );
}
