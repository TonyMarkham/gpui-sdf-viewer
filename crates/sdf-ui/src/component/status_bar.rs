use crate::{
    AppState,
    constants::{
        STATUS_BAR_HEIGHT, STATUS_BAR_PADDING, STATUS_BAR_SELECTOR, STATUS_BAR_TEXT_SIZE,
        STATUS_EXTRACTING, STATUS_EXTRACTION_FAILED, STATUS_GAME_DATA_MISSING,
        STATUS_GAME_FOLDER_BAD, STATUS_GAME_FOLDER_INVALID, STATUS_GAME_FOLDER_OK,
        STATUS_NO_ADAPTER, STATUS_NO_GAME_FOLDER, STATUS_NO_SCENE, STATUS_SEPARATOR,
    },
    state::offline::OfflineStatus,
};

use gpui::{
    App, Entity, InteractiveElement as _, IntoElement, ParentElement, RenderOnce, Styled, Window,
    div,
};
use gpui_component::ActiveTheme;

/// Bottom strip summarizing the offline state (game folder validity,
/// extraction progress/failures) and the renderer: adapter, frame size,
/// scene name, FPS, and any active failure.
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
        let app = self.app_state.read(cx);
        let offline = app.offline().read(cx);
        let canvas = app.canvas().read(cx);

        let mut segments = offline_segments(&offline.status());

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
                app.load_error()
                    .map_or_else(String::new, ToString::to_string)
            },
            ToString::to_string,
        );

        segments.push(scene);
        if error.is_empty() {
            segments.push(adapter);
            segments.push(format!("{size} px"));
            if !fps.is_empty() {
                segments.push(fps);
            }
        } else {
            segments.push(error);
        }

        let status = segments.join(STATUS_SEPARATOR);

        #[cfg(test)]
        crate::component::capture::set_status(&status);

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

/// The offline part of the status line: game-folder validity first (a
/// rejection, a missing folder, an invalid folder, missing extracted data, or
/// ok), then the extraction progress and failure. Takes the whole status
/// snapshot — the render call site passes it as one value, so no argument can
/// be transposed or dropped there — and stays a pure function so every branch
/// is pinned by tests.
fn offline_segments(status: &OfflineStatus) -> Vec<String> {
    let mut segments: Vec<String> = Vec::new();

    if !status.game_root_valid {
        if let Some(rejection) = &status.rejection {
            segments.push(format!("{STATUS_GAME_FOLDER_BAD}{rejection}"));
        } else if status.game_root.is_empty() {
            segments.push(String::from(STATUS_NO_GAME_FOLDER));
        } else {
            segments.push(String::from(STATUS_GAME_FOLDER_INVALID));
        }
    } else if !status.data_present {
        segments.push(String::from(STATUS_GAME_DATA_MISSING));
    } else {
        segments.push(String::from(STATUS_GAME_FOLDER_OK));
    }

    if status.extraction_running {
        segments.push(format!("{STATUS_EXTRACTING}{}", status.progress_line));
    }
    if let Some(failure) = &status.extraction_failed {
        segments.push(format!("{STATUS_EXTRACTION_FAILED}{failure}"));
    }

    segments
}

// ---------------------------------------------------------------------------------------------- //

#[cfg(test)]
mod tests;
