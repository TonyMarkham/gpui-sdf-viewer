use crate::{renderer::Renderer, scene::SdfScene};

use gpui::{Context, RenderImage};
use std::{sync::Arc, time::Instant};

/// Default mip-level selector (`params.x`): sample the field at level 0.
const FIELD_LEVEL_DEFAULT: f32 = 0.0;

/// Default contour-overlay band width (`params.y`): the pass is off.
const CONTOUR_BAND_DEFAULT: f32 = 0.0;

/// The state behind an [`SdfCanvas`](crate::SdfCanvas): the wgpu renderer, the active scene, the
/// last presented frame, and status information for host UIs.
pub struct State {
    pub(crate) renderer: Option<Renderer>,
    pub(crate) renderer_error: Option<String>,
    pub(crate) scene: Option<SdfScene>,
    pub(crate) compile_error: Option<String>,
    pub(crate) frame: Option<Arc<RenderImage>>,
    pub(crate) size: (u32, u32),
    pub(crate) mouse_uv: [f32; 2],
    pub(crate) field_level: f32,
    pub(crate) contour_band: f32,
    pub(crate) animated: bool,
    pub(crate) start: Instant,
    pub(crate) last_present: Option<Instant>,
    pub(crate) fps: f32,
}

impl State {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            renderer: None,
            renderer_error: None,
            scene: None,
            compile_error: None,
            frame: None,
            size: (0, 0),
            mouse_uv: [0.0, 0.0],
            field_level: FIELD_LEVEL_DEFAULT,
            contour_band: CONTOUR_BAND_DEFAULT,
            animated: false,
            start: Instant::now(),
            last_present: None,
            fps: 0.0,
        }
    }

    /// Selects the scene to render. Cheap when the scene is unchanged —
    /// including its data binding, so two scenes sharing a body but not a
    /// manifest entry still switch.
    pub fn set_scene(&mut self, scene: SdfScene, cx: &mut Context<Self>) {
        let unchanged = self.scene.as_ref().is_some_and(|current| {
            current.source() == scene.source() && current.data_key() == scene.data_key()
        });
        if unchanged {
            return;
        }
        self.animated = scene.animated();
        self.compile_error = None;
        self.scene = Some(scene);
        cx.notify();
    }

    /// Name of the active scene, if any.
    pub fn scene_name(&self) -> Option<&str> {
        self.scene.as_ref().map(SdfScene::name)
    }

    /// Adapter summary of the active renderer, if it came up.
    pub fn adapter_info(&self) -> Option<&str> {
        self.renderer.as_ref().map(Renderer::adapter_summary)
    }

    /// The active failure, if any: renderer bring-up, scene compilation, or
    /// data loading.
    pub fn error(&self) -> Option<&str> {
        self.compile_error
            .as_deref()
            .or(self.renderer_error.as_deref())
    }

    /// Size of the last rendered frame in device pixels.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Smoothed frames-per-second of the last presented frames, once warmed
    /// up.
    pub fn fps(&self) -> Option<f32> {
        (self.fps > 0.0).then_some(self.fps)
    }

    /// Whether the active scene binds game data (and so gets the contour
    /// overlay and a meaningful mip-level selector).
    pub fn has_data(&self) -> bool {
        self.scene
            .as_ref()
            .is_some_and(|scene| scene.data().is_some())
    }

    /// Number of mip levels the active scene's data declares, for UI ranges.
    pub fn field_level_count(&self) -> Option<u32> {
        self.scene
            .as_ref()
            .and_then(|scene| scene.data())
            .map(|data| data.level_count())
    }

    /// The active mip-level selector: a 0..1 position across the field's
    /// chain.
    pub fn field_level(&self) -> f32 {
        self.field_level
    }

    /// Selects the mip level the field helpers sample, as a 0..1 position
    /// across the active data's chain; a no-op without an active scene.
    pub fn set_field_level(&mut self, level: f32, cx: &mut Context<Self>) {
        let level = level.clamp(0.0, 1.0);
        if (self.field_level - level).abs() <= f32::EPSILON {
            return;
        }
        self.field_level = level;
        cx.notify();
    }

    /// The active contour-overlay band width (`params.y`); 0.0 means off.
    pub fn contour_band(&self) -> f32 {
        self.contour_band
    }

    /// Sets the contour-overlay band width (`params.y`); 0.0 skips the pass.
    pub fn set_contour_band(&mut self, band: f32, cx: &mut Context<Self>) {
        let band = band.max(0.0);
        if (self.contour_band - band).abs() <= f32::EPSILON {
            return;
        }
        self.contour_band = band;
        cx.notify();
    }
}
