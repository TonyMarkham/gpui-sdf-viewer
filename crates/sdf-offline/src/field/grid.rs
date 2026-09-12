use crate::{
    Config, DdsInfo, ManifestRecord, OfflineError, OfflineResult,
    constants::DISPLAY_SEPARATOR,
    utilities::{contained_join, sha256_hex},
};

// ---------------------------------------------------------------------------------------------- //

use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------------------------- //

pub(crate) const DDS_EXTENSION: &str = ".dds";
pub(crate) const TILE_GRID_EDGE: u32 = 16;
pub(crate) const TILE_EDGE: u32 = 512;
pub(crate) const STUB_EDGE: u32 = 8;
const R8_BIT_COUNT: u32 = 8;
const FOURCC_NONE: [u8; 4] = [0; 4];
const MISSING_REPORT_LIMIT: usize = 8;
const COORDINATE_DELIMITER: &str = "_";

// ---------------------------------------------------------------------------------------------- //

pub(crate) fn layer_records(config: &Config, prefix: &str) -> OfflineResult<Vec<ManifestRecord>> {
    let manifest_path = extract_manifest_path(config)?;
    let records = ManifestRecord::load(&manifest_path)?;

    let tiles: Vec<ManifestRecord> = records
        .into_iter()
        .filter(|record| is_layer_tile(&record.path, prefix))
        .collect();

    if tiles.is_empty() {
        return Err(OfflineError::manifest(format!(
            "manifest `{}` has no `{prefix}*{DDS_EXTENSION}` tiles; run `extract` first",
            manifest_path.display()
        )));
    }

    Ok(tiles)
}

pub(crate) fn extract_manifest_path(config: &Config) -> OfflineResult<PathBuf> {
    Ok(config.paths()?.dds_dir.join(&config.extract.manifest))
}

pub(crate) fn tile_path(config: &Config, record: &ManifestRecord) -> OfflineResult<PathBuf> {
    contained_join(&config.paths()?.dds_dir, &record.path)
}

pub(crate) fn load_tile(
    config: &Config,
    record: &ManifestRecord,
) -> OfflineResult<(Vec<u8>, DdsInfo)> {
    let path = tile_path(config, record)?;
    let data = std::fs::read(&path)
        .map_err(|e| OfflineError::io(format!("read `{}`: {e}", path.display())))?;

    let digest = sha256_hex(&data);
    if digest != record.sha256 {
        return Err(OfflineError::manifest(format!(
            "`{}` hash {digest} does not match manifest {}; re-run `extract`",
            path.display(),
            record.sha256
        )));
    }

    let info = DdsInfo::parse(&data)?;
    if info.fourcc != FOURCC_NONE {
        return Err(OfflineError::dds(format!(
            "`{}` fourcc `{:?}` unsupported; tiles are uncompressed 8-bit",
            path.display(),
            String::from_utf8_lossy(&info.fourcc)
        )));
    }
    if info.bit_count != R8_BIT_COUNT {
        return Err(OfflineError::dds(format!(
            "`{}` bit count {} unsupported; tiles are {R8_BIT_COUNT}",
            path.display(),
            info.bit_count
        )));
    }

    Ok((data, info))
}

pub(crate) fn mip_count_max(edge: u32) -> u32 {
    edge.trailing_zeros() + 1
}

pub(crate) fn chain_len(edge: u32, mip_count: u32) -> usize {
    (0..mip_count)
        .map(|level| {
            let edge = (edge >> level) as usize;
            edge * edge
        })
        .sum()
}

fn is_layer_tile(path: &str, prefix: &str) -> bool {
    path.rsplit(DISPLAY_SEPARATOR)
        .next()
        .is_some_and(|name| name.starts_with(prefix) && name.ends_with(DDS_EXTENSION))
}

pub(crate) fn tile_index(path: &str, prefix: &str) -> OfflineResult<Option<(u32, u32)>> {
    let name = path
        .rsplit(DISPLAY_SEPARATOR)
        .next()
        .ok_or_else(|| OfflineError::manifest(String::from("empty manifest path")))?;

    let coordinates = name
        .strip_prefix(prefix)
        .ok_or_else(|| {
            OfflineError::manifest(format!(
                "manifest path `{path}` does not start with `{prefix}`"
            ))
        })?
        .strip_suffix(DDS_EXTENSION)
        .ok_or_else(|| {
            OfflineError::manifest(format!(
                "manifest path `{path}` does not end with `{DDS_EXTENSION}`"
            ))
        })?;

    let parts: Vec<&str> = coordinates.split(COORDINATE_DELIMITER).collect();
    if parts.len() < 3 {
        return Ok(None);
    }

    let x = parse_coordinate(parts[parts.len() - 2], path)?;
    let y = parse_coordinate(parts[parts.len() - 1], path)?;
    if x >= TILE_GRID_EDGE || y >= TILE_GRID_EDGE {
        return Err(OfflineError::manifest(format!(
            "manifest path `{path}` tile {x}{COORDINATE_DELIMITER}{y} outside the {TILE_GRID_EDGE}x{TILE_GRID_EDGE} grid"
        )));
    }

    Ok(Some((x, y)))
}

fn parse_coordinate(value: &str, path: &str) -> OfflineResult<u32> {
    value
        .parse::<u32>()
        .map_err(|e| OfflineError::manifest(format!("tile coordinate `{value}` in `{path}`: {e}")))
}

pub(crate) fn require_complete(seen: &[bool], prefix: &str) -> OfflineResult<()> {
    let missing: Vec<String> = seen
        .iter()
        .enumerate()
        .filter(|(_, present)| !*present)
        .map(|(index, _)| {
            format!(
                "{}{COORDINATE_DELIMITER}{}",
                index % TILE_GRID_EDGE as usize,
                index / TILE_GRID_EDGE as usize
            )
        })
        .collect();
    if missing.is_empty() {
        return Ok(());
    }

    let shown = if missing.len() > MISSING_REPORT_LIMIT {
        format!("{}, ...", missing[..MISSING_REPORT_LIMIT].join(", "))
    } else {
        missing.join(", ")
    };

    Err(OfflineError::manifest(format!(
        "manifest is missing {} `{prefix}*` tile(s): {shown}; re-run `extract`",
        missing.len()
    )))
}

// Placement is exercised by the geometry tests only; the export pipeline
// ships per-tile payloads and never assembles a flat field.
#[cfg(test)]
pub(crate) fn place_mip(
    data: &[u8],
    header_stride: usize,
    x: u32,
    y: u32,
    level: u32,
    field_edge: u32,
    field: &mut [u8],
) -> OfflineResult<()> {
    let tile_edge = (TILE_EDGE >> level) as usize;
    let mip_len = tile_edge * tile_edge;
    let start = header_stride + tile_mip_offset(level);
    let mip = data.get(start..start + mip_len).ok_or_else(|| {
        OfflineError::dds(format!(
            "tile mip{level} truncated: {} bytes, need {} for the {tile_edge}² surface",
            data.len(),
            start + mip_len
        ))
    })?;

    let stride = field_edge as usize;
    for (tile_row, row) in mip.chunks_exact(tile_edge).enumerate() {
        let field_start = (y as usize * tile_edge + tile_row) * stride + x as usize * tile_edge;
        field[field_start..field_start + tile_edge].copy_from_slice(row);
    }

    Ok(())
}

#[cfg(test)]
fn tile_mip_offset(level: u32) -> usize {
    (0..level)
        .map(|index| {
            let edge = (TILE_EDGE >> index) as usize;
            edge * edge
        })
        .sum()
}

#[cfg(test)]
pub(crate) fn place_stub_fill(
    value: u8,
    x: u32,
    y: u32,
    level: u32,
    field_edge: u32,
    field: &mut [u8],
) -> OfflineResult<()> {
    let tile_edge = (TILE_EDGE >> level) as usize;
    let stride = field_edge as usize;
    let base_row = y as usize * tile_edge;
    let base_col = x as usize * tile_edge;
    for row_index in 0..tile_edge {
        let start = (base_row + row_index) * stride + base_col;
        field[start..start + tile_edge].fill(value);
    }

    Ok(())
}

pub(crate) fn stub_constant(data: &[u8], header_stride: usize, path: &Path) -> OfflineResult<u8> {
    let payload = data.get(header_stride..).ok_or_else(|| {
        OfflineError::dds(format!(
            "`{}` stub truncated: {} bytes, header is {header_stride}",
            path.display(),
            data.len()
        ))
    })?;
    let Some(&value) = payload.first() else {
        return Err(OfflineError::dds(format!(
            "`{}` stub has no mip payload",
            path.display()
        )));
    };
    if payload.iter().any(|&byte| byte != value) {
        return Err(OfflineError::dds(format!(
            "`{}` stub payload is not constant; only constant-fill stubs are supported",
            path.display()
        )));
    }

    Ok(value)
}

// ---------------------------------------------------------------------------------------------- //

#[cfg(test)]
mod tests {
    use super::{
        TILE_GRID_EDGE, chain_len, mip_count_max, place_mip, place_stub_fill, stub_constant,
        tile_index,
    };

    use crate::{OfflineError, OfflineResult};

    use std::path::Path;

    // ------------------------------------------------------------------------------------------ //

    const TEST_SDF_PATH: &str = "ui/cd_worldmap_land_sdf_32768x32768_0_0.dds";
    const TEST_SDF_PREFIX: &str = "cd_worldmap_land_sdf";
    const TEST_BLUR_PATH: &str = "ui/cd_worldmap_blur_height_0_0.dds";
    const TEST_BLUR_PREFIX: &str = "cd_worldmap_blur_height";
    const TEST_UNPREFIXED_PATH: &str = "ui/cd_worldmap_blur_height.dds";
    const TEST_OUT_OF_GRID_PATH: &str = "ui/cd_worldmap_blur_height_16_0.dds";
    const TEST_TILE_PATH: &str = "test-tile.dds";
    const TEST_TILE_EDGE: u32 = 512;
    const TEST_TILE_EDGE_USIZE: usize = 512;
    const TEST_STUB_TILE_EDGE: u32 = 8;
    const TEST_SINGLE_MIP_COUNT: u32 = 1;
    const TEST_STUB_MIP_COUNT: u32 = 4;
    const TEST_MAX_MIP_COUNT: u32 = 10;
    const TEST_MIP_CHAIN_LEN: usize = 348_160;
    const TEST_MIP0_CHAIN_LEN: usize = 262_144;
    const TEST_STUB_CHAIN_LEN: usize = 85;
    const TEST_HEADER_STRIDE: usize = 128;
    const TEST_FIELD_EDGE: u32 = 1024;
    const TEST_FIELD_STRIDE: usize = 1024;
    const TEST_MIP0_VALUE: u8 = 0xA7;
    const TEST_MIP1_VALUE: u8 = 0x3C;
    const TEST_STUB_VALUE: u8 = 0x55;
    const TEST_OTHER_VALUE: u8 = 0x2A;
    const TEST_ROW_PATTERN_MODULUS: usize = 254;

    // ------------------------------------------------------------------------------------------ //

    #[test]
    fn tile_index_parses_grid_coordinates() -> OfflineResult<()> {
        ensure(
            tile_index(TEST_SDF_PATH, TEST_SDF_PREFIX)? == Some((0, 0)),
            "the four-part tile name does not parse to its grid slot",
        )?;
        ensure(
            tile_index(TEST_BLUR_PATH, TEST_BLUR_PREFIX)? == Some((0, 0)),
            "the three-part tile name does not parse to its grid slot",
        )
    }

    #[test]
    fn tile_index_ignores_names_without_coordinates() -> OfflineResult<()> {
        ensure(
            tile_index(TEST_UNPREFIXED_PATH, TEST_BLUR_PREFIX)?.is_none(),
            "the coordinate-less tile name was not ignored",
        )
    }

    #[test]
    fn tile_index_rejects_coordinates_outside_the_grid() -> OfflineResult<()> {
        match tile_index(TEST_OUT_OF_GRID_PATH, TEST_BLUR_PREFIX) {
            Err(e) => ensure(
                format!("{e}").contains(&format!(
                    "outside the {}x{} grid",
                    TILE_GRID_EDGE, TILE_GRID_EDGE
                )),
                "the out-of-grid tile name produced an unexpected error",
            ),
            Ok(_) => Err(OfflineError::manifest(String::from(
                "the out-of-grid tile name passed tile_index",
            ))),
        }
    }

    #[test]
    fn chain_geometry_matches_the_tile_format() -> OfflineResult<()> {
        ensure(
            mip_count_max(TEST_TILE_EDGE) == TEST_MAX_MIP_COUNT,
            "the mip0 mip-count range does not match the tile format",
        )?;
        ensure(
            mip_count_max(TEST_STUB_TILE_EDGE) == TEST_STUB_MIP_COUNT,
            "the stub mip-count range does not match the stub format",
        )?;
        ensure(
            chain_len(TEST_TILE_EDGE, TEST_STUB_MIP_COUNT) == TEST_MIP_CHAIN_LEN,
            "the mip0 chain length does not match the tile format",
        )?;
        ensure(
            chain_len(TEST_TILE_EDGE, TEST_SINGLE_MIP_COUNT) == TEST_MIP0_CHAIN_LEN,
            "the mip0-only chain length does not match the tile format",
        )?;
        ensure(
            chain_len(TEST_STUB_TILE_EDGE, TEST_STUB_MIP_COUNT) == TEST_STUB_CHAIN_LEN,
            "the stub chain length does not match the stub format",
        )
    }

    #[test]
    fn place_mip_places_level_zero_rows_in_field_order() -> OfflineResult<()> {
        let mut data = vec![0u8; TEST_HEADER_STRIDE + TEST_MIP0_CHAIN_LEN];
        for (index, byte) in data[TEST_HEADER_STRIDE..].iter_mut().enumerate() {
            *byte = (index % TEST_ROW_PATTERN_MODULUS + 1) as u8;
        }
        let mut field = vec![0u8; TEST_FIELD_STRIDE * TEST_FIELD_STRIDE];
        place_mip(
            &data,
            TEST_HEADER_STRIDE,
            1,
            0,
            0,
            TEST_FIELD_EDGE,
            &mut field,
        )?;

        for (tile_row, source_row) in data[TEST_HEADER_STRIDE..]
            .as_chunks::<TEST_TILE_EDGE_USIZE>()
            .0
            .iter()
            .enumerate()
        {
            let start = tile_row * TEST_FIELD_STRIDE + TEST_TILE_EDGE_USIZE;
            ensure(
                field[start..start + TEST_TILE_EDGE_USIZE] == *source_row,
                "the placed mip0 row does not match the tile row",
            )?;
        }
        ensure(
            field.iter().enumerate().all(|(index, &byte)| {
                byte == 0
                    || (index / TEST_FIELD_STRIDE < TEST_TILE_EDGE_USIZE
                        && index % TEST_FIELD_STRIDE >= TEST_TILE_EDGE_USIZE)
            }),
            "the mip0 placement wrote outside its region",
        )
    }

    #[test]
    fn place_mip_places_level_one_after_the_mip0_region() -> OfflineResult<()> {
        let mip1_edge = TEST_TILE_EDGE_USIZE >> 1;
        let mut data = vec![0u8; TEST_HEADER_STRIDE + TEST_MIP0_CHAIN_LEN + mip1_edge * mip1_edge];
        data[TEST_HEADER_STRIDE..TEST_HEADER_STRIDE + TEST_MIP0_CHAIN_LEN].fill(TEST_MIP0_VALUE);
        data[TEST_HEADER_STRIDE + TEST_MIP0_CHAIN_LEN..].fill(TEST_MIP1_VALUE);
        let mut field = vec![0u8; TEST_FIELD_STRIDE * TEST_FIELD_STRIDE];
        place_mip(
            &data,
            TEST_HEADER_STRIDE,
            1,
            1,
            1,
            TEST_FIELD_EDGE,
            &mut field,
        )?;

        for row_index in 0..mip1_edge {
            let start = (row_index + mip1_edge) * TEST_FIELD_STRIDE + mip1_edge;
            ensure(
                field[start..start + mip1_edge]
                    .iter()
                    .all(|&byte| byte == TEST_MIP1_VALUE),
                "the placed mip1 region does not hold the level-1 bytes",
            )?;
        }
        ensure(
            field.iter().enumerate().all(|(index, &byte)| {
                byte == 0
                    || (index / TEST_FIELD_STRIDE >= mip1_edge
                        && index / TEST_FIELD_STRIDE < 2 * mip1_edge
                        && index % TEST_FIELD_STRIDE >= mip1_edge
                        && index % TEST_FIELD_STRIDE < 2 * mip1_edge)
            }),
            "the mip1 placement wrote outside its region",
        )
    }

    #[test]
    fn place_stub_fill_fills_only_its_region() -> OfflineResult<()> {
        let mip1_edge = TEST_TILE_EDGE_USIZE >> 1;
        let mut field = vec![0u8; TEST_FIELD_STRIDE * TEST_FIELD_STRIDE];
        place_stub_fill(TEST_STUB_VALUE, 1, 0, 0, TEST_FIELD_EDGE, &mut field)?;
        place_stub_fill(TEST_OTHER_VALUE, 0, 2, 1, TEST_FIELD_EDGE, &mut field)?;

        ensure(
            field.iter().enumerate().all(|(index, &byte)| {
                let row = index / TEST_FIELD_STRIDE;
                let col = index % TEST_FIELD_STRIDE;
                let in_mip0_region = row < TEST_TILE_EDGE_USIZE && col >= TEST_TILE_EDGE_USIZE;
                let in_mip1_region = row >= 2 * mip1_edge && row < 3 * mip1_edge && col < mip1_edge;
                byte == if in_mip0_region {
                    TEST_STUB_VALUE
                } else if in_mip1_region {
                    TEST_OTHER_VALUE
                } else {
                    0
                }
            }),
            "the stub fill wrote the wrong bytes",
        )
    }

    #[test]
    fn stub_constant_verifies_a_constant_fill() -> OfflineResult<()> {
        let mut data = vec![0u8; TEST_HEADER_STRIDE];
        data.extend_from_slice(&[TEST_STUB_VALUE; TEST_STUB_CHAIN_LEN]);
        ensure(
            stub_constant(&data, TEST_HEADER_STRIDE, Path::new(TEST_TILE_PATH))? == TEST_STUB_VALUE,
            "the constant stub did not yield its fill value",
        )?;

        let mut non_constant = data.clone();
        if let Some(byte) = non_constant.last_mut() {
            *byte = TEST_OTHER_VALUE;
        }
        expect_error(
            stub_constant(&non_constant, TEST_HEADER_STRIDE, Path::new(TEST_TILE_PATH)),
            "stub payload is not constant",
            "a non-constant stub passed stub_constant",
        )?;

        let header_only = vec![0u8; TEST_HEADER_STRIDE];
        expect_error(
            stub_constant(&header_only, TEST_HEADER_STRIDE, Path::new(TEST_TILE_PATH)),
            "stub has no mip payload",
            "an empty stub passed stub_constant",
        )?;

        let truncated = vec![0u8; TEST_HEADER_STRIDE - 1];
        expect_error(
            stub_constant(&truncated, TEST_HEADER_STRIDE, Path::new(TEST_TILE_PATH)),
            "stub truncated",
            "a truncated stub passed stub_constant",
        )
    }

    // ------------------------------------------------------------------------------------------ //

    fn expect_error(result: OfflineResult<u8>, expected: &str, failure: &str) -> OfflineResult<()> {
        match result {
            Err(e) => ensure(
                format!("{e}").contains(expected),
                "the stub produced an unexpected error",
            ),
            Ok(_) => Err(OfflineError::manifest(String::from(failure))),
        }
    }

    fn ensure(condition: bool, message: &str) -> OfflineResult<()> {
        if condition {
            Ok(())
        } else {
            Err(OfflineError::manifest(String::from(message)))
        }
    }
}
