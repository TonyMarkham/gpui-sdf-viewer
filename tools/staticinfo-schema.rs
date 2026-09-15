// staticinfo schema analyzer.
// usage:
//   si-schema schema <header.bin> <body.bin> [max-offset]
//     per-offset (after name end) byte histograms across all records:
//     offsets that are CONSTANT across records are schema markers.
//   si-schema keys <header.bin> <body.bin>
//     histogram of 4-aligned u32 property keys in [0x0F0000, 0x100000).
use std::fs;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let header = fs::read(&args[2]).expect("header");
    let body = fs::read(&args[3]).expect("body");
    let count = u16::from_le_bytes([header[0], header[1]]) as usize;
    let stride = if header.len() == 2 + count * 6 {
        6usize
    } else if header.len() == 2 + count * 8 {
        8usize
    } else {
        panic!("header len {} does not fit count {} at stride 6 or 8", header.len(), count);
    };
    let key_wide = stride == 8;

    let mut spans: Vec<(usize, usize)> = Vec::new();
    for i in 0..count {
        let b = 2 + i * stride;
        let ob = b + if key_wide { 4 } else { 2 };
        let off =
            u32::from_le_bytes([header[ob], header[ob + 1], header[ob + 2], header[ob + 3]]) as usize;
        let end = header
            .get(2 + (i + 1) * stride)
            .map(|_| {
                let ob2 = 2 + (i + 1) * stride + if key_wide { 4 } else { 2 };
                u32::from_le_bytes([
                    header[ob2],
                    header[ob2 + 1],
                    header[ob2 + 2],
                    header[ob2 + 3],
                ]) as usize
            })
            .unwrap_or(body.len());
        spans.push((off, end.min(body.len())));
    }

    // record extents → name end offsets
    let mut name_ends: Vec<(usize, usize)> = Vec::new(); // (payload_start, record_end)
    for (i, &(start, end)) in spans.iter().enumerate() {
        if start + 8 > end || start + 8 > body.len() {
            name_ends.push((start, end));
            continue;
        }
        let len_off = start + if key_wide { 4 } else { 2 };
        let len = u32::from_le_bytes([
            body[len_off],
            body[len_off + 1],
            body[len_off + 2],
            body[len_off + 3],
        ]) as usize;
        let payload = len_off + 4 + len + 1; // name + NUL
        let payload = if payload <= end && len > 0 && len < 4096 {
            payload
        } else {
            start // name parse failed; treat whole record as payload (rare)
        };
        name_ends.push((payload, end));
    }

    match args[1].as_str() {
        "schema" => {
            let max_off: usize = args.get(4).map(|s| s.parse().unwrap()).unwrap_or(24);
            println!("records: {}  body: {} B", count, body.len());
            for off in 0..max_off {
                // histogram of body[payload_start + off] across records with room
                let mut counts = std::collections::BTreeMap::<u8, usize>::new();
                let mut have = 0usize;
                for &(ps, end) in &name_ends {
                    if ps + off < end {
                        have += 1;
                        *counts.entry(body[ps + off]).or_insert(0) += 1;
                    }
                }
                if have == 0 {
                    break;
                }
                // constant offset = exactly one distinct value across all records
                if counts.len() == 1 {
                    let (v, c) = counts.iter().next().unwrap();
                    println!(
                        "  +{:<3} CONST {:02X}  ({}/{})",
                        off, v, c, have
                    );
                } else {
                    let top: Vec<String> = counts
                        .iter()
                        .rev()
                        .take(3)
                        .map(|(v, c)| format!("{:02X}:{}", v, c))
                        .collect();
                    println!(
                        "  +{:<3} varies ({} distinct)  top: {}",
                        off,
                        counts.len(),
                        top.join(" ")
                    );
                }
            }
        }
        "keys" => {
            let mut hist = std::collections::BTreeMap::<u32, usize>::new();
            let mut hits = 0usize;
            for &(start, end) in &name_ends {
                for o in start..end.saturating_sub(3) {
                    let v = u32::from_le_bytes([body[o], body[o + 1], body[o + 2], body[o + 3]]);
                    if (0x0F0000..0x100000).contains(&v) {
                        *hist.entry(v).or_insert(0) += 1;
                        hits += 1;
                    }
                }
            }
            println!("records: {}  key-range hits: {}", count, hits);
            let mut top: Vec<(u32, usize)> = hist.into_iter().collect();
            top.sort_by_key(|&(_, c)| std::cmp::Reverse(c));
            for (k, c) in top.iter().take(30) {
                println!("  {:06X}  x{}", k, c);
            }
        }
        m => panic!("mode {m}"),
    }
}
