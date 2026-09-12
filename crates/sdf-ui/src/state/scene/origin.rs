use std::path::PathBuf;

#[derive(Clone, Debug)]
pub(crate) enum SceneOrigin {
    File(PathBuf),
}
