mod data_request;

use self::data_request::DataRequest;
use crate::{SceneData, SdfError, SdfResult};

use soul_attributes::soul;
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
/// // sdf-scene: data = "manifest.json"
/// // sdf-scene: layer = "coast"
/// ```
///
/// `name` controls the label shown in the UI; `animated` selects whether the
/// scene is re-rendered every frame with a running `time` value.
///
/// `data` and `layer` turn the scene into a data scene: `data` names a VS-20
/// export manifest, resolved relative to the scene file, and `layer` selects
/// one self-contained entry from the manifest's `layers` array by `name`
/// (a hard error when absent). The manifest is resolved and structurally
/// validated at load, and the entry's tile payloads are verified against
/// their declared sha256 and uploaded into the field texture when it is
/// first built, at first render; see [`SdfScene::from_file`] and
/// [`SdfCanvasState`](crate::SdfCanvasState). Scenes created through
/// [`SdfScene::from_str`] cannot resolve a manifest (there is no file to
/// resolve against), so the renderer rejects them; data scenes are loaded
/// from disk through [`SdfScene::from_file`].
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
/// The prelude reserves the names `u`, `sd_*`, `op_*`, `rot`, `field`,
/// `field_lod`, `field_raw`, `field_coord`, `field_uv`, `FieldCoord`,
/// `t_field`, `s_field`, `s_field_nearest`, and `FIELD_*` for its helpers;
/// scenes must not define them.
#[derive(Clone, Debug)]
pub struct SdfScene {
    name: String,
    animated: bool,
    render_override: bool,
    data_request: Option<DataRequest>,
    data: Option<SceneData>,
    source: String,
}

impl SdfScene {
    /// Parses a scene from raw WGSL source. `fallback_name` is used when the
    /// header does not carry a `name` directive.
    #[soul(id = "concept.sdf-scene-contract", step = "parse + validate")]
    pub fn from_str(source: &str, fallback_name: &str) -> SdfResult<Self> {
        let mut name: Option<String> = None;
        let mut animated = false;
        let mut manifest: Option<String> = None;
        let mut layer: Option<String> = None;
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
                        "data" => manifest = Some(String::from(value)),
                        "layer" => layer = Some(String::from(value)),
                        _ => {}
                    }
                }
                None => {
                    body.push_str(line);
                    body.push('\n');
                }
            }
        }

        let data_request = match (manifest, layer) {
            (Some(manifest), Some(layer)) => Some(DataRequest { manifest, layer }),
            (None, None) => None,
            _ => {
                return Err(SdfError::scene_invalid(
                    "the `data` and `layer` directives are a pair; a scene must carry both or neither",
                ));
            }
        };

        let render_override = body.contains(RENDER_ENTRY);
        if !body.contains(SCENE_ENTRY) && !render_override {
            return Err(SdfError::scene_invalid(
                "the scene must define `fn scene(p: vec2f, time: f32) -> f32` (or a `fn render(p: vec2f, time: f32) -> vec4f` override)",
            ));
        }

        Ok(Self {
            name: name.unwrap_or_else(|| String::from(fallback_name)),
            animated,
            render_override,
            data_request,
            data: None,
            source: body,
        })
    }

    /// Reads and parses a scene from a file on disk. A data scene's manifest
    /// resolves relative to the scene file's directory and is loaded here, so
    /// manifest failures surface through the scene-load error path; payload
    /// verification and upload happen later, when the field texture is first
    /// built at render time.
    pub fn from_file(path: &Path) -> SdfResult<Self> {
        let source = std::fs::read_to_string(path).map_err(SdfError::scene_file)?;
        let fallback_name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map_or_else(|| String::from("scene"), String::from);
        let mut scene = Self::from_str(&source, &fallback_name)?;

        if let Some(request) = &scene.data_request {
            let manifest_path = path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(&request.manifest);
            scene.data = Some(SceneData::load(&manifest_path, &request.layer)?);
        }

        Ok(scene)
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

    /// The scene's resolved data, if it is a data scene.
    pub(crate) fn data(&self) -> Option<&SceneData> {
        self.data.as_ref()
    }

    /// Whether the header carries the `data` + `layer` directives. Scenes
    /// parsed through [`SdfScene::from_str`] keep the directives unresolved;
    /// only [`SdfScene::from_file`] can resolve them, so a requested-but-
    /// unresolved scene must not render with the inert no-data binding.
    pub fn data_requested(&self) -> bool {
        self.data_request.is_some()
    }

    /// Identity of the scene's data binding: the manifest and layer the field
    /// texture was built from, or `None` for the inert no-data binding.
    pub(crate) fn data_key(&self) -> Option<String> {
        self.data
            .as_ref()
            .map(|data| format!("{}#{}", data.manifest.display(), data.layer))
    }

    /// The full WGSL shader module for this scene: prelude (with the field
    /// geometry constants the data declares, or the inert 1×1 shape), user
    /// body, and the rasterizer entry points (with either the built-in 2D
    /// shading or the scene's `render` override).
    pub(crate) fn module_source(&self) -> String {
        let main = if self.render_override {
            MAIN_RENDER_OVERRIDE
        } else {
            MAIN_DEFAULT
        };
        let (grid, levels) = self
            .data
            .as_ref()
            .map_or((1, 1), |data| (data.grid, data.level_count()));
        format!(
            "{UNIFORM_BINDINGS}\n{}\n{SDF_HELPERS}\n{}\n{main}",
            field_helpers(grid, levels),
            self.source
        )
    }
}

const HEADER_DIRECTIVE: &str = "// sdf-scene:";
const SCENE_ENTRY: &str = "fn scene(";
const RENDER_ENTRY: &str = "fn render(";

/// The uniform block and every field binding. One bind group shape serves
/// both scene classes: data scenes bind the field texture array, scenes
/// without data bind a 1×1 zero-filled stand-in so the field helpers stay
/// inert.
pub(crate) const UNIFORM_BINDINGS: &str = r#"struct Uniforms {
    resolution: vec2f,
    time: f32,
    aspect: f32,
    mouse: vec2f,
    params: vec4f,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var t_field: texture_2d_array<f32>;
@group(0) @binding(2) var s_field: sampler;
@group(0) @binding(3) var s_field_nearest: sampler;
"#;

/// The field sampling helpers, specialized for one layer geometry: `grid` is
/// the tile-grid edge (1 for the inert no-data binding) and `levels` the mip
/// chain length.
///
/// `field_uv` maps canvas coordinates onto the field; `FIELD_V_FLIP` selects
/// the vertical orientation. Calibrated against the map pipeline's crops:
/// the map repo's crop writer slices its rows from field row 0 upward
/// (crop top-left = field coords (6016, 3072) for Hernand bay), and this
/// prelude with `FIELD_V_FLIP = 1.0` puts field row 0 at the canvas top —
/// both images orient field rows top-down, so the GUI presents the crops
/// unmirrored. Mirrored output would flip this one constant, here and
/// nowhere else.
pub(crate) fn field_helpers(grid: u32, levels: u32) -> String {
    let header =
        format!("const FIELD_TILES: u32 = {grid}u;\nconst FIELD_LEVELS: u32 = {levels}u;\n");
    let mut source = header;
    source.push_str(FIELD_HELPERS);
    source
}

const FIELD_HELPERS: &str = r#"const FIELD_V_FLIP: f32 = 1.0;

struct FieldCoord {
    uv: vec2f,
    layer: u32,
};

fn field_uv(p: vec2f) -> vec2f {
    let x = (p.x / u.aspect + 1.0) * 0.5;
    let up = (p.y + 1.0) * 0.5;
    let y = mix(up, 1.0 - up, FIELD_V_FLIP);
    return clamp(vec2f(x, y), vec2f(0.0), vec2f(1.0 - 1e-6));
}

fn field_coord(p: vec2f) -> FieldCoord {
    let grid_uv = field_uv(p) * f32(FIELD_TILES);
    let tile = floor(grid_uv);
    return FieldCoord(grid_uv - tile, u32(tile.y * f32(FIELD_TILES) + tile.x));
}

fn field_lod(p: vec2f, level: f32) -> f32 {
    let lod = clamp(level, 0.0, f32(FIELD_LEVELS - 1u));
    let coord = field_coord(p);
    return textureSampleLevel(t_field, s_field, coord.uv, coord.layer, lod).r;
}

fn field_raw(p: vec2f, level: f32) -> f32 {
    let lod = clamp(level, 0.0, f32(FIELD_LEVELS - 1u));
    let coord = field_coord(p);
    return textureSampleLevel(t_field, s_field_nearest, coord.uv, coord.layer, lod).r;
}

fn field(p: vec2f) -> f32 {
    return field_lod(p, u.params.x);
}
"#;

const SDF_HELPERS: &str = r#"fn sd_circle(p: vec2f, r: f32) -> f32 {
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
