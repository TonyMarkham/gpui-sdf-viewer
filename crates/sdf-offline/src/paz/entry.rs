use crate::{Compression, Crypto, OfflineError, OfflineResult};

// ---------------------------------------------------------------------------------------------- //

use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
};

// ---------------------------------------------------------------------------------------------- //

#[derive(Debug, Clone)]
pub(crate) struct PazEntry {
    pub(crate) path: String,
    pub(crate) paz_file: PathBuf,
    pub(crate) offset: u64,
    pub(crate) comp_size: u32,
    pub(crate) orig_size: u32,
    pub(crate) flags: u32,
}

impl PazEntry {
    pub(crate) fn compressed(&self) -> bool {
        self.comp_size != self.orig_size
    }

    pub(crate) fn compression(&self) -> OfflineResult<Compression> {
        Compression::from_flags(self.flags)
    }

    pub(crate) fn matches_glob(&self, pattern: &str) -> bool {
        glob_match(pattern, &self.path) || glob_match(pattern, file_name(&self.path))
    }

    pub(crate) fn read(&self) -> OfflineResult<Vec<u8>> {
        let read_size = if self.compressed() {
            self.comp_size as usize
        } else {
            self.orig_size as usize
        };

        let mut file = File::open(&self.paz_file)
            .map_err(|e| OfflineError::io(format!("open `{}`: {e}", self.paz_file.display())))?;

        file.seek(SeekFrom::Start(self.offset)).map_err(|e| {
            OfflineError::io(format!(
                "seek `{}` to {}: {e}",
                self.paz_file.display(),
                self.offset
            ))
        })?;

        let mut raw = vec![0u8; read_size];
        file.read_exact(&mut raw).map_err(|e| {
            OfflineError::io(format!(
                "read {} bytes from `{}`: {e}",
                read_size,
                self.paz_file.display()
            ))
        })?;

        let mut data = raw;
        let crypto = Crypto::from_flags(self.flags)?;
        match crypto {
            Crypto::ChaCha20 => crate::paz::crypto::decrypt(&mut data, file_name(&self.path))?,
            Crypto::None => {}
            other => {
                return Err(OfflineError::crypt(format!(
                    "entry `{}`: unsupported crypto {other:?}",
                    self.path
                )));
            }
        }

        match self.compressed() {
            false => Ok(data),
            true => match self.compression()? {
                Compression::Lz4 => {
                    let mut out = vec![0u8; self.orig_size as usize];
                    lz4_flex::block::decompress_into(&data, &mut out).map_err(|e| {
                        OfflineError::paz(format!(
                            "lz4 decompress `{}`: {e}{}",
                            self.path,
                            if matches!(crypto, Crypto::ChaCha20) {
                                " (check for new key material)"
                            } else {
                                ""
                            }
                        ))
                    })?;
                    Ok(out)
                }
                Compression::Partial => {
                    crate::dds::partial::reconstruct(&data, self.orig_size as usize)
                }
                other => Err(OfflineError::paz(format!(
                    "entry `{}`: unsupported compression {other:?}",
                    self.path
                ))),
            },
        }
    }
}

// ---------------------------------------------------------------------------------------------- //

pub(crate) fn file_name(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[i + 1..],
        None => path,
    }
}

pub(crate) fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti) = (0, 0);
    let (mut star, mut mark) = (None, 0);

    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }

    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }

    pi == p.len()
}

// ---------------------------------------------------------------------------------------------- //

#[cfg(test)]
mod tests {
    use super::{PazEntry, file_name, glob_match};

    use crate::{OfflineError, OfflineResult, paz::crypto::decrypt};

    use std::path::PathBuf;

    // ------------------------------------------------------------------------------------------ //

    const TEST_DIR_NAME: &str = "sdf-offline-paz-entry";
    const TEST_ENTRY_PATH: &str = "ui/cd_worldmap_land_sdf_32768x32768_0_0.dds";
    const TEST_BASENAME: &str = "cd_worldmap_land_sdf_32768x32768_0_0.dds";
    const TEST_SCENARIO_CHACHA_LZ4: &str = "chacha-lz4";
    const TEST_SCENARIO_PARTIAL: &str = "partial";
    const TEST_PLAINTEXT_LEN: usize = 4096;
    const TEST_CRYPTO_CHACHA20: u32 = 3;
    const TEST_CRYPTO_NONE: u32 = 0;
    const TEST_COMPRESSION_LZ4: u32 = 2;
    const TEST_COMPRESSION_PARTIAL: u32 = 1;
    const TEST_CRYPTO_SHIFT: u32 = 20;
    const TEST_COMPRESSION_SHIFT: u32 = 16;
    const TEST_HEADER_SIZE: usize = 128;
    const TEST_MIP_COUNT: u32 = 1;
    const TEST_WIDTH: u32 = 64;
    const TEST_HEIGHT: u32 = 64;
    const TEST_BIT_COUNT: u32 = 8;
    const TEST_COMP_SIZE_OFFSET: usize = 32;
    const TEST_DECOMP_SIZE_OFFSET: usize = 36;
    const TEST_BIT_COUNT_OFFSET: usize = 88;
    const TEST_FOURCC_OFFSET: usize = 84;
    const TEST_MIP_COUNT_OFFSET: usize = 28;
    const TEST_WIDTH_OFFSET: usize = 16;
    const TEST_HEIGHT_OFFSET: usize = 12;
    const TEST_U32_SIZE: usize = 4;

    // ------------------------------------------------------------------------------------------ //

    /// The encrypted entry path: ChaCha20 (basename KDF) then LZ4, the shape
    /// the encrypted packs use. The paz bytes are produced by applying the
    /// keystream once — decrypt is symmetric — so `read` must strip both
    /// layers and return the plaintext.
    #[test]
    fn read_strips_chacha20_then_lz4() -> OfflineResult<()> {
        scenario(TEST_SCENARIO_CHACHA_LZ4, |paz_file| {
            let plaintext: Vec<u8> = (0..TEST_PLAINTEXT_LEN)
                .map(|index| (index % 32) as u8)
                .collect();
            let compressed = lz4_flex::block::compress(&plaintext);
            assert!(
                compressed.len() < plaintext.len(),
                "the fixture payload must actually compress, {} vs {}",
                compressed.len(),
                plaintext.len()
            );

            let mut encrypted = compressed;
            decrypt(&mut encrypted, TEST_BASENAME)?;

            std::fs::write(&paz_file, &encrypted)
                .map_err(|e| OfflineError::io(format!("write `{}`: {e}", paz_file.display())))?;
            let entry = PazEntry {
                path: String::from(TEST_ENTRY_PATH),
                paz_file,
                offset: 0,
                comp_size: encrypted.len() as u32,
                orig_size: plaintext.len() as u32,
                flags: (TEST_CRYPTO_CHACHA20 << TEST_CRYPTO_SHIFT)
                    | (TEST_COMPRESSION_LZ4 << TEST_COMPRESSION_SHIFT),
            };

            match entry.read() {
                Ok(data) => assert_eq!(
                    data, plaintext,
                    "read must strip ChaCha20 then LZ4 back to the plaintext"
                ),
                Err(error) => unreachable!("the encrypted entry must read back: {error}"),
            }

            Ok(())
        })
    }

    /// The partial-DDS path: a compressed block behind a real DDS header,
    /// reconstructed by `dds::partial::reconstruct` through `PazEntry::read`.
    #[test]
    fn read_reconstructs_a_partial_dds_blob() -> OfflineResult<()> {
        scenario(TEST_SCENARIO_PARTIAL, |paz_file| {
            let payload: Vec<u8> = (0..TEST_PLAINTEXT_LEN)
                .map(|index| (index.wrapping_mul(7) % 32) as u8)
                .collect();
            let compressed = lz4_flex::block::compress(&payload);

            let mut blob = partial_header(compressed.len() as u32, payload.len() as u32);
            blob.extend_from_slice(&compressed);

            // The reconstructor keeps the header verbatim, sizes included.
            let mut expected = blob[..TEST_HEADER_SIZE].to_vec();
            expected.extend_from_slice(&payload);

            std::fs::write(&paz_file, &blob)
                .map_err(|e| OfflineError::io(format!("write `{}`: {e}", paz_file.display())))?;
            let entry = PazEntry {
                path: String::from(TEST_ENTRY_PATH),
                paz_file,
                offset: 0,
                comp_size: blob.len() as u32,
                orig_size: (TEST_HEADER_SIZE + payload.len()) as u32,
                flags: (TEST_CRYPTO_NONE << TEST_CRYPTO_SHIFT)
                    | (TEST_COMPRESSION_PARTIAL << TEST_COMPRESSION_SHIFT),
            };

            match entry.read() {
                Ok(data) => assert_eq!(
                    data, expected,
                    "read must reconstruct the partial-DDS blob header plus payload"
                ),
                Err(error) => unreachable!("the partial blob must read back: {error}"),
            }

            Ok(())
        })
    }

    #[test]
    fn file_name_takes_the_last_segment() {
        assert_eq!(file_name(TEST_ENTRY_PATH), TEST_BASENAME);
        assert_eq!(file_name(TEST_BASENAME), TEST_BASENAME);
    }

    #[test]
    fn glob_match_stars_and_case() {
        assert!(glob_match(
            "cd_worldmap_land_sdf*",
            file_name(TEST_ENTRY_PATH)
        ));
        assert!(glob_match("cd_worldmap_*", TEST_BASENAME));
        assert!(glob_match("*0_0.dds", TEST_BASENAME));
        assert!(!glob_match("cd_worldmap_road_sdf*", TEST_BASENAME));
        assert!(!glob_match("cd_worldmap_land_sdf_?_?", TEST_BASENAME));
        assert!(
            !glob_match("cd_worldmap_land_sdf*", TEST_ENTRY_PATH),
            "the glob matches file names, not the full entry path"
        );
    }

    // ------------------------------------------------------------------------------------------ //

    fn scenario(name: &str, check: impl FnOnce(PathBuf) -> OfflineResult<()>) -> OfflineResult<()> {
        let dir =
            std::env::temp_dir().join(format!("{TEST_DIR_NAME}-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)
            .map_err(|e| OfflineError::io(format!("create `{}`: {e}", dir.display())))?;

        let result = check(dir.join("0.paz"));
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    /// A minimal uncompressed 8-bit DDS header with the compressed-block
    /// sizes in the reserved1 words the partial reconstructor reads.
    fn partial_header(comp_size: u32, decomp_size: u32) -> Vec<u8> {
        let mut data = vec![0u8; TEST_HEADER_SIZE];
        data[0..4].copy_from_slice(b"DDS ");
        data[TEST_HEIGHT_OFFSET..TEST_HEIGHT_OFFSET + TEST_U32_SIZE]
            .copy_from_slice(&TEST_HEIGHT.to_le_bytes());
        data[TEST_WIDTH_OFFSET..TEST_WIDTH_OFFSET + TEST_U32_SIZE]
            .copy_from_slice(&TEST_WIDTH.to_le_bytes());
        data[TEST_MIP_COUNT_OFFSET..TEST_MIP_COUNT_OFFSET + TEST_U32_SIZE]
            .copy_from_slice(&TEST_MIP_COUNT.to_le_bytes());
        data[TEST_FOURCC_OFFSET..TEST_FOURCC_OFFSET + TEST_U32_SIZE].copy_from_slice(&[0u8; 4]);
        data[TEST_BIT_COUNT_OFFSET..TEST_BIT_COUNT_OFFSET + TEST_U32_SIZE]
            .copy_from_slice(&TEST_BIT_COUNT.to_le_bytes());
        data[TEST_COMP_SIZE_OFFSET..TEST_COMP_SIZE_OFFSET + TEST_U32_SIZE]
            .copy_from_slice(&comp_size.to_le_bytes());
        data[TEST_DECOMP_SIZE_OFFSET..TEST_DECOMP_SIZE_OFFSET + TEST_U32_SIZE]
            .copy_from_slice(&decomp_size.to_le_bytes());
        data
    }
}
