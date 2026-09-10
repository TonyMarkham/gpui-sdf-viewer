mod error;

// ---------------------------------------------------------------------------------------------- //

pub use crate::error::{Error as SdfAppError, result::Result as SdfAppResult};

// ---------------------------------------------------------------------------------------------- //

use sdf_ui::{UiAssets, build_root_view};

use gpui::App;
use gpui_platform::application;

fn main() {
    application().with_assets(UiAssets).run(initialize);
}

fn initialize(cx: &mut App) {
    if let Err(error) = try_initialize(cx) {
        eprintln!("{error}");
        cx.quit();
    }
}

fn try_initialize(cx: &mut App) -> SdfAppResult<()> {
    sdf_ui::initialize(cx).map_err(SdfAppError::ui)?;

    cx.open_window(sdf_ui::window_options(), build_root_view)
        .map_err(|error| SdfAppError::app(&format!("failed to open the main window: {error}")))?;

    cx.activate(true);

    Ok(())
}
