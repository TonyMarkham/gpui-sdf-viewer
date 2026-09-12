// sdf-scene: name = "Mountain"
// sdf-scene: data = "config:sdf/manifest.json"
// sdf-scene: layer = "mountain"
//
// Game-data scene: the two-level mountain hatch from the map pipeline's
// config.toml ([sdf] mountain = { core = { low = 120, high = 130 },
// halo = { low = 96, high = 118 }, core_weight = 0.32, halo_weight = 0.14 }).

const PAPER: vec3f = vec3f(0.930, 0.900, 0.840);
const INK: vec3f = vec3f(0.200, 0.160, 0.130);

const CORE_LOW: f32 = 120.0;
const CORE_HIGH: f32 = 130.0;
const CORE_WEIGHT: f32 = 0.32;
const HALO_LOW: f32 = 96.0;
const HALO_HIGH: f32 = 118.0;
const HALO_WEIGHT: f32 = 0.14;

fn render(p: vec2f, _time: f32) -> vec4f {
    let value = field(p) * 255.0;
    let coverage = clamp(
        smoothstep(CORE_LOW, CORE_HIGH, value) * CORE_WEIGHT
            + smoothstep(HALO_LOW, HALO_HIGH, value) * HALO_WEIGHT,
        0.0,
        1.0,
    );
    return vec4f(mix(PAPER, INK, coverage), 1.0);
}
