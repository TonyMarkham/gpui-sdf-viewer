use crate::{UiError, UiResult, constants::SCENE_DIR_ENV};
use sdf_component::SdfScene;
use soul_attr::soul;

/// Where a scene entry comes from: a user-supplied `.wgsl` file under the
/// scenes directory, or one of the component's embedded examples.
#[derive(Clone, Debug)]
pub(crate) enum SceneOrigin {
    File(std::path::PathBuf),
    Embedded(usize),
}

#[derive(Clone, Debug)]
pub(crate) struct SceneEntry {
    pub name: String,
    pub origin: SceneOrigin,
}

/// The list of scenes available to the UI plus the embedded scene sources.
pub(crate) struct SceneLibrary {
    entries: Vec<SceneEntry>,
    embedded: Vec<SdfScene>,
}

impl SceneLibrary {
    /// Scans the scenes directory for user-supplied `.wgsl` files and appends
    /// the component's embedded examples. The directory is created when
    /// missing so users have an obvious drop-in location.
    #[soul(id = "concept.sdf-scene-contract", step = "scan scenes dir")]
    pub fn discover() -> Self {
        let directory = std::env::var(SCENE_DIR_ENV)
            .unwrap_or_else(|_| crate::constants::SCENE_DIR_DEFAULT.to_string());
        let _ = std::fs::create_dir_all(&directory);

        let mut entries = Vec::new();
        if let Ok(paths) = std::fs::read_dir(&directory) {
            for path in paths.flatten() {
                let path = path.path();
                let is_wgsl = path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("wgsl"));
                if !is_wgsl {
                    continue;
                }
                if let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) {
                    entries.push(SceneEntry {
                        name: String::from(name),
                        origin: SceneOrigin::File(path),
                    });
                }
            }
            entries.sort_by(|a, b| a.name.cmp(&b.name));
        }

        let embedded = SdfScene::embedded_examples();
        for (index, scene) in embedded.iter().enumerate() {
            entries.push(SceneEntry {
                name: String::from(scene.name()),
                origin: SceneOrigin::Embedded(index),
            });
        }

        Self { entries, embedded }
    }

    pub fn entries(&self) -> &[SceneEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Loads the full scene content for the entry at `index`.
    #[soul(id = "concept.sdf-scene-contract", step = "load entry")]
    pub fn load(&self, index: usize) -> UiResult<SdfScene> {
        let Some(entry) = self.entries.get(index) else {
            return Err(UiError::scene(sdf_component::SdfError::scene_invalid(
                "scene index out of range",
            )));
        };

        match &entry.origin {
            SceneOrigin::File(path) => SdfScene::from_file(path).map_err(UiError::scene),
            SceneOrigin::Embedded(embedded_index) => {
                self.embedded.get(*embedded_index).cloned().ok_or_else(|| {
                    UiError::scene(sdf_component::SdfError::scene_invalid(
                        "embedded scene index out of range",
                    ))
                })
            }
        }
    }
}
