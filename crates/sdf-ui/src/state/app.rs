use crate::{
    constants::{CONTOUR_BAND_DEFAULT, CONTOUR_BAND_MAX},
    state::{offline::OfflineState, scene::SceneLibrary},
};
use gpui_component::slider::{SliderEvent, SliderState};
use sdf_component::SdfCanvasState;

use gpui::{App as GpuiApp, AppContext as _, Context, Entity};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------------------------- //

/// Top-level application state: the offline config/status, the scene library,
/// the active scene, the data-scene controls, and the SDF canvas the root
/// view renders.
pub struct App {
    offline: Entity<OfflineState>,
    library: SceneLibrary,
    scenes_dir: Option<PathBuf>,
    active: Option<usize>,
    active_name: Option<String>,
    load_error: Option<String>,
    canvas: Entity<SdfCanvasState>,
    level_slider: Entity<SliderState>,
    density_slider: Entity<SliderState>,
    contour_enabled: bool,
    extraction_done_seen: bool,
}

impl App {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let offline = cx.new(|_| OfflineState::load());
        Self::build(offline, None, cx)
    }

    /// Test seam: an app state over an injected offline entity and an
    /// explicit scenes directory.
    pub(crate) fn build(
        offline: Entity<OfflineState>,
        scenes_dir: Option<&Path>,
        cx: &mut Context<Self>,
    ) -> Self {
        let library = match scenes_dir {
            Some(directory) => SceneLibrary::discover_in(directory),
            None => SceneLibrary::discover(),
        };
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
            offline,
            library,
            scenes_dir: scenes_dir.map(Path::to_path_buf),
            active: None,
            active_name: None,
            load_error: None,
            canvas,
            level_slider,
            density_slider,
            contour_enabled: false,
            extraction_done_seen: false,
        };
        cx.observe(&state.offline, Self::on_offline_changed)
            .detach();

        if state.offline.read(cx).needs_setup() {
            return state;
        }
        if state.library.len() > 0 {
            state.active = Some(0);
        }
        state.load_active(cx);
        state
    }

    pub(crate) fn offline(&self) -> &Entity<OfflineState> {
        &self.offline
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

    /// Whether the shell must show the setup panel instead of the canvas.
    pub fn needs_setup(&self, cx: &GpuiApp) -> bool {
        self.offline.read(cx).needs_setup()
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

    fn on_offline_changed(&mut self, _offline: Entity<OfflineState>, cx: &mut Context<Self>) {
        let finished = self.offline.read(cx).extraction_finished();
        if finished && !self.extraction_done_seen {
            self.extraction_done_seen = true;
            self.library = match self.scenes_dir.as_deref() {
                Some(directory) => SceneLibrary::discover_in(directory),
                None => SceneLibrary::discover(),
            };
            self.active = if self.library.len() > 0 {
                Some(0)
            } else {
                None
            };
            self.load_active(cx);
            cx.notify();
        } else if !finished {
            self.extraction_done_seen = false;
        }
    }

    fn load_active(&mut self, cx: &mut Context<Self>) {
        self.load_error = None;
        self.active_name = None;
        let Some(active) = self.active else {
            return;
        };

        match self.offline.read(cx).data_dir() {
            Err(error) => self.load_error = Some(error.to_string()),
            Ok(data_dir) => match self.library.load(active, &data_dir) {
                Ok(scene) => {
                    self.active_name = Some(String::from(scene.name()));
                    self.canvas
                        .update(cx, |canvas, cx| canvas.set_scene(scene, cx));
                }
                Err(error) => {
                    self.load_error = Some(error.to_string());
                }
            },
        }
    }
}
