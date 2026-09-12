use crate::{
    Config, ManifestRecord, OfflineError, OfflineResult, Pamt, Progress,
    constants::{DISPLAY_SEPARATOR, PAMT_FILE_NAME},
    run::PROGRESS_EVERY,
    utilities::{contained_join, sha256_hex},
};

// ---------------------------------------------------------------------------------------------- //

use std::path::Path;

// ---------------------------------------------------------------------------------------------- //

const STEP: &str = "extract";

pub fn run(config: &Config, progress: &mut dyn FnMut(Progress)) -> OfflineResult<()> {
    config.check()?;
    let paths = config.paths()?;
    let mut records = Vec::new();

    for pack in &config.extract.packs {
        let pack_dir = paths.game_root.join(pack);
        let pamt_path = pack_dir.join(PAMT_FILE_NAME);
        let raw = std::fs::read(&pamt_path)
            .map_err(|e| OfflineError::io(format!("read `{}`: {e}", pamt_path.display())))?;

        let pamt = Pamt::parse(&raw, &pack_dir, 0)?;
        progress(Progress::layer(
            STEP,
            pack,
            format!("pack scanned: {} entries", pamt.entries.len()),
        ));

        let mut extracted = 0usize;
        for entry in &pamt.entries {
            if !config
                .extract
                .includes
                .iter()
                .any(|pattern| entry.matches_glob(pattern))
            {
                continue;
            }

            let out_path = contained_join(&paths.dds_dir, &entry.path)?;
            let data = entry.read()?;
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| OfflineError::io(format!("create `{}`: {e}", parent.display())))?;
            }
            std::fs::write(&out_path, &data)
                .map_err(|e| OfflineError::io(format!("write `{}`: {e}", out_path.display())))?;

            records.push(ManifestRecord {
                path: entry.path.clone(),
                paz_file: display_path(&entry.paz_file),
                offset: entry.offset,
                comp_size: entry.comp_size,
                orig_size: entry.orig_size,
                flags: entry.flags,
                sha256: sha256_hex(&data),
            });

            extracted += 1;
            if extracted.is_multiple_of(PROGRESS_EVERY) {
                progress(Progress::batch(
                    STEP,
                    pack,
                    format!("extracted {extracted} matching files…"),
                ));
            }
        }
    }

    write_manifest(config, &records, progress)?;

    Ok(())
}

// ---------------------------------------------------------------------------------------------- //

pub(crate) fn display_path(path: &Path) -> String {
    path.display()
        .to_string()
        .replace(std::path::MAIN_SEPARATOR, DISPLAY_SEPARATOR)
}

fn write_manifest(
    config: &Config,
    records: &[ManifestRecord],
    progress: &mut dyn FnMut(Progress),
) -> OfflineResult<()> {
    let path = config.paths()?.dds_dir.join(&config.extract.manifest);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| OfflineError::io(format!("create `{}`: {e}", parent.display())))?;
    }

    let json = serde_json::to_string_pretty(records)
        .map_err(|e| OfflineError::json(format!("serialize manifest `{}`: {e}", path.display())))?;
    std::fs::write(&path, json)
        .map_err(|e| OfflineError::io(format!("write `{}`: {e}", path.display())))?;

    progress(Progress::layer(
        STEP,
        &config.extract.manifest,
        format!("wrote `{}`: {} files", display_path(&path), records.len()),
    ));

    Ok(())
}
