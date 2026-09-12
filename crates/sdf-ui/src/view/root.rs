use crate::{
    AppState,
    component::{
        control_bar::ControlBar, navigation::Navigation, setup_panel::SetupPanel,
        status_bar::StatusBar, title_bar::TitleBar,
    },
    constants::APPLICATION_TITLE,
};

use gpui::{
    Context, Entity, IntoElement, ParentElement, Render, Styled, Subscription, Window, div,
};
use gpui_component::ActiveTheme;

pub struct Root {
    app_state: Entity<AppState>,
    _subscriptions: Vec<Subscription>,
}

impl Root {
    pub fn new(app_state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let subscriptions = vec![cx.observe(&app_state, Self::app_state_changed)];

        Self {
            app_state,
            _subscriptions: subscriptions,
        }
    }

    fn app_state_changed(&mut self, _app_state: Entity<AppState>, cx: &mut Context<Self>) {
        cx.notify();
    }
}

impl Render for Root {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app = self.app_state.clone();

        if self.app_state.read(cx).needs_setup(cx) {
            let app = self.app_state.clone();
            return div()
                .size_full()
                .flex()
                .flex_col()
                .bg(cx.theme().background)
                .child(TitleBar::new().child(APPLICATION_TITLE))
                .child(SetupPanel::new(app.clone()))
                .child(StatusBar::new(app));
        }

        let level_slider = self.app_state.read(cx).level_slider().clone();
        let density_slider = self.app_state.read(cx).density_slider().clone();

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .child(TitleBar::new().child(APPLICATION_TITLE))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(Navigation::new(app.clone()))
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .flex_col()
                            .child(crate::view::canvas::CanvasHost::new(app.clone()))
                            .child(ControlBar::new(app.clone(), level_slider, density_slider))
                            .child(StatusBar::new(app)),
                    ),
            )
    }
}
