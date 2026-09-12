use crate::{SdfError, SdfResult};

use serde_json::Value;

pub(crate) fn field_u32(entry: &Value, key: &str) -> SdfResult<u32> {
    let value = entry.get(key).and_then(Value::as_u64).ok_or_else(|| {
        SdfError::data(&format!(
            "manifest layer is missing a valid `{key}` (u64) field"
        ))
    })?;
    u32::try_from(value)
        .map_err(|_| SdfError::data(&format!("manifest layer `{key}` {value} exceeds u32")))
}
