mod button;

// ---------------------------------------------------------------------------------------------- //

use crate::{
    AppState,
    constants::{
        NAVIGATION_GAME_FOLDER_BUTTON_ID, NAVIGATION_GAME_FOLDER_LABEL,
        NAVIGATION_HEADER_TEXT_SIZE, NAVIGATION_SCENES_HEADER, NAVIGATION_SELECTOR,
        NAVIGATION_WIDTH,
    },
};

use gpui::{
    App, Entity, InteractiveElement as _, IntoElement, ParentElement, RenderOnce, Styled, Window,
    div,
};
use gpui_component::{
    ActiveTheme, IconName, Selectable, Sizable,
    button::{Button, ButtonVariants},
};

/// Vertical strip listing every discoverable SDF scene; clicking one selects
/// and loads it into the canvas. The game-folder button at the bottom reopens
/// the picker at any time.
#[derive(IntoElement)]
pub(crate) struct Navigation {
    app_state: Entity<AppState>,
}

impl Navigation {
    pub(crate) fn new(app_state: Entity<AppState>) -> Self {
        Self { app_state }
    }
}

impl RenderOnce for Navigation {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let active = self.app_state.read(cx).active();
        let names = self.app_state.read(cx).scene_names();
        let offline = self.app_state.read(cx).offline().clone();

        let mut list = div().flex().flex_col().gap_2().items_center();
        for (index, name) in names.iter().enumerate() {
            let is_active = active == Some(index);
            let state = self.app_state.clone();
            let tooltip = name.clone();

            let button = Button::new(index)
                .with_size(NAVIGATION_WIDTH)
                .custom(button::variant(is_active, cx))
                .icon(IconName::Palette)
                .tooltip(tooltip)
                .selected(is_active)
                .on_click(move |_, _, cx| {
                    state.update(cx, |state, cx| state.select(index, cx));
                });

            list = list.child(button);
        }

        div()
            .flex()
            .flex_col()
            .h_full()
            .w(NAVIGATION_WIDTH)
            .flex_shrink_0()
            .border_r_1()
            .border_color(theme.border)
            .bg(theme.sidebar)
            .text_color(theme.sidebar_foreground)
            .debug_selector(|| NAVIGATION_SELECTOR.to_string())
            .child(
                div()
                    .w_full()
                    .flex()
                    .justify_center()
                    .pt_2()
                    .text_size(NAVIGATION_HEADER_TEXT_SIZE)
                    .text_color(theme.muted_foreground)
                    .child(NAVIGATION_SCENES_HEADER),
            )
            .child(list.flex_1())
            .child(
                div().w_full().flex().justify_center().pb_2().child(
                    Button::new(NAVIGATION_GAME_FOLDER_BUTTON_ID)
                        .with_size(NAVIGATION_WIDTH)
                        .custom(button::variant(false, cx))
                        .icon(IconName::Folder)
                        .tooltip(NAVIGATION_GAME_FOLDER_LABEL)
                        .debug_selector(|| String::from(NAVIGATION_GAME_FOLDER_BUTTON_ID))
                        .on_click(move |_, _, cx| {
                            offline.update(cx, |state, cx| state.choose_game_folder(cx));
                        }),
                ),
            )
    }
}
