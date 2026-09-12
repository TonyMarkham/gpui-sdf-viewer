use crate::{
    OfflineError, OfflineResult,
    constants::{HEX_CHARS_PER_BYTE, U32_SIZE},
};

// ---------------------------------------------------------------------------------------------- //

use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};

// ---------------------------------------------------------------------------------------------- //

/// Joins a data-derived relative path onto a base directory, rejecting any
/// value that could escape the base: absolute paths replace it, rooted paths
/// (`\dir`) graft beside it, drive-prefixed paths (`C:name`) replace it
/// wholesale, and `..` components traverse out of it. The parsed `0.pamt`
/// entry paths and the extract manifest records are game data, not trusted
/// input, so every join onto an output directory goes through here before
/// any filesystem call.
pub(crate) fn contained_join(base: &Path, entry_path: &str) -> OfflineResult<PathBuf> {
    let relative = Path::new(entry_path);
    let escapes = relative.is_absolute()
        || relative.has_root()
        || relative
            .components()
            .any(|component| matches!(component, Component::Prefix(_) | Component::ParentDir));
    if escapes {
        return Err(OfflineError::io(format!(
            "path `{entry_path}` escapes the output directory `{}`",
            base.display()
        )));
    }

    Ok(base.join(relative))
}

// ---------------------------------------------------------------------------------------------- //

pub(crate) fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset + U32_SIZE)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

pub(crate) fn try_read_u32(data: &[u8], offset: usize) -> OfflineResult<u32> {
    read_u32(data, offset)
        .ok_or_else(|| OfflineError::dds(format!("u32 read truncated at offset {offset}")))
}

pub(crate) fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    let mut hex = String::with_capacity(digest.len() * HEX_CHARS_PER_BYTE);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

// ---------------------------------------------------------------------------------------------- //

#[cfg(test)]
mod tests {
    use super::contained_join;

    use crate::{OfflineError, OfflineResult};

    use std::path::{Path, PathBuf};

    // ------------------------------------------------------------------------------------------ //

    const TEST_BASE_DIR: &str = "C:/Users/tony/.config/cd-map-offline/data/dds";
    const TEST_TILE_PATH: &str = "ui/cd_worldmap_land_sdf_32768x32768_0_0.dds";
    const TEST_FLAT_PATH: &str = "cd_worldmap_land_sdf_32768x32768_0_0.dds";
    const TEST_ABSOLUTE_PATH: &str = "C:/Users/tony/evil.dds";
    const TEST_ROOTED_PATH: &str = "/evil.dds";
    const TEST_DRIVE_RELATIVE_PATH: &str = "C:evil.dds";
    const TEST_TRAVERSING_PATH: &str = "../cd_worldmap_land_sdf_evil.dds";
    const TEST_DEEP_TRAVERSING_PATH: &str = "ui/../../cd_worldmap_land_sdf_evil.dds";

    // ------------------------------------------------------------------------------------------ //

    #[test]
    fn contained_join_keeps_data_paths_under_the_base() -> OfflineResult<()> {
        let base = Path::new(TEST_BASE_DIR);
        ensure(
            contained_join(base, TEST_TILE_PATH)?
                == PathBuf::from(TEST_BASE_DIR).join(TEST_TILE_PATH),
            "a plain nested data path must join under the base",
        )?;
        ensure(
            contained_join(base, TEST_FLAT_PATH)?
                == PathBuf::from(TEST_BASE_DIR).join(TEST_FLAT_PATH),
            "a plain flat data path must join under the base",
        )
    }

    #[test]
    fn contained_join_rejects_every_escape_shape() {
        let base = Path::new(TEST_BASE_DIR);
        for (path, shape) in [
            (TEST_ABSOLUTE_PATH, "absolute"),
            (TEST_ROOTED_PATH, "rooted"),
            (TEST_DRIVE_RELATIVE_PATH, "drive-prefixed"),
            (TEST_TRAVERSING_PATH, "parent"),
            (TEST_DEEP_TRAVERSING_PATH, "nested parent"),
        ] {
            let joined = contained_join(base, path);
            assert!(
                joined.is_err(),
                "the {shape} path `{path}` must not join under the base"
            );
        }
    }

    // ------------------------------------------------------------------------------------------ //

    fn ensure(condition: bool, message: &str) -> OfflineResult<()> {
        if condition {
            Ok(())
        } else {
            Err(OfflineError::manifest(String::from(message)))
        }
    }
}
