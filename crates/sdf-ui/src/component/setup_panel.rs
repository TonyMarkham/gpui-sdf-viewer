use crate::{
    AppState,
    constants::{
        SETUP_EXPLANATION, SETUP_EXTRACT_BUTTON_ID, SETUP_EXTRACT_LABEL, SETUP_FAILURE_PREFIX,
        SETUP_FOLDER_LABEL, SETUP_NO_FOLDER, SETUP_PANEL_MAX_WIDTH, SETUP_PANEL_SELECTOR,
        SETUP_PICK_BUTTON_ID, SETUP_PICKER_LABEL, SETUP_REJECTION_PREFIX, SETUP_TEXT_SIZE,
    },
};

use gpui::{
    App, Entity, InteractiveElement as _, IntoElement, ParentElement, RenderOnce, SharedString,
    Styled, Window, div,
};
use gpui_component::{ActiveTheme, Disableable as _, IconName, button::Button, h_flex};

/// The first-run view: pick the Crimson Desert install folder, then run the
/// extraction. Shown in place of the canvas while the game folder is
/// missing/invalid or no extracted data exists yet.
#[derive(IntoElement)]
pub(crate) struct SetupPanel {
    app_state: Entity<AppState>,
}

impl SetupPanel {
    pub(crate) fn new(app_state: Entity<AppState>) -> Self {
        Self { app_state }
    }
}

impl ParentElement for SetupPanel {
    fn extend(&mut self, _elements: impl IntoIterator<Item = gpui::AnyElement>) {}
}

impl RenderOnce for SetupPanel {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let offline = self.app_state.read(cx).offline().clone();
        let state = offline.read(cx);

        let folder = if state.game_root().is_empty() {
            String::from(SETUP_NO_FOLDER)
        } else {
            String::from(state.game_root())
        };
        #[cfg(test)]
        let mut panel_lines = vec![format!("{SETUP_FOLDER_LABEL}{folder}")];

        let mut panel = div()
            .id("setup-panel")
            .flex()
            .flex_col()
            .gap_4()
            .max_w(SETUP_PANEL_MAX_WIDTH)
            .text_size(SETUP_TEXT_SIZE);

        panel = panel
            .child(
                div()
                    .text_color(theme.muted_foreground)
                    .child(SETUP_EXPLANATION),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_color(theme.muted_foreground)
                            .child(SETUP_FOLDER_LABEL),
                    )
                    .child(div().child(SharedString::from(folder))),
            );

        let offline_for_click = offline.clone();
        panel = panel.child(
            Button::new(SETUP_PICK_BUTTON_ID)
                .label(SETUP_PICKER_LABEL)
                .icon(IconName::FolderOpen)
                .debug_selector(|| String::from(SETUP_PICK_BUTTON_ID))
                .on_click(move |_, _, cx| {
                    offline_for_click.update(cx, |state, cx| state.choose_game_folder(cx));
                }),
        );

        if let Some(rejection) = state.rejection() {
            let line = format!("{SETUP_REJECTION_PREFIX}{rejection}");
            #[cfg(test)]
            panel_lines.push(line.clone());
            panel = panel.child(
                div()
                    .text_color(theme.danger)
                    .child(SharedString::from(line)),
            );
        }
        if let Some(failure) = state.load_failure() {
            let line = format!("{SETUP_REJECTION_PREFIX}{failure}");
            #[cfg(test)]
            panel_lines.push(line.clone());
            panel = panel.child(
                div()
                    .text_color(theme.danger)
                    .child(SharedString::from(line)),
            );
        }

        if state.game_root_valid() {
            let offline_for_extract = offline.clone();
            panel = panel.child(
                Button::new(SETUP_EXTRACT_BUTTON_ID)
                    .label(SETUP_EXTRACT_LABEL)
                    .icon(IconName::Play)
                    .disabled(state.extraction_running())
                    .debug_selector(|| String::from(SETUP_EXTRACT_BUTTON_ID))
                    .on_click(move |_, _, cx| {
                        offline_for_extract.update(cx, |state, cx| state.start_extraction(cx));
                    }),
            );
        }

        if state.extraction_running() || state.extraction_failed().is_some() {
            let mut line = String::new();
            if state.extraction_running() {
                line = String::from(state.progress_line());
            }
            if let Some(failure) = state.extraction_failed() {
                line = format!("{SETUP_FAILURE_PREFIX}{failure}");
            }
            if !line.is_empty() {
                #[cfg(test)]
                panel_lines.push(line.clone());
                panel = panel.child(
                    div()
                        .text_color(if state.extraction_failed().is_some() {
                            theme.danger
                        } else {
                            theme.muted_foreground
                        })
                        .child(SharedString::from(line)),
                );
            }
        }

        #[cfg(test)]
        crate::component::capture::set_setup_panel(panel_lines);

        div()
            .debug_selector(|| SETUP_PANEL_SELECTOR.to_string())
            .flex()
            .flex_1()
            .items_center()
            .justify_center()
            .p_4()
            .child(panel)
    }
}
