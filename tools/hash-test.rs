// Test whether the factionnode post-name id = a 16-bit hash of the record name.
// usage: hash-test <fn-header.bin> <fn-body.bin>
use std::fs;

const HASH_INITVAL: u32 = 0x000C_5EDE;

fn hashlittle(data: &[u8], initval: u32) -> u32 {
    let mut length = data.len();
    let mut a = 0xDEAD_BEEFu32
        .wrapping_add(length as u32)
        .wrapping_add(initval);
    let mut b = a;
    let mut c = a;
    let mut off = 0usize;
    let word = |d: &[u8], o: usize| u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]);
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
    let tw = |o: usize| -> u32 {
        let mut bytes = [0u8; 4];
        if length >= o + 4 {
            bytes.copy_from_slice(&tail[o..o + 4]);
        } else if length > o {
            bytes[..length - o].copy_from_slice(&tail[o..length]);
        }
        u32::from_le_bytes(bytes)
    };
    c = c.wrapping_add(tw(8));
    b = b.wrapping_add(tw(4));
    a = a.wrapping_add(tw(0));
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

// simple FNV-1a 32 for comparison
fn fnv1a(data: &[u8]) -> u32 {
    let mut h = 0x811C9DC5u32;
    for &b in data {
        h ^= b as u32;
        h = h.wrapping_mul(0x01000193);
    }
    h
}

fn main() {
    let header = fs::read(std::env::args().nth(1).expect("hdr")).expect("hdr");
    let body = fs::read(std::env::args().nth(2).expect("body")).expect("body");
    let count = u16::from_le_bytes([header[0], header[1]]) as usize;
    let stride = 8usize; // factionnode
    let mut pairs: Vec<(String, u32)> = Vec::new();
    for i in 0..count {
        let b = 2 + i * stride;
        let ob = b + 4;
        let off =
            u32::from_le_bytes([header[ob], header[ob + 1], header[ob + 2], header[ob + 3]])
                as usize;
        let end = if i + 1 < count {
            let ob2 = 2 + (i + 1) * stride + 4;
            u32::from_le_bytes([
                header[ob2],
                header[ob2 + 1],
                header[ob2 + 2],
                header[ob2 + 3],
            ]) as usize
        } else {
            body.len()
        };
        if off + 8 > end || off + 8 > body.len() {
            continue;
        }
        let len = u32::from_le_bytes([
            body[off + 4],
            body[off + 5],
            body[off + 6],
            body[off + 7],
        ]) as usize;
        let name_at = off + 8;
        if len == 0 || len > 256 || name_at + len + 5 > end {
            continue;
        }
        let name = String::from_utf8_lossy(&body[name_at..name_at + len]).to_string();
        // post-name id: the u32 right after the NUL
        let id = u32::from_le_bytes([
            body[name_at + len + 1],
            body[name_at + len + 2],
            body[name_at + len + 3],
            body[name_at + len + 4],
        ]);
        pairs.push((name, id));
    }
    println!("pairs: {}", pairs.len());

    // candidate transforms to test
    let variants: Vec<(&str, Box<dyn Fn(&str) -> u32>)> = vec![
        (
            "hashlittle(lower) & 0xFFFF",
            Box::new(|n: &str| hashlittle(n.to_lowercase().as_bytes(), HASH_INITVAL) & 0xFFFF),
        ),
        (
            "hashlittle(lower) >> 16",
            Box::new(|n: &str| hashlittle(n.to_lowercase().as_bytes(), HASH_INITVAL) >> 16),
        ),
        (
            "hashlittle(raw) & 0xFFFF",
            Box::new(|n: &str| hashlittle(n.as_bytes(), HASH_INITVAL) & 0xFFFF),
        ),
        (
            "hashlittle(raw,0) & 0xFFFF",
            Box::new(|n: &str| hashlittle(n.as_bytes(), 0) & 0xFFFF),
        ),
        (
            "fnv1a(lower) & 0xFFFF",
            Box::new(|n: &str| fnv1a(n.to_lowercase().as_bytes()) & 0xFFFF),
        ),
        (
            "fnv1a(raw) & 0xFFFF",
            Box::new(|n: &str| fnv1a(n.as_bytes()) & 0xFFFF),
        ),
        (
            "hashlittle(lower)+0x0F0000 == id (full u32)",
            Box::new(|n: &str| {
                0x0F0000u32
                    .wrapping_add(hashlittle(n.to_lowercase().as_bytes(), HASH_INITVAL) & 0xFFFF)
            }),
        ),
    ];

    for (label, f) in &variants {
        let mut hit = 0usize;
        let mut first = String::new();
        for (name, id) in pairs.iter().take(200) {
            if f(name) == *id {
                hit += 1;
                if first.is_empty() {
                    first = format!("{} -> {:06X}", name, id);
                }
            }
        }
        println!("{:<42} {hit}/200  {first}", label);
    }

    // print 5 sample (name, id) pairs for manual inspection
    for (name, id) in pairs.iter().take(5) {
        println!("sample: {} -> {:06X}", name, id);
    }
}
