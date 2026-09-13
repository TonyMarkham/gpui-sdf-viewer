mod button;

// ---------------------------------------------------------------------------------------------- //

use crate::{
    AppState,
    constants::{
        NAVIGATION_COMPOSITE_BUTTON_ID, NAVIGATION_COMPOSITE_HEADER, NAVIGATION_COMPOSITE_SELECTOR,
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
        let has_composite = self.app_state.read(cx).has_composite();
        let offline = self.app_state.read(cx).offline().clone();
        let scene_count = names.len() - usize::from(has_composite);

        let mut list = div().flex().flex_col().gap_2().items_center();
        for (index, name) in names.iter().take(scene_count).enumerate() {
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

        let mut shell = div()
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
            .child(list.flex_1());

        if has_composite {
            let composite_index = scene_count;
            let is_active = active == Some(composite_index);
            let state = self.app_state.clone();

            let button = Button::new(NAVIGATION_COMPOSITE_BUTTON_ID)
                .with_size(NAVIGATION_WIDTH)
                .custom(button::variant(is_active, cx))
                .icon(IconName::Map)
                .tooltip(NAVIGATION_COMPOSITE_HEADER)
                .selected(is_active)
                .debug_selector(|| String::from(NAVIGATION_COMPOSITE_BUTTON_ID))
                .on_click(move |_, _, cx| {
                    state.update(cx, |state, cx| state.select(composite_index, cx));
                });

            shell = shell.child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .pb_2()
                    .debug_selector(|| NAVIGATION_COMPOSITE_SELECTOR.to_string())
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .justify_center()
                            .text_size(NAVIGATION_HEADER_TEXT_SIZE)
                            .text_color(theme.muted_foreground)
                            .child(NAVIGATION_COMPOSITE_HEADER),
                    )
                    .child(button),
            );
        }

        shell.child(
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
