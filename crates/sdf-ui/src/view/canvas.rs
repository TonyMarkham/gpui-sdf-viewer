use crate::{
    AppState,
    constants::{CANVAS_ERROR_MAX_WIDTH, CANVAS_ERROR_TEXT_SIZE, CANVAS_HOST_SELECTOR},
};

use gpui::{
    App, Entity, InteractiveElement as _, IntoElement, ParentElement, RenderOnce, Styled, Window,
    div,
};
use gpui_component::{ActiveTheme, h_flex};
use sdf_component::sdf_canvas;

/// The main content area: the SDF canvas fills it; while the renderer or the
/// active scene is broken, an inline failure message is overlaid.
#[derive(IntoElement)]
pub(crate) struct CanvasHost {
    app_state: Entity<AppState>,
}

impl CanvasHost {
    pub(crate) fn new(app_state: Entity<AppState>) -> Self {
        Self { app_state }
    }
}

impl ParentElement for CanvasHost {
    fn extend(&mut self, _elements: impl IntoIterator<Item = gpui::AnyElement>) {}
}

impl RenderOnce for CanvasHost {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let canvas = self.app_state.read(cx).canvas().clone();
        let error = canvas.read(cx).error().map_or_else(
            || {
                self.app_state
                    .read(cx)
                    .load_error()
                    .map_or_else(String::new, ToString::to_string)
            },
            ToString::to_string,
        );

        let mut host = div()
            .id("canvas-host")
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .debug_selector(|| CANVAS_HOST_SELECTOR.to_string())
            .child(sdf_canvas(canvas.clone()));

        if !error.is_empty() {
            host = host.child(
                div()
                    .absolute()
                    .size_full()
                    .top_0()
                    .left_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .p_4()
                    .child(
                        h_flex()
                            .max_w(CANVAS_ERROR_MAX_WIDTH)
                            .text_color(cx.theme().danger)
                            .text_size(CANVAS_ERROR_TEXT_SIZE)
                            .child(error),
                    ),
            );
        }

        host
    }
}
