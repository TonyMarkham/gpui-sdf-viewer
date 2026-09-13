use crate::{
    constants::{CONTOUR_BAND_DEFAULT, CONTOUR_BAND_MAX, NAVIGATION_COMPOSITE_HEADER},
    state::{offline::OfflineState, scene::CompositeSpec, scene::SceneLibrary},
};
use gpui_component::slider::{SliderEvent, SliderState};
use sdf_component::SdfCanvasState;

use gpui::{App as GpuiApp, AppContext as _, Context, Entity};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------------------------- //

/// Top-level application state: the offline config/status, the scene library,
/// the synthetic composite spec, the active scene, the data-scene controls,
/// and the SDF canvas the root view renders.
pub struct App {
    offline: Entity<OfflineState>,
    library: SceneLibrary,
    composite: Option<CompositeSpec>,
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
        let composite = CompositeSpec::from_layers(offline.read(cx).composite_layers());
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
            composite,
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
        if state.library.len() > 0 || state.composite.is_some() {
            // The first file scene is the startup selection; with no file
            // scenes at all, the composite is the config-driven default view.
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

    /// Names of all discoverable scenes, in display order: the file scenes
    /// alphabetically, then the synthetic Composite entry when configured.
    pub fn scene_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .library
            .entries()
            .iter()
            .map(|entry| entry.name.clone())
            .collect();
        if self.composite.is_some() {
            names.push(String::from(NAVIGATION_COMPOSITE_HEADER));
        }
        names
    }

    /// Whether the synthetic composite entry is on the list.
    pub fn has_composite(&self) -> bool {
        self.composite.is_some()
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
    /// already active. The index past the last file scene is the composite
    /// slot, valid only when the spec is non-empty.
    pub fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.active == Some(index) {
            return;
        }
        let composite_slot = self.composite.is_some() && index == self.library.len();
        if index >= self.library.len() && !composite_slot {
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
            // Config does not change mid-run; re-deriving here is for
            // symmetry with the library re-discovery.
            self.composite = CompositeSpec::from_layers(self.offline.read(cx).composite_layers());
            self.active = if self.library.len() > 0 || self.composite.is_some() {
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

        if self.composite.is_some() && active == self.library.len() {
            self.load_composite(cx);
            return;
        }

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

    /// Loads the synthetic composite: assembles the scene source from the
    /// config's layer stack, stages it through the `config:` rewrite path,
    /// and parses it. Load failures surface in `load_error`/status bar
    /// exactly like scene loads.
    fn load_composite(&mut self, cx: &mut Context<Self>) {
        let Some(spec) = self.composite.clone() else {
            return;
        };
        match self.offline.read(cx).data_dir() {
            Err(error) => self.load_error = Some(error.to_string()),
            Ok(data_dir) => match spec.stage(&data_dir) {
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
