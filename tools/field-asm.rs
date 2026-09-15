// Assemble a tiled field's mip0 from the app export into a downsampled PGM.
// usage: field-asm <data_dir> <tile_prefix> <out.pgm> [threshold]
use std::fs;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = PathBuf::from(&args[1]);
    let prefix: &str = &args[2];
    let out = PathBuf::from(&args[3]);
    let threshold: u8 = args.get(4).map(|s| s.parse().unwrap()).unwrap_or(122);

    let manifest_raw = fs::read_to_string(dir.join("sdf").join("manifest.json")).expect("manifest");

    let mut payloads: Vec<(u32, u32, String)> = Vec::new();
    // slice the layers array into per-layer chunks keyed by tile_prefix,
    // then split each layer's tiles array into per-tile objects
    let mut cursor = 0usize;
    while let Some(ti) = manifest_raw[cursor..].find("\"tiles\"") {
        let abs = cursor + ti;
        // find this layer's tile_prefix (appears before "tiles" in the layer object)
        let head = &manifest_raw[cursor..abs];
        let prefix_here = head
            .rfind("\"tile_prefix\"")
            .and_then(|i| {
                let after = &head[i..];
                let colon = after.find(':')?;
                let rest = &after[colon..];
                let a = rest.find('"')? + 1;
                let b = rest[a..].find('"')? + a;
                Some(rest[a..b].to_string())
            })
            .unwrap_or_default();
        // layer extent: to the next "tile_prefix" or end
        let layer_end = manifest_raw[abs..]
            .find("\"tile_prefix\"")
            .map(|j| abs + j)
            .unwrap_or(manifest_raw.len());
        if prefix_here == prefix {
            // split the tiles array into objects
            for chunk in manifest_raw[abs..layer_end].split('{') {
                let l = chunk.split('}').next().unwrap_or("");
                let get = |k: &str| -> Option<String> {
                    let key = format!("\"{}\"", k);
                    let i = l.find(&key)? + key.len();
                    let rest = &l[i..];
                    let i = rest.find(':')? + 1;
                    let rest = rest[i..].trim_start();
                    let end = rest.find([',', '}']).unwrap_or(rest.len());
                    let mut v = rest[..end].trim().to_string();
                    if v.starts_with('"') {
                        v = v.trim_matches('"').to_string();
                    }
                    Some(v)
                };
                let (Some(payload), Some(xs), Some(ys)) = (get("payload"), get("x"), get("y"))
                else {
                    continue;
                };
                let x: u32 = match xs.parse() {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let y: u32 = match ys.parse() {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                payloads.push((x, y, payload));
            }
        }
        cursor = layer_end;
    }
    println!("tiles for {prefix}: {}", payloads.len());
    payloads.sort();
    payloads.dedup();
    println!("after dedupe: {}", payloads.len());
    assert_eq!(payloads.len(), 256, "expected the full 16x16 grid");

    let edge = 512usize;
    let per = 16usize; // output cells per tile edge
    let small = 16 * per; // 256
    let block = edge / per; // 32
    let mut field = vec![0u8; small * small];
    for (x, y, payload) in &payloads {
        let raw = fs::read(dir.join("sdf").join(payload)).expect("payload");
        // stub payloads are a tiny constant-fill chain, not a 512^2 surface
        let stub = raw.len() < 262144;
        for sy in 0..per {
            for sx in 0..per {
                let avg = if stub {
                    raw[0]
                } else {
                    let mut sum = 0u64;
                    for yy in 0..block {
                        let base = (sy * block + yy) * edge + sx * block;
                        for xx in 0..block {
                            sum += raw[base + xx] as u64;
                        }
                    }
                    (sum / (block * block) as u64) as u8
                };
                let gx = (*x as usize) * per + sx;
                let gy = (*y as usize) * per + sy;
                field[gy * small + gx] = if avg >= threshold { 255 } else { 0 };
            }
        }
    }

    let mut pgm = format!("P5\n{small} {small}\n255\n").into_bytes();
    pgm.extend_from_slice(&field);
    fs::write(&out, &pgm).expect("write pgm");
    println!("wrote {}", out.display());
}
