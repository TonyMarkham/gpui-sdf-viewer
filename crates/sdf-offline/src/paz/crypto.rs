use crate::{OfflineError, OfflineResult, constants::U32_SIZE, utilities::read_u32};

// ---------------------------------------------------------------------------------------------- //

use chacha20::{
    ChaCha20,
    cipher::{KeyIvInit, StreamCipher, StreamCipherSeek},
};

// ---------------------------------------------------------------------------------------------- //

const CRYPTO_FIELD_NONE: u32 = 0x0;
const CRYPTO_FIELD_ICE: u32 = 0x1;
const CRYPTO_FIELD_AES: u32 = 0x2;
const CRYPTO_FIELD_CHACHA20: u32 = 0x3;
const CRYPTO_SHIFT: u32 = 20;
const CRYPTO_MASK: u32 = 0x0F;
const HASH_INITVAL: u32 = 0x000C_5EDE;
const IV_XOR: u32 = 0x6061_6263;
const KEY_WORDS: usize = 8;
const KEY_BYTES: usize = KEY_WORDS * U32_SIZE;
const NONCE_WORDS: usize = 3;
const NONCE_BYTES: usize = NONCE_WORDS * U32_SIZE;
const CHACHA_BLOCK_BYTES: u64 = 64;
const XOR_DELTAS: [u32; KEY_WORDS] = [
    0x0000_0000,
    0x0A0A_0A0A,
    0x0C0C_0C0C,
    0x0606_0606,
    0x0E0E_0E0E,
    0x0A0A_0A0A,
    0x0606_0606,
    0x0202_0202,
];
const LOOKUP3_INIT: u32 = 0xDEAD_BEEF;
const LOOKUP3_WORD_B_OFFSET: usize = U32_SIZE;
const LOOKUP3_WORD_C_OFFSET: usize = LOOKUP3_WORD_B_OFFSET + U32_SIZE;
const LOOKUP3_BLOCK_BYTES: usize = LOOKUP3_WORD_C_OFFSET + U32_SIZE;
const MIX_ROTATION_C_1: u32 = 4;
const MIX_ROTATION_A_1: u32 = 6;
const MIX_ROTATION_B_1: u32 = 8;
const MIX_ROTATION_C_2: u32 = 16;
const MIX_ROTATION_A_2: u32 = 19;
const MIX_ROTATION_B_2: u32 = 4;
const FINAL_ROTATION_B_1: u32 = 14;
const FINAL_ROTATION_C_1: u32 = 11;
const FINAL_ROTATION_A_1: u32 = 25;
const FINAL_ROTATION_B_2: u32 = 16;
const FINAL_ROTATION_C_2: u32 = 4;
const FINAL_ROTATION_A_2: u32 = 14;
const FINAL_ROTATION_B_3: u32 = 24;

// Dormant for the unencrypted map pack (0012); carried because the entry
// reader is generic over the pack flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Crypto {
    None,
    Ice,
    Aes,
    ChaCha20,
}

impl Crypto {
    pub(crate) fn from_flags(flags: u32) -> OfflineResult<Self> {
        let field = (flags >> CRYPTO_SHIFT) & CRYPTO_MASK;

        match field {
            CRYPTO_FIELD_NONE => Ok(Crypto::None),
            CRYPTO_FIELD_ICE => Ok(Crypto::Ice),
            CRYPTO_FIELD_AES => Ok(Crypto::Aes),
            CRYPTO_FIELD_CHACHA20 => Ok(Crypto::ChaCha20),
            n => Err(OfflineError::crypt(format!("unknown crypto field {n}"))),
        }
    }
}
pub(crate) fn decrypt(data: &mut [u8], basename: &str) -> OfflineResult<()> {
    let seed = hashlittle(basename.to_lowercase().as_bytes(), HASH_INITVAL)?;

    let mut key = [0u8; KEY_BYTES];
    for (i, delta) in XOR_DELTAS.iter().enumerate() {
        key[i * U32_SIZE..][..U32_SIZE].copy_from_slice(&(seed ^ IV_XOR ^ delta).to_le_bytes());
    }

    let mut nonce = [0u8; NONCE_BYTES];
    for word in nonce.as_chunks_mut::<U32_SIZE>().0 {
        word.copy_from_slice(&seed.to_le_bytes());
    }

    let mut cipher = ChaCha20::new(&key.into(), &nonce.into());
    cipher
        .try_seek(seed as u64 * CHACHA_BLOCK_BYTES)
        .map_err(|e| OfflineError::crypt(format!("chacha seek `{basename}`: {e}")))?;
    cipher.apply_keystream(data);

    Ok(())
}

// ---------------------------------------------------------------------------------------------- //

fn hashlittle(data: &[u8], initval: u32) -> OfflineResult<u32> {
    let mut length = data.len();
    let mut a = LOOKUP3_INIT
        .wrapping_add(length as u32)
        .wrapping_add(initval);
    let mut b = a;
    let mut c = a;
    let mut off = 0usize;

    while length > LOOKUP3_BLOCK_BYTES {
        a = a.wrapping_add(word(data, off)?);
        b = b.wrapping_add(word(data, off + LOOKUP3_WORD_B_OFFSET)?);
        c = c.wrapping_add(word(data, off + LOOKUP3_WORD_C_OFFSET)?);
        a = a.wrapping_sub(c) ^ c.rotate_left(MIX_ROTATION_C_1);
        c = c.wrapping_add(b);
        b = b.wrapping_sub(a) ^ a.rotate_left(MIX_ROTATION_A_1);
        a = a.wrapping_add(c);
        c = c.wrapping_sub(b) ^ b.rotate_left(MIX_ROTATION_B_1);
        b = b.wrapping_add(a);
        a = a.wrapping_sub(c) ^ c.rotate_left(MIX_ROTATION_C_2);
        c = c.wrapping_add(b);
        b = b.wrapping_sub(a) ^ a.rotate_left(MIX_ROTATION_A_2);
        a = a.wrapping_add(c);
        c = c.wrapping_sub(b) ^ b.rotate_left(MIX_ROTATION_B_2);
        b = b.wrapping_add(a);
        off += LOOKUP3_BLOCK_BYTES;
        length -= LOOKUP3_BLOCK_BYTES;
    }

    let mut tail = [0u8; LOOKUP3_BLOCK_BYTES];
    tail[..length].copy_from_slice(&data[off..off + length]);

    c = c.wrapping_add(tail_word(&tail, LOOKUP3_WORD_C_OFFSET, length));
    b = b.wrapping_add(tail_word(&tail, LOOKUP3_WORD_B_OFFSET, length));
    a = a.wrapping_add(tail_word(&tail, 0, length));

    if length < U32_SIZE {
        return Ok(c);
    }

    c ^= b;
    c = c.wrapping_sub(b.rotate_left(FINAL_ROTATION_B_1));
    a ^= c;
    a = a.wrapping_sub(c.rotate_left(FINAL_ROTATION_C_1));
    b ^= a;
    b = b.wrapping_sub(a.rotate_left(FINAL_ROTATION_A_1));
    c ^= b;
    c = c.wrapping_sub(b.rotate_left(FINAL_ROTATION_B_2));
    a ^= c;
    a = a.wrapping_sub(c.rotate_left(FINAL_ROTATION_C_2));
    b ^= a;
    b = b.wrapping_sub(a.rotate_left(FINAL_ROTATION_A_2));
    c ^= b;
    c = c.wrapping_sub(b.rotate_left(FINAL_ROTATION_B_3));

    Ok(c)
}

fn tail_word(tail: &[u8; LOOKUP3_BLOCK_BYTES], word_offset: usize, length: usize) -> u32 {
    let word_end = word_offset + U32_SIZE;
    let mut bytes = [0u8; U32_SIZE];

    if length >= word_end {
        bytes.copy_from_slice(&tail[word_offset..word_end]);
    } else if length > word_offset {
        bytes[..length - word_offset].copy_from_slice(&tail[word_offset..length]);
    }

    u32::from_le_bytes(bytes)
}

fn word(data: &[u8], off: usize) -> OfflineResult<u32> {
    read_u32(data, off).ok_or_else(|| OfflineError::crypt("hashlittle read truncated"))
}

// ---------------------------------------------------------------------------------------------- //

#[cfg(test)]
mod tests {
    use super::{CRYPTO_SHIFT, Crypto, decrypt};

    use crate::{OfflineResult, utilities::sha256_hex};

    // ------------------------------------------------------------------------------------------ //

    /// The basename the golden digest below was computed against; changing
    /// either half invalidates the other.
    const TEST_BASENAME: &str = "cd_worldmap_land_sdf_32768x32768_0_0.dds";
    /// sha256 of `decrypt` applied to `TEST_PLAINTEXT` for `TEST_BASENAME`,
    /// pinning the ported basename KDF + ChaCha20 seek + keystream against
    /// drift.
    const TEST_GOLDEN_DIGEST: &str =
        "09c93d633aabfd759401209c7f9ea8379b448ccc8c38cdf7220cbb8789f54158";
    const TEST_PLAINTEXT: &[u8] = &[
        0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE,
        0xFF, 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0xFE, 0xDC, 0xBA, 0x98, 0x76, 0x54,
        0x32, 0x10,
    ];
    const TEST_UNKNOWN_FIELD: u32 = 5;

    // ------------------------------------------------------------------------------------------ //

    #[test]
    fn from_flags_maps_every_field_value() -> OfflineResult<()> {
        let expected = [
            (0u32, Crypto::None),
            (1, Crypto::Ice),
            (2, Crypto::Aes),
            (3, Crypto::ChaCha20),
        ];
        for (field, crypto) in expected {
            let flags = field << CRYPTO_SHIFT;
            match Crypto::from_flags(flags) {
                Ok(decoded) => assert_eq!(
                    decoded, crypto,
                    "crypto field {field} must decode to {crypto:?}"
                ),
                Err(error) => unreachable!("crypto field {field} must decode: {error}"),
            }
        }

        Ok(())
    }

    #[test]
    fn from_flags_rejects_an_unknown_field() {
        let flags = TEST_UNKNOWN_FIELD << CRYPTO_SHIFT;
        let failure = match Crypto::from_flags(flags) {
            Err(error) => format!("{error}"),
            Ok(crypto) => format!("{crypto:?}"),
        };
        assert!(
            failure.contains("unknown crypto field 5"),
            "an unknown crypto field must be a named error, got: {failure}"
        );
    }

    /// Decrypt is a keystream XOR, so applying it once more restores the
    /// plaintext — the round trip pins the whole `decrypt` path (KDF, key
    /// layout, nonce, seek, keystream) against wiring regressions.
    #[test]
    fn decrypt_round_trips_through_itself() -> OfflineResult<()> {
        let mut encrypted = TEST_PLAINTEXT.to_vec();
        decrypt(&mut encrypted, TEST_BASENAME)?;

        let mut restored = encrypted;
        decrypt(&mut restored, TEST_BASENAME)?;
        assert_eq!(
            restored.as_slice(),
            TEST_PLAINTEXT,
            "a second decrypt must restore the plaintext"
        );

        Ok(())
    }

    /// The golden digest pins the ported derivation absolutely: any drift in
    /// the lookup3 seed, the XOR deltas, the nonce layout, or the block seek
    /// changes it.
    #[test]
    fn decrypt_matches_the_golden_digest() -> OfflineResult<()> {
        let mut data = TEST_PLAINTEXT.to_vec();
        decrypt(&mut data, TEST_BASENAME)?;

        let digest = sha256_hex(&data);
        assert!(
            digest == TEST_GOLDEN_DIGEST,
            "the decrypt golden digest drifted, got {digest}"
        );

        Ok(())
    }
}
