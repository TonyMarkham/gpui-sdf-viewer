use crate::{
    AppState,
    constants::{
        STATUS_BAR_HEIGHT, STATUS_BAR_PADDING, STATUS_BAR_SELECTOR, STATUS_BAR_TEXT_SIZE,
        STATUS_NO_ADAPTER, STATUS_NO_SCENE, STATUS_SEPARATOR,
    },
};

use gpui::{
    App, Entity, InteractiveElement as _, IntoElement, ParentElement, RenderOnce, Styled, Window,
    div,
};
use gpui_component::ActiveTheme;

/// Bottom strip summarizing the renderer: adapter, frame size, scene name,
/// FPS, and any active failure.
#[derive(IntoElement)]
pub(crate) struct StatusBar {
    app_state: Entity<AppState>,
}

impl StatusBar {
    pub(crate) fn new(app_state: Entity<AppState>) -> Self {
        Self { app_state }
    }
}

impl ParentElement for StatusBar {
    fn extend(&mut self, _elements: impl IntoIterator<Item = gpui::AnyElement>) {}
}

impl RenderOnce for StatusBar {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let canvas = self.app_state.read(cx).canvas().read(cx);

        let adapter = canvas
            .adapter_info()
            .unwrap_or(STATUS_NO_ADAPTER)
            .to_string();
        let (width, height) = canvas.size();
        let size = if width > 0 {
            format!("{width}×{height}")
        } else {
            String::from("-")
        };
        let fps = canvas
            .fps()
            .map_or_else(String::new, |fps| format!("{fps:.0} fps"));
        let scene = canvas
            .scene_name()
            .map_or_else(|| String::from(STATUS_NO_SCENE), ToString::to_string);
        let error = canvas.error().map_or_else(
            || {
                self.app_state
                    .read(cx)
                    .load_error()
                    .map_or_else(String::new, ToString::to_string)
            },
            ToString::to_string,
        );

        let status = if error.is_empty() {
            let fps_segment = if fps.is_empty() {
                String::new()
            } else {
                format!("{STATUS_SEPARATOR}{fps}")
            };
            format!("{scene}{STATUS_SEPARATOR}{adapter}{STATUS_SEPARATOR}{size} px{fps_segment}")
        } else {
            format!("{scene}{STATUS_SEPARATOR}{error}")
        };

        div()
            .id("status-bar")
            .flex()
            .items_center()
            .h(STATUS_BAR_HEIGHT)
            .px(STATUS_BAR_PADDING)
            .flex_shrink_0()
            .border_t_1()
            .border_color(theme.border)
            .bg(theme.status_bar)
            .text_size(STATUS_BAR_TEXT_SIZE)
            .text_color(theme.muted_foreground)
            .debug_selector(|| STATUS_BAR_SELECTOR.to_string())
            .child(status)
    }
}
