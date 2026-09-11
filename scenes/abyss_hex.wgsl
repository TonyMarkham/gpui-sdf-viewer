// sdf-scene: name = "Abyss hex"
// sdf-scene: data = "D:/git/cd-data-extract/.work/fields/manifest.json"
// sdf-scene: layer = "abyss_hex"
//
// Game-data scene: the abyss-hex band recipe from the map pipeline's
// config.toml ([sdf] abyss_hex = { low = 120, high = 134 }).

const PAPER: vec3f = vec3f(0.930, 0.900, 0.840);
const INK: vec3f = vec3f(0.110, 0.140, 0.220);

const LOW: f32 = 120.0;
const HIGH: f32 = 134.0;

fn render(p: vec2f, _time: f32) -> vec4f {
    let coverage = smoothstep(LOW, HIGH, field(p) * 255.0);
    return vec4f(mix(PAPER, INK, coverage), 1.0);
}
