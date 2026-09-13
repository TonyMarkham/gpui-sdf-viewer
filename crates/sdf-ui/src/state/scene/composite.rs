use super::load_rewritten;
use crate::{
    UiError, UiResult,
    constants::NAVIGATION_COMPOSITE_HEADER,
    state::scene_data::{Resolution, resolve_scene_source},
};
use sdf_component::SdfScene;
use sdf_offline::config::{BandKind, CompositeLayer};
use soul_attributes::soul;

// ---------------------------------------------------------------------------------------------- //

const COMPOSITE_STEM: &str = "composite";

/// The shipped PAPER color every shipped scene shades its coverage over.
const PAPER: &str = "const PAPER: vec3f = vec3f(0.930, 0.900, 0.840);";

/// The synthetic composite view: the config's layer stack — each layer an
/// export route with its own styled bands — painted bottom → top in one
/// generated scene. `None` for an empty stack — the composite entry is
/// hidden entirely. The styling comes from the config alone; nothing here
/// mirrors any scene file.
#[derive(Clone, Debug)]
pub(crate) struct CompositeSpec {
    layers: Vec<CompositeLayer>,
}

impl CompositeSpec {
    /// The spec over the config's layer stack; `None` when empty.
    pub(crate) fn from_layers(layers: Vec<CompositeLayer>) -> Option<Self> {
        if layers.is_empty() {
            None
        } else {
            Some(Self { layers })
        }
    }

    /// The composite scene source: header (`name`, `data` through the
    /// `config:` scheme, `layers` in config order) plus a generated `render`
    /// body painting the layers' bands bottom-to-top — each band mixes its
    /// config ink over the previous color, at its config weight.
    pub(crate) fn scene_source(&self) -> String {
        let helper = |index: usize| {
            if self.layers.len() == 1 {
                // The one-layer composite degrades to the styled single
                // field: the base helper is what samples field 0.
                String::from("field")
            } else {
                format!("field_{index}")
            }
        };
        let mut body =
            String::from("fn render(p: vec2f, _time: f32) -> vec4f {\n    var color = PAPER;\n");
        for (index, layer) in self.layers.iter().enumerate() {
            let field = helper(index);
            for band in &layer.bands {
                let coverage = match band.kind {
                    BandKind::Ramp => format!(
                        "smoothstep({}, {}, value)",
                        format_byte(band.low),
                        format_byte(band.high)
                    ),
                    BandKind::Band => format!(
                        "select(0.0, 1.0, value >= {} && value < {})",
                        format_byte(band.low),
                        format_byte(band.high)
                    ),
                };
                body.push_str(&format!(
                    "    {{\n        let value = {field}(p) * 255.0;\n        let coverage = {coverage};\n        color = mix(color, vec3f({}, {}, {}), coverage * {});\n    }}\n",
                    format_ink(band.ink[0]),
                    format_ink(band.ink[1]),
                    format_ink(band.ink[2]),
                    format_ink(band.weight)
                ));
            }
        }
        body.push_str("    return vec4f(color, 1.0);\n}\n");
        format!(
            "// sdf-scene: name = \"{NAVIGATION_COMPOSITE_HEADER}\"\n\
             // sdf-scene: data = \"config:sdf/manifest.json\"\n\
             // sdf-scene: layers = \"{}\"\n\
             \n{PAPER}\n\n{body}",
            self.layers
                .iter()
                .map(|layer| layer.name.as_str())
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    /// Stages the composite through the host's `config:` rewrite path: the
    /// source's `config:` value is rewritten to the resolved absolute path
    /// and the rewritten copy is parsed through `SdfScene::from_file` on a
    /// uniquely named temp file — the same path `load_rewritten` uses.
    #[soul(id = "concept.sdf-scene-contract", step = "composite staging")]
    pub(crate) fn stage(&self, data_dir: &std::path::Path) -> UiResult<SdfScene> {
        let source = self.scene_source();
        match resolve_scene_source(&source, data_dir).map_err(UiError::scene)? {
            Resolution::Rewritten { source } => load_rewritten(&source, COMPOSITE_STEM),
            Resolution::Unchanged => Err(UiError::scene(sdf_component::SdfError::scene_invalid(
                "the composite source carries no `config:` data value to resolve",
            ))),
        }
    }
}

// ---------------------------------------------------------------------------------------------- //

/// Formats a band edge as a WGSL float literal (bytes).
fn format_byte(value: f32) -> String {
    format_wgsl_float(value)
}

/// Formats one ink component (or weight) as a WGSL float literal.
fn format_ink(value: f32) -> String {
    format_wgsl_float(value)
}

/// WGSL float literals must carry a decimal point; `{:?}` gives `122.0`,
/// `0.29`.
fn format_wgsl_float(value: f32) -> String {
    format!("{value:?}")
}
