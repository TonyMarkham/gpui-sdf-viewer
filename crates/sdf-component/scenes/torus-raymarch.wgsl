// sdf-scene: name = "Torus raymarch"
// sdf-scene: animated = true
//
// Demonstrates the `fn render` escape hatch: a 3D raymarched torus that takes
// over shading entirely. Return straight (non-premultiplied) alpha.

fn sd_torus(p: vec3f, t: vec2f) -> f32 {
    let q = vec2f(length(p.xz) - t.x, p.y);
    return length(q) - t.y;
}

fn spin_y(p: vec3f, a: f32) -> vec3f {
    let c = cos(a);
    let s = sin(a);
    return vec3f(c * p.x + s * p.z, p.y, -s * p.x + c * p.z);
}

fn scene3(p: vec3f, time: f32) -> f32 {
    let spin = spin_y(p, time * 0.4);
    return sd_torus(spin, vec2f(0.75, 0.3));
}

fn render(uv: vec2f, time: f32) -> vec4f {
    let ro = vec3f(2.6 * sin(time * 0.5), 1.4, 2.6 * cos(time * 0.5));
    let forward = normalize(-ro);
    let right = normalize(cross(forward, vec3f(0.0, 1.0, 0.0)));
    let up = cross(right, forward);
    let rd = normalize(forward * 1.7 + right * uv.x + up * uv.y);

    var t = 0.0;
    var hit = false;
    for (var i = 0; i < 96; i++) {
        let p = ro + rd * t;
        let d = scene3(p, time);
        if (d < 0.0015) {
            hit = true;
            break;
        }
        t += max(d * 0.9, 0.01);
        if (t > 8.0) {
            break;
        }
    }
    if (!hit) {
        return vec4f(0.0, 0.0, 0.0, 0.0);
    }

    let p = ro + rd * t;
    let eps = 0.002;
    let n = normalize(vec3f(
        scene3(p + vec3f(eps, 0.0, 0.0), time) - scene3(p - vec3f(eps, 0.0, 0.0), time),
        scene3(p + vec3f(0.0, eps, 0.0), time) - scene3(p - vec3f(0.0, eps, 0.0), time),
        scene3(p + vec3f(0.0, 0.0, eps), time) - scene3(p - vec3f(0.0, 0.0, eps), time),
    ));

    let light = normalize(vec3f(0.6, 0.8, -0.4));
    let diffuse = clamp(dot(n, light), 0.0, 1.0);
    let color = vec3f(0.318, 0.525, 0.953) * (0.25 + 0.75 * diffuse);
    return vec4f(color, 1.0);
}