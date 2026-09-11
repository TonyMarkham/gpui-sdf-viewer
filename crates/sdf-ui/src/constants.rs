use gpui::{Pixels, px};

pub(crate) const APPLICATION_TITLE: &str = "SDF Renderer";

pub const BLUEPRINT_FUNCTIONAL_THEME_JSON: &str =
    include_str!("../assets/themes/blueprint-functional.json");

pub(crate) const NAVIGATION_SCENES_HEADER: &str = "Scenes";

pub(crate) const SCENE_DIR_ENV: &str = "SDF_SCENES_DIR";
pub(crate) const SCENE_DIR_DEFAULT: &str = "scenes";

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

pub(crate) const CONTROL_BAR_SELECTOR: &str = "control-bar";
pub(crate) const CONTROL_BAR_HEIGHT: Pixels = px(36.0);
pub(crate) const CONTROL_BAR_PADDING: Pixels = px(10.0);
pub(crate) const CONTROL_BAR_TEXT_SIZE: Pixels = px(11.0);
pub(crate) const CONTROL_BAR_SLIDER_WIDTH: Pixels = px(160.0);
pub(crate) const CONTROL_BAR_GAP: Pixels = px(12.0);

pub(crate) const CONTROL_LEVEL_LABEL: &str = "LOD";
pub(crate) const CONTROL_CONTOUR_LABEL: &str = "Contours";
pub(crate) const CONTOUR_BAND_DEFAULT: f32 = 8.0;
pub(crate) const CONTOUR_BAND_MAX: f32 = 32.0;
