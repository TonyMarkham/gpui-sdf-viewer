// sdf-scene: name = "Inspect"
// sdf-scene: data = "config:sdf/manifest.json"
// sdf-scene: layer = "coast"
//
// Debug scene: nearest-neighbor raw bytes at the mip level chosen by
// u.params.x (the mip level control). Shows stub placement, tile seams, and
// the full chain the manifest declares — the LOD ground-truth tool: if the
// game's own mips stay recipe-compatible (byte distributions unchanged per
// level), thresholds hold at every zoom; if the game re-encoded the chain,
// the drift is visible here first. Copy this file and change `layer` (e.g.
// to "blur_height") to inspect any other exported field the same way.

fn render(p: vec2f, _time: f32) -> vec4f {
    let raw = field_raw(p, u.params.x);
    return vec4f(vec3f(raw), 1.0);
}
