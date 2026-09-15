// M0 title-SDF evidence: decode a regiontitle DDS (uncompressed 8-bit, mip chain),
// measure the byte distribution, render the single-ramp recovery.
// usage: title-decode <dds> <out-prefix>
use std::fs;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dds = fs::read(&args[1]).expect("dds");
    let out = &args[2];
    assert_eq!(&dds[0..4], b"DDS ");
    let w = u32::from_le_bytes([dds[12], dds[13], dds[14], dds[15]]) as usize;
    let h = u32::from_le_bytes([dds[16], dds[17], dds[18], dds[19]]) as usize;
    let mips = u32::from_le_bytes([dds[27], dds[28], dds[29], dds[30]]) as usize;
    let fourcc = &dds[84..88];
    let bits = u16::from_le_bytes([dds[88], dds[89]]);
    println!("{w}x{h} mips {mips} fourcc {:?} bits {bits} file {} B", fourcc, dds.len());
    assert_eq!(fourcc, &[0, 0, 0, 0]);
    assert_eq!(bits, 8);
    let mip0 = &dds[128..128 + w * h];

    let mut hist = [0u64; 256];
    for &b in mip0 {
        hist[b as usize] += 1;
    }
    let max = (0..=255u32).rev().find(|&v| hist[v as usize] > 0).unwrap() as usize;
    let min = (0..=255u32).find(|&v| hist[v as usize] > 0).unwrap() as usize;
    let total = (w * h) as u64;
    let mut pct = |p: f64| -> usize {
        let want = (total as f64 * p) as u64;
        let mut acc = 0u64;
        for v in 0..256 {
            acc += hist[v];
            if acc >= want {
                return v;
            }
        }
        255
    };
    println!("min {min}  max {max}  zeros {} ({:.2}%)", hist[0], hist[0] as f64 / total as f64 * 100.0);
    println!(
        "p50 {}  p90 {}  p95 {}  p99 {}  p99.9 {}",
        pct(0.50),
        pct(0.90),
        pct(0.95),
        pct(0.99),
        pct(0.999)
    );
    let nz: u64 = total - hist[0];
    println!("nonzero: {nz} ({:.2}%)", nz as f64 / total as f64 * 100.0);
    let mut npct = |p: f64| -> usize {
        let want = (nz as f64 * p) as u64;
        let mut acc = 0u64;
        for v in 1..256 {
            acc += hist[v];
            if acc >= want {
                return v;
            }
        }
        255
    };
    println!(
        "nonzero p05 {}  p10 {}  p25 {}  p50 {}  p75 {}  p90 {}  p95 {}",
        npct(0.05),
        npct(0.10),
        npct(0.25),
        npct(0.50),
        npct(0.75),
        npct(0.90),
        npct(0.95)
    );
    let mut top: Vec<(usize, u64)> = hist.iter().copied().enumerate().filter(|&(_, c)| c > 0).collect();
    top.sort_by(|a, b| b.1.cmp(&a.1));
    println!("top bins: {:?}", &top[..top.len().min(8)]);
    // bimodality: mass below half-max vs above
    let half = max / 2;
    let below: u64 = hist[..=half].iter().sum();
    let above: u64 = hist[half + 1..].iter().sum();
    println!("mass <= {half}: {below}   > {half}: {above}");

    // raw visual (normalized to max)
    let mut raw = Vec::with_capacity(w * h);
    for &b in mip0 {
        raw.push(((b as f64 / max as f64) * 255.0) as u8);
    }
    fs::write(format!("{out}-raw.pgm"), pgm(w, h, &raw)).unwrap();

    // single-ramp recovery at the VS-11 shape: v = b/133, smoothstep(0.42, 0.55)
    let norm = 133.0f64;
    let (lo, hi) = (0.42f64, 0.55f64);
    let mut cov = Vec::with_capacity(w * h);
    for &b in mip0 {
        let v = b as f64 / norm;
        let t = ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
        let s = t * t * (3.0 - 2.0 * t);
        cov.push((s * 255.0) as u8);
    }
    fs::write(format!("{out}-ramp.pgm"), pgm(w, h, &cov)).unwrap();
    println!("wrote {out}-raw.pgm, {out}-ramp.pgm");
}

fn pgm(w: usize, h: usize, data: &[u8]) -> Vec<u8> {
    let mut out = format!("P5\n{w} {h}\n255\n").into_bytes();
    out.extend_from_slice(data);
    out
}
