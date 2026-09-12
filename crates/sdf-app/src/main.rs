mod arguments;
mod error;

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------------------------- //

pub use crate::{
    arguments::{Arguments, command::Command, query_config::ConfigQuery},
    error::{Error as SdfAppError, result::Result as SdfAppResult},
};

// ---------------------------------------------------------------------------------------------- //

use sdf_ui::{UiAssets, build_root_view};

use clap::Parser as _;
use gpui::App;
use gpui_platform::application;

// ---------------------------------------------------------------------------------------------- //

fn main() {
    match Arguments::parse().command {
        None => start_gui(),
        Some(Command::Extract {
            game_root,
            data_dir,
            save_config,
        }) => match arguments::extract::run(game_root, data_dir, save_config) {
            Ok(()) => {}
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        },
        Some(Command::Config {
            query: ConfigQuery::Path,
        }) => match sdf_offline::Config::config_path() {
            Ok(path) => println!("{}", path.display()),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        },
    }
}

fn start_gui() {
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
