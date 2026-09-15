use std::fs;
use std::path::PathBuf;

/// staticinfo container: header = u16 record count, then count ×
/// (u16 key, u32 offset) ordered by offset; body = concatenated records.
struct Container {
    body: Vec<u8>,
    records: Vec<(u32, u32, String)>, // (key, offset, name)
}

fn parse_container(header_path: &PathBuf, body_path: &PathBuf) -> Option<Container> {
    let header = fs::read(header_path).ok()?;
    let body = fs::read(body_path).ok()?;
    if header.len() < 2 {
        return None;
    }
    let count = u16::from_le_bytes([header[0], header[1]]) as usize;
    // entry stride varies per table: 6 (u16 key + u32 offset) or 8 (u32 key + u32 offset)
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

    // names: each record starts [u16 tag][u32 nameLen][name bytes][00]...
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

fn hexdump(body: &[u8], start: usize, end: usize) -> String {
    let mut s = String::new();
    let mut i = start;
    while i < end && i < body.len() {
        let e = (i + 16).min(end).min(body.len());
        let row = &body[i..e];
        let mut hex = String::new();
        let mut asc = String::new();
        for (k, &b) in row.iter().enumerate() {
            hex.push_str(&format!("{:02X}", b));
            if k + 1 < row.len() {
                hex.push(' ');
            }
            asc.push(if (32..127).contains(&b) { b as char } else { '.' });
        }
        s.push_str(&format!("{:8}  {:<48}  {}\n", i, hex, asc));
        i += 16;
    }
    s
}

/// aligned f32 scan; prints values in any of the plausible map ranges
fn float_scan(body: &[u8], start: usize, end: usize) -> String {
    let mut s = String::new();
    let mut o = start;
    while o + 4 <= end && o + 4 <= body.len() {
        let f = f32::from_le_bytes([body[o], body[o + 1], body[o + 2], body[o + 3]]);
        let plausible = f.is_finite()
            && ((-20000.0..=20000.0).contains(&f) && f.abs() > 0.001
                || (0.0..=1.0).contains(&f) && f != 0.0
                || (1.0..=8192.0).contains(&f));
        if plausible {
            s.push_str(&format!("  @{o}: {f}\n"));
        }
        o += 4;
    }
    s
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // usage: si <table-prefix> <mode> [args...]
    //   modes: records               list (key, offset, name)
    //   dump   <name-substr> [n]     hexdump first n matching records
    //   scan   <name-substr>         float-scan matching records
    //   find   <f32 value>           records whose payload contains this float (tol f32 eps)
    let prefix = &args[1];
    let header = PathBuf::from(format!("{}-header.bin", prefix));
    let body_p = PathBuf::from(format!("{}-body.bin", prefix));
    // accept either -header.bin/-body.bin or the original names
    let header = if header.exists() {
        header
    } else {
        PathBuf::from(format!("{}.staticinfoheader", prefix))
    };
    let body_p = if body_p.exists() {
        body_p
    } else {
        PathBuf::from(format!("{}.staticinfobody", prefix))
    };
    let Some(c) = parse_container(&header, &body_p) else {
        println!("cannot parse {}", prefix);
        return;
    };
    println!(
        "{}: {} records, body {} B\n",
        prefix,
        c.records.len(),
        c.body.len()
    );

    match args[2].as_str() {
        "records" => {
            for (k, off, name) in &c.records {
                println!("{:6}  {:8}  {}", k, off, name);
            }
        }
        "dump" => {
            let want = &args[3];
            let n: usize = args.get(4).map(|s| s.parse().unwrap()).unwrap_or(1);
            let mut shown = 0;
            for w in c.records.windows(2) {
                if w[0].2.to_lowercase().contains(&want.to_lowercase()) {
                    let (start, end) = (w[0].1 as usize, w[1].1 as usize);
                    println!("--- key {} @ {}..{}  {} ({} B)", w[0].0, start, end, w[0].2, end - start);
                    print!("{}", hexdump(&c.body, start, end));
                    shown += 1;
                    if shown >= n {
                        break;
                    }
                }
            }
            if shown == 0 {
                println!("no record name contains `{}`", want);
            }
        }
        "dumpat" => {
            let idx: usize = args[3].parse().unwrap();
            if idx + 1 < c.records.len() {
                let (start, end) = (c.records[idx].1 as usize, c.records[idx + 1].1 as usize);
                println!(
                    "--- #{} key {} @ {}..{} ({} B)",
                    idx, c.records[idx].0, start, end, end - start
                );
                print!("{}", hexdump(&c.body, start, end));
                print!("--- float scan:\n{}", float_scan(&c.body, start, end));
            }
        }
        "scan" => {
            let want = &args[3];
            for w in c.records.windows(2) {
                if w[0].2.to_lowercase().contains(&want.to_lowercase()) {
                    let (start, end) = (w[0].1 as usize, w[1].1 as usize);
                    println!("--- key {} @ {}..{}  {} ({} B)", w[0].0, start, end, w[0].2, end - start);
                    print!("{}", float_scan(&c.body, start, end));
                }
            }
        }
        "triples" => {
            // aligned (x,y,z) world triples, near-zero excluded; up to 3 per record
            for w in c.records.windows(2) {
                let (start, end) = (w[0].1 as usize, (w[1].1 as usize).min(c.body.len()));
                let mut o = start;
                let mut hits = 0;
                let mut last = (f32::NAN, f32::NAN, f32::NAN);
                while o + 12 <= end && hits < 3 {
                    let x = f32::from_le_bytes([c.body[o], c.body[o + 1], c.body[o + 2], c.body[o + 3]]);
                    let y = f32::from_le_bytes([c.body[o + 4], c.body[o + 5], c.body[o + 6], c.body[o + 7]]);
                    let z = f32::from_le_bytes([c.body[o + 8], c.body[o + 9], c.body[o + 10], c.body[o + 11]]);
                    if (-17000.0..3500.0).contains(&x)
                        && x.abs() > 50.0
                        && (0.0..9000.0).contains(&y)
                        && (-12000.0..9500.0).contains(&z)
                        && z.abs() > 50.0
                        && (x, y, z) != last
                    {
                        println!("{}\t{}\t{}\t{}\t@{}", w[0].2, x, y, z, o - start);
                        last = (x, y, z);
                        hits += 1;
                    }
                    o += 4;
                }
            }
        }
        "find" => {
            let v: f32 = args[3].parse().unwrap();
            for w in c.records.windows(2) {
                let (start, end) = (w[0].1 as usize, (w[1].1 as usize).min(c.body.len()));
                let mut o = start;
                while o + 4 <= end {
                    let f = f32::from_le_bytes([c.body[o], c.body[o + 1], c.body[o + 2], c.body[o + 3]]);
                    if (f - v).abs() < v.abs() * 1e-6 + 1e-4 {
                        println!("{:.6} found in `{}` @ {} (key {})", v, w[0].2, o, w[0].0);
                        break;
                    }
                    o += 4;
                }
            }
        }
        m => println!("unknown mode {}", m),
    }
}
