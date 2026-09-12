use super::origin::SceneOrigin;

/// A discoverable scene: a user-supplied `.wgsl` file under the scenes
/// directory.
#[derive(Clone, Debug)]
pub(crate) struct SceneEntry {
    pub name: String,
    pub origin: SceneOrigin,
}
