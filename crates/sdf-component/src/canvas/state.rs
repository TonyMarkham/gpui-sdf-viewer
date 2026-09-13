use crate::{renderer::Renderer, scene::SdfScene};

use gpui::{Context, RenderImage};
use std::{sync::Arc, time::Instant};

/// Default mip-level selector (`params.x`): sample the field at level 0.
const FIELD_LEVEL_DEFAULT: f32 = 0.0;

/// Default contour-overlay band width (`params.y`): the pass is off.
const CONTOUR_BAND_DEFAULT: f32 = 0.0;

/// Zoom bounds for the map view: the field fits at the floor; past the
/// ceiling the Inspect scene is already showing texel-nearest blocks.
pub(crate) const VIEW_ZOOM_MIN: f32 = 1.0;
pub(crate) const VIEW_ZOOM_MAX: f32 = 64.0;

/// The default view: the fitted full-field mapping (identity).
const VIEW_CENTER_DEFAULT: [f32; 2] = [0.5, 0.5];
const VIEW_ZOOM_DEFAULT: f32 = 1.0;

/// Viewport clamp keeping the field covering the viewport: `center ∈
/// [0.5/z, 1 - 0.5/z]` per axis. At the zoom floor this forces
/// `center = [0.5, 0.5]` — exactly the identity view — so the floor and the
/// pan can never disagree into a stuck-off-center view.
pub(crate) fn clamped_view_center(center: [f32; 2], zoom: f32) -> [f32; 2] {
    let low = 0.5 / zoom;
    let high = 1.0 - low;
    [center[0].clamp(low, high), center[1].clamp(low, high)]
}

/// The view center that zooms `z0 → z1` about `anchor` (a viewport fraction,
/// which is the base field-uv mapping of the cursor): the world uv under the
/// anchor stays fixed, `center' = center + (anchor - 0.5) * (1/z0 - 1/z1)`.
pub(crate) fn zoomed_view_center(center: [f32; 2], anchor: [f32; 2], z0: f32, z1: f32) -> [f32; 2] {
    [
        center[0] + (anchor[0] - 0.5) * (1.0 / z0 - 1.0 / z1),
        center[1] + (anchor[1] - 0.5) * (1.0 / z0 - 1.0 / z1),
    ]
}

/// The view center after a drag by `delta` (viewport fractions, screen
/// orientation): content follows the cursor, so the center moves against the
/// delta, scaled by the zoom. The uv y axis is screen-aligned — the prelude's
/// `FIELD_V_FLIP` makes uv y run top-down like screen y — so the delta
/// carries over per axis unflipped.
pub(crate) fn panned_view_center(center: [f32; 2], delta: [f32; 2], zoom: f32) -> [f32; 2] {
    [center[0] - delta[0] / zoom, center[1] - delta[1] / zoom]
}

/// The per-scroll-event zoom factor: multiplicative on the raw pixel delta,
/// `exp2(-delta_y * sensitivity)`. The negative sensitivity makes wheel-up
/// (positive pixel delta) zoom in, matching map conventions.
const ZOOM_WHEEL_SENSITIVITY: f32 = -0.0054;

/// Rejects absurd wheel deltas.
const ZOOM_WHEEL_FACTOR_MIN: f32 = 0.5;
const ZOOM_WHEEL_FACTOR_MAX: f32 = 2.0;

/// The zoom factor one scroll event applies, given the event's pixel delta
/// (already scaled by `window.line_height()`). Calibrated so one notched
/// wheel step (≈ 3 lines at a ~20px line height, ≈ 60px) lands near ×1.25;
/// the math is multiplicative on raw pixel deltas, so trackpads zoom
/// continuously. Per-event factors clamp to [0.5, 2.0].
pub(crate) fn wheel_zoom_factor(pixel_delta_y: f32) -> f32 {
    (-pixel_delta_y * ZOOM_WHEEL_SENSITIVITY)
        .exp2()
        .clamp(ZOOM_WHEEL_FACTOR_MIN, ZOOM_WHEEL_FACTOR_MAX)
}

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
    /// The map view in field-uv space: the field-uv point at the viewport
    /// center and the zoom factor. Stored uv-fraction-based so window
    /// resizes re-render the same uv window instead of shifting it.
    pub(crate) view_center: [f32; 2],
    pub(crate) view_zoom: f32,
    /// Left-drag bookkeeping: whether a pan drag is running and the last
    /// cursor position as a viewport fraction.
    pub(crate) dragging: bool,
    pub(crate) drag_cursor: [f32; 2],
    pub(crate) animated: bool,
    pub(crate) start: Instant,
    pub(crate) last_present: Option<Instant>,
    pub(crate) fps: f32,
    /// The mip level last submitted to the renderer (`params.x`), recorded by
    /// the paint loop on each successful render. `None` until the renderer
    /// first submits for the active scene.
    pub(crate) submitted_field_level: Option<f32>,
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
            view_center: VIEW_CENTER_DEFAULT,
            view_zoom: VIEW_ZOOM_DEFAULT,
            dragging: false,
            drag_cursor: [0.0, 0.0],
            animated: false,
            start: Instant::now(),
            last_present: None,
            fps: 0.0,
            submitted_field_level: None,
        }
    }

    /// Selects the scene to render. Cheap when the scene is unchanged —
    /// including its data binding, so two scenes sharing a body but not a
    /// manifest entry still switch. Each scene change opens fitted: the view
    /// resets to the identity, deterministically.
    pub fn set_scene(&mut self, scene: SdfScene, cx: &mut Context<Self>) {
        let unchanged = self.scene.as_ref().is_some_and(|current| {
            current.source() == scene.source() && current.data_key() == scene.data_key()
        });
        if unchanged {
            return;
        }
        self.animated = scene.animated();
        self.compile_error = None;
        self.reset_view_state();
        self.submitted_field_level = None;
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

    /// Identity of the active scene's data binding, if any: the manifest and
    /// the ordered layer names it binds (see `SdfScene::data_key`).
    pub fn data_key(&self) -> Option<String> {
        self.scene.as_ref().and_then(SdfScene::data_key)
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

    /// The mip level the paint loop last submitted to the renderer
    /// (`params.x`): the LOD slider's base bias refined by the map view's
    /// zoom, as derived at the render call site.
    pub fn submitted_field_level(&self) -> Option<f32> {
        self.submitted_field_level
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

    /// The map view's center in field-uv space (the field-uv point at the
    /// viewport center).
    pub fn view_center(&self) -> [f32; 2] {
        self.view_center
    }

    /// The map view's zoom factor; 1.0 is the fitted full-field view.
    pub fn view_zoom(&self) -> f32 {
        self.view_zoom
    }

    /// Zooms about `anchor` (a viewport fraction — the base field-uv mapping
    /// of the cursor), keeping the world uv under the anchor fixed. The zoom
    /// clamps to [`VIEW_ZOOM_MIN`], [`VIEW_ZOOM_MAX`]; the center clamps to
    /// the viewport.
    pub fn zoom_at(&mut self, anchor: [f32; 2], zoom: f32, cx: &mut Context<Self>) {
        let zoom = zoom.clamp(VIEW_ZOOM_MIN, VIEW_ZOOM_MAX);
        if (zoom - self.view_zoom).abs() <= f32::EPSILON {
            return;
        }
        let center = zoomed_view_center(self.view_center, anchor, self.view_zoom, zoom);
        self.view_zoom = zoom;
        self.view_center = clamped_view_center(center, zoom);
        cx.notify();
    }

    /// Pans by `delta` (viewport fractions, screen orientation — positive y
    /// is down, matching the uv flip): content follows the cursor, clamped to
    /// keep the field covering the viewport.
    pub fn pan_by(&mut self, delta: [f32; 2], cx: &mut Context<Self>) {
        let center = clamped_view_center(
            panned_view_center(self.view_center, delta, self.view_zoom),
            self.view_zoom,
        );
        if (center[0] - self.view_center[0]).abs() <= f32::EPSILON
            && (center[1] - self.view_center[1]).abs() <= f32::EPSILON
        {
            return;
        }
        self.view_center = center;
        cx.notify();
    }

    /// Resets the view to the fitted full-field identity.
    pub fn reset_view(&mut self, cx: &mut Context<Self>) {
        if self.view_center == VIEW_CENTER_DEFAULT && self.view_zoom == VIEW_ZOOM_DEFAULT {
            return;
        }
        self.reset_view_state();
        cx.notify();
    }

    /// Begins a left-drag pan at `cursor` (a viewport fraction).
    pub(crate) fn begin_drag(&mut self, cursor: [f32; 2]) {
        self.dragging = true;
        self.drag_cursor = cursor;
    }

    /// Ends the drag; the mouse-up handler calls this regardless of where the
    /// release landed.
    pub(crate) fn end_drag(&mut self) {
        self.dragging = false;
    }

    /// Moves an active drag to `cursor`: pans by the cursor delta and records
    /// the new position. Moves outside the canvas keep panning (clamped), so
    /// the gesture does not stutter at the canvas edge.
    pub(crate) fn drag_move(&mut self, cursor: [f32; 2], cx: &mut Context<Self>) {
        if !self.dragging {
            return;
        }
        let delta = [
            cursor[0] - self.drag_cursor[0],
            cursor[1] - self.drag_cursor[1],
        ];
        self.drag_cursor = cursor;
        self.pan_by(delta, cx);
    }

    fn reset_view_state(&mut self) {
        self.view_center = VIEW_CENTER_DEFAULT;
        self.view_zoom = VIEW_ZOOM_DEFAULT;
        self.dragging = false;
        self.drag_cursor = [0.0, 0.0];
    }
}
