use std::fs;
fn main() {
    // grammar probe: records containing [FF FF FF FF][0x0F____ u32] within 6 bytes
    for (name, hdr, bd) in [
        ("regioninfo", "gd-ri-header.bin", "gd-ri-body.bin"),
        ("factionnode", "gd-fn-header.bin", "gd-fn-body.bin"),
        ("uimaptextureinfo", "gd-ui-header.bin", "gd-ui-body.bin"),
        ("bitmapposition", "gd-bm-header.bin", "gd-bm-body.bin"),
        ("fieldinfo", "gd-fi-header.bin", "gd-fi-body.bin"),
    ] {
        let header = fs::read(hdr).expect("hdr");
        let body = fs::read(bd).expect("body");
        let count = u16::from_le_bytes([header[0], header[1]]) as usize;
        let stride = if header.len() == 2 + count * 6 { 6usize } else { 8usize };
        let kw = stride == 8;
        let mut spans: Vec<(usize, usize)> = Vec::new();
        for i in 0..count {
            let b = 2 + i * stride;
            let ob = b + if kw { 4 } else { 2 };
            let off = u32::from_le_bytes([header[ob], header[ob+1], header[ob+2], header[ob+3]]) as usize;
            let end = if i + 1 < count {
                let o2 = 2 + (i+1)*stride + if kw { 4 } else { 2 };
                u32::from_le_bytes([header[o2], header[o2+1], header[o2+2], header[o2+3]]) as usize
            } else { body.len() };
            spans.push((off, end.min(body.len())));
        }
        let mut with = 0usize; let mut total = 0usize;
        for (i, &(s, e)) in spans.iter().enumerate() {
            total += 1;
            let _ = i;
            let mut found = false;
            for o in s..e.saturating_sub(7) {
                if body[o+1..o+5] == [0xFF, 0xFF, 0xFF, 0xFF] {
                    let v = u32::from_le_bytes([body[o+5], body[o+6], body[o+7], body.get(o+8).copied().unwrap_or(0)]);
                    if (0x0F0000..0x100000).contains(&v) { found = true; break; }
                }
            }
            if found { with += 1 }
        }
        println!("{name}: {with}/{total} records contain the [count][-1][id] pattern");
    }
    // misc probe: ASCII name runs and their spacing
    let misc = fs::read("miscdec.bin").expect("misc");
    let mut name_pos: Vec<(usize, String)> = Vec::new();
    let mut i = 0usize;
    while i + 12 < misc.len() {
        // u32 len followed by an uppercase-starting printable name
        let len = u32::from_le_bytes([misc[i], misc[i+1], misc[i+2], misc[i+3]]) as usize;
        if (8..128).contains(&len) && i + 4 + len + 1 <= misc.len() {
            let name = &misc[i+4..i+4+len];
            if name.iter().all(|&b| (b == b'_' as u8) || (b >= 0x41 && b <= 0x7A)) && name[0].is_ascii_uppercase() {
                let s = String::from_utf8_lossy(name).to_string();
                name_pos.push((i, s));
                i += 4 + len;
                continue;
            }
        }
        i += 1;
    }
    println!("misc: {} embedded names", name_pos.len());
    for w in name_pos.windows(2).take(6) {
        println!("  @{:>9}  {}   (stride to next: {})", w[0].0, w[0].1, w[1].0 - w[0].0);
    }
    if name_pos.len() > 1 {
        let first = name_pos[0].0;
        let last = name_pos.last().unwrap().0;
        println!("  span: {} B over {} names -> avg stride {}", last - first, name_pos.len() - 1, (last - first) / (name_pos.len() - 1));
    }
}
