/// Per-frame inputs for the renderer, in canvas UV space where applicable.
pub(crate) struct FrameRequest {
    pub width: u32,
    pub height: u32,
    pub time: f32,
    pub mouse: [f32; 2],
    /// `params.x` — the field mip-level selector; `params.y` — the contour
    /// overlay band width in field bytes (0.0 = overlay off).
    pub params: [f32; 2],
}
