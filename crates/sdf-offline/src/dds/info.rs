use crate::{
    OfflineError, OfflineResult,
    constants::{DDS_MAGIC, U32_SIZE},
    utilities::try_read_u32,
};

// ---------------------------------------------------------------------------------------------- //

const HEIGHT_OFFSET: usize = 12;
const WIDTH_OFFSET: usize = 16;
const MIP_COUNT_OFFSET: usize = 28;
const PIXEL_FORMAT_FOURCC_OFFSET: usize = 84;
const PIXEL_FORMAT_FOURCC_SIZE: usize = 4;
const RGB_BIT_COUNT_OFFSET: usize = 88;
const HEADER_CENSUS_MIN_BYTES: usize = RGB_BIT_COUNT_OFFSET + U32_SIZE;

#[derive(Debug)]
pub(crate) struct DdsInfo {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) mip_count: u32,
    pub(crate) fourcc: [u8; 4],
    pub(crate) bit_count: u32,
}

impl DdsInfo {
    pub(crate) fn parse(data: &[u8]) -> OfflineResult<DdsInfo> {
        if data.len() < HEADER_CENSUS_MIN_BYTES {
            return Err(OfflineError::dds(format!(
                "dds truncated: {} bytes, need {HEADER_CENSUS_MIN_BYTES} for the header census",
                data.len()
            )));
        }

        if data[..DDS_MAGIC.len()] != DDS_MAGIC {
            return Err(OfflineError::dds("dds magic mismatch"));
        }

        let mut fourcc = [0u8; PIXEL_FORMAT_FOURCC_SIZE];
        fourcc.copy_from_slice(
            &data
                [PIXEL_FORMAT_FOURCC_OFFSET..PIXEL_FORMAT_FOURCC_OFFSET + PIXEL_FORMAT_FOURCC_SIZE],
        );

        Ok(DdsInfo {
            width: try_read_u32(data, WIDTH_OFFSET)?,
            height: try_read_u32(data, HEIGHT_OFFSET)?,
            mip_count: try_read_u32(data, MIP_COUNT_OFFSET)?,
            fourcc,
            bit_count: try_read_u32(data, RGB_BIT_COUNT_OFFSET)?,
        })
    }
}
