mod composite;
mod entry;
mod origin;

pub(crate) use self::composite::CompositeSpec;
use self::{entry::SceneEntry, origin::SceneOrigin};
use crate::{
    UiError, UiResult,
    constants::SCENE_DIR_ENV,
    state::scene_data::{Resolution, resolve_scene_source},
};
use sdf_component::SdfScene;
use soul_attributes::soul;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

// ---------------------------------------------------------------------------------------------- //

const TEMP_DIR_NAME: &str = "cd-map-offline-scenes";

/// Distinguishes concurrent loads inside one process; the process id
/// distinguishes concurrent app instances, so no two loads ever stage to the
/// same file.
static STAGED_LOAD: AtomicU64 = AtomicU64::new(0);

/// The list of scenes available to the UI.
pub(crate) struct SceneLibrary {
    entries: Vec<SceneEntry>,
}

impl SceneLibrary {
    /// Scans the scenes directory for user-supplied `.wgsl` files. The
    /// directory is created when missing so users have an obvious drop-in
    /// location.
    #[soul(id = "concept.sdf-scene-contract", step = "scan scenes dir")]
    pub fn discover() -> Self {
        let directory = std::env::var(SCENE_DIR_ENV)
            .unwrap_or_else(|_| crate::constants::SCENE_DIR_DEFAULT.to_string());
        Self::discover_in(Path::new(&directory))
    }

    pub fn discover_in(directory: &Path) -> Self {
        let _ = std::fs::create_dir_all(directory);

        let mut entries = Vec::new();
        if let Ok(paths) = std::fs::read_dir(directory) {
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

        Self { entries }
    }

    pub fn entries(&self) -> &[SceneEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Loads the full scene content for the entry at `index`. `config:`
    /// values in the header resolve through the app's data directory; unknown
    /// schemes are a parse error naming the value.
    #[soul(id = "concept.sdf-scene-contract", step = "load entry")]
    pub fn load(&self, index: usize, data_dir: &Path) -> UiResult<SdfScene> {
        let Some(entry) = self.entries.get(index) else {
            return Err(UiError::scene(sdf_component::SdfError::scene_invalid(
                "scene index out of range",
            )));
        };

        let SceneOrigin::File(path) = &entry.origin;
        load_scene(path, data_dir)
    }
}

// ---------------------------------------------------------------------------------------------- //

/// Reads the scene file, resolves `config:` values in its header, and parses
/// it through the component. A rewritten scene cannot be parsed by
/// `SdfScene::from_str` alone — `from_str` keeps data directives unresolved —
/// so the resolved source goes through `SdfScene::from_file` on a temp copy;
/// the manifest value is absolute by then, so resolution against the temp
/// directory is a no-op. The copy is named per load (unique across processes),
/// so concurrent app instances cannot race on the same staged file.
fn load_scene(path: &Path, data_dir: &Path) -> UiResult<SdfScene> {
    let source = std::fs::read_to_string(path).map_err(|error| {
        UiError::scene(sdf_component::SdfError::data(&format!(
            "the scene file `{}` could not be read: {error}",
            path.display()
        )))
    })?;

    match resolve_scene_source(&source, data_dir).map_err(UiError::scene)? {
        Resolution::Unchanged => SdfScene::from_file(path).map_err(UiError::scene),
        Resolution::Rewritten { source } => load_rewritten(&source, file_stem(path)),
    }
}

fn load_rewritten(source: &str, stem: &str) -> UiResult<SdfScene> {
    let directory = std::env::temp_dir().join(TEMP_DIR_NAME);
    std::fs::create_dir_all(&directory).map_err(|error| {
        UiError::scene(sdf_component::SdfError::data(&format!(
            "the resolved scene copy could not be staged: {error}"
        )))
    })?;
    let load = STAGED_LOAD.fetch_add(1, Ordering::Relaxed);
    let path = directory.join(format!("{stem}-{}-{load}.wgsl", std::process::id()));
    std::fs::write(&path, source).map_err(|error| {
        UiError::scene(sdf_component::SdfError::data(&format!(
            "the resolved scene copy could not be written: {error}"
        )))
    })?;

    let scene = SdfScene::from_file(&path).map_err(UiError::scene);
    let _ = std::fs::remove_file(&path);
    scene
}

fn file_stem(path: &Path) -> &str {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("scene")
}
