/// Per-frame inputs for the renderer, in canvas UV space where applicable.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct FrameRequest {
    pub width: u32,
    pub height: u32,
    pub time: f32,
    pub mouse: [f32; 2],
    /// The view in field-uv space: the field-uv point at the viewport
    /// center and the zoom factor (1.0 = the fitted full-field view).
    pub view_center: [f32; 2],
    pub view_zoom: f32,
    /// `params.x` — the field mip-level selector; `params.y` — the contour
    /// overlay band width in field bytes (0.0 = overlay off).
    pub params: [f32; 2],
}
