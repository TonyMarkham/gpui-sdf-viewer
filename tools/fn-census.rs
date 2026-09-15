// Byte-aligned world-triple census for factionnode.staticinfo.
// The docs' rule: float-pattern scan within record extents, every byte offset
// (the property bags are byte-packed; a 4-aligned scan misses entries).
// usage: fn-census <prefix> [name-substr...]
use std::fs;
use std::path::PathBuf;

struct Container {
    body: Vec<u8>,
    records: Vec<(u32, u32, String)>,
}

fn parse_container(header_path: &PathBuf, body_path: &PathBuf) -> Option<Container> {
    let header = fs::read(header_path).ok()?;
    let body = fs::read(body_path).ok()?;
    if header.len() < 2 {
        return None;
    }
    let count = u16::from_le_bytes([header[0], header[1]]) as usize;
    let (stride, key_wide) = if header.len() == 2 + count * 6 {
        (6usize, false)
    } else if header.len() == 2 + count * 8 {
        (8usize, true)
    } else {
        return None;
    };
    let mut records = Vec::with_capacity(count);
    for i in 0..count {
        let b = 2 + i * stride;
        let key = if key_wide {
            u32::from_le_bytes([header[b], header[b + 1], header[b + 2], header[b + 3]])
        } else {
            u16::from_le_bytes([header[b], header[b + 1]]) as u32
        };
        let ob = b + if key_wide { 4 } else { 2 };
        let off = u32::from_le_bytes([
            header[ob],
            header[ob + 1],
            header[ob + 2],
            header[ob + 3],
        ]) as usize;
        records.push((key, off as u32, String::new()));
    }
    for i in 0..records.len() {
        let start = records[i].1 as usize;
        let end = records
            .get(i + 1)
            .map(|r| r.1 as usize)
            .unwrap_or(body.len());
        let len_off = start + if key_wide { 4 } else { 2 };
        if len_off + 4 > end || len_off + 4 > body.len() {
            continue;
        }
        let len = u32::from_le_bytes([
            body[len_off],
            body[len_off + 1],
            body[len_off + 2],
            body[len_off + 3],
        ]) as usize;
        let name_at = len_off + 4;
        if len == 0 || len > 4096 || name_at + len > end || name_at + len > body.len() {
            continue;
        }
        let name = String::from_utf8_lossy(&body[name_at..name_at + len]).to_string();
        records[i].2 = name;
    }
    Some(Container { body, records })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let prefix = &args[1];
    let header = PathBuf::from(format!("{}-header.bin", prefix));
    let body_p = PathBuf::from(format!("{}-body.bin", prefix));
    let Some(c) = parse_container(&header, &body_p) else {
        println!("cannot parse {}", prefix);
        return;
    };
    println!("{}: {} records, body {} B", prefix, c.records.len(), c.body.len());

    let mut with_pos = 0usize;
    let mut strict_pos = 0usize;
    let mut rel_mod4 = [0usize; 4];
    for w in c.records.windows(2) {
        let (start, end) = (w[0].1 as usize, (w[1].1 as usize).min(c.body.len()));
        let mut o = start;
        let mut loose = None;
        let mut strict = None;
        while o + 12 <= end {
            let x = f32::from_le_bytes([c.body[o], c.body[o + 1], c.body[o + 2], c.body[o + 3]]);
            let y = f32::from_le_bytes([c.body[o + 4], c.body[o + 5], c.body[o + 6], c.body[o + 7]]);
            let z = f32::from_le_bytes([c.body[o + 8], c.body[o + 9], c.body[o + 10], c.body[o + 11]]);
            if (-17000.0..3500.0).contains(&x)
                && x.abs() > 50.0
                && (0.0..9000.0).contains(&y)
                && (-12000.0..9500.0).contains(&z)
                && z.abs() > 50.0
            {
                if loose.is_none() {
                    loose = Some((x, y, z, o - start));
                }
                // strict: the game's own measured position space (factionnode
                // x <= 0, terrain-like y); kills bag-byte false positives
                if strict.is_none()
                    && (-17000.0..10.0).contains(&x)
                    && (100.0..2000.0).contains(&y)
                    && (-12000.0..9500.0).contains(&z)
                    && z.abs() > 50.0
                {
                    strict = Some((x, y, z, o - start));
                }
                if strict.is_some() {
                    break;
                }
            }
            o += 1;
        }
        if let Some((x, y, z, off)) = loose {
            with_pos += 1;
            rel_mod4[off % 4] += 1;
        }
        if let Some((x, y, z, off)) = strict {
            strict_pos += 1;
            let lname = w[0].2.to_lowercase();
            if args[2..].iter().any(|s| lname.contains(&s.to_lowercase())) {
                println!(
                    "  `{}` key {}  strict ({:.4}, {:.4}, {:.4}) @rel+{} (rel%4={})   loose {:?}",
                    w[0].2, w[0].0, x, y, z, off, off % 4,
                    loose.map(|(a, b, c2, d)| (a, b, c2, d))
                );
            }
        }
    }
    println!(
        "records: {}   loose first-triple: {}   strict first-triple: {}",
        c.records.len(),
        with_pos,
        strict_pos
    );
    println!("loose-first (offset-from-record-start) mod 4: {:?}", rel_mod4);
}
