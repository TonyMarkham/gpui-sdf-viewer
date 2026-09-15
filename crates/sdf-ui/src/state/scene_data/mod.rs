mod scheme;

use self::scheme::{Scheme, scheme};
use sdf_component::SdfError;
use soul_attributes::soul;

// ---------------------------------------------------------------------------------------------- //

use std::path::Path;

// ---------------------------------------------------------------------------------------------- //

const HEADER_DIRECTIVE: &str = "// sdf-scene:";
const DATA_KEY: &str = "data";
const CONFIG_SCHEME: &str = "config";

// ---------------------------------------------------------------------------------------------- //

/// The outcome of resolving a scene source against the app config: either the
/// source carries no `config:` values and is loaded as-is, or it was
/// rewritten and must be parsed from the rewritten copy.
pub(crate) enum Resolution {
    Unchanged,
    Rewritten { source: String },
}

/// Rewrites `data = "config:<remainder>"` (or unquoted) header directives to
/// the resolved absolute path under the app's data directory — the directory
/// the extraction pipeline writes to (`<data_dir>/<remainder>`; default
/// `<config dir>/data`). Everything else passes through untouched; an
/// unknown scheme is an error naming the value. Windows drive paths
/// (`D:/…`) are not schemes.
#[soul(id = "concept.game-data-pipeline", step = "config: scheme rewrite")]
pub(crate) fn resolve_scene_source(source: &str, data_dir: &Path) -> Result<Resolution, SdfError> {
    let mut rewritten = false;
    let mut lines = Vec::new();

    for line in source.lines() {
        let mut resolved_line = String::from(line);
        if let Some(directive) = line.trim().strip_prefix(HEADER_DIRECTIVE)
            && let Some((key, value)) = directive.split_once('=')
            && key.trim() == DATA_KEY
        {
            let value = value.trim().trim_matches('"');
            match scheme(value) {
                Scheme::Plain => {}
                Scheme::Drive => {}
                Scheme::Config(remainder) => {
                    let target = data_dir.join(&remainder);
                    let target = target.to_string_lossy().replace('\\', "/");
                    let quoted = format!("\"{value}\"");
                    resolved_line = if line.contains(&quoted) {
                        line.replace(&quoted, &format!("\"{target}\""))
                    } else {
                        line.replace(value, &target)
                    };
                    rewritten = true;
                }
                Scheme::Unknown(scheme) => {
                    return Err(SdfError::scene_invalid(&format!(
                        "unknown data value `{value}`: scheme `{scheme}:` is not supported; only `{CONFIG_SCHEME}:` is"
                    )));
                }
            }
        }
        lines.push(resolved_line);
    }

    if rewritten {
        Ok(Resolution::Rewritten {
            source: lines.join("\n"),
        })
    } else {
        Ok(Resolution::Unchanged)
    }
}

// ---------------------------------------------------------------------------------------------- //

#[cfg(test)]
mod tests {
    use super::{Resolution, resolve_scene_source};

    use sdf_component::SdfError;
    use std::path::Path;

    // ------------------------------------------------------------------------------------------ //

    const TEST_DATA_DIR: &str = "C:/Users/tony/.config/cd-map-offline/data";
    const TEST_BODY: &str = "\nconst PAPER: vec3f = vec3f(1.0, 1.0, 1.0);\nfn render(p: vec2f, time: f32) -> vec4f {\n    return vec4f(field(p), 0.0, 0.0, 1.0);\n}\n";

    // ------------------------------------------------------------------------------------------ //

    #[test]
    fn config_values_rewrite_to_the_config_directory() {
        let source = format!(
            "// sdf-scene: name = \"Coast\"\n// sdf-scene: data = \"config:sdf/manifest.json\"\n// sdf-scene: layer = \"coast\"{TEST_BODY}"
        );
        let resolved = expect_resolved(&source);
        let Resolution::Rewritten { source } = resolved else {
            unreachable!("the config value must rewrite");
        };

        assert!(
            source.contains(&format!("data = \"{TEST_DATA_DIR}/sdf/manifest.json\"")),
            "the config value must resolve under the data directory, got: {source}"
        );
        assert!(
            source.contains("layer = \"coast\"") && source.contains(TEST_BODY.trim_end()),
            "the rest of the scene must pass through untouched"
        );
    }

    #[test]
    fn unquoted_config_values_rewrite_to_the_config_directory() {
        let source = format!(
            "// sdf-scene: name = \"Coast\"\n// sdf-scene: data = config:sdf/manifest.json\n// sdf-scene: layer = \"coast\"{TEST_BODY}"
        );
        let resolved = expect_resolved(&source);
        let Resolution::Rewritten { source } = resolved else {
            unreachable!("the unquoted config value must rewrite");
        };

        assert!(
            source.contains(&format!("data = {TEST_DATA_DIR}/sdf/manifest.json")),
            "an unquoted config value must rewrite to the resolved absolute \
             path, got: {source}"
        );
        assert!(
            !source.contains("config:sdf/manifest.json"),
            "no `config:` prefix may survive the rewrite, got: {source}"
        );
        assert!(
            source.contains("layer = \"coast\"") && source.contains(TEST_BODY.trim_end()),
            "the rest of the scene must pass through untouched"
        );
    }

    #[test]
    fn plain_paths_pass_through_unchanged() {
        let source = "// sdf-scene: data = \"manifest.json\"\n// sdf-scene: layer = \"coast\"\nfn scene(p: vec2f, time: f32) -> f32 { return 0.0; }\n";
        assert!(matches!(resolve(source), Ok(Resolution::Unchanged)));
    }

    #[test]
    fn windows_drive_paths_are_not_schemes() {
        let source = "// sdf-scene: data = \"D:/exports/manifest.json\"\n// sdf-scene: layer = \"coast\"\nfn scene(p: vec2f, time: f32) -> f32 { return 0.0; }\n";
        assert!(matches!(resolve(source), Ok(Resolution::Unchanged)));
    }

    #[test]
    fn unknown_schemes_are_a_parse_error_naming_the_value() {
        let source = "// sdf-scene: data = \"http://example.com/manifest.json\"\n// sdf-scene: layer = \"coast\"\nfn scene(p: vec2f, time: f32) -> f32 { return 0.0; }\n";
        let failure = match resolve(source) {
            Err(error) => error.to_string(),
            Ok(_) => String::new(),
        };
        assert!(
            failure.contains("http://example.com/manifest.json") && failure.contains("http:"),
            "the unknown scheme must be a parse error naming the value, got: {failure}"
        );
    }

    #[test]
    fn non_data_directives_are_untouched() {
        let source = "// sdf-scene: name = \"River\"\n// sdf-scene: animated = true\nfn scene(p: vec2f, time: f32) -> f32 { return 0.0; }\n";
        assert!(matches!(resolve(source), Ok(Resolution::Unchanged)));
    }

    // ------------------------------------------------------------------------------------------ //

    fn resolve(source: &str) -> Result<Resolution, SdfError> {
        resolve_scene_source(source, Path::new(TEST_DATA_DIR))
    }

    fn expect_resolved(source: &str) -> Resolution {
        match resolve(source) {
            Ok(resolution) => resolution,
            Err(error) => unreachable!("unexpected resolution failure: {error}"),
        }
    }
}
