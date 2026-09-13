use gpui::RenderImage;
use std::sync::Arc;

pub enum Presentation {
    /// Nothing to do this frame (no scene, or the renderer failed to come up).
    Idle,
    /// A fresh frame was submitted and landed during this paint.
    Painted {
        image: Arc<RenderImage>,
        previous: Option<Arc<RenderImage>>,
    },
    /// A submission is in flight; another frame is needed to present it.
    Pending,
}
