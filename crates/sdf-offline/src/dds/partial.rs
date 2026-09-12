use crate::{
    OfflineError, OfflineResult,
    constants::{DDS_MAGIC, U32_SIZE},
    utilities::try_read_u32,
};

// ---------------------------------------------------------------------------------------------- //

const HEADER_SIZE: usize = 128;
const HEIGHT_OFFSET: usize = 12;
const WIDTH_OFFSET: usize = 16;
const DEPTH_OFFSET: usize = 24;
const MIP_COUNT_OFFSET: usize = 28;
const RESERVED1_OFFSET: usize = 32;
const FOURCC_OFFSET: usize = 84;
const FOURCC_SIZE: usize = 4;
const FOURCC_NONE: [u8; 4] = [0; 4];
const RGB_BIT_COUNT_OFFSET: usize = 88;
const UNCOMPRESSED_BIT_COUNT: u32 = 8;
const CAPS2_OFFSET: usize = 112;
const MULTI_CHUNK_MIN_MIPS: u32 = 5;
const MULTI_CHUNK_MAX_BLOCKS: u32 = 4;

pub(crate) fn reconstruct(blob: &[u8], orig_size: usize) -> OfflineResult<Vec<u8>> {
    if blob.len() < HEADER_SIZE {
        return Err(OfflineError::dds(format!(
            "partial dds blob truncated: {} bytes, header is {HEADER_SIZE}",
            blob.len()
        )));
    }

    if blob[..DDS_MAGIC.len()] != DDS_MAGIC {
        return Err(OfflineError::dds("partial dds magic mismatch"));
    }

    let fourcc = blob
        .get(FOURCC_OFFSET..FOURCC_OFFSET + FOURCC_SIZE)
        .ok_or_else(|| {
            OfflineError::dds(format!("partial dds truncated at offset {FOURCC_OFFSET}"))
        })?;
    if fourcc != FOURCC_NONE {
        return Err(OfflineError::dds(format!(
            "partial dds fourcc `{:?}` unsupported; only uncompressed 8-bit surfaces",
            String::from_utf8_lossy(fourcc)
        )));
    }

    let bit_count = try_read_u32(blob, RGB_BIT_COUNT_OFFSET)?;
    if bit_count != UNCOMPRESSED_BIT_COUNT {
        return Err(OfflineError::dds(format!(
            "partial dds bit count {bit_count} unsupported; only {UNCOMPRESSED_BIT_COUNT}"
        )));
    }

    let width = try_read_u32(blob, WIDTH_OFFSET)?;
    let height = try_read_u32(blob, HEIGHT_OFFSET)?;
    let depth = try_read_u32(blob, DEPTH_OFFSET)?;
    let mip_count = try_read_u32(blob, MIP_COUNT_OFFSET)?;
    let caps2 = try_read_u32(blob, CAPS2_OFFSET)?;

    let multi_chunk = mip_count > MULTI_CHUNK_MIN_MIPS && caps2 == 0 && depth < 2;

    let mut blocks = Vec::new();
    if multi_chunk {
        let count = mip_count.min(MULTI_CHUNK_MAX_BLOCKS) as usize;
        let mut mip_width = width;
        let mut mip_height = height;
        for i in 0..count {
            let decomp_size = mip_width as usize * mip_height as usize;
            let comp_size = try_read_u32(blob, RESERVED1_OFFSET + i * U32_SIZE)? as usize;
            blocks.push((comp_size, decomp_size));
            mip_width >>= 1;
            mip_height >>= 1;
        }
    } else {
        let comp_size = try_read_u32(blob, RESERVED1_OFFSET)? as usize;
        let decomp_size = try_read_u32(blob, RESERVED1_OFFSET + U32_SIZE)? as usize;
        blocks.push((comp_size, decomp_size));
    }

    let mut out = blob[..HEADER_SIZE].to_vec();
    let mut pos = HEADER_SIZE;
    for (comp_size, decomp_size) in blocks {
        if comp_size == decomp_size {
            let raw = blob.get(pos..pos + decomp_size).ok_or_else(|| {
                OfflineError::dds(format!("partial dds raw block truncated at {pos}"))
            })?;
            out.extend_from_slice(raw);
        } else {
            let src = blob.get(pos..pos + comp_size).ok_or_else(|| {
                OfflineError::dds(format!("partial dds lz4 block truncated at {pos}"))
            })?;
            let mut dst = vec![0u8; decomp_size];
            lz4_flex::block::decompress_into(src, &mut dst)
                .map_err(|e| OfflineError::dds(format!("partial dds lz4 block at {pos}: {e}")))?;
            out.extend_from_slice(&dst);
        }
        pos += comp_size;
    }

    let tail = blob
        .get(pos..)
        .ok_or_else(|| OfflineError::dds(format!("partial dds tail truncated at {pos}")))?;
    out.extend_from_slice(tail);

    if out.len() != orig_size {
        return Err(OfflineError::dds(format!(
            "partial dds reconstructed {} bytes, expected {orig_size}",
            out.len()
        )));
    }

    Ok(out)
}
