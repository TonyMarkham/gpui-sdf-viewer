use crate::scene::{UNIFORM_BINDINGS, field_helpers};
use soul_attributes::soul;

/// The component theme's contour-overlay ink: presentation, so it lives here
/// and not in any scene.
const OVERLAY_KERNEL: &str = r#"struct Varyings {
    @builtin(position) position: vec4f,
};

const INK: vec3f = vec3f(0.05, 0.07, 0.10);

fn canvas_uv(pixel: vec2f) -> vec2f {
    return vec2f(
        (pixel.x / u.resolution.x * 2.0 - 1.0) * u.aspect,
        1.0 - pixel.y / u.resolution.y * 2.0,
    );
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> Varyings {
    let corners = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
    var out: Varyings;
    out.position = vec4f(corners[index], 0.0, 1.0);
    return out;
}

@fragment
fn fs_overlay(in: Varyings) -> @location(0) vec4f {
    let uv = canvas_uv(in.position.xy);
    let v = field_lod(uv, u.params.x) * 255.0;
    let scaled = v / max(u.params.y, 1e-6);
    let band = fract(scaled);
    let aa = fwidth(scaled);
    // A flat quad (aa == 0) carries no band-crossing information, and
    // smoothstep with equal edges is undefined (0/0 for a band-aligned
    // constant); such pixels draw no line. Region boundaries still draw
    // through their varying edge quads, where aa > 0.
    var line = 0.0;
    if (aa > 0.0) {
        line = 1.0 - smoothstep(0.0, aa * 1.5, min(band, 1.0 - band));
    }
    return vec4f(INK * line, line);
}
"#;

/// The contour overlay's WGSL module: the shared uniform block and field
/// helpers for `grid` tiles and `levels` mip levels, plus the overlay's
/// kernel. A component-level pass, not a scene: it alpha-blends contour lines
/// over whatever the scene pass drew, sampling the same data texture.
#[soul(id = "interaction.sdf.render-frame", step = "contour overlay pass")]
pub(crate) fn module_source(grid: u32, levels: u32) -> String {
    format!(
        "{UNIFORM_BINDINGS}\n{}\n{OVERLAY_KERNEL}",
        field_helpers(grid, levels)
    )
}
