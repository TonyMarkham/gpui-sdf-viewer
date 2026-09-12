use crate::{SdfError, SdfResult, utilities::field_u32};

use serde_json::Value;

/// One entry of a layer's mip chain as declared by the manifest: the pixel
/// edge of the level inside a real tile's payload and its byte length.
#[derive(Clone, Debug)]
pub struct Level {
    pub size: u32,
    pub bytes: u32,
}

/// Parses and structurally validates the layer's mip chain: levels ascend
/// from 0, every level is nonzero, each edge halves the previous one, and
/// each byte length matches its level's pixel count.
pub(crate) fn levels(entry: &Value) -> SdfResult<Vec<Level>> {
    let raw = entry
        .get("mips")
        .and_then(Value::as_array)
        .filter(|mips| !mips.is_empty())
        .ok_or_else(|| SdfError::data("manifest layer carries no `mips` table"))?;

    let mut levels: Vec<Level> = Vec::with_capacity(raw.len());
    for (index, mip) in raw.iter().enumerate() {
        let level = field_u32(mip, "level")?;
        if level != index as u32 {
            return Err(SdfError::data(&format!(
                "manifest layer mip table must list levels 0.. in order, found level {level} at position {index}"
            )));
        }
        let size = field_u32(mip, "size")?;
        if size == 0 {
            return Err(SdfError::data(&format!(
                "manifest layer mip level {level} carries a zero-size level"
            )));
        }
        if let Some(previous) = levels.last()
            && size != previous.size / 2
        {
            return Err(SdfError::data(&format!(
                "manifest layer mip level {level} size {size} does not halve level {} size {}",
                level - 1,
                previous.size
            )));
        }
        let bytes = field_u32(mip, "bytes")?;
        let expected = size.checked_mul(size).ok_or_else(|| {
            SdfError::data(&format!(
                "manifest layer mip level {level} size {size} exceeds the supported range"
            ))
        })?;
        if bytes != expected {
            return Err(SdfError::data(&format!(
                "manifest layer mip level {level} declares {bytes} bytes for a {size}² level"
            )));
        }
        levels.push(Level { size, bytes });
    }

    Ok(levels)
}
