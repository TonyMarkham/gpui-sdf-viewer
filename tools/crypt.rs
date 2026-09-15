// ChaCha20 (RFC 8439) + the repo's basename KDF, ported from
// crates/sdf-offline/src/paz/crypto.rs. Verifies against real encrypted entries.
// usage: crypt <mode> ...
//   dec <paz-or-raw>  — handled by caller modes below
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

const HASH_INITVAL: u32 = 0x000C_5EDE;
const IV_XOR: u32 = 0x6061_6263;
const XOR_DELTAS: [u32; 8] = [
    0x0000_0000,
    0x0A0A_0A0A,
    0x0C0C_0C0C,
    0x0606_0606,
    0x0E0E_0E0E,
    0x0A0A_0A0A,
    0x0606_0606,
    0x0202_0202,
];

fn hashlittle(data: &[u8], initval: u32) -> u32 {
    let mut length = data.len();
    let mut a = 0xDEAD_BEEFu32
        .wrapping_add(length as u32)
        .wrapping_add(initval);
    let mut b = a;
    let mut c = a;
    let mut off = 0usize;
    let word = |d: &[u8], o: usize| -> u32 {
        u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
    };
    while length > 12 {
        a = a.wrapping_add(word(data, off));
        b = b.wrapping_add(word(data, off + 4));
        c = c.wrapping_add(word(data, off + 8));
        a = a.wrapping_sub(c) ^ c.rotate_left(4);
        c = c.wrapping_add(b);
        b = b.wrapping_sub(a) ^ a.rotate_left(6);
        a = a.wrapping_add(c);
        c = c.wrapping_sub(b) ^ b.rotate_left(8);
        b = b.wrapping_add(a);
        a = a.wrapping_sub(c) ^ c.rotate_left(16);
        c = c.wrapping_add(b);
        b = b.wrapping_sub(a) ^ a.rotate_left(19);
        a = a.wrapping_add(c);
        c = c.wrapping_sub(b) ^ b.rotate_left(4);
        b = b.wrapping_add(a);
        off += 12;
        length -= 12;
    }
    let mut tail = [0u8; 12];
    tail[..length].copy_from_slice(&data[off..off + length]);
    let tail_word = |o: usize| -> u32 {
        let mut bytes = [0u8; 4];
        if length >= o + 4 {
            bytes.copy_from_slice(&tail[o..o + 4]);
        } else if length > o {
            bytes[..length - o].copy_from_slice(&tail[o..length]);
        }
        u32::from_le_bytes(bytes)
    };
    c = c.wrapping_add(tail_word(8));
    b = b.wrapping_add(tail_word(4));
    a = a.wrapping_add(tail_word(0));
    if length < 4 {
        return c;
    }
    c ^= b;
    c = c.wrapping_sub(b.rotate_left(14));
    a ^= c;
    a = a.wrapping_sub(c.rotate_left(11));
    b ^= a;
    b = b.wrapping_sub(a.rotate_left(25));
    c ^= b;
    c = c.wrapping_sub(b.rotate_left(16));
    a ^= c;
    a = a.wrapping_sub(c.rotate_left(4));
    b ^= a;
    b = b.wrapping_sub(a.rotate_left(14));
    c ^= b;
    c = c.wrapping_sub(b.rotate_left(24));
    c
}

fn quarter(s: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    s[a] = s[a].wrapping_add(s[b]);
    s[d] ^= s[a];
    s[d] = s[d].rotate_left(16);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] ^= s[c];
    s[b] = s[b].rotate_left(12);
    s[a] = s[a].wrapping_add(s[b]);
    s[d] ^= s[a];
    s[d] = s[d].rotate_left(8);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] ^= s[c];
    s[b] = s[b].rotate_left(7);
}

fn chacha_block(key: &[u8; 32], counter: u32, nonce: &[u8; 12]) -> [u8; 64] {
    let mut s = [
        0x6170_7865u32,
        0x3320_646e,
        0x7962_2d32,
        0x6b20_6574,
        u32::from_le_bytes(key[0..4].try_into().unwrap()),
        u32::from_le_bytes(key[4..8].try_into().unwrap()),
        u32::from_le_bytes(key[8..12].try_into().unwrap()),
        u32::from_le_bytes(key[12..16].try_into().unwrap()),
        u32::from_le_bytes(key[16..20].try_into().unwrap()),
        u32::from_le_bytes(key[20..24].try_into().unwrap()),
        u32::from_le_bytes(key[24..28].try_into().unwrap()),
        u32::from_le_bytes(key[28..32].try_into().unwrap()),
        counter,
        u32::from_le_bytes(nonce[0..4].try_into().unwrap()),
        u32::from_le_bytes(nonce[4..8].try_into().unwrap()),
        u32::from_le_bytes(nonce[8..12].try_into().unwrap()),
    ];
    let start = s;
    for _ in 0..10 {
        quarter(&mut s, 0, 4, 8, 12);
        quarter(&mut s, 1, 5, 9, 13);
        quarter(&mut s, 2, 6, 10, 14);
        quarter(&mut s, 3, 7, 11, 15);
        quarter(&mut s, 0, 5, 10, 15);
        quarter(&mut s, 1, 6, 11, 12);
        quarter(&mut s, 2, 7, 8, 13);
        quarter(&mut s, 3, 4, 9, 14);
    }
    let mut out = [0u8; 64];
    for i in 0..16 {
        let v = s[i].wrapping_add(start[i]);
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    out
}

/// Full LZ4 block decompression (block format, as in gamedata-read.rs).
fn lz4_decompress(src: &[u8], dst_len: usize) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(dst_len);
    let mut i = 0usize;
    loop {
        if i >= src.len() || out.len() >= dst_len {
            break;
        }
        let token = src[i];
        i += 1;
        let mut lit_len = (token >> 4) as usize;
        if lit_len == 15 {
            loop {
                let b = *src.get(i)?;
                i += 1;
                lit_len += b as usize;
                if b != 255 {
                    break;
                }
            }
        }
        if lit_len > 0 {
            let lit = src.get(i..i + lit_len)?;
            i += lit_len;
            let take = lit_len.min(dst_len - out.len());
            out.extend_from_slice(&lit[..take]);
            if out.len() >= dst_len {
                break;
            }
        }
        if i + 2 > src.len() {
            break;
        }
        let offset = u16::from_le_bytes([src[i], src[i + 1]]) as usize;
        i += 2;
        if offset == 0 || offset > out.len() {
            return None;
        }
        let mut match_len = (token & 0xF) as usize + 4;
        if token & 0xF == 15 {
            loop {
                let b = *src.get(i)?;
                i += 1;
                match_len += b as usize;
                if b != 255 {
                    break;
                }
            }
        }
        let mut start = out.len() - offset;
        for _ in 0..match_len {
            if out.len() >= dst_len {
                break;
            }
            let b = out[start];
            out.push(b);
            start += 1;
        }
    }
    if out.len() != dst_len {
        return None;
    }
    Some(out)
}

/// The repo's decrypt: keystream XOR with KDF(key, nonce) and seek seed*64.
fn decrypt(data: &mut [u8], basename: &str) {
    let seed = hashlittle(basename.to_lowercase().as_bytes(), HASH_INITVAL);
    let mut key = [0u8; 32];
    for (i, delta) in XOR_DELTAS.iter().enumerate() {
        key[i * 4..i * 4 + 4].copy_from_slice(&(seed ^ IV_XOR ^ delta).to_le_bytes());
    }
    let mut nonce = [0u8; 12];
    for w in 0..3 {
        nonce[w * 4..w * 4 + 4].copy_from_slice(&seed.to_le_bytes());
    }
    let start_block = seed; // seek seed*64 bytes = block seed
    for (chunk, k) in data.chunks_mut(64).zip(0u32..) {
        let ks = chacha_block(&key, start_block.wrapping_add(k), &nonce);
        for (b, ks_b) in chunk.iter_mut().zip(ks.iter()) {
            *b ^= ks_b;
        }
    }
}

fn entry_bytes(root: &PathBuf, pack: &str, rec: &Rec) -> Option<Vec<u8>> {
    let crypto = (rec.flags >> 20) & 0xF;
    if crypto != 3 {
        return None;
    }
    let paz = root.join(pack).join(format!("{}.paz", rec.flags & 0xFF));
    let mut file = fs::File::open(&paz).ok()?;
    file.seek(SeekFrom::Start(rec.offset)).ok()?;
    let mut blob = vec![0u8; rec.comp as usize];
    file.read_exact(&mut blob).ok()?;
    Some(blob)
}

#[derive(Clone)]
struct Rec {
    path: String,
    offset: u64,
    comp: u32,
    orig: u32,
    flags: u32,
}

fn load_csv(p: &PathBuf) -> Vec<Rec> {
    let raw = fs::read_to_string(p).expect("csv");
    let mut out = Vec::new();
    for (i, line) in raw.lines().enumerate() {
        if i == 0 {
            continue;
        }
        let f: Vec<&str> = line.split(',').collect();
        if f.len() == 5 {
            out.push(Rec {
                path: f[0].to_string(),
                offset: f[4].parse().unwrap_or(0),
                comp: f[2].parse().unwrap_or(0),
                orig: f[1].parse().unwrap_or(0),
                flags: f[3].parse().unwrap_or(0),
            });
        }
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args[1].as_str();
    let save = mode == "save";
    let root = PathBuf::from(&args[2]);
    let pack = args[3].clone();
    let recs = load_csv(&PathBuf::from(&args[4]));

    for want in &args[5..] {
        let Some(rec) = recs.iter().find(|r| r.path.contains(want.as_str())) else {
            println!("MISS {want}");
            continue;
        };
        let Some(blob) = entry_bytes(&root, &pack, rec) else {
            println!("SKIP {want} (not chacha20)");
            continue;
        };
        // crypto applies to the compressed payload; basename = file name
        let basename = rec.path.rsplit('/').next().unwrap_or(&rec.path).to_string();
        let mut data = blob;
        decrypt(&mut data, &basename);
        // stored pipeline: orig -> lz4 -> encrypt; unwrap in reverse
        if data.len() != rec.orig as usize {
            if let Some(full) = lz4_decompress(&data, rec.orig as usize) {
                data = full;
            }
        }
        if save {
            // write next to the CSV (inside tools/), never the CWD — keeps
            // decrypted game files under the gitignore umbrella
            let out_dir = PathBuf::from(&args[4])
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."));
            let name = basename.replace(['/', '\\'], "_");
            let out = out_dir.join(&name);
            fs::write(&out, &data).expect("write");
            println!("saved {} ({} B)", out.display(), data.len());
            continue;
        }
        let printable = data
            .iter()
            .take(64)
            .filter(|&&b| (32..127).contains(&b) || b == b'\n' || b == b'\r' || b == b'\t')
            .count();
        println!(
            "{}  basename `{}`  {} B  printable {}/64",
            rec.path,
            basename,
            data.len(),
            printable
        );
        let head: String = data
            .iter()
            .take(160)
            .map(|&b| if (32..127).contains(&b) { b as char } else { '.' })
            .collect();
        println!("   head: {head}");
    }
}
