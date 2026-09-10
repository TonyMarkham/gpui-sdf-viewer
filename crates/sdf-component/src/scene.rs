use crate::error::{Error, Result};
use soul_attr::soul;
use std::path::Path;

/// A parsed SDF scene: user-authored WGSL plus metadata from `// sdf-scene:`
/// header directives.
///
/// # Scene contract
///
/// An SDF file is a WGSL fragment that is spliced into a shader module
/// together with a fixed prelude of helpers and a fixed rasterizer kernel.
///
/// Header directives (all optional, one per line):
///
/// ```text
/// // sdf-scene: name = "Metaballs"
/// // sdf-scene: animated = true
/// ```
///
/// `name` controls the label shown in the UI; `animated` selects whether the
/// scene is re-rendered every frame with a running `time` value.
///
/// The scene body must define:
///
/// ```wgsl
/// fn scene(p: vec2f, time: f32) -> f32
/// ```
///
/// ...unless it takes over shading entirely (e.g. a 3D raymarcher) with:
///
/// ```wgsl
/// fn render(p: vec2f, time: f32) -> vec4f
/// ```
///
/// When `render` is present it is called per pixel with the same `p`
/// convention and must return a color with straight (non-premultiplied)
/// alpha; the returned color is premultiplied by the component before
/// presentation.
///
/// The prelude reserves the names `u`, `sd_*`, `op_*` and `rot` for its
/// helpers; scenes must not define them.
#[derive(Clone, Debug)]
pub struct SdfScene {
    name: String,
    animated: bool,
    render_override: bool,
    source: String,
}

impl SdfScene {
    /// Parses a scene from raw WGSL source. `fallback_name` is used when the
    /// header does not carry a `name` directive.
    #[soul(id = "concept.sdf-scene-contract", step = "parse + validate")]
    pub fn from_str(source: &str, fallback_name: &str) -> Result<Self> {
        let mut name: Option<String> = None;
        let mut animated = false;
        let mut body = String::new();

        for line in source.lines() {
            let trimmed = line.trim();
            match trimmed.strip_prefix(HEADER_DIRECTIVE) {
                Some(directive) => {
                    let Some((key, value)) = directive.split_once('=') else {
                        continue;
                    };
                    let key = key.trim();
                    let value = value.trim().trim_matches('"');
                    match key {
                        "name" => name = Some(String::from(value)),
                        "animated" => animated = value.eq_ignore_ascii_case("true"),
                        _ => {}
                    }
                }
                None => {
                    body.push_str(line);
                    body.push('\n');
                }
            }
        }

        let render_override = body.contains(RENDER_ENTRY);
        if !body.contains(SCENE_ENTRY) && !render_override {
            return Err(Error::scene_invalid(
                "the scene must define `fn scene(p: vec2f, time: f32) -> f32` (or a `fn render(p: vec2f, time: f32) -> vec4f` override)",
            ));
        }

        Ok(Self {
            name: name.unwrap_or_else(|| String::from(fallback_name)),
            animated,
            render_override,
            source: body,
        })
    }

    /// Reads and parses a scene from a file on disk.
    pub fn from_file(path: &Path) -> Result<Self> {
        let source = std::fs::read_to_string(path).map_err(Error::scene_file)?;
        let fallback_name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map_or_else(|| String::from("scene"), String::from);
        Self::from_str(&source, &fallback_name)
    }

    /// The scenes compiled into the component: guaranteed-valid examples that
    /// double as the reference for the file contract.
    pub fn embedded_examples() -> Vec<SdfScene> {
        [
            ("metaballs", include_str!("../scenes/metaballs.wgsl")),
            ("rounded-grid", include_str!("../scenes/rounded-grid.wgsl")),
            (
                "torus-raymarch",
                include_str!("../scenes/torus-raymarch.wgsl"),
            ),
        ]
        .into_iter()
        .filter_map(
            |(fallback_name, source)| match Self::from_str(source, fallback_name) {
                Ok(scene) => Some(scene),
                Err(error) => {
                    eprintln!("sdf-component: invalid embedded scene: {error}");
                    None
                }
            },
        )
        .collect()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    pub fn animated(&self) -> bool {
        self.animated
    }

    /// Returns the full WGSL shader module for this scene: prelude, user body,
    /// and the rasterizer entry points (with either the built-in 2D shading or
    /// the scene's `render` override).
    pub(crate) fn module_source(&self) -> String {
        let main = if self.render_override {
            MAIN_RENDER_OVERRIDE
        } else {
            MAIN_DEFAULT
        };
        format!("{PRELUDE}\n{}\n{main}", self.source)
    }
}

const HEADER_DIRECTIVE: &str = "// sdf-scene:";
const SCENE_ENTRY: &str = "fn scene(";
const RENDER_ENTRY: &str = "fn render(";

const PRELUDE: &str = r#"// sdf-component prelude — reserved names: u, sd_*, op_*, rot.
struct Uniforms {
    resolution: vec2f,
    time: f32,
    aspect: f32,
    mouse: vec2f,
    params: vec4f,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

fn sd_circle(p: vec2f, r: f32) -> f32 {
    return length(p) - r;
}

fn sd_box(p: vec2f, b: vec2f) -> f32 {
    let d = abs(p) - b;
    return length(max(d, vec2f(0.0))) + min(max(d.x, d.y), 0.0);
}

fn sd_round_box(p: vec2f, b: vec2f, r: f32) -> f32 {
    let q = abs(p) - b + vec2f(r);
    return length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

fn sd_segment(p: vec2f, a: vec2f, b: vec2f) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
}

fn sd_hexagon(p: vec2f, r: f32) -> f32 {
    let k = vec2f(-0.866025404, 0.5);
    let a = abs(p);
    let b = a - 2.0 * min(dot(k, a), 0.0) * k;
    let c = b - vec2f(clamp(b.x, -k.y * r, k.y * r), r);
    return length(c) * sign(c.y);
}

fn rot(p: vec2f, a: f32) -> vec2f {
    let c = cos(a);
    let s = sin(a);
    return vec2f(c * p.x + s * p.y, s * p.x - c * p.y);
}

fn op_union(a: f32, b: f32) -> f32 {
    return min(a, b);
}

fn op_subtract(a: f32, b: f32) -> f32 {
    return max(a, -b);
}

fn op_intersect(a: f32, b: f32) -> f32 {
    return max(a, b);
}

fn op_smooth_union(a: f32, b: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return mix(b, a, h) - k * h * (1.0 - h);
}

fn op_smooth_subtract(a: f32, b: f32, k: f32) -> f32 {
    let h = clamp(0.5 - 0.5 * (b + a) / k, 0.0, 1.0);
    return mix(b, -a, h) + k * h * (1.0 - h);
}

fn op_smooth_intersect(a: f32, b: f32, k: f32) -> f32 {
    let h = clamp(0.5 - 0.5 * (b - a) / k, 0.0, 1.0);
    return mix(b, a, h) + k * h * (1.0 - h);
}
"#;

const MAIN_DEFAULT: &str = r#"struct Varyings {
    @builtin(position) position: vec4f,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> Varyings {
    let corners = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
    var out: Varyings;
    out.position = vec4f(corners[index], 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4f {
    let uv = canvas_uv(in.position.xy);
    let color = scene_shaded(uv);
    return vec4f(color.rgb * color.a, color.a);
}

fn canvas_uv(pixel: vec2f) -> vec2f {
    return vec2f(
        (pixel.x / u.resolution.x * 2.0 - 1.0) * u.aspect,
        1.0 - pixel.y / u.resolution.y * 2.0,
    );
}

fn scene_shaded(uv: vec2f) -> vec4f {
    let d = scene(uv, u.time);
    let aa = 1.5 * 2.0 / u.resolution.y;

    let nx = scene(uv + vec2f(aa, 0.0), u.time) - scene(uv - vec2f(aa, 0.0), u.time);
    let ny = scene(uv + vec2f(0.0, aa), u.time) - scene(uv - vec2f(0.0, aa), u.time);
    let normal = select(vec2f(0.0, 1.0), normalize(vec2f(nx, ny)), length(vec2f(nx, ny)) > 0.0001);

    let light = normalize(vec2f(-0.55, 0.85));
    let lambert = clamp(dot(normal, light) * 0.5 + 0.5, 0.0, 1.0);
    let base = vec3f(0.145, 0.388, 0.922);
    let highlight = vec3f(0.400, 0.620, 1.000);
    var fill = mix(base * (0.5 + 0.5 * lambert), highlight, pow(lambert, 4.0) * 0.5);

    let edge = clamp(1.0 - smoothstep(-aa, aa, abs(d)), 0.0, 1.0);
    fill = mix(fill, fill * 0.45, edge);

    let alpha = 1.0 - smoothstep(-aa, aa, d);
    return vec4f(fill, alpha);
}
"#;

const MAIN_RENDER_OVERRIDE: &str = r#"struct Varyings {
    @builtin(position) position: vec4f,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> Varyings {
    let corners = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
    var out: Varyings;
    out.position = vec4f(corners[index], 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4f {
    let pixel = in.position.xy;
    let uv = vec2f(
        (pixel.x / u.resolution.x * 2.0 - 1.0) * u.aspect,
        1.0 - pixel.y / u.resolution.y * 2.0,
    );
    let color = render(uv, u.time);
    return vec4f(color.rgb * color.a, color.a);
}
"#;
