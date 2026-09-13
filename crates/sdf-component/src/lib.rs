//! A reusable signed-distance-field rendering component for gpui.
//!
//! The component owns a wgpu pipeline that evaluates user-supplied SDF scenes
//! (see [`SdfScene`]) into an offscreen texture, reads the frames back to the
//! CPU asynchronously, and presents them inside gpui windows as ordinary
//! images. See [`SdfCanvas`] and [`SdfCanvasState`].

mod canvas;
mod data;
mod error;
mod overlay;
mod renderer;
mod scene;
pub(crate) mod utilities;

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------------------------- //

pub use crate::{
    canvas::{
        Canvas as SdfCanvas, presentation::Presentation, sdf_canvas, state::State as SdfCanvasState,
    },
    data::{level::Level, scene_data::SceneData, scene_data::SceneField, scene_tile::SceneTile},
    error::{Error as SdfError, result::Result as SdfResult},
    scene::SdfScene,
};
