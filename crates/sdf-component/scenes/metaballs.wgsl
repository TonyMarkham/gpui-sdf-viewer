// sdf-scene: name = "Metaballs"
// sdf-scene: animated = true
//
// Smoothly blended metaballs orbiting the origin. The last one follows the
// mouse (u.mouse is in the same coordinate space as `p`).

fn scene(p: vec2f, time: f32) -> f32 {
    var d = sd_circle(p - vec2f(cos(time * 0.7) * 0.55, sin(time * 0.9) * 0.4), 0.28);
    d = op_smooth_union(d, sd_circle(p - vec2f(cos(time * 0.31 + 2.0) * 0.6, sin(time * 0.53) * 0.5), 0.22), 0.25);
    d = op_smooth_union(d, sd_circle(p - vec2f(sin(time * 0.43) * 0.7, cos(time * 0.61 + 1.0) * 0.35), 0.18), 0.25);
    let mouse_ball = sd_circle(p - u.mouse, 0.16);
    d = op_smooth_union(d, mouse_ball, 0.3);
    return d;
}