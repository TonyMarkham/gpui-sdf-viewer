use gpui::{Context, Entity, Window};

use crate::RootView;

/// The test window's root: renders the suite's root view.
pub(crate) struct AppHost {
    pub(crate) root: Entity<RootView>,
}

impl gpui::Render for AppHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        self.root.clone()
    }
}
