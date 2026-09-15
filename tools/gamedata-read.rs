use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

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

/// Full LZ4 block decompression.
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
            if out.len() + lit_len > dst_len {
                out.extend_from_slice(&lit[..dst_len - out.len()]);
                break;
            }
            out.extend_from_slice(lit);
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
    Some(out)
}

/// The "partial" compression: header + per-mip lz4/raw blocks (dds/partial.rs).
/// bpp: bytes per pixel (the repo's R8 tiles are 1; 16-bit surfaces are 2).
fn partial_reconstruct_bpp(blob: &[u8], orig: usize, bpp: usize) -> Option<Vec<u8>> {
    if blob.len() < 128 || &blob[0..4] != b"DDS " {
        return None;
    }
    let u32at = |o: usize| u32::from_le_bytes([blob[o], blob[o + 1], blob[o + 2], blob[o + 3]]);
    let width = u32at(16);
    let height = u32at(12);
    let depth = u32at(24);
    let mip_count = u32at(28);
    let caps2 = u32at(112);
    let multi = mip_count > 5 && caps2 == 0 && depth < 2;
    let mut blocks: Vec<(usize, usize)> = Vec::new();
    if multi {
        let count = mip_count.min(4) as usize;
        let (mut mw, mut mh) = (width, height);
        for k in 0..count {
            blocks.push((u32at(32 + k * 4) as usize, (mw * mh) as usize * bpp));
            mw >>= 1;
            mh >>= 1;
        }
    } else {
        blocks.push((u32at(32) as usize, u32at(36) as usize * bpp));
    }
    let mut out = blob[..128].to_vec();
    let mut pos = 128usize;
    for (c, d) in blocks {
        if c == d {
            out.extend_from_slice(blob.get(pos..pos + d)?);
        } else {
            let s = blob.get(pos..pos + c)?;
            out.extend_from_slice(&lz4_decompress(s, d)?);
        }
        pos += c;
    }
    out.extend_from_slice(blob.get(pos..)?);
    if out.len() != orig {
        return None;
    }
    Some(out)
}

fn partial_reconstruct(blob: &[u8], orig: usize) -> Option<Vec<u8>> {
    partial_reconstruct_bpp(blob, orig, 1)
        .or_else(|| partial_reconstruct_bpp(blob, orig, 2))
        .or_else(|| partial_reconstruct_bpp(blob, orig, 4))
}

fn read_entry(root: &PathBuf, pack: &str, rec: &Rec) -> Option<Vec<u8>> {
    let crypto = (rec.flags >> 20) & 0xF;
    if crypto != 0 {
        return None;
    }
    let compf = (rec.flags >> 16) & 0xF;
    let paz = root.join(pack).join(format!("{}.paz", rec.flags & 0xFF));
    let mut file = fs::File::open(&paz).ok()?;
    file.seek(SeekFrom::Start(rec.offset)).ok()?;
    if rec.comp == rec.orig {
        let mut raw = vec![0u8; rec.orig as usize];
        file.read_exact(&mut raw).ok()?;
        return Some(raw);
    }
    let mut blob = vec![0u8; rec.comp as usize];
    file.read_exact(&mut blob).ok()?;
    match compf {
        1 => partial_reconstruct(&blob, rec.orig as usize),
        _ => lz4_decompress(&blob, rec.orig as usize),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // usage: gamedata-read <root> <pack> <csv> <out-prefix> <path> [<path>...]
    let root = PathBuf::from(&args[1]);
    let pack = args[2].clone();
    let recs = load_csv(&PathBuf::from(&args[3]));
    let prefix = &args[4];
    for want in &args[5..] {
        let Some(rec) = recs.iter().find(|r| &r.path == want) else {
            println!("MISS {want}");
            continue;
        };
        match read_entry(&root, &pack, rec) {
            Some(data) => {
                let name = want.rsplit('/').next().unwrap_or(want);
                let out = format!("{prefix}-{name}");
                fs::write(&out, &data).expect("write");
                println!("OK   {want} -> {} ({} B)", out, data.len());
            }
            None => println!("FAIL {want} (encrypted or decompress error)"),
        }
    }
}
