// sdf-scene: name = "River"
// sdf-scene: data = "D:/git/cd-data-extract/.work/fields/manifest.json"
// sdf-scene: layer = "river"
//
// Game-data scene: the river trench recipe from the map pipeline's
// config.toml ([sdf] river = { trench = 108, floor = 32 }). The trench range
// separates the channel from shore by shape, not by band: near-shore shallow
// shares the range, so the opening is what draws the water.

const PAPER: vec3f = vec3f(0.930, 0.900, 0.840);
const INK: vec3f = vec3f(0.170, 0.270, 0.470);

const TRENCH: f32 = 108.0;
const FLOOR: f32 = 32.0;

fn render(p: vec2f, _time: f32) -> vec4f {
    let value = field(p) * 255.0;
    let coverage = select(0.0, 1.0, value >= FLOOR && value < TRENCH);
    return vec4f(mix(PAPER, INK, coverage), 1.0);
}
