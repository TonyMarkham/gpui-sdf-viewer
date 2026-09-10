use crate::{
    AppState,
    component::{navigation::Navigation, status_bar::StatusBar, title_bar::TitleBar},
    constants::APPLICATION_TITLE,
    view::canvas::CanvasHost,
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
                    .child(Navigation::new(self.app_state.clone()))
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .flex_col()
                            .child(CanvasHost::new(self.app_state.clone()))
                            .child(StatusBar::new(self.app_state.clone())),
                    ),
            )
    }
}
