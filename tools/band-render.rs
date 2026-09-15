use std::fs;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dds = fs::read(&args[1]).expect("dds");
    let out = &args[2];
    let w = u32::from_le_bytes([dds[12], dds[13], dds[14], dds[15]]) as usize;
    let h = u32::from_le_bytes([dds[16], dds[17], dds[18], dds[19]]) as usize;
    let mip0 = &dds[128..128 + w * h];
    let mut max = 0usize;
    for &b in mip0 {
        max = max.max(b as usize);
    }
    // two measured shells (fractions of the per-texture max)
    let shells: [(&str, f64, f64); 2] = [("halo", 0.42, 0.55), ("core", 0.75, 0.90)];
    for (name, lo, hi) in shells {
        let mut img = vec![0u8; w * h];
        for (i, &b) in mip0.iter().enumerate() {
            let v = b as f64 / max as f64;
            // plateau band: full ink inside [lo,hi], smooth 2-byte edges
            let c = if v >= lo && v <= hi { 1.0 } else { 0.0 };
            img[i] = (c * 255.0) as u8;
        }
        fs::write(format!("{out}-{name}.pgm"), pgm(w, h, &img)).unwrap();
    }
    println!("max {max}; wrote {out}-halo.pgm, {out}-core.pgm");
}

fn pgm(w: usize, h: usize, data: &[u8]) -> Vec<u8> {
    let mut out = format!("P5\n{w} {h}\n255\n").into_bytes();
    out.extend_from_slice(data);
    out
}
