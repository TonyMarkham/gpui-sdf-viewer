mod component;
mod constants;
mod design;
mod error;
mod state;
mod view;

// ---------------------------------------------------------------------------------------------- //

pub use crate::{
    error::{Error as UiError, result::Result as UiResult},
    state::app::App as AppState,
    view::root::Root as RootView,
};

use sdf_component::SdfCanvasState;

pub use gpui_component_assets::Assets as UiAssets;

// ---------------------------------------------------------------------------------------------- //

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use crate::constants::WAYLAND_COMPOSITOR_NAME;

use gpui::{App, AppContext, Entity, Window, WindowDecorations, WindowOptions};
use gpui_component::{Root as RootComponent, Theme, TitleBar};

pub fn initialize(cx: &mut App) -> UiResult<()> {
    gpui_component::init(cx);

    let theme_config = design::theme::load()?;
    let theme = Theme::global_mut(cx);

    theme.apply_config(&theme_config);
    theme.list_active = theme.accent;
    theme.list_active_border = gpui::transparent_black();

    Ok(())
}

pub fn window_options() -> WindowOptions {
    WindowOptions {
        titlebar: Some(TitleBar::title_bar_options()),
        window_decorations: Some(window_decorations()),
        ..Default::default()
    }
}

fn window_decorations() -> WindowDecorations {
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    if gpui::guess_compositor() == WAYLAND_COMPOSITOR_NAME {
        return WindowDecorations::Client;
    }
    WindowDecorations::Server
}

pub fn build_root_view(window: &mut Window, cx: &mut App) -> Entity<RootComponent> {
    let app_state = cx.new(AppState::new);

    let root_view = cx.new(move |cx| RootView::new(app_state, cx));

    cx.new(|cx| RootComponent::new(root_view, window, cx))
}

/// Convenience alias exposing the canvas state entity type used by the shell.
pub type CanvasState = SdfCanvasState;

#[cfg(test)]
mod tests;
