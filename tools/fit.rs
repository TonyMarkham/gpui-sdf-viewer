// Fit render-fraction -> field-uv affine against land/sea classes.
// u = a + b*fx ; v = c + d*fy   (fx,fy in [0,1] over the render)
use std::fs;

fn main() {
    let n = 256usize;
    let field = fs::read("land256.pgm").expect("field pgm");
    let mut off = 0usize;
    let mut nl = 0;
    while nl < 3 {
        if field[off] == b'\n' {
            nl += 1;
        }
        off += 1;
    }
    let fdata = &field[off..];
    assert_eq!(fdata.len(), n * n);
    let cls = fs::read("ref256.cls").expect("cls");
    let cf = fs::read("ref256.cf").expect("cf");
    assert_eq!(cls.len(), n * n);

    let sample: Vec<usize> = (0..n * n).filter(|&i| cf[i] == 1).collect();
    let coarse: Vec<usize> = sample.iter().step_by(32).copied().collect();
    println!("confident: {}  coarse: {}", sample.len(), coarse.len());

    let score = |a: f32, b: f32, c: f32, d: f32, cells: &[usize]| -> f32 {
        let mut hit = 0usize;
        let tot = cells.len();
        for &i in cells {
            let fx = (i % n) as f32 / (n - 1) as f32;
            let fy = (i / n) as f32 / (n - 1) as f32;
            let u = a + b * fx;
            let v = c + d * fy;
            // out-of-bounds = miss (no skipping)
            if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
                continue;
            }
            let gu = (u * (n - 1) as f32) as usize;
            let gv = (v * (n - 1) as f32) as usize;
            if (fdata[gv * n + gu] == 255) == (cls[i] == 255) {
                hit += 1;
            }
        }
        hit as f32 / tot as f32
    };

    let mut best = (0.0f32, 1.0f32, 0.0f32, 1.0f32, 0.0f32);
    for a in (-40..=40).map(|k| k as f32 * 0.04) {
        for b in (70..=130).chain((-130..=-70).rev()).map(|k| k as f32 * 0.02) {
            for c in (-100..=100).map(|k| k as f32 * 0.04) {
                for d in (70..=130).chain((-130..=-70).rev()).map(|k| k as f32 * 0.02) {
                    let s = score(a, b, c, d, &coarse);
                    if s > best.4 {
                        best = (a, b, c, d, s);
                    }
                }
            }
        }
    }
    println!(
        "coarse best: u = {:.2} + {:.2}*fx  v = {:.2} + {:.2}*fy  acc {:.3}",
        best.0, best.1, best.2, best.3, best.4
    );

    let (a0, b0, c0, d0, _) = best;
    for a in (-5..=5).map(|k| a0 + k as f32 * 0.005) {
        for b in (-5..=5).map(|k| b0 + k as f32 * 0.005) {
            for c in (-10..=10).map(|k| c0 + k as f32 * 0.01) {
                for d in (-5..=5).map(|k| d0 + k as f32 * 0.005) {
                    let s = score(a, b, c, d, &coarse);
                    if s > best.4 {
                        best = (a, b, c, d, s);
                    }
                }
            }
        }
    }
    println!(
        "refined:     u = {:.3} + {:.3}*fx  v = {:.3} + {:.3}*fy  acc(coarse) {:.3}",
        best.0, best.1, best.2, best.3, best.4
    );
    println!(
        "refined full-sample acc: {:.3}",
        score(best.0, best.1, best.2, best.3, &sample)
    );

    println!(
        "window-A (z_top at top)     acc {:.3}",
        score(0.0, 1.0, 0.0, 1.0, &sample)
    );
    println!(
        "window-B (flipped vertical) acc {:.3}",
        score(0.0, 1.0, 1.0, -1.0, &sample)
    );
    // content-bounds mapping: u=(x-cxmin)/(cspan), v=(czmax-z)/(czspan) expressed
    // as render->field-uv: field u = u_min + (u_win - 0.156)/0.631 ... precompute:
    // content x [-13350,-1050] -> field u [0.156,0.787]; z [-7120,4070] -> v_A [0.212,0.786]
    // render fx in [0,1] maps to content directly:
    // field_u = 0.156 + 0.631*fx ; field v = 0.786 - 0.574*fy
    println!(
        "content-bounds mapping      acc {:.3}",
        score(0.156, 0.631, 0.786, -0.574, &sample)
    );
}
