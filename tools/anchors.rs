// Extract the (x,y,z) triple at offset 8+nameLen+1 from uimaptextureinfo records.
// usage: anchors <body.bin> <header.bin> <substr> [<substr>...]
use std::fs;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let body = fs::read(&args[1]).expect("body");
    let header = fs::read(&args[2]).expect("header");
    let count = u16::from_le_bytes([header[0], header[1]]) as usize;
    let stride = if header.len() == 2 + count * 6 { 6 } else { 8 };
    let key_wide = stride == 8;

    let mut records = Vec::new();
    for i in 0..count {
        let b = 2 + i * stride;
        let key = if key_wide {
            u32::from_le_bytes([header[b], header[b + 1], header[b + 2], header[b + 3]])
        } else {
            u16::from_le_bytes([header[b], header[b + 1]]) as u32
        };
        let ob = b + if key_wide { 4 } else { 2 };
        let off = u32::from_le_bytes([header[ob], header[ob + 1], header[ob + 2], header[ob + 3]])
            as usize;
        records.push((key, off));
    }

    let f32_at = |o: usize| -> f32 {
        f32::from_le_bytes([body[o], body[o + 1], body[o + 2], body[o + 3]])
    };

    for want in &args[3..] {
        for w in records.windows(2) {
            let start = w[0].1;
            let end = w[1].1.min(body.len());
            if start + 8 > end {
                continue;
            }
            let len_off = start + if key_wide { 4 } else { 2 };
            let len = u32::from_le_bytes([
                body[len_off],
                body[len_off + 1],
                body[len_off + 2],
                body[len_off + 3],
            ]) as usize;
            let name_at = len_off + 4;
            if name_at + len > end {
                continue;
            }
            let name = String::from_utf8_lossy(&body[name_at..name_at + len]).to_string();
            if !name.contains(want.as_str()) {
                continue;
            }
            // triple at 8 + nameLen + 1 (the NUL)
            let t = name_at + len + 1;
            if t + 12 > end {
                continue;
            }
            println!(
                "{}\tkey {}\t ({:.1}, {:.1}, {:.1})\ttriple@{}",
                name,
                w[0].0,
                f32_at(t),
                f32_at(t + 4),
                f32_at(t + 8),
                t - start
            );
            break;
        }
    }
}
