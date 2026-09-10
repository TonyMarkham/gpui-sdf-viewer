use crate::state::scene::SceneLibrary;
use sdf_component::SdfCanvasState;

use gpui::{AppContext as _, Context, Entity};

/// Top-level application state: the scene library, the active scene, and the
/// SDF canvas the root view renders.
pub struct App {
    library: SceneLibrary,
    active: Option<usize>,
    active_name: Option<String>,
    load_error: Option<String>,
    canvas: Entity<SdfCanvasState>,
}

impl App {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let library = SceneLibrary::discover();
        let canvas = cx.new(SdfCanvasState::new);

        let mut state = Self {
            library,
            active: None,
            active_name: None,
            load_error: None,
            canvas,
        };
        if state.library.len() > 0 {
            state.active = Some(0);
        }
        state.load_active(cx);
        state
    }

    pub fn canvas(&self) -> &Entity<SdfCanvasState> {
        &self.canvas
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

    fn load_active(&mut self, cx: &mut Context<Self>) {
        self.load_error = None;
        self.active_name = None;
        let Some(active) = self.active else {
            return;
        };

        match self.library.load(active) {
            Ok(scene) => {
                self.active_name = Some(String::from(scene.name()));
                self.canvas
                    .update(cx, |canvas, cx| canvas.set_scene(scene, cx));
            }
            Err(error) => {
                self.load_error = Some(error.to_string());
            }
        }
    }
}
