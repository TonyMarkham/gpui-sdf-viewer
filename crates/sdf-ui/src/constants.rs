use gpui::{Pixels, px};

pub(crate) const APPLICATION_TITLE: &str = "cd-map-offline";

pub const BLUEPRINT_FUNCTIONAL_THEME_JSON: &str =
    include_str!("../assets/themes/blueprint-functional.json");

pub(crate) const NAVIGATION_SCENES_HEADER: &str = "Scenes";
pub(crate) const NAVIGATION_COMPOSITE_HEADER: &str = "Composite";
pub(crate) const NAVIGATION_COMPOSITE_SELECTOR: &str = "navigation-composite";
pub(crate) const NAVIGATION_COMPOSITE_BUTTON_ID: &str = "composite";

pub(crate) const SCENE_DIR_ENV: &str = "SDF_SCENES_DIR";
pub(crate) const SCENE_DIR_DEFAULT: &str = "scenes";

pub(crate) const SETUP_PANEL_SELECTOR: &str = "setup-panel";
pub(crate) const SETUP_PANEL_MAX_WIDTH: Pixels = px(560.0);
pub(crate) const SETUP_TEXT_SIZE: Pixels = px(13.0);
pub(crate) const SETUP_EXPLANATION: &str = "Point cd-map-offline at your Crimson Desert install. The worldmap SDF \
fields are extracted from the game packs into your user config directory.";
pub(crate) const SETUP_FOLDER_LABEL: &str = "Game folder:";
pub(crate) const SETUP_NO_FOLDER: &str = "not set";
pub(crate) const SETUP_PICKER_LABEL: &str = "Select folder…";
pub(crate) const SETUP_EXTRACT_LABEL: &str = "Extract now";
pub(crate) const SETUP_REJECTION_PREFIX: &str = "Rejected: ";
pub(crate) const SETUP_FAILURE_PREFIX: &str = "Extraction failed: ";
pub(crate) const SETUP_PICK_BUTTON_ID: &str = "select-folder";
pub(crate) const SETUP_EXTRACT_BUTTON_ID: &str = "extract-now";
pub(crate) const NAVIGATION_GAME_FOLDER_LABEL: &str = "Game folder";
pub(crate) const NAVIGATION_GAME_FOLDER_BUTTON_ID: &str = "game-folder";

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub(crate) const WAYLAND_COMPOSITOR_NAME: &str = "Wayland";

pub(crate) const CANVAS_HOST_SELECTOR: &str = "canvas-host";
pub(crate) const STATUS_BAR_SELECTOR: &str = "status-bar";
pub(crate) const NAVIGATION_SELECTOR: &str = "navigation-scenes";

pub(crate) const NAVIGATION_WIDTH: Pixels = px(64.0);
pub(crate) const NAVIGATION_HEADER_TEXT_SIZE: Pixels = px(10.0);

pub(crate) const STATUS_BAR_HEIGHT: Pixels = px(26.0);
pub(crate) const STATUS_BAR_PADDING: Pixels = px(10.0);
pub(crate) const STATUS_BAR_TEXT_SIZE: Pixels = px(11.0);

pub(crate) const CANVAS_ERROR_TEXT_SIZE: Pixels = px(14.0);
pub(crate) const CANVAS_ERROR_MAX_WIDTH: Pixels = px(640.0);

pub(crate) const STATUS_SEPARATOR: &str = "  ·  ";
pub(crate) const STATUS_NO_ADAPTER: &str = "wgpu renderer unavailable";
pub(crate) const STATUS_NO_SCENE: &str = "No scene selected";
pub(crate) const STATUS_NO_GAME_FOLDER: &str = "No game folder set";
pub(crate) const STATUS_GAME_FOLDER_OK: &str = "Game folder ok";
pub(crate) const STATUS_GAME_FOLDER_INVALID: &str = "Game folder invalid";
pub(crate) const STATUS_GAME_DATA_MISSING: &str = "Game data missing";
pub(crate) const STATUS_GAME_FOLDER_BAD: &str = "Game folder rejected: ";
pub(crate) const STATUS_EXTRACTING: &str = "Extracting: ";
pub(crate) const STATUS_EXTRACTION_FAILED: &str = "Extraction failed: ";

pub(crate) const CONTROL_BAR_SELECTOR: &str = "control-bar";
pub(crate) const CONTROL_BAR_HEIGHT: Pixels = px(36.0);
pub(crate) const CONTROL_BAR_PADDING: Pixels = px(10.0);
pub(crate) const CONTROL_BAR_TEXT_SIZE: Pixels = px(11.0);
pub(crate) const CONTROL_BAR_SLIDER_WIDTH: Pixels = px(160.0);
pub(crate) const CONTROL_BAR_GAP: Pixels = px(12.0);

pub(crate) const CONTROL_LEVEL_LABEL: &str = "LOD";
pub(crate) const CONTROL_CONTOUR_LABEL: &str = "Contours";
pub(crate) const CONTROL_VIEW_RESET_LABEL: &str = "1:1";
pub(crate) const CONTROL_VIEW_RESET_BUTTON_ID: &str = "view-reset";
pub(crate) const CONTOUR_BAND_DEFAULT: f32 = 8.0;
pub(crate) const CONTOUR_BAND_MAX: f32 = 32.0;
