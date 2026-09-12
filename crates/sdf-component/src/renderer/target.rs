use wgpu::Texture;

/// The render target backing the current frame size.
pub(crate) struct Target {
    pub(crate) texture: Texture,
    pub(crate) width: u32,
    pub(crate) height: u32,
}
