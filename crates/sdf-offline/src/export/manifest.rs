use crate::{ExportLayer, OfflineError, OfflineResult};

// ---------------------------------------------------------------------------------------------- //

use serde::{Deserialize, Serialize};
use std::path::Path;

// ---------------------------------------------------------------------------------------------- //

pub(crate) const FORMAT_TAG: &str = "cd-map-sdf-field";
pub(crate) const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ExportManifest {
    pub(crate) format: String,
    pub(crate) version: u32,
    pub(crate) layers: Vec<ExportLayer>,
}

impl ExportManifest {
    pub(crate) fn new(layers: Vec<ExportLayer>) -> ExportManifest {
        ExportManifest {
            format: String::from(FORMAT_TAG),
            version: FORMAT_VERSION,
            layers,
        }
    }

    pub(crate) fn load(path: &Path) -> OfflineResult<ExportManifest> {
        let data = std::fs::read_to_string(path)
            .map_err(|e| OfflineError::io(format!("read `{}`: {e}", path.display())))?;
        let manifest: ExportManifest = serde_json::from_str(&data).map_err(|e| {
            OfflineError::manifest(format!("parse export manifest `{}`: {e}", path.display()))
        })?;

        if manifest.format != FORMAT_TAG {
            return Err(OfflineError::manifest(format!(
                "export manifest `{}` format `{}` is not `{FORMAT_TAG}`",
                path.display(),
                manifest.format
            )));
        }
        if manifest.version != FORMAT_VERSION {
            return Err(OfflineError::manifest(format!(
                "export manifest `{}` version {} is not {FORMAT_VERSION}",
                path.display(),
                manifest.version
            )));
        }

        Ok(manifest)
    }
}

// ---------------------------------------------------------------------------------------------- //

#[cfg(test)]
mod tests {
    use super::{FORMAT_TAG, FORMAT_VERSION};

    use crate::{ExportManifest, OfflineError, OfflineResult};

    use std::path::{Path, PathBuf};

    // ------------------------------------------------------------------------------------------ //

    const TEST_DIR_NAME: &str = "sdf-offline-export-manifest";
    const TEST_MANIFEST_NAME: &str = "manifest.json";
    const TEST_WRONG_FORMAT_TAG: &str = "other-format";
    const TEST_WRONG_VERSION: u32 = 2;
    const TEST_SCENARIO_WRONG_FORMAT: &str = "wrong-format";
    const TEST_SCENARIO_WRONG_VERSION: &str = "wrong-version";

    // ------------------------------------------------------------------------------------------ //

    #[test]
    fn load_rejects_an_unknown_format_tag() -> OfflineResult<()> {
        let dir = scenario_dir(TEST_SCENARIO_WRONG_FORMAT);
        let result = write_manifest(
            &dir,
            &ExportManifest {
                format: String::from(TEST_WRONG_FORMAT_TAG),
                version: FORMAT_VERSION,
                layers: Vec::new(),
            },
        )
        .and_then(|path| match ExportManifest::load(&path) {
            Err(e) => ensure(
                format!("{e}").contains(&format!(
                    "format `{TEST_WRONG_FORMAT_TAG}` is not `{FORMAT_TAG}`"
                )),
                "the unknown format tag produced an unexpected error",
            ),
            Ok(_) => Err(OfflineError::manifest(String::from(
                "an unknown format tag passed ExportManifest::load",
            ))),
        });
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    #[test]
    fn load_rejects_an_unknown_version() -> OfflineResult<()> {
        let dir = scenario_dir(TEST_SCENARIO_WRONG_VERSION);
        let result = write_manifest(
            &dir,
            &ExportManifest {
                format: String::from(FORMAT_TAG),
                version: TEST_WRONG_VERSION,
                layers: Vec::new(),
            },
        )
        .and_then(|path| match ExportManifest::load(&path) {
            Err(e) => ensure(
                format!("{e}").contains(&format!(
                    "version {TEST_WRONG_VERSION} is not {FORMAT_VERSION}"
                )),
                "the unknown version produced an unexpected error",
            ),
            Ok(_) => Err(OfflineError::manifest(String::from(
                "an unknown version passed ExportManifest::load",
            ))),
        });
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    // ------------------------------------------------------------------------------------------ //

    fn scenario_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{TEST_DIR_NAME}-{name}-{}", std::process::id()))
    }

    fn write_manifest(dir: &Path, manifest: &ExportManifest) -> OfflineResult<PathBuf> {
        std::fs::create_dir_all(dir)
            .map_err(|e| OfflineError::io(format!("create `{}`: {e}", dir.display())))?;
        let json = serde_json::to_string(manifest)
            .map_err(|e| OfflineError::json(format!("serialize the test export manifest: {e}")))?;
        let path = dir.join(TEST_MANIFEST_NAME);
        std::fs::write(&path, json)
            .map_err(|e| OfflineError::io(format!("write `{}`: {e}", path.display())))?;
        Ok(path)
    }

    fn ensure(condition: bool, message: &str) -> OfflineResult<()> {
        if condition {
            Ok(())
        } else {
            Err(OfflineError::manifest(String::from(message)))
        }
    }
}
