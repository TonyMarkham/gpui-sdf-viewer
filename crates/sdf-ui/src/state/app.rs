use crate::{
    constants::{CONTOUR_BAND_DEFAULT, CONTOUR_BAND_MAX},
    state::scene::SceneLibrary,
};
use gpui_component::slider::{SliderEvent, SliderState};
use sdf_component::SdfCanvasState;

use gpui::{AppContext as _, Context, Entity};

/// Top-level application state: the scene library, the active scene, the
/// data-scene controls, and the SDF canvas the root view renders.
pub struct App {
    library: SceneLibrary,
    active: Option<usize>,
    active_name: Option<String>,
    load_error: Option<String>,
    canvas: Entity<SdfCanvasState>,
    level_slider: Entity<SliderState>,
    density_slider: Entity<SliderState>,
    contour_enabled: bool,
}

impl App {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let library = SceneLibrary::discover();
        let canvas = cx.new(SdfCanvasState::new);
        let level_slider = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(1.0)
                .step(0.01)
                .default_value(0.0)
        });
        let density_slider = cx.new(|_| {
            SliderState::new()
                .min(1.0)
                .max(CONTOUR_BAND_MAX)
                .step(1.0)
                .default_value(CONTOUR_BAND_DEFAULT)
        });

        cx.subscribe(&level_slider, Self::on_level_change).detach();
        cx.subscribe(&density_slider, Self::on_density_change)
            .detach();

        let mut state = Self {
            library,
            active: None,
            active_name: None,
            load_error: None,
            canvas,
            level_slider,
            density_slider,
            contour_enabled: false,
        };
        if state.library.len() > 0 {
            state.active = Some(0);
        }
        state.load_active(cx);
        state
    }

    pub fn canvas(&self) -> &Entity<SdfCanvasState> {
        &self.canvas
    }

    /// The mip-level selector control (a 0..1 position across the field's
    /// chain).
    pub fn level_slider(&self) -> &Entity<SliderState> {
        &self.level_slider
    }

    /// The contour band density control (band width in field bytes).
    pub fn density_slider(&self) -> &Entity<SliderState> {
        &self.density_slider
    }

    /// Whether the contour overlay is switched on.
    pub fn contour_enabled(&self) -> bool {
        self.contour_enabled
    }

    /// Names of all discoverable scenes, in display order.
    pub fn scene_names(&self) -> Vec<String> {
        self.library
            .entries()
            .iter()
            .map(|entry| entry.name.clone())
            .collect()
    }

    /// Index of the active scene, if one is selected.
    pub fn active(&self) -> Option<usize> {
        self.active
    }

    /// The failure that prevented the active scene from loading, if any.
    pub fn load_error(&self) -> Option<&str> {
        self.load_error.as_deref()
    }

    /// Selects and loads the scene at `index`; no-op when out of range or
    /// already active.
    pub fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.active == Some(index) {
            return;
        }
        if index >= self.library.len() {
            return;
        }
        self.active = Some(index);
        self.load_active(cx);
        cx.notify();
    }

    /// Toggles the contour overlay: on binds the density slider's band width
    /// to the canvas, off zeroes it (the renderer skips the pass).
    pub fn toggle_contour(&mut self, cx: &mut Context<Self>) {
        self.contour_enabled = !self.contour_enabled;
        let band = if self.contour_enabled {
            self.density_slider.read(cx).value().end()
        } else {
            0.0
        };
        self.canvas
            .update(cx, |canvas, cx| canvas.set_contour_band(band, cx));
        cx.notify();
    }

    fn on_level_change(
        &mut self,
        _slider: Entity<SliderState>,
        event: &SliderEvent,
        cx: &mut Context<Self>,
    ) {
        let SliderEvent::Change(value) = event else {
            return;
        };
        let level = value.end();
        self.canvas
            .update(cx, |canvas, cx| canvas.set_field_level(level, cx));
        cx.notify();
    }

    fn on_density_change(
        &mut self,
        _slider: Entity<SliderState>,
        event: &SliderEvent,
        cx: &mut Context<Self>,
    ) {
        let SliderEvent::Change(value) = event else {
            return;
        };
        let band = value.end();
        if self.contour_enabled {
            self.canvas
                .update(cx, |canvas, cx| canvas.set_contour_band(band, cx));
        }
        cx.notify();
    }

    fn load_active(&mut self, cx: &mut Context<Self>) {
        self.load_error = None;
        self.active_name = None;
        let Some(active) = self.active else {
            return;
        };

        match self.library.load(active) {
            Ok(scene) => {
                self.active_name = Some(String::from(scene.name()));
                self.canvas
                    .update(cx, |canvas, cx| canvas.set_scene(scene, cx));
            }
            Err(error) => {
                self.load_error = Some(error.to_string());
            }
        }
    }
}
