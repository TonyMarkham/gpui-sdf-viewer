use crate::{TileKind, constants::DISPLAY_SEPARATOR, field::grid::DDS_EXTENSION};

// ---------------------------------------------------------------------------------------------- //

const PAYLOAD_EXTENSION: &str = ".r8";

pub(crate) struct TilePayload {
    pub(crate) path: String,
    pub(crate) payload: String,
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) kind: TileKind,
    pub(crate) source_sha256: String,
    pub(crate) bytes: Vec<u8>,
}

impl TilePayload {
    pub(crate) fn convert(
        extract_path: &str,
        source_sha256: &str,
        x: u32,
        y: u32,
        kind: TileKind,
        bytes: Vec<u8>,
    ) -> TilePayload {
        TilePayload {
            path: String::from(extract_path),
            payload: payload_name(extract_path),
            x,
            y,
            kind,
            source_sha256: String::from(source_sha256),
            bytes,
        }
    }
}

fn payload_name(extract_path: &str) -> String {
    let name = extract_path
        .rsplit(DISPLAY_SEPARATOR)
        .next()
        .unwrap_or(extract_path);
    match name.strip_suffix(DDS_EXTENSION) {
        Some(stem) => format!("{stem}{PAYLOAD_EXTENSION}"),
        None => format!("{name}{PAYLOAD_EXTENSION}"),
    }
}
