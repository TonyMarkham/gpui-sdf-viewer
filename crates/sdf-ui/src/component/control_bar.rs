use crate::{
    AppState,
    constants::{
        CONTROL_BAR_GAP, CONTROL_BAR_HEIGHT, CONTROL_BAR_PADDING, CONTROL_BAR_SELECTOR,
        CONTROL_BAR_SLIDER_WIDTH, CONTROL_BAR_TEXT_SIZE, CONTROL_CONTOUR_LABEL,
        CONTROL_LEVEL_LABEL, CONTROL_VIEW_RESET_BUTTON_ID, CONTROL_VIEW_RESET_LABEL,
    },
};

use gpui::{
    App, Entity, InteractiveElement as _, IntoElement, ParentElement, RenderOnce, Styled, Window,
    div,
};
use gpui_component::{
    ActiveTheme, Disableable as _,
    button::Button,
    slider::{Slider, SliderState},
    switch::Switch,
};

/// The strip beside the canvas carrying the data-scene controls: the mip
/// level selector (the field LOD), the contour overlay toggle, the overlay's
/// band density, and the map view's reset control with its zoom readout.
/// Inert while the active scene carries no data.
#[derive(IntoElement)]
pub(crate) struct ControlBar {
    app_state: Entity<AppState>,
    level: Entity<SliderState>,
    density: Entity<SliderState>,
}

impl ControlBar {
    pub(crate) fn new(
        app_state: Entity<AppState>,
        level: Entity<SliderState>,
        density: Entity<SliderState>,
    ) -> Self {
        Self {
            app_state,
            level,
            density,
        }
    }
}

impl ParentElement for ControlBar {
    fn extend(&mut self, _elements: impl IntoIterator<Item = gpui::AnyElement>) {}
}

impl RenderOnce for ControlBar {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let app = self.app_state.read(cx);
        let enabled = app.canvas().read(cx).has_data();
        let contour = app.contour_enabled();
        let band = app.canvas().read(cx).contour_band();
        let density_label = if contour {
            format!("{band:.0}")
        } else {
            String::from("off")
        };
        let zoom = app.canvas().read(cx).view_zoom();
        let zoom_label = format!("{zoom:.1}×");

        #[cfg(test)]
        let mut bar_lines = vec![zoom_label.clone()];
        #[cfg(test)]
        if !enabled {
            bar_lines.push(format!("{CONTROL_VIEW_RESET_LABEL} disabled"));
        }

        let bar = div()
            .id("control-bar")
            .flex()
            .items_center()
            .h(CONTROL_BAR_HEIGHT)
            .px(CONTROL_BAR_PADDING)
            .gap(CONTROL_BAR_GAP)
            .flex_shrink_0()
            .border_t_1()
            .border_color(theme.border)
            .bg(theme.status_bar)
            .text_size(CONTROL_BAR_TEXT_SIZE)
            .text_color(theme.muted_foreground)
            .debug_selector(|| CONTROL_BAR_SELECTOR.to_string())
            .child(CONTROL_LEVEL_LABEL)
            .child(
                Slider::new(&self.level)
                    .disabled(!enabled)
                    .w(CONTROL_BAR_SLIDER_WIDTH),
            )
            .child(CONTROL_CONTOUR_LABEL)
            .child(
                Switch::new("contour-overlay")
                    .checked(contour)
                    .disabled(!enabled)
                    .on_click({
                        let state = self.app_state.clone();
                        move |_, _, cx| {
                            state.update(cx, |state, cx| state.toggle_contour(cx));
                        }
                    }),
            )
            .child(
                Slider::new(&self.density)
                    .disabled(!enabled || !contour)
                    .w(CONTROL_BAR_SLIDER_WIDTH),
            )
            .child(density_label)
            .child(
                Button::new(CONTROL_VIEW_RESET_BUTTON_ID)
                    .label(CONTROL_VIEW_RESET_LABEL)
                    .disabled(!enabled)
                    .debug_selector(|| String::from(CONTROL_VIEW_RESET_BUTTON_ID))
                    .on_click({
                        let state = self.app_state.clone();
                        move |_, _, cx| {
                            state.update(cx, |state, cx| {
                                state
                                    .canvas()
                                    .update(cx, |canvas, cx| canvas.reset_view(cx));
                            });
                        }
                    }),
            )
            .child(zoom_label);

        #[cfg(test)]
        crate::component::capture::set_control_bar(bar_lines);

        bar
    }
}
