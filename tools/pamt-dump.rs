use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

fn read_u32(d: &[u8], off: &mut usize) -> Option<u32> {
    let b = d.get(*off..*off + 4)?;
    *off += 4;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn read_string(d: &[u8], off: &mut usize) -> Option<String> {
    let len = *d.get(*off)? as usize;
    *off += 1;
    let b = d.get(*off..*off + len)?;
    *off += len;
    Some(String::from_utf8_lossy(b).into_owned())
}

struct Rec {
    path: String,
    offset: u64,
    comp: u32,
    orig: u32,
    flags: u32,
}

fn parse(d: &[u8], pack_dir: &PathBuf, pack_name: &str) -> Vec<Rec> {
    let mut off = 0usize;
    let mut out = Vec::new();
    off += 4; // magic
    let paz_count = read_u32(d, &mut off).unwrap_or(0) as usize;
    off += 8 + paz_count * 8 + paz_count.saturating_sub(1) * 4;
    let folder_size = read_u32(d, &mut off).unwrap_or(0) as usize;
    let folder_end = (off + folder_size).min(d.len());
    let mut prefix = String::new();
    while off < folder_end {
        let parent = read_u32(d, &mut off).unwrap_or(0);
        let name = read_string(d, &mut off).unwrap_or_default();
        if parent == u32::MAX {
            prefix = name;
        }
    }
    let node_size = read_u32(d, &mut off).unwrap_or(0) as usize;
    let node_start = off;
    let node_end = (node_start + node_size).min(d.len());
    let mut nodes: BTreeMap<u32, (u32, String)> = BTreeMap::new();
    while off < node_end {
        let rel = (off - node_start) as u32;
        let parent = read_u32(d, &mut off).unwrap_or(0);
        let name = read_string(d, &mut off).unwrap_or_default();
        nodes.insert(rel, (parent, name));
    }
    let folder_count = read_u32(d, &mut off).unwrap_or(0) as usize;
    off += 4 + folder_count * 16;
    while off + 20 <= d.len() {
        let node_ref = read_u32(d, &mut off).unwrap_or(0);
        let offset = read_u32(d, &mut off).unwrap_or(0);
        let comp = read_u32(d, &mut off).unwrap_or(0);
        let orig = read_u32(d, &mut off).unwrap_or(0);
        let flags = read_u32(d, &mut off).unwrap_or(0);
        let mut cur = node_ref;
        let mut parts: Vec<&str> = Vec::new();
        let mut depth = 0;
        while cur != u32::MAX && depth < 64 {
            match nodes.get(&cur) {
                Some((parent, name)) => {
                    parts.push(name.as_str());
                    cur = *parent;
                }
                None => break,
            }
            depth += 1;
        }
        let node_path: String = parts.iter().rev().copied().collect();
        let path = if prefix.is_empty() {
            node_path
        } else {
            format!("{prefix}/{node_path}")
        };
        let paz_index = flags & 0xFF;
        let _ = pack_dir; // paz file = pack_dir/<paz_index>.paz, resolved by the reader
        out.push(Rec { path, offset: offset as u64, comp, orig, flags });
    }
    let _ = pack_name;
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let root = PathBuf::from(&args[1]);
    match args.get(2).map(|s| s.as_str()) {
        Some("grep") => {
            let needle = args[3].to_lowercase();
            let mut packs: Vec<PathBuf> = fs::read_dir(&root)
                .expect("root")
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir() && p.join("0.pamt").is_file())
                .collect();
            packs.sort();
            for pack in &packs {
                let name = pack.file_name().unwrap().to_string_lossy().to_string();
                let raw = fs::read(pack.join("0.pamt")).expect("pamt");
                for rec in parse(&raw, pack, &name) {
                    if rec.path.to_lowercase().contains(&needle) {
                        println!("{name}\t{}\t{}\t{}\t{}", rec.path, rec.orig, rec.flags, rec.offset);
                    }
                }
            }
        }
        Some(pack) => {
            let pack_dir = root.join(pack);
            let raw = fs::read(pack_dir.join("0.pamt")).expect("pamt");
            let out_path = PathBuf::from(format!("dump-{pack}.csv"));
            let mut f = fs::File::create(&out_path).expect("create");
            writeln!(f, "path,orig,comp,flags,offset").unwrap();
            for rec in parse(&raw, &pack_dir, pack) {
                writeln!(f, "{},{},{},{},{}", rec.path, rec.orig, rec.comp, rec.flags, rec.offset).unwrap();
            }
            println!("wrote {} entries to {}", out_path.display(), out_path.display());
        }
        other => panic!("mode must be a pack number or grep; got {other:?}"),
    }
}
