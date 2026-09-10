//! A reusable signed-distance-field rendering component for gpui.
//!
//! The component owns a wgpu pipeline that evaluates user-supplied SDF scenes
//! (see [`SdfScene`]) into an offscreen texture, reads the frames back to the
//! CPU asynchronously, and presents them inside gpui windows as ordinary
//! images. See [`SdfCanvas`] and [`SdfCanvasState`].

mod canvas;
mod error;
mod renderer;
mod scene;

pub use crate::{
    canvas::{SdfCanvas, SdfCanvasState, sdf_canvas},
    error::{Error as SdfError, Result as SdfResult},
    scene::SdfScene,
};

#[cfg(test)]
mod tests;
