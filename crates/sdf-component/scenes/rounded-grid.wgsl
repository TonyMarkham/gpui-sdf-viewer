// sdf-scene: name = "Rounded grid"
//
// A static grid of rounded boxes whose corner radius varies across space.

fn scene(p: vec2f, _time: f32) -> f32 {
    let cell = vec2f(0.36, 0.36);
    let q = vec2f(p.x - cell.x * round(p.x / cell.x), p.y - cell.y * round(p.y / cell.y));
    let radius = 0.03 + 0.03 * sin(p.x * 4.0 + p.y * 3.0);
    return sd_round_box(q, vec2f(0.13, 0.13), radius);
}