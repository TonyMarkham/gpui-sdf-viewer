use crate::{OfflineError, OfflineResult};

// ---------------------------------------------------------------------------------------------- //

const COMPRESSION_SHIFT: u32 = 16;
const COMPRESSION_MASK: u32 = 0x0F;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Compression {
    None,
    Partial,
    Lz4,
    Custom,
    Zlib,
}

impl Compression {
    pub(crate) fn from_flags(flags: u32) -> OfflineResult<Self> {
        let field = (flags >> COMPRESSION_SHIFT) & COMPRESSION_MASK;

        match field {
            0 => Ok(Compression::None),
            1 => Ok(Compression::Partial),
            2 => Ok(Compression::Lz4),
            3 => Ok(Compression::Custom),
            4 => Ok(Compression::Zlib),
            n => Err(OfflineError::paz(format!("unknown compression field {n}"))),
        }
    }
}

// ---------------------------------------------------------------------------------------------- //

#[cfg(test)]
mod tests {
    use super::{COMPRESSION_SHIFT, Compression};

    use crate::OfflineResult;

    // ------------------------------------------------------------------------------------------ //

    const TEST_CRYPTO_FIELD_CHACHA20: u32 = 3;
    const TEST_UNKNOWN_FIELD: u32 = 5;

    // ------------------------------------------------------------------------------------------ //

    #[test]
    fn from_flags_maps_every_field_value() -> OfflineResult<()> {
        let expected = [
            (0u32, Compression::None),
            (1, Compression::Partial),
            (2, Compression::Lz4),
            (3, Compression::Custom),
            (4, Compression::Zlib),
        ];
        for (field, compression) in expected {
            let flags = field << COMPRESSION_SHIFT;
            match Compression::from_flags(flags) {
                Ok(decoded) => assert_eq!(
                    decoded, compression,
                    "compression field {field} must decode to {compression:?}"
                ),
                Err(error) => unreachable!("compression field {field} must decode: {error}"),
            }
        }

        Ok(())
    }

    #[test]
    fn from_flags_masks_out_the_other_fields() {
        let flags = (TEST_CRYPTO_FIELD_CHACHA20 << 20) | (2 << COMPRESSION_SHIFT);
        match Compression::from_flags(flags) {
            Ok(Compression::Lz4) => {}
            other => unreachable!(
                "the crypto bits must not disturb the compression field, got: {other:?}"
            ),
        }
    }

    #[test]
    fn from_flags_rejects_an_unknown_field() {
        let flags = TEST_UNKNOWN_FIELD << COMPRESSION_SHIFT;
        let failure = match Compression::from_flags(flags) {
            Err(error) => format!("{error}"),
            Ok(compression) => format!("{compression:?}"),
        };
        assert!(
            failure.contains("unknown compression field 5"),
            "an unknown compression field must be a named error, got: {failure}"
        );
    }
}
