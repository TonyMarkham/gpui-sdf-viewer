// sdf-scene: name = "Coast"
// sdf-scene: data = "config:sdf/manifest.json"
// sdf-scene: layer = "coast"
//
// Game-data scene: the coast recipe from the map pipeline's config.toml
// ([sdf] coast = { low = 122, high = 132 }; zero level 128, which also
// carves rivers from land). Coverage bytes, not a distance field — so this
// scene takes over shading instead of scene_shaded.

const PAPER: vec3f = vec3f(0.930, 0.900, 0.840);
const INK: vec3f = vec3f(0.150, 0.130, 0.110);

const LOW: f32 = 122.0;
const HIGH: f32 = 132.0;

fn render(p: vec2f, _time: f32) -> vec4f {
    let coverage = smoothstep(LOW, HIGH, field(p) * 255.0);
    return vec4f(mix(PAPER, INK, coverage), 1.0);
}
