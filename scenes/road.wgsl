// sdf-scene: name = "Road"
// sdf-scene: data = "D:/git/cd-data-extract/.work/fields/manifest.json"
// sdf-scene: layer = "road"
//
// Game-data scene: the road band recipe from the map pipeline's config.toml
// ([sdf] road = { low = 116, high = 136 }).

const PAPER: vec3f = vec3f(0.930, 0.900, 0.840);
const INK: vec3f = vec3f(0.320, 0.220, 0.120);

const LOW: f32 = 116.0;
const HIGH: f32 = 136.0;

fn render(p: vec2f, _time: f32) -> vec4f {
    let coverage = smoothstep(LOW, HIGH, field(p) * 255.0);
    return vec4f(mix(PAPER, INK, coverage), 1.0);
}
