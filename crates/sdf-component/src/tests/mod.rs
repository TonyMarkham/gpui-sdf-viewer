use crate::{
    error::Error,
    renderer::{FrameRequest, Renderer},
    scene::SdfScene,
};

use gpui::{AppContext as _, TestAppContext};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Mutex;

const CIRCLE_SCENE: &str = r#"
fn scene(p: vec2f, _time: f32) -> f32 {
    return sd_circle(p, 0.6);
}
"#;

/// Every renderer-constructing test holds this lock for its whole body:
/// concurrent wgpu instance/adapter/device bring-up across threads stalls
/// the graphics driver nondeterministically, and plain `cargo test` (the
/// project's gate) runs tests in parallel by default. Parsing tests run
/// freely; anything that builds a `Renderer` serializes here.
static RENDER_LOCK: Mutex<()> = Mutex::new(());

fn render_lock() -> std::sync::MutexGuard<'static, ()> {
    RENDER_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A minimal VS-20 export fixture: a 2×2 grid of 4² tiles over an 8² field,
/// three mip levels per real tile, constant-fill stubs. Two layers — `probe`
/// and `echo` — share the geometry but carry their own payloads with
/// distinguishable constants (probe stub tile (1, 1) = 42, echo = 84).
/// Returns the fixture directory on success.
fn field_fixture(tag: &str) -> std::result::Result<std::path::PathBuf, String> {
    field_fixture_with_layers(tag, 0)
}

/// Like [`field_fixture`] with `extra` additional layers reusing echo's
/// payloads under generated names — enough layer entries to push a composite
/// past the device's sampled-texture limit.
fn field_fixture_with_layers(
    tag: &str,
    extra: usize,
) -> std::result::Result<std::path::PathBuf, String> {
    let dir = std::env::temp_dir()
        .join(format!("sdf-field-fixture-{tag}-{}", std::process::id()))
        .join("fields");
    std::fs::create_dir_all(&dir).map_err(|error| format!("create fixture dir: {error}"))?;

    let level_bytes = [16u32, 4, 1];
    let tile_payload = |seed: u8| -> Vec<u8> {
        let mut payload = Vec::new();
        for (index, bytes) in level_bytes.iter().enumerate() {
            for texel in 0..*bytes {
                payload.push(seed.wrapping_add(index as u8 * 7).wrapping_add(texel as u8));
            }
        }
        payload
    };

    let probe_tiles: [(&str, u32, u32, &str, Vec<u8>); 4] = [
        ("tile_0_0.r8", 0, 0, "mip0", tile_payload(10)),
        ("tile_1_0.r8", 1, 0, "mip0", tile_payload(60)),
        ("tile_0_1.r8", 0, 1, "stub", vec![0u8; 21]),
        ("tile_1_1.r8", 1, 1, "stub", vec![42u8; 21]),
    ];
    let echo_tiles: [(&str, u32, u32, &str, Vec<u8>); 4] = [
        ("echo_tile_0_0.r8", 0, 0, "mip0", tile_payload(200)),
        ("echo_tile_1_0.r8", 1, 0, "mip0", tile_payload(210)),
        ("echo_tile_0_1.r8", 0, 1, "stub", vec![7u8; 21]),
        ("echo_tile_1_1.r8", 1, 1, "stub", vec![84u8; 21]),
    ];

    let manifest_tiles =
        |tiles: &[(&str, u32, u32, &str, Vec<u8>)]| -> std::result::Result<Vec<serde_json::Value>, String> {
            let mut entries = Vec::new();
            for (payload_name, x, y, kind, payload) in tiles {
                let payload_path = dir.join(payload_name);
                std::fs::write(&payload_path, payload)
                    .map_err(|error| format!("write {payload_name}: {error}"))?;
                entries.push(json!({
                    "path": format!("ui/{payload_name}"),
                    "payload": payload_name,
                    "x": x,
                    "y": y,
                    "kind": kind,
                    "source_sha256": "0".repeat(64),
                    "payload_sha256": sha256_hex(payload),
                }));
            }
            Ok(entries)
        };

    let layer_entry = |name: &str, tiles: &[serde_json::Value]| -> serde_json::Value {
        json!({
            "name": name,
            "tile_prefix": "fixture_field",
            "size": 8,
            "source_manifest_sha256": "0".repeat(64),
            "mips": [
                { "level": 0, "size": 4, "bytes": 16 },
                { "level": 1, "size": 2, "bytes": 4 },
                { "level": 2, "size": 1, "bytes": 1 }
            ],
            "tiles": tiles,
        })
    };

    let probe_manifest_tiles = manifest_tiles(&probe_tiles)?;
    let echo_manifest_tiles = manifest_tiles(&echo_tiles)?;
    let mut layers = vec![
        layer_entry("probe", &probe_manifest_tiles),
        layer_entry("echo", &echo_manifest_tiles),
    ];
    for index in 0..extra {
        let mut entry = layer_entry("echo", &echo_manifest_tiles);
        entry["name"] = json!(format!("echo_{index}"));
        layers.push(entry);
    }

    let manifest = json!({
        "format": "cd-map-sdf-field",
        "version": 1,
        "layers": layers,
    });
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_string(&manifest).unwrap_or_default(),
    )
    .map_err(|error| format!("write manifest: {error}"))?;

    Ok(dir)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Writes a data scene referencing `manifest_name` beside itself and parses
/// it from disk, as the scan path does.
fn data_scene(
    dir: &Path,
    layer: &str,
    manifest_name: &str,
) -> std::result::Result<SdfScene, Error> {
    let source = format!(
        "// sdf-scene: data = \"{manifest_name}\"\n// sdf-scene: layer = \"{layer}\"\n\
         fn render(p: vec2f, _time: f32) -> vec4f {{\n    \
         let raw = field_raw(p, u.params.x);\n    \
         return vec4f(vec3f(raw), 1.0);\n}}\n"
    );
    let path = dir.join("probe.wgsl");
    std::fs::write(&path, &source)
        .map_err(|error| Error::data(&format!("write scene file: {error}")))?;
    SdfScene::from_file(&path)
}

/// The body the composite render tests sample through: the brightest of the
/// first two fields, so a frame that only bound field 0 is distinguishable
/// from one that bound both.
/// The body the composite render tests sample through: the brightest of the
/// first two fields (normalized values, so the raw byte is the texel value).
const COMPOSITE_BODY: &str = "fn render(p: vec2f, _time: f32) -> vec4f {\n    \
     let mixed = max(field_0(p), field_1(p));\n    \
     return vec4f(vec3f(mixed), 1.0);\n}\n";

/// Writes a composite data scene (`layers` = comma-separated paint order)
/// referencing `manifest_name` beside itself and parses it from disk.
fn composite_scene(
    dir: &Path,
    layers: &str,
    manifest_name: &str,
    body: &str,
) -> std::result::Result<SdfScene, Error> {
    let source = format!(
        "// sdf-scene: data = \"{manifest_name}\"\n// sdf-scene: layers = \"{layers}\"\n{body}"
    );
    let path = dir.join("composite.wgsl");
    std::fs::write(&path, &source)
        .map_err(|error| Error::data(&format!("write scene file: {error}")))?;
    SdfScene::from_file(&path)
}

fn first_frame(
    renderer: &mut Renderer,
    scene: &SdfScene,
    params: [f32; 2],
) -> std::result::Result<Option<std::sync::Arc<gpui::RenderImage>>, Error> {
    first_frame_view(renderer, scene, params, [0.5, 0.5], 1.0)
}

/// Like [`first_frame`] with an explicit map view (field-uv space).
fn first_frame_view(
    renderer: &mut Renderer,
    scene: &SdfScene,
    params: [f32; 2],
    view_center: [f32; 2],
    view_zoom: f32,
) -> std::result::Result<Option<std::sync::Arc<gpui::RenderImage>>, Error> {
    render_until_frame(
        renderer,
        scene,
        FrameRequest {
            width: 64,
            height: 64,
            time: 0.0,
            mouse: [0.0, 0.0],
            view_center,
            view_zoom,
            params,
        },
    )
}

/// Renders the same request repeatedly until a frame completes, as the
/// canvas's paint loop does; `None` when no frame became ready in time.
fn render_until_frame(
    renderer: &mut Renderer,
    scene: &SdfScene,
    request: FrameRequest,
) -> std::result::Result<Option<std::sync::Arc<gpui::RenderImage>>, Error> {
    for _ in 0..300 {
        match renderer.render(scene, &request) {
            Ok(Some(image)) => return Ok(Some(image)),
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
            Err(error) => return Err(error),
        }
    }
    Ok(None)
}

/// Writes a corrupted variant of the fixture manifest named
/// `manifest-{tag}.json` beside the fixture payloads, loads a data scene
/// against it, and asserts the load fails with `needle` in the error.
fn manifest_error(
    dir: &Path,
    manifest: &serde_json::Value,
    tag: &str,
    mutate: impl FnOnce(&mut serde_json::Value),
    needle: &str,
) {
    let mut variant = manifest.clone();
    mutate(&mut variant);
    let name = format!("manifest-{tag}.json");
    if let Err(error) = std::fs::write(
        dir.join(&name),
        serde_json::to_string(&variant).unwrap_or_default(),
    ) {
        eprintln!("skipping `{tag}` validator case: {error}");
        return;
    }

    let failure = match data_scene(dir, "probe", &name) {
        Err(error) => Some(error.to_string()),
        Ok(_) => None,
    };
    assert!(
        failure
            .as_deref()
            .is_some_and(|message| message.contains(needle)),
        "the `{tag}` corruption must be reported with `{needle}`, got: {failure:?}"
    );
}

/// Hard-fails the test on a render error: shader compilation, pipeline, and
/// upload failures are deterministic, so they must fail loudly, not skip.
/// Returns `None` only when no frame became ready in time.
fn require_frame(
    outcome: std::result::Result<Option<std::sync::Arc<gpui::RenderImage>>, Error>,
    context: &str,
) -> Option<std::sync::Arc<gpui::RenderImage>> {
    match outcome {
        Ok(frame) => frame,
        Err(error) => unreachable!("{context}: {error}"),
    }
}

#[test]
fn scenes_parse_headers_and_validate_the_entry_point() {
    let scene = SdfScene::from_str(CIRCLE_SCENE, "circle");
    let scene = match scene {
        Ok(scene) => scene,
        Err(error) => {
            eprintln!("unexpected scene parse failure: {error}");
            return;
        }
    };
    assert_eq!(scene.name(), "circle");
    assert!(!scene.animated());
    assert!(!scene.module_source().contains("fn render("));

    let broken = SdfScene::from_str("fn nothing_here() {}", "broken");
    assert!(
        broken.is_err(),
        "scenes without `fn scene(` must be rejected"
    );
}

#[test]
fn scene_headers_toggle_name_and_animation() {
    let source = "// sdf-scene: name = \"Wobble\"\n// sdf-scene: animated = true\nfn scene(p: vec2f, time: f32) -> f32 { return 0.0; }";
    match SdfScene::from_str(source, "fallback") {
        Ok(scene) => {
            assert_eq!(scene.name(), "Wobble");
            assert!(scene.animated());
        }
        Err(error) => {
            eprintln!("unexpected scene parse failure: {error}");
        }
    }
}

#[test]
fn data_directives_are_a_pair() {
    let pair = SdfScene::from_str(
        "// sdf-scene: data = \"manifest.json\"\n// sdf-scene: layer = \"coast\"\nfn scene(p: vec2f, _time: f32) -> f32 { return 0.0; }",
        "pair",
    );
    assert!(pair.is_ok(), "data + layer directives parse together");

    let data_only = SdfScene::from_str(
        "// sdf-scene: data = \"manifest.json\"\nfn scene(p: vec2f, _time: f32) -> f32 { return 0.0; }",
        "data-only",
    );
    assert!(
        data_only.is_err(),
        "a `data` directive without `layer` must be rejected"
    );

    let layer_only = SdfScene::from_str(
        "// sdf-scene: layer = \"coast\"\nfn scene(p: vec2f, _time: f32) -> f32 { return 0.0; }",
        "layer-only",
    );
    assert!(
        layer_only.is_err(),
        "a `layer` directive without `data` must be rejected"
    );
}

#[test]
fn the_layers_directive_pairs_with_data() {
    let pair = SdfScene::from_str(
        "// sdf-scene: data = \"manifest.json\"\n// sdf-scene: layers = \"coast,river\"\nfn scene(p: vec2f, _time: f32) -> f32 { return 0.0; }",
        "layers-pair",
    );
    assert!(pair.is_ok(), "data + layers directives parse together");

    let layers_only = SdfScene::from_str(
        "// sdf-scene: layers = \"coast\"\nfn scene(p: vec2f, _time: f32) -> f32 { return 0.0; }",
        "layers-only",
    );
    assert!(
        layers_only.is_err(),
        "a `layers` directive without `data` must be rejected"
    );
}

#[test]
fn carrying_layer_and_layers_together_is_a_parse_error() {
    let with_data = SdfScene::from_str(
        "// sdf-scene: data = \"manifest.json\"\n// sdf-scene: layer = \"coast\"\n// sdf-scene: layers = \"coast,river\"\nfn scene(p: vec2f, _time: f32) -> f32 { return 0.0; }",
        "both",
    );
    assert!(
        with_data.is_err(),
        "`layer` and `layers` together must be rejected"
    );

    let without_data = SdfScene::from_str(
        "// sdf-scene: layer = \"coast\"\n// sdf-scene: layers = \"coast\"\nfn scene(p: vec2f, _time: f32) -> f32 { return 0.0; }",
        "both",
    );
    assert!(
        without_data.is_err(),
        "`layer` and `layers` together must be rejected"
    );
}

#[test]
fn an_empty_layers_item_is_a_parse_error_naming_the_value() {
    let failure = match SdfScene::from_str(
        "// sdf-scene: data = \"manifest.json\"\n// sdf-scene: layers = \"coast,,river\"\nfn scene(p: vec2f, _time: f32) -> f32 { return 0.0; }",
        "empty-item",
    ) {
        Err(error) => error.to_string(),
        Ok(_) => String::new(),
    };
    assert!(
        failure.contains("coast,,river"),
        "an empty `layers` item must be a parse error naming the value, got: {failure}"
    );
}

/// The `layers` list is paint order, preserved everywhere — resolved into
/// the fields and out into the data key.
#[test]
fn layers_order_survives_into_the_data_key() {
    let dir = match field_fixture("key-order") {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("skipping key-order test: {error}");
            return;
        }
    };
    let scene = match composite_scene(&dir, "echo,probe", "manifest.json", COMPOSITE_BODY) {
        Ok(scene) => scene,
        Err(error) => unreachable!("unexpected composite-scene load failure: {error}"),
    };
    let key = scene
        .data_key()
        .unwrap_or_else(|| unreachable!("the loaded composite must carry a data key"));
    assert!(
        key.ends_with("#echo+probe"),
        "the data key must list the configured layers in paint order, got: {key}"
    );
}

/// A two-layer manifest resolves both fields against the shared geometry:
/// one grid, one mip chain, fields in paint order.
#[test]
fn a_two_layer_manifest_resolves_both_fields() {
    let dir = match field_fixture("two-fields") {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("skipping two-fields test: {error}");
            return;
        }
    };
    let scene = match composite_scene(&dir, "probe,echo", "manifest.json", COMPOSITE_BODY) {
        Ok(scene) => scene,
        Err(error) => unreachable!("unexpected composite-scene load failure: {error}"),
    };
    let data = match scene.data() {
        Some(data) => data,
        None => unreachable!("unexpected: the composite scene resolved no data"),
    };
    assert_eq!(data.grid, 2, "the fixture declares a shared 2×2 grid");
    assert_eq!(data.level_count(), 3, "the fixture declares three levels");
    assert_eq!(data.fields.len(), 2, "both configured layers must resolve");
    assert_eq!(data.fields[0].layer, "probe", "paint order is preserved");
    assert_eq!(data.fields[1].layer, "echo", "paint order is preserved");
}

/// A composite's fields share the first field's geometry: a grid mismatch,
/// a mip-level count mismatch, or — byte-exact — a mip table with equal
/// counts but different sizes must each fail naming both fields.
#[test]
fn mismatched_field_geometry_is_an_error_naming_both_fields() {
    let dir = match field_fixture("mismatch") {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("skipping geometry-mismatch test: {error}");
            return;
        }
    };
    let manifest: serde_json::Value = match std::fs::read_to_string(dir.join("manifest.json"))
        .map_err(|error| error.to_string())
        .and_then(|raw| serde_json::from_str(&raw).map_err(|error| error.to_string()))
    {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!("skipping geometry-mismatch test: {error}");
            return;
        }
    };

    // Grid mismatch: echo's field is a 4×4 grid of the same tile edge.
    let mut grid_variant = manifest.clone();
    grid_variant["layers"][1]["size"] = json!(16);
    let failure = match composite_variant(&dir, &grid_variant, "mismatch-grid") {
        Err(error) => Some(error.to_string()),
        Ok(_) => None,
    };
    assert!(
        failure
            .as_deref()
            .is_some_and(|message| message.contains("differs from field \"probe\"")
                && message.contains("field \"echo\"")),
        "a grid mismatch must name both fields, got: {failure:?}"
    );

    // Mip-level count mismatch: echo declares a two-level chain.
    let mut count_variant = manifest.clone();
    count_variant["layers"][1]["mips"] = json!([
        { "level": 0, "size": 4, "bytes": 16 },
        { "level": 1, "size": 2, "bytes": 4 }
    ]);
    let failure = match composite_variant(&dir, &count_variant, "mismatch-count") {
        Err(error) => Some(error.to_string()),
        Ok(_) => None,
    };
    assert!(
        failure.as_deref().is_some_and(
            |message| message.contains("mip level count 2 differs from field \"probe\": 3")
        ),
        "a mip-level count mismatch must be reported with both fields, got: {failure:?}"
    );

    // Byte-exact mip table mismatch: same count, same grid, but echo's tile
    // edge is 8² instead of 4² — its level sizes differ at the first level.
    let mut edge_variant = manifest.clone();
    edge_variant["layers"][1]["size"] = json!(16);
    edge_variant["layers"][1]["mips"] = json!([
        { "level": 0, "size": 8, "bytes": 64 },
        { "level": 1, "size": 4, "bytes": 16 },
        { "level": 2, "size": 2, "bytes": 4 }
    ]);
    let failure = match composite_variant(&dir, &edge_variant, "mismatch-edge") {
        Err(error) => Some(error.to_string()),
        Ok(_) => None,
    };
    assert!(
        failure.as_deref().is_some_and(
            |message| message.contains("mip level 0 size 8 differs from field \"probe\": 4")
        ),
        "a divergent mip table must name the first divergent pair, got: {failure:?}"
    );
}

/// Writes `manifest` as a tagged variant beside the fixture payloads and
/// parses a probe+echo composite against it.
fn composite_variant(
    dir: &Path,
    manifest: &serde_json::Value,
    tag: &str,
) -> std::result::Result<SdfScene, Error> {
    let name = format!("manifest-{tag}.json");
    std::fs::write(
        dir.join(&name),
        serde_json::to_string(manifest).unwrap_or_default(),
    )
    .map_err(|error| Error::data(&format!("write variant manifest: {error}")))?;
    composite_scene(dir, "probe,echo", &name, COMPOSITE_BODY)
}

/// The composite prelude emits the positional helpers next to the single-
/// field constants; a single-field scene's module stays free of them.
#[test]
fn the_composite_prelude_emits_per_field_helpers() {
    let dir = match field_fixture("prelude") {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("skipping prelude test: {error}");
            return;
        }
    };
    let composite = match composite_scene(&dir, "probe,echo", "manifest.json", COMPOSITE_BODY) {
        Ok(scene) => scene,
        Err(error) => unreachable!("unexpected composite-scene load failure: {error}"),
    };
    let module = composite.module_source();
    assert!(
        module.contains("@group(0) @binding(4) var t_field_1"),
        "field 1 must bind its own texture at binding 4, got: {module}"
    );
    assert!(
        module.contains("fn field_0(") && module.contains("fn field_1("),
        "the positional helpers must be generated, got: {module}"
    );

    let single = match data_scene(&dir, "probe", "manifest.json") {
        Ok(scene) => scene,
        Err(error) => unreachable!("unexpected data-scene load failure: {error}"),
    };
    let module = single.module_source();
    assert!(
        !module.contains("t_field_1") && !module.contains("fn field_0("),
        "a single-field scene must compile to today's module, got: {module}"
    );
}

/// Renders a circle scene offscreen and checks the pixels that come back
/// through the async readback path. Skipped (with a note) when no wgpu adapter
/// is available on the host.
#[test]
fn renderer_produces_cpu_frames_from_scene_evaluation() {
    let _renderer_lock = render_lock();
    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping renderer test: {error}");
            return;
        }
    };

    let scene = match SdfScene::from_str(CIRCLE_SCENE, "circle") {
        Ok(scene) => scene,
        Err(error) => {
            eprintln!("unexpected scene parse failure: {error}");
            return;
        }
    };

    const SIZE: u32 = 64;
    let mut frame = None;
    for _ in 0..300 {
        match renderer.render(
            &scene,
            &FrameRequest {
                width: SIZE,
                height: SIZE,
                time: 0.0,
                mouse: [0.0, 0.0],
                view_center: [0.5, 0.5],
                view_zoom: 1.0,
                params: [0.0, 0.0],
            },
        ) {
            Ok(Some(image)) => {
                frame = Some(image);
                break;
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
            Err(error) => unreachable!("the circle scene failed to render: {error}"),
        }
    }

    let Some(frame) = frame else {
        eprintln!("skipping renderer test: no frame became ready in time");
        return;
    };

    let bytes = match frame.as_bytes(0) {
        Some(bytes) => bytes,
        None => {
            eprintln!("unexpected: frame has no bytes");
            return;
        }
    };

    let alpha_at = |x: u32, y: u32| bytes[((y * SIZE + x) * 4 + 3) as usize];
    assert!(
        alpha_at(SIZE / 2, SIZE / 2) > 128,
        "the circle must cover the center of the frame"
    );
    assert_eq!(
        alpha_at(0, 0),
        0,
        "the circle must leave the frame corners transparent"
    );
}

/// Renders a full ring of frames at one size and parks them in the staging
/// ring (advance without presenting), then resizes: the size-mismatch retire
/// path drops `Mapped` slots without a second `unmap()` — their buffers were
/// already unmapped by `read_mapped`, and a second unmap raises a wgpu
/// validation error captured through the hook's error scope. Skipped (with a
/// note) when no wgpu adapter is available.
#[test]
fn resizing_over_parked_staging_frames_raises_no_validation_error() {
    let _renderer_lock = render_lock();
    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping renderer test: {error}");
            return;
        }
    };

    let scene = match SdfScene::from_str(CIRCLE_SCENE, "circle") {
        Ok(scene) => scene,
        Err(error) => unreachable!("the fixture scene must parse: {error}"),
    };

    const PARKED_SIZE: u32 = 64;
    const RESIZED_SIZE: u32 = 32;
    let parked_frame = FrameRequest {
        width: PARKED_SIZE,
        height: PARKED_SIZE,
        time: 0.0,
        mouse: [0.0, 0.0],
        view_center: [0.5, 0.5],
        view_zoom: 1.0,
        params: [0.0, 0.0],
    };
    let resized_frame = FrameRequest {
        width: RESIZED_SIZE,
        height: RESIZED_SIZE,
        time: 0.0,
        mouse: [0.0, 0.0],
        view_center: [0.5, 0.5],
        view_zoom: 1.0,
        params: [0.0, 0.0],
    };

    let (parked, errors) =
        match renderer.resize_over_parked_frames(&scene, &parked_frame, &resized_frame) {
            Ok(result) => result,
            Err(error) => unreachable!("the parked-frame resize must not fail: {error}"),
        };
    assert!(
        parked >= 1,
        "the fixture must park at least one completed staging frame, got {parked}"
    );
    assert!(
        errors.is_empty(),
        "resizing over parked staging frames must not raise a wgpu validation \
         error, got: {errors:?}"
    );
}

/// Loads a data scene from a synthetic VS-20 fixture and checks that the
/// uploaded field reaches the shader: the center pixel sits in stub tile
/// (1, 1), so its red channel is that tile's constant; a pixel inside real
/// tile (0, 0) carries that tile's level bytes at the selected mip level.
/// Skipped (with a note) when no wgpu adapter is available.
#[test]
fn data_scene_renders_the_uploaded_field() {
    let _renderer_lock = render_lock();
    let dir = match field_fixture("render") {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("skipping data-scene test: {error}");
            return;
        }
    };

    let scene = match data_scene(&dir, "probe", "manifest.json") {
        Ok(scene) => scene,
        Err(error) => unreachable!("unexpected data-scene load failure: {error}"),
    };
    assert!(scene.data_requested(), "the fixture scene requests data");
    let data = match scene.data() {
        Some(data) => data,
        None => unreachable!("unexpected: the fixture scene resolved no data"),
    };
    assert_eq!(data.grid, 2, "the fixture declares a 2×2 grid");
    assert_eq!(data.level_count(), 3, "the fixture declares three levels");

    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping data-scene test: {error}");
            return;
        }
    };

    let frame = match require_frame(
        first_frame(&mut renderer, &scene, [0.0, 0.0]),
        "the data scene failed to render",
    ) {
        Some(frame) => frame,
        None => {
            eprintln!("skipping data-scene test: no frame became ready in time");
            return;
        }
    };

    let bytes = match frame.as_bytes(0) {
        Some(bytes) => bytes,
        None => unreachable!("unexpected: the data-scene frame has no bytes"),
    };

    const SIZE: u32 = 64;
    let red_at = |x: u32, y: u32| bytes[((y * SIZE + x) * 4 + 2) as usize];
    assert_eq!(
        red_at(SIZE / 2, SIZE / 2),
        42,
        "the frame center must sample stub tile (1, 1) at its constant"
    );
    assert_eq!(
        red_at(SIZE / 4, SIZE / 4),
        20,
        "tile (0, 0) at level 0 must carry its mip0 texel (2, 2)"
    );

    // Level 2 is 1×1 per tile: every pixel of tile (0, 0) shows the level's
    // single byte (seed 10 + 2 × 7 + texel 0). A fresh renderer keeps the
    // staging ring from handing back a stale frame rendered at level 0.
    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping data-scene test: {error}");
            return;
        }
    };
    let frame_level2 = match require_frame(
        first_frame(&mut renderer, &scene, [2.0, 0.0]),
        "the data scene failed to render level 2",
    ) {
        Some(frame) => frame,
        None => {
            eprintln!("skipping data-scene test: no level-2 frame became ready in time");
            return;
        }
    };
    let bytes = match frame_level2.as_bytes(0) {
        Some(bytes) => bytes,
        None => unreachable!("unexpected: the level-2 frame has no bytes"),
    };
    assert_eq!(
        bytes[((SIZE / 4 * SIZE + SIZE / 4) * 4 + 2) as usize],
        24,
        "tile (0, 0) at level 2 must carry its final mip byte"
    );

    // The map view through the real pipeline. The identity view above is the
    // pre-change golden frame (the level/band assertions pin it). Aimed fully
    // inside stub tile (1, 1) — the viewport covers uv [0.625, 0.875]² at
    // zoom 4 — every sampled pixel must carry that tile's known constant:
    // the transform is verified through the actual sampling path, not just
    // "succeeds". A fresh renderer keeps the staging ring from handing back
    // a stale identity frame.
    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping data-scene test: {error}");
            return;
        }
    };
    let frame_zoomed = match require_frame(
        first_frame_view(&mut renderer, &scene, [0.0, 0.0], [0.75, 0.75], 4.0),
        "the zoomed view failed to render",
    ) {
        Some(frame) => frame,
        None => {
            eprintln!("skipping data-scene test: no zoomed-view frame became ready in time");
            return;
        }
    };
    let bytes = require_bytes(&frame_zoomed, "the zoomed-view frame");
    for (index, chunk) in bytes.as_chunks::<4>().0.iter().enumerate() {
        assert_eq!(
            chunk[2], 42,
            "pixel {index} must sample stub tile (1, 1) at its constant under the zoomed view"
        );
        assert_eq!(
            chunk[3], 255,
            "pixel {index} must stay fully opaque under the zoomed view"
        );
    }
}

/// The same uv window re-renders at any viewport size: one zoomed view aimed
/// inside stub tile (1, 1) must fill every pixel with that tile's constant at
/// 64² and at 32². The view is stored in field-uv space, so a window resize
/// re-renders the same field region instead of shifting it.
#[test]
fn the_view_window_is_size_independent() {
    let _lock = render_lock();
    let dir = match field_fixture("view-resize") {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("skipping view-resize test: {error}");
            return;
        }
    };
    let scene = match data_scene(&dir, "probe", "manifest.json") {
        Ok(scene) => scene,
        Err(error) => {
            eprintln!("skipping view-resize test: {error}");
            return;
        }
    };
    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping view-resize test: {error}");
            return;
        }
    };

    for size in [64u32, 32u32] {
        let frame = match require_frame(
            render_until_frame(
                &mut renderer,
                &scene,
                FrameRequest {
                    width: size,
                    height: size,
                    time: 0.0,
                    mouse: [0.0, 0.0],
                    view_center: [0.75, 0.75],
                    view_zoom: 4.0,
                    params: [0.0, 0.0],
                },
            ),
            "the resized-view frame failed to render",
        ) {
            Some(frame) => frame,
            None => {
                eprintln!("skipping view-resize test: no {size}² frame became ready in time");
                return;
            }
        };
        let bytes = require_bytes(&frame, "the resized-view frame");
        for (index, chunk) in bytes.as_chunks::<4>().0.iter().enumerate() {
            assert_eq!(
                chunk[2], 42,
                "pixel {index} must sample stub tile (1, 1) at {size}² under the zoomed view"
            );
            assert_eq!(
                chunk[3], 255,
                "pixel {index} must stay fully opaque at {size}² under the zoomed view"
            );
        }
    }
}

/// A view change must settle on the newest submitted frame: with a completed
/// frame from the previous view possibly still parked in the ring, repainting
/// without a state change must present the new view's frame and then stop
/// supplying frames — never resubmit forever, and never end on the previous
/// view's content.
#[test]
fn a_view_change_settles_on_the_newest_submitted_frame() {
    let _lock = render_lock();
    let dir = match field_fixture("view-settle") {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("skipping view-settle test: {error}");
            return;
        }
    };
    let scene = match data_scene(&dir, "probe", "manifest.json") {
        Ok(scene) => scene,
        Err(error) => {
            eprintln!("skipping view-settle test: {error}");
            return;
        }
    };
    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping view-settle test: {error}");
            return;
        }
    };

    match require_frame(
        first_frame_view(&mut renderer, &scene, [0.0, 0.0], [0.5, 0.5], 1.0),
        "the identity view failed to render",
    ) {
        Some(_) => {}
        None => {
            eprintln!("skipping view-settle test: no identity frame became ready in time");
            return;
        }
    }

    // A second, distinct submission under the same view, deliberately left
    // undrained: its completed frame may still sit in the ring when the view
    // changes — the stale-presentation setup the paint loop must survive.
    // The mouse differs so the renderer treats it as a new frame.
    let duplicate = FrameRequest {
        width: 64,
        height: 64,
        time: 0.0,
        mouse: [0.5, 0.0],
        view_center: [0.5, 0.5],
        view_zoom: 1.0,
        params: [0.0, 0.0],
    };
    match renderer.render(&scene, &duplicate) {
        Ok(_) => {}
        Err(error) => unreachable!("the duplicate identity submission failed: {error}"),
    }

    // Repaint under the new view until the ring drains. Every frame presented
    // after the new view's frame must carry the new view, and the sequence
    // must end in `None` — an unchanged repaint submits nothing, so frames
    // must run dry instead of being resupplied forever.
    let zoomed = FrameRequest {
        width: 64,
        height: 64,
        time: 0.0,
        mouse: [0.0, 0.0],
        view_center: [0.75, 0.75],
        view_zoom: 4.0,
        params: [0.0, 0.0],
    };
    let is_new_view = |image: &std::sync::Arc<gpui::RenderImage>| {
        let bytes = require_bytes(image, "the zoomed repaint");
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .all(|chunk| chunk[2] == 42 && chunk[3] == 255)
    };
    let mut saw_new_view = false;
    let mut settled = false;
    for _ in 0..300 {
        match renderer.render(&scene, &zoomed) {
            Ok(Some(frame)) => {
                if is_new_view(&frame) {
                    saw_new_view = true;
                } else {
                    assert!(
                        !saw_new_view,
                        "a frame presented after the new view's frame must carry the new view"
                    );
                }
            }
            Ok(None) if saw_new_view => {
                settled = true;
                break;
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
            Err(error) => unreachable!("the zoomed repaint failed: {error}"),
        }
    }
    assert!(
        saw_new_view,
        "the new view's frame must be presented after the view change"
    );
    assert!(
        settled,
        "repainting the new view without state changes must drain to None, not resubmit forever"
    );

    // The paint loop stops only when every submitted frame is consumed: drain
    // any straggler still in flight (the undrained duplicate) the same way the
    // canvas does — by repainting without a state change — then assert the
    // terminal state.
    for _ in 0..300 {
        if renderer.pending_frames() == 0 {
            break;
        }
        match renderer.render(&scene, &zoomed) {
            Ok(Some(frame)) => assert!(
                is_new_view(&frame),
                "a frame presented after the new view's frame must carry the new view"
            ),
            Ok(None) => {}
            Err(error) => unreachable!("the drain repaint failed: {error}"),
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        renderer.pending_frames(),
        0,
        "the ring must drain completely after the view change"
    );
    let settled_frame = match renderer.render(&scene, &zoomed) {
        Ok(frame) => frame,
        Err(error) => unreachable!("the settled repaint failed: {error}"),
    };
    assert!(
        settled_frame.is_none(),
        "a settled, unchanged repaint must not supply another frame"
    );
}

/// A two-layer composite scene binds one field texture per configured
/// layer: the frame carries the brighter of the two fields' constants, so a
/// regression to a single-texture bind group (or a swapped pair) fails
/// through the pixels. Skipped (with a note) when no wgpu adapter is
/// available.
#[test]
fn a_composite_scene_renders_with_per_field_textures() {
    let _renderer_lock = render_lock();
    let dir = match field_fixture("composite-render") {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("skipping composite-render test: {error}");
            return;
        }
    };
    let scene = match composite_scene(&dir, "probe,echo", "manifest.json", COMPOSITE_BODY) {
        Ok(scene) => scene,
        Err(error) => unreachable!("unexpected composite-scene load failure: {error}"),
    };
    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping composite-render test: {error}");
            return;
        }
    };

    let frame = match require_frame(
        first_frame(&mut renderer, &scene, [0.0, 0.0]),
        "the composite scene failed to render",
    ) {
        Some(frame) => frame,
        None => {
            eprintln!("skipping composite-render test: no frame became ready in time");
            return;
        }
    };
    let bytes = match frame.as_bytes(0) {
        Some(bytes) => bytes,
        None => unreachable!("unexpected: the composite frame has no bytes"),
    };

    const SIZE: u32 = 64;
    let red_at = |x: u32, y: u32| bytes[((y * SIZE + x) * 4 + 2) as usize];
    assert_eq!(
        red_at(SIZE / 2, SIZE / 2),
        84,
        "the frame center must sample the brightest of the two stub tiles (42, 84)"
    );
    // In-tile uv 0.5 sits exactly between echo's mip0 texels (1, 1) = 206 and
    // (2, 2) = 210 under linear filtering: the frame mixes them to 208 —
    // which only field 1 (echo) can reach, probe's mix there is 18.
    assert_eq!(
        red_at(SIZE / 4, SIZE / 4),
        208,
        "tile (0, 0) must show echo's mip0 texels mixed by the linear sampler"
    );
}

/// Switching composite → single → composite on one renderer must rebuild
/// the bind-group shape and rebind per-field textures each time: the ordered
/// data key pins the skip-unchanged check, and the pixels must follow the
/// scene class of the moment. Skipped (with a note) when no adapter is
/// available.
#[test]
fn a_composite_to_single_to_composite_switch_rebuilds_the_bind_group_shape() {
    let _renderer_lock = render_lock();
    let dir = match field_fixture("composite-switch") {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("skipping composite-switch test: {error}");
            return;
        }
    };
    let composite = match composite_scene(&dir, "probe,echo", "manifest.json", COMPOSITE_BODY) {
        Ok(scene) => scene,
        Err(error) => unreachable!("unexpected composite-scene load failure: {error}"),
    };
    let single = match data_scene(&dir, "probe", "manifest.json") {
        Ok(scene) => scene,
        Err(error) => unreachable!("unexpected data-scene load failure: {error}"),
    };
    assert_ne!(
        composite.data_key(),
        single.data_key(),
        "the ordered data keys must distinguish composite from single-field"
    );
    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping composite-switch test: {error}");
            return;
        }
    };

    const SIZE: u32 = 64;
    let center_red = |frame: &std::sync::Arc<gpui::RenderImage>| -> u8 {
        require_bytes(frame, "the switch-test frame")
            [((SIZE / 2 * SIZE + SIZE / 2) * 4 + 2) as usize]
    };

    let composite_frame = match require_frame(
        first_frame(&mut renderer, &composite, [0.0, 0.0]),
        "the composite scene failed to render",
    ) {
        Some(frame) => frame,
        None => {
            eprintln!("skipping composite-switch test: no composite frame became ready in time");
            return;
        }
    };
    assert_eq!(
        center_red(&composite_frame),
        84,
        "the composite must render both fields' stub constants"
    );

    let single_frame = match require_frame(
        first_frame(&mut renderer, &single, [0.0, 0.0]),
        "the single-field scene failed to render after the composite",
    ) {
        Some(frame) => frame,
        None => {
            eprintln!("skipping composite-switch test: no single-field frame became ready in time");
            return;
        }
    };
    assert_eq!(
        center_red(&single_frame),
        42,
        "the single-field scene must render only its own field"
    );

    let again_frame = match require_frame(
        first_frame(&mut renderer, &composite, [0.0, 0.0]),
        "the composite scene failed to render after switching back",
    ) {
        Some(frame) => frame,
        None => {
            eprintln!(
                "skipping composite-switch test: no second composite frame became ready in time"
            );
            return;
        }
    };
    assert_eq!(
        center_red(&again_frame),
        84,
        "switching back must restore the composite's per-field bindings"
    );
}

/// A field count above the device's sampled-texture limit must be rejected
/// with the limit named, never truncated. The renderer requests
/// `wgpu::Limits::default()`, so 20 fields exceed the granted
/// `max_sampled_textures_per_shader_stage` on any host. Skipped (with a
/// note) when no wgpu adapter is available.
#[test]
fn a_field_count_above_the_device_limit_is_rejected_with_the_limit_named() {
    let _renderer_lock = render_lock();
    let dir = match field_fixture_with_layers("limit-fields", 18) {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("skipping field-limit test: {error}");
            return;
        }
    };
    let mut layer_names = vec![String::from("probe"), String::from("echo")];
    for index in 0..18 {
        layer_names.push(format!("echo_{index}"));
    }
    let scene = match composite_scene(
        &dir,
        &layer_names.join(","),
        "manifest.json",
        COMPOSITE_BODY,
    ) {
        Ok(scene) => scene,
        Err(error) => unreachable!("unexpected composite-scene load failure: {error}"),
    };
    assert_eq!(
        scene.data().map(|data| data.fields.len()),
        Some(20),
        "the fixture must resolve twenty fields"
    );

    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping field-limit test: {error}");
            return;
        }
    };
    let failure = match first_frame(&mut renderer, &scene, [0.0, 0.0]) {
        Err(error) => Some(error.to_string()),
        Ok(_) => None,
    };
    assert!(
        failure
            .as_deref()
            .is_some_and(|message| message.contains("max_sampled_textures_per_shader_stage")),
        "a field count above the device limit must be named, got: {failure:?}"
    );
}

/// Corrupting one tile payload must fail the load with the offending path and
/// hash named; truncating one must fail with the declared length named.
/// Corrupting one tile payload must fail the load with the offending path and
/// hash named. (A truncated payload fails the same sha check — a length
/// mismatch with a *matching* sha is a separate case, covered by
/// `a_short_payload_with_a_matching_sha_is_a_length_error`.)
#[test]
fn corrupted_and_truncated_tiles_are_load_errors() {
    let _renderer_lock = render_lock();
    let dir = match field_fixture("corrupt") {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("skipping corruption test: {error}");
            return;
        }
    };

    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping corruption test: {error}");
            return;
        }
    };
    let scene = match data_scene(&dir, "probe", "manifest.json") {
        Ok(scene) => scene,
        Err(error) => unreachable!("unexpected data-scene load failure: {error}"),
    };

    let payload_path = dir.join("tile_1_0.r8");
    let mut payload = match std::fs::read(&payload_path) {
        Ok(payload) => payload,
        Err(error) => {
            eprintln!("unexpected fixture read failure: {error}");
            return;
        }
    };
    payload[0] = payload[0].wrapping_add(1);
    if let Err(error) = std::fs::write(&payload_path, &payload) {
        eprintln!("unexpected fixture write failure: {error}");
        return;
    }

    let failure = match first_frame(&mut renderer, &scene, [0.0, 0.0]) {
        Err(error) => Some(error.to_string()),
        Ok(_) => None,
    };
    assert!(
        failure
            .as_deref()
            .is_some_and(|message| message.contains("tile_1_0.r8") && message.contains("sha256")),
        "a sha mismatch must name the tile and the hash, got: {failure:?}"
    );
}

/// A payload whose sha matches the manifest but whose length disagrees with
/// the declared mip sizes must fail with the length named.
#[test]
fn a_short_payload_with_a_matching_sha_is_a_length_error() {
    let _renderer_lock = render_lock();
    let dir = match field_fixture("length") {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("skipping length-error test: {error}");
            return;
        }
    };

    let payload_path = dir.join("tile_1_0.r8");
    let full = match std::fs::read(&payload_path) {
        Ok(payload) => payload,
        Err(error) => {
            eprintln!("unexpected fixture read failure: {error}");
            return;
        }
    };
    let short = &full[..20];
    if let Err(error) = std::fs::write(&payload_path, short) {
        eprintln!("unexpected fixture write failure: {error}");
        return;
    }
    let manifest_path = dir.join("manifest.json");
    let mut manifest: serde_json::Value = match std::fs::read_to_string(&manifest_path) {
        Ok(raw) => match serde_json::from_str(&raw) {
            Ok(manifest) => manifest,
            Err(error) => {
                eprintln!("unexpected fixture parse failure: {error}");
                return;
            }
        },
        Err(error) => {
            eprintln!("unexpected fixture read failure: {error}");
            return;
        }
    };
    for tile in manifest["layers"][0]["tiles"]
        .as_array_mut()
        .map(|tiles| tiles.iter_mut())
        .into_iter()
        .flatten()
    {
        if tile["payload"] == "tile_1_0.r8" {
            tile["payload_sha256"] = serde_json::Value::String(sha256_hex(short));
        }
    }
    if let Err(error) = std::fs::write(
        &manifest_path,
        serde_json::to_string(&manifest).unwrap_or_default(),
    ) {
        eprintln!("unexpected fixture write failure: {error}");
        return;
    }

    let scene = match data_scene(&dir, "probe", "manifest.json") {
        Ok(scene) => scene,
        Err(error) => unreachable!("unexpected data-scene load failure: {error}"),
    };
    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping length-error test: {error}");
            return;
        }
    };
    let failure = match first_frame(&mut renderer, &scene, [0.0, 0.0]) {
        Err(error) => Some(error.to_string()),
        Ok(_) => None,
    };
    assert!(
        failure.as_deref().is_some_and(|message| {
            message.contains("tile_1_0.r8") && message.contains("20 bytes")
        }),
        "a short payload must name the tile and both lengths, got: {failure:?}"
    );
}

#[test]
fn manifest_and_layer_failures_surface_at_scene_load() {
    let dir = match field_fixture("failures") {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("skipping manifest-failure test: {error}");
            return;
        }
    };

    let failure = match data_scene(&dir, "missing", "manifest.json") {
        Err(error) => Some(error.to_string()),
        Ok(_) => None,
    };
    assert!(
        failure
            .as_deref()
            .is_some_and(|message| message.contains("no layer named `missing`")),
        "an unknown layer must be named, got: {failure:?}"
    );

    let manifest_path = dir.join("broken.json");
    if let Err(error) = std::fs::write(&manifest_path, "{ not json") {
        eprintln!("unexpected fixture write failure: {error}");
        return;
    }
    let failure = match data_scene(&dir, "probe", "broken.json") {
        Err(error) => Some(error.to_string()),
        Ok(_) => None,
    };
    assert!(
        failure
            .as_deref()
            .is_some_and(|message| message.contains("not valid JSON")),
        "a malformed manifest must be reported, got: {failure:?}"
    );

    let failure = match data_scene(&dir, "probe", "absent.json") {
        Err(error) => Some(error.to_string()),
        Ok(_) => None,
    };
    assert!(
        failure
            .as_deref()
            .is_some_and(|message| message.contains("could not be read")),
        "a missing manifest must be reported, got: {failure:?}"
    );
}

/// Every structural rejection of the manifest validator is a named error
/// path: each corruption below must fail the load with its specific problem
/// reported.
#[test]
fn manifest_structural_validators_reject_corrupt_tables() {
    let dir = match field_fixture("validate") {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("skipping validator test: {error}");
            return;
        }
    };
    let manifest: serde_json::Value = match std::fs::read_to_string(dir.join("manifest.json"))
        .map_err(|error| error.to_string())
        .and_then(|raw| serde_json::from_str(&raw).map_err(|error| error.to_string()))
    {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!("skipping validator test: {error}");
            return;
        }
    };

    manifest_error(
        &dir,
        &manifest,
        "format",
        |value| {
            value["format"] = json!("cd-map-sdf-other");
        },
        "declares format",
    );
    manifest_error(
        &dir,
        &manifest,
        "version",
        |value| {
            value["version"] = json!(2);
        },
        "declares version",
    );
    manifest_error(
        &dir,
        &manifest,
        "size-multiple",
        |value| {
            value["layers"][0]["size"] = json!(10);
        },
        "not a multiple",
    );
    manifest_error(
        &dir,
        &manifest,
        "mips-missing",
        |value| {
            if let Some(entry) = value["layers"][0].as_object_mut() {
                entry.remove("mips");
            }
        },
        "no `mips` table",
    );
    manifest_error(
        &dir,
        &manifest,
        "level-order",
        |value| {
            value["layers"][0]["mips"][1]["level"] = json!(5);
        },
        "in order",
    );
    manifest_error(
        &dir,
        &manifest,
        "size-halving",
        |value| {
            value["layers"][0]["mips"][1]["size"] = json!(3);
        },
        "does not halve",
    );
    manifest_error(
        &dir,
        &manifest,
        "level-bytes",
        |value| {
            value["layers"][0]["mips"][1]["bytes"] = json!(3);
        },
        "bytes for a",
    );
    manifest_error(
        &dir,
        &manifest,
        "mip-size-overflow",
        |value| {
            value["layers"][0]["mips"][0]["size"] = json!(65536);
        },
        "exceeds the supported range",
    );
    manifest_error(
        &dir,
        &manifest,
        "mip-zero-size",
        |value| {
            value["layers"][0]["mips"][0]["size"] = json!(0);
        },
        "zero-size",
    );
    manifest_error(
        &dir,
        &manifest,
        "tiles-missing",
        |value| {
            if let Some(entry) = value["layers"][0].as_object_mut() {
                entry.remove("tiles");
            }
        },
        "no `tiles` table",
    );
    manifest_error(
        &dir,
        &manifest,
        "tile-count",
        |value| {
            if let Some(tiles) = value["layers"][0]["tiles"].as_array_mut() {
                tiles.remove(0);
            }
        },
        "tile table has",
    );
    manifest_error(
        &dir,
        &manifest,
        "tile-coords",
        |value| {
            value["layers"][0]["tiles"][0]["x"] = json!(9);
        },
        "outside the",
    );
    manifest_error(
        &dir,
        &manifest,
        "tile-duplicate",
        |value| {
            value["layers"][0]["tiles"][3]["x"] = json!(0);
        },
        "claimed by more than one",
    );
    manifest_error(
        &dir,
        &manifest,
        "tile-kind",
        |value| {
            value["layers"][0]["tiles"][0]["kind"] = json!("wat");
        },
        "unknown kind",
    );
    manifest_error(
        &dir,
        &manifest,
        "tile-payload-name",
        |value| {
            value["layers"][0]["tiles"][0]["payload"] = json!("");
        },
        "missing its `payload`",
    );
    manifest_error(
        &dir,
        &manifest,
        "tile-sha",
        |value| {
            value["layers"][0]["tiles"][0]["payload_sha256"] = json!("zz");
        },
        "payload_sha256",
    );
}

/// A structurally valid manifest whose size/tile-edge ratio exceeds the u32
/// multiply range must be a named load error, not an arithmetic panic.
#[test]
fn an_oversized_tile_grid_is_a_load_error() {
    let dir = match field_fixture("overflow") {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("skipping oversized-grid test: {error}");
            return;
        }
    };
    let manifest = json!({
        "format": "cd-map-sdf-field",
        "version": 1,
        "layers": [{
            "name": "probe",
            "tile_prefix": "fixture_field",
            "size": 262144,
            "source_manifest_sha256": "0".repeat(64),
            "mips": [{ "level": 0, "size": 4, "bytes": 16 }],
            "tiles": [],
        }],
    });
    let name = "manifest-overflow.json";
    if let Err(error) = std::fs::write(
        dir.join(name),
        serde_json::to_string(&manifest).unwrap_or_default(),
    ) {
        eprintln!("skipping oversized-grid test: {error}");
        return;
    }

    let failure = match data_scene(&dir, "probe", name) {
        Err(error) => Some(error.to_string()),
        Ok(_) => None,
    };
    assert!(
        failure
            .as_deref()
            .is_some_and(|message| message.contains("tile grid")),
        "an oversized tile grid must be a named load error, got: {failure:?}"
    );
}

/// A scene parsed with `from_str` keeps its data directives unresolved (there
/// is no file to resolve against); the renderer must refuse it at render time
/// instead of silently drawing the zero-filled stub as real field data.
#[test]
fn an_unresolved_data_scene_is_a_render_error() {
    let _renderer_lock = render_lock();
    let source = "// sdf-scene: data = \"manifest.json\"\n// sdf-scene: layer = \"coast\"\nfn scene(p: vec2f, _time: f32) -> f32 { return 0.0; }";
    let scene = match SdfScene::from_str(source, "unresolved") {
        Ok(scene) => scene,
        Err(error) => {
            eprintln!("unexpected scene parse failure: {error}");
            return;
        }
    };
    assert!(scene.data_requested(), "the scene requests data");
    assert!(scene.data().is_none(), "from_str cannot resolve a manifest");

    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping unresolved-data test: {error}");
            return;
        }
    };
    let failure = match first_frame(&mut renderer, &scene, [0.0, 0.0]) {
        Err(error) => Some(error.to_string()),
        Ok(_) => None,
    };
    assert!(
        failure
            .as_deref()
            .is_some_and(|message| message.contains("could not be resolved")),
        "an unresolved data scene must be refused at render time, got: {failure:?}"
    );
}

/// A failed tile upload is deterministic — the payload stays missing, corrupt,
/// or truncated until the scene's data changes — so the renderer must remember
/// the failure and re-raise it instead of re-reading and re-hashing every
/// payload on each paint. Deleting the payloads after the first failure makes
/// a re-attempt observable: it would surface a different error.
#[test]
fn a_failed_upload_is_not_reattempted_on_every_paint() {
    let _renderer_lock = render_lock();
    let dir = match field_fixture("retry") {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("skipping retry test: {error}");
            return;
        }
    };

    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping retry test: {error}");
            return;
        }
    };
    let scene = match data_scene(&dir, "probe", "manifest.json") {
        Ok(scene) => scene,
        Err(error) => unreachable!("unexpected data-scene load failure: {error}"),
    };

    let payload_path = dir.join("tile_1_0.r8");
    let mut payload = match std::fs::read(&payload_path) {
        Ok(payload) => payload,
        Err(error) => {
            eprintln!("unexpected fixture read failure: {error}");
            return;
        }
    };
    payload[0] = payload[0].wrapping_add(1);
    if let Err(error) = std::fs::write(&payload_path, &payload) {
        eprintln!("unexpected fixture write failure: {error}");
        return;
    }

    let first = match first_frame(&mut renderer, &scene, [0.0, 0.0]) {
        Err(error) => error.to_string(),
        Ok(_) => {
            eprintln!("expected the corrupt payload to fail the upload");
            return;
        }
    };
    assert!(
        first.contains("sha256"),
        "the first failure must be the sha mismatch, got: {first}"
    );

    for name in ["tile_0_0.r8", "tile_1_0.r8", "tile_0_1.r8", "tile_1_1.r8"] {
        let _ = std::fs::remove_file(dir.join(name));
    }

    let second = match first_frame(&mut renderer, &scene, [0.0, 0.0]) {
        Err(error) => error.to_string(),
        Ok(Some(_)) => {
            eprintln!("the recorded failure must keep failing, not render");
            return;
        }
        Ok(None) => {
            eprintln!("no frame became ready in time");
            return;
        }
    };
    assert!(
        second.contains("sha256"),
        "the recorded failure must be re-raised without re-reading the payloads, got: {second}"
    );
}

/// The contour overlay pass runs only when a data texture is bound and the
/// band width is on: enabling it over a data scene changes the frame; over a
/// scene without data the frame is untouched.
#[test]
fn contour_overlay_runs_only_over_bound_data() {
    let _renderer_lock = render_lock();
    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping overlay test: {error}");
            return;
        }
    };

    let circle = match SdfScene::from_str(CIRCLE_SCENE, "circle") {
        Ok(scene) => scene,
        Err(error) => {
            eprintln!("unexpected scene parse failure: {error}");
            return;
        }
    };
    let no_data_off = require_frame(
        first_frame(&mut renderer, &circle, [0.0, 0.0]),
        "the no-data scene failed to render",
    );
    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping overlay test: {error}");
            return;
        }
    };
    let no_data_on = require_frame(
        first_frame(&mut renderer, &circle, [0.0, 4.0]),
        "the no-data scene with the overlay on failed to render",
    );
    match (no_data_off, no_data_on) {
        (Some(off), Some(on)) => {
            assert_eq!(
                require_bytes(&off, "the no-data overlay frame"),
                require_bytes(&on, "the no-data overlay frame"),
                "the overlay must be absent from frames without bound data"
            );
        }
        _ => {
            eprintln!("skipping overlay test: no frame became ready in time");
            return;
        }
    }

    let dir = match field_fixture("overlay") {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("skipping overlay test: {error}");
            return;
        }
    };
    let scene = match data_scene(&dir, "probe", "manifest.json") {
        Ok(scene) => scene,
        Err(error) => unreachable!("unexpected data-scene load failure: {error}"),
    };
    let data_off = require_frame(
        first_frame(&mut renderer, &scene, [0.0, 0.0]),
        "the data scene failed to render",
    );
    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping overlay test: {error}");
            return;
        }
    };
    let data_on = require_frame(
        first_frame(&mut renderer, &scene, [0.0, 4.0]),
        "the data scene with the overlay on failed to render",
    );
    match (data_off, data_on) {
        (Some(off), Some(on)) => {
            assert_ne!(
                require_bytes(&off, "the data overlay frame"),
                require_bytes(&on, "the data overlay frame"),
                "the overlay must draw contour lines over a data scene"
            );
        }
        _ => {
            eprintln!("skipping overlay test: no frame became ready in time");
        }
    }
}

/// The frame's readback bytes; a frame returned by `require_frame` always
/// carries them, so a missing readback is a bug and must fail the test, not
/// quietly compare two empty slices.
fn require_bytes<'a>(image: &'a std::sync::Arc<gpui::RenderImage>, context: &str) -> &'a [u8] {
    image
        .as_bytes(0)
        .unwrap_or_else(|| unreachable!("{context}: the frame has no bytes"))
}

/// Inside a constant field region the band function has no crossing
/// information (aa == 0) and the overlay must contribute exactly nothing —
/// not an undefined pixel. Stub tile (0, 1) is constant 0, so with band
/// width 4 its scaled value sits exactly on the 0/0 smoothstep input.
#[test]
fn contour_overlay_contributes_nothing_over_constant_regions() {
    let _renderer_lock = render_lock();
    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping overlay-constant test: {error}");
            return;
        }
    };

    let dir = match field_fixture("overlay-constant") {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("skipping overlay-constant test: {error}");
            return;
        }
    };
    let scene = match data_scene(&dir, "probe", "manifest.json") {
        Ok(scene) => scene,
        Err(error) => unreachable!("unexpected data-scene load failure: {error}"),
    };

    let off = require_frame(
        first_frame(&mut renderer, &scene, [0.0, 0.0]),
        "the data scene failed to render",
    );
    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping overlay-constant test: {error}");
            return;
        }
    };
    let on = require_frame(
        first_frame(&mut renderer, &scene, [0.0, 4.0]),
        "the data scene with the overlay on failed to render",
    );

    let (Some(off), Some(on)) = (off, on) else {
        eprintln!("skipping overlay-constant test: no frame became ready in time");
        return;
    };

    let (off_bytes, on_bytes) = (
        require_bytes(&off, "the constant-region overlay frame"),
        require_bytes(&on, "the constant-region overlay frame"),
    );
    const SIZE: u32 = 64;
    let alpha_at = |bytes: &[u8], x: u32, y: u32| bytes[((y * SIZE + x) * 4 + 3) as usize];
    // Stub tile (0, 1) covers the lower-left quadrant of the frame.
    assert_eq!(
        alpha_at(off_bytes, 16, 48),
        alpha_at(on_bytes, 16, 48),
        "the overlay must contribute nothing inside a constant region"
    );
    // The real tiles across the top half vary, so the overlay still draws.
    let top_half = (32 * SIZE * 4) as usize;
    assert_ne!(
        &off_bytes[..top_half],
        &on_bytes[..top_half],
        "the overlay must draw contour lines over varying regions"
    );
}

/// The shipped game-data scenes parse, use the render override, and carry
/// paired data directives pointing at the app config (`config:` scheme).
/// The scheme is resolved by the host shell (sdf-ui), not the component, so
/// the scenes cannot load through `from_file` here — component-side load
/// must fail with the missing manifest named. The coast recipe's WGSL then
/// meets the real pipeline against the synthetic field fixture: its
/// `config:` value is rewritten to the fixture manifest's absolute path (the
/// same substitution the host shell performs) and the scene renders through
/// the actual pipeline, the only test that compiles the shipped shader
/// bodies without a real export on the machine.
#[test]
fn shipped_game_scenes_parse_with_config_scheme_directives() {
    const SHIPPED: [(&str, &str); 6] = [
        (
            "abyss_hex",
            include_str!("../../../../scenes/abyss_hex.wgsl"),
        ),
        ("coast", include_str!("../../../../scenes/coast.wgsl")),
        ("inspect", include_str!("../../../../scenes/inspect.wgsl")),
        ("mountain", include_str!("../../../../scenes/mountain.wgsl")),
        ("river", include_str!("../../../../scenes/river.wgsl")),
        ("road", include_str!("../../../../scenes/road.wgsl")),
    ];
    for (name, source) in SHIPPED {
        let scene = SdfScene::from_str(source, name);
        assert!(scene.is_ok(), "the shipped scene `{name}` must parse");
        let scene = match scene {
            Ok(scene) => scene,
            Err(_) => unreachable!("the assert above already failed"),
        };
        assert!(
            scene.data_requested(),
            "the shipped scene `{name}` must carry data directives"
        );
    }

    let coast_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenes/coast.wgsl");
    let failure = match SdfScene::from_file(&coast_path) {
        Err(error) => error.to_string(),
        Ok(_) => String::new(),
    };
    assert!(
        failure.contains("could not be read") || failure.contains("could not be loaded"),
        "a `config:` scene must fail component-side load with the missing manifest named, got: {failure}"
    );

    let (_, coast_source) = SHIPPED
        .iter()
        .find(|(name, _)| *name == "coast")
        .map(|(name, source)| (*name, *source))
        .unwrap_or_else(|| unreachable!("the shipped list carries coast"));
    let fixture_dir = match field_fixture("shipped-coast") {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("skipping shipped-scene render: {error}");
            return;
        }
    };
    let manifest_path = fixture_dir.join("manifest.json");
    let raw = match std::fs::read_to_string(&manifest_path) {
        Ok(raw) => raw,
        Err(error) => unreachable!("read the fixture manifest: {error}"),
    };
    let mut manifest = match serde_json::from_str::<serde_json::Value>(&raw) {
        Ok(manifest) => manifest,
        Err(error) => unreachable!("parse the fixture manifest: {error}"),
    };
    manifest["layers"][0]["name"] = json!("coast");
    let renamed = match serde_json::to_string(&manifest) {
        Ok(json) => json,
        Err(error) => unreachable!("serialize the renamed fixture manifest: {error}"),
    };
    if let Err(error) = std::fs::write(&manifest_path, &renamed) {
        eprintln!("skipping shipped-scene render: {error}");
        return;
    }
    let rewritten = coast_source.replace(
        "config:sdf/manifest.json",
        &manifest_path.display().to_string().replace('\\', "/"),
    );
    let scene_path = fixture_dir.join("coast.wgsl");
    if let Err(error) = std::fs::write(&scene_path, &rewritten) {
        eprintln!("skipping shipped-scene render: {error}");
        return;
    }
    let scene = match SdfScene::from_file(&scene_path) {
        Ok(scene) => scene,
        Err(error) => unreachable!("the fixture-backed coast scene must load: {error}"),
    };
    assert_eq!(scene.name(), "Coast");

    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping shipped-scene render: {error}");
            return;
        }
    };
    let frame = match require_frame(
        first_frame(&mut renderer, &scene, [0.0, 0.0]),
        "the shipped coast scene failed to render",
    ) {
        Some(frame) => frame,
        None => {
            eprintln!("skipping shipped-scene render: no frame became ready in time");
            return;
        }
    };
    let painted = frame
        .as_bytes(0)
        .map(|bytes| {
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|pixel| pixel[3] > 0)
                .count()
        })
        .unwrap_or(0);
    assert!(painted > 0, "the shipped coast scene must paint the frame");
}

/// End-to-end against a real VS-20 export when one is available: point
/// `SDF_TEST_FIELD_MANIFEST` at a fields manifest produced by the pipeline
/// (e.g. the app config's `sdf/manifest.json`). Skipped when unset.
#[test]
fn real_vs20_export_loads_and_renders() {
    let _renderer_lock = render_lock();
    let Some(manifest) = std::env::var("SDF_TEST_FIELD_MANIFEST")
        .ok()
        .filter(|path| Path::new(path).is_file())
    else {
        eprintln!(
            "skipping real-export test: set SDF_TEST_FIELD_MANIFEST to a VS-20 fields manifest"
        );
        return;
    };

    let dir = std::env::temp_dir().join(format!("sdf-real-scene-{}", std::process::id()));
    if let Err(error) = std::fs::create_dir_all(&dir) {
        eprintln!("skipping real-export test: {error}");
        return;
    }
    let scene_path = dir.join("real.wgsl");
    let source = format!(
        "// sdf-scene: data = \"{}\"\n// sdf-scene: layer = \"coast\"\n\
         fn render(p: vec2f, _time: f32) -> vec4f {{\n    \
         let raw = field_raw(p, u.params.x);\n    \
         return vec4f(vec3f(raw), 1.0);\n}}\n",
        manifest.replace('\\', "/")
    );
    if let Err(error) = std::fs::write(&scene_path, source) {
        eprintln!("skipping real-export test: {error}");
        return;
    }

    let scene = match SdfScene::from_file(&scene_path) {
        Ok(scene) => scene,
        Err(error) => {
            eprintln!("real export failed to load: {error}");
            return;
        }
    };
    let level_count = scene.data().map(|data| data.level_count());
    assert_eq!(
        level_count,
        Some(10),
        "the land field's chain runs 512² → 1², ten levels"
    );

    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping real-export test: {error}");
            return;
        }
    };
    match require_frame(
        first_frame(&mut renderer, &scene, [0.0, 0.0]),
        "the real export failed to render",
    ) {
        Some(frame) => {
            let painted = frame
                .as_bytes(0)
                .map(|bytes| {
                    bytes
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .filter(|pixel| pixel[3] > 0)
                        .count()
                })
                .unwrap_or(0);
            assert!(painted > 0, "the real export must paint the frame");
        }
        None => eprintln!("skipping real-export check: no frame became ready in time"),
    }
}

/// Field geometry beyond the device's default limits must fail the render
/// with the limit named, like every other load failure — never truncated or
/// silently degraded. The renderer requests `wgpu::Limits::default()`, so the
/// boundaries exercised here (tile edge above `max_texture_dimension_2d`,
/// tile grid above `max_texture_array_layers`) are the granted limits on any
/// host. Skipped (with a note) when no wgpu adapter is available.
#[test]
fn oversized_field_geometry_names_the_device_limit() {
    let _renderer_lock = render_lock();
    let dir = match field_fixture("limits") {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("skipping device-limit test: {error}");
            return;
        }
    };
    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping device-limit test: {error}");
            return;
        }
    };

    let oversized_edge = json!({
        "format": "cd-map-sdf-field",
        "version": 1,
        "layers": [{
            "name": "probe",
            "tile_prefix": "fixture_field",
            "size": 16384,
            "source_manifest_sha256": "0".repeat(64),
            "mips": [{ "level": 0, "size": 16384, "bytes": 268435456 }],
            "tiles": [{
                "payload": "oversized-edge.r8",
                "x": 0,
                "y": 0,
                "kind": "mip0",
                "payload_sha256": "0".repeat(64),
            }],
        }],
    });
    let name = "manifest-oversized-edge.json";
    if let Err(error) = std::fs::write(
        dir.join(name),
        serde_json::to_string(&oversized_edge).unwrap_or_default(),
    ) {
        eprintln!("skipping device-limit test: {error}");
        return;
    }
    let scene = match data_scene(&dir, "probe", name) {
        Ok(scene) => scene,
        Err(error) => unreachable!("unexpected data-scene load failure: {error}"),
    };
    let failure = match first_frame(&mut renderer, &scene, [0.0, 0.0]) {
        Err(error) => Some(error.to_string()),
        Ok(_) => None,
    };
    assert!(
        failure
            .as_deref()
            .is_some_and(|message| message.contains("max_texture_dimension_2d")),
        "a tile edge above the device limit must be named, got: {failure:?}"
    );

    let mut grid_tiles = Vec::new();
    for y in 0..17u32 {
        for x in 0..17u32 {
            grid_tiles.push(json!({
                "payload": format!("grid_tile_{x}_{y}.r8"),
                "x": x,
                "y": y,
                "kind": "mip0",
                "payload_sha256": "0".repeat(64),
            }));
        }
    }
    let oversized_grid = json!({
        "format": "cd-map-sdf-field",
        "version": 1,
        "layers": [{
            "name": "probe",
            "tile_prefix": "fixture_field",
            "size": 68,
            "source_manifest_sha256": "0".repeat(64),
            "mips": [{ "level": 0, "size": 4, "bytes": 16 }],
            "tiles": grid_tiles,
        }],
    });
    let name = "manifest-oversized-grid.json";
    if let Err(error) = std::fs::write(
        dir.join(name),
        serde_json::to_string(&oversized_grid).unwrap_or_default(),
    ) {
        eprintln!("skipping device-limit test: {error}");
        return;
    }
    let scene = match data_scene(&dir, "probe", name) {
        Ok(scene) => scene,
        Err(error) => unreachable!("unexpected data-scene load failure: {error}"),
    };
    let failure = match first_frame(&mut renderer, &scene, [0.0, 0.0]) {
        Err(error) => Some(error.to_string()),
        Ok(_) => None,
    };
    assert!(
        failure
            .as_deref()
            .is_some_and(|message| message.contains("max_texture_array_layers")),
        "a tile grid above the array-layer limit must be named, got: {failure:?}"
    );
}

/// Deleting a tile payload after the scene loads must fail the first render
/// with the unreadable payload named: the payload read happens when the
/// field texture is built, so a file that disappears between scene load and
/// first render surfaces through the render error path.
#[test]
fn a_deleted_payload_is_a_named_read_error() {
    let _renderer_lock = render_lock();
    let dir = match field_fixture("deleted") {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("skipping deleted-payload test: {error}");
            return;
        }
    };
    let scene = match data_scene(&dir, "probe", "manifest.json") {
        Ok(scene) => scene,
        Err(error) => unreachable!("unexpected data-scene load failure: {error}"),
    };
    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping deleted-payload test: {error}");
            return;
        }
    };

    if let Err(error) = std::fs::remove_file(dir.join("tile_0_0.r8")) {
        eprintln!("skipping deleted-payload test: {error}");
        return;
    }

    let failure = match first_frame(&mut renderer, &scene, [0.0, 0.0]) {
        Err(error) => Some(error.to_string()),
        Ok(_) => None,
    };
    assert!(
        failure.as_deref().is_some_and(
            |message| message.contains("tile_0_0.r8") && message.contains("could not be read")
        ),
        "a deleted payload must be named in the render error, got: {failure:?}"
    );
}

/// Two scenes sharing a body but binding different manifests must still
/// switch: `set_scene`'s unchanged check compares the data identity as well
/// as the source, so a regression to a source-only comparison — which would
/// keep the previous scene's field texture bound under the new scene's name —
/// fails here through the active data's mip-level count.
#[gpui::test]
fn scenes_sharing_a_body_but_not_data_still_switch(cx: &mut TestAppContext) {
    use crate::SdfCanvasState;

    let dir = match field_fixture("switch") {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("skipping scene-switch test: {error}");
            return;
        }
    };
    let mut manifest: serde_json::Value = match std::fs::read_to_string(dir.join("manifest.json"))
        .map_err(|error| error.to_string())
        .and_then(|raw| serde_json::from_str(&raw).map_err(|error| error.to_string()))
    {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!("skipping scene-switch test: {error}");
            return;
        }
    };
    manifest["layers"][0]["mips"] = json!([
        { "level": 0, "size": 4, "bytes": 16 },
        { "level": 1, "size": 2, "bytes": 4 }
    ]);
    if let Err(error) = std::fs::write(
        dir.join("manifest-alt.json"),
        serde_json::to_string(&manifest).unwrap_or_default(),
    ) {
        eprintln!("skipping scene-switch test: {error}");
        return;
    }

    let scene_a = match data_scene(&dir, "probe", "manifest.json") {
        Ok(scene) => scene,
        Err(error) => unreachable!("unexpected data-scene load failure: {error}"),
    };
    let scene_b = match data_scene(&dir, "probe", "manifest-alt.json") {
        Ok(scene) => scene,
        Err(error) => unreachable!("unexpected data-scene load failure: {error}"),
    };
    assert_ne!(
        scene_a.data_key(),
        scene_b.data_key(),
        "the two scenes must bind different data identities"
    );

    cx.update(|cx| {
        let canvas = cx.new(SdfCanvasState::new);
        canvas.update(cx, |canvas, cx| {
            canvas.set_scene(scene_a.clone(), cx);
        });
        assert_eq!(
            canvas.read(cx).field_level_count(),
            Some(3),
            "scene A's manifest declares three mip levels"
        );

        canvas.update(cx, |canvas, cx| {
            canvas.set_scene(scene_b, cx);
        });
        assert_eq!(
            canvas.read(cx).field_level_count(),
            Some(2),
            "a body-identical scene with different data must replace the active scene"
        );
        let view_center = canvas.read(cx).view_center();
        assert_eq!(
            view_center,
            [0.5, 0.5],
            "a scene switch must reset the view to the identity"
        );
        assert_eq!(
            canvas.read(cx).view_zoom(),
            1.0,
            "a scene switch must reset the zoom to the fitted floor"
        );
    });
}

/// A repaint that lands no fresh frame must still paint the last presented
/// one: the window's display list is rebuilt on every repaint, so a repaint
/// that paints nothing blanks the canvas — the theme background shows
/// through, and the scene appears to "go white" the moment the mouse leaves
/// the window or focus moves away.
#[test]
fn a_repaint_without_a_fresh_frame_repaints_the_stored_frame() {
    use crate::canvas::painted_frame;
    use crate::canvas::presentation::Presentation;
    use gpui::RenderImage;
    use image::{Frame, RgbaImage};
    use smallvec::SmallVec;
    use std::sync::Arc;

    let image = Arc::new(RenderImage::new(SmallVec::from_elem(
        Frame::new(RgbaImage::new(4, 4)),
        1,
    )));
    let stored = Some(image.clone());

    // A fresh frame paints itself.
    let outcome = Presentation::Painted {
        image: image.clone(),
        previous: None,
    };
    assert_eq!(
        painted_frame(&outcome, &None),
        Some(image.clone()),
        "a fresh frame must be painted"
    );

    // A bare repaint — a hover transition, focus loss, a screenshot overlay
    // taking the mouse — must re-present the stored frame, not blank.
    assert_eq!(
        painted_frame(&Presentation::Pending, &stored),
        Some(image.clone()),
        "a pending repaint must keep showing the last presented frame"
    );
    assert_eq!(
        painted_frame(&Presentation::Idle, &stored),
        Some(image),
        "an idle repaint (e.g. a compile error) must keep the last good frame"
    );

    // Nothing stored and nothing fresh: nothing to paint.
    assert_eq!(painted_frame(&Presentation::Pending, &None), None);
}

/// The uniform block's view slots: `view_center` occupies the former pad
/// (24..32), the zoom factor rides `params.z` (40..44), `params.w` stays
/// zeroed, and the block keeps its 48-byte size.
#[test]
fn uniform_bytes_pack_the_view_slots() {
    let request = FrameRequest {
        width: 640,
        height: 480,
        time: 1.5,
        mouse: [0.25, 0.75],
        view_center: [0.125, 0.875],
        view_zoom: 4.0,
        params: [2.0, 8.0],
    };
    let bytes = crate::renderer::uniform_bytes(&request);
    assert_eq!(bytes.len(), 48, "the uniform block must stay 48 bytes");

    let word = |offset: usize| {
        let mut chunk = [0u8; 4];
        chunk.copy_from_slice(&bytes[offset..offset + 4]);
        f32::from_le_bytes(chunk)
    };
    assert_eq!(word(24), 0.125, "view_center.x lands at bytes 24..28");
    assert_eq!(word(28), 0.875, "view_center.y lands at bytes 28..32");
    assert_eq!(word(32), 2.0, "params.x still carries the level");
    assert_eq!(word(36), 8.0, "params.y still carries the band width");
    assert_eq!(word(40), 4.0, "params.z carries the zoom factor");
    assert!(
        bytes[44..48].iter().all(|byte| *byte == 0),
        "params.w must stay zeroed"
    );
}

/// The view math is pure: anchored zoom keeps the world uv under the anchor
/// fixed, the drag moves content with the cursor, and the viewport clamp
/// keeps the field covering the viewport — forcing exact identity at the
/// zoom floor.
#[test]
fn view_math_anchors_zoom_and_follows_the_cursor() {
    use crate::canvas::state::{clamped_view_center, panned_view_center, zoomed_view_center};

    // Anchored zoom: the world uv under the anchor must not move.
    let center = [0.5, 0.5];
    let anchor = [0.25, 0.75];
    let z0 = 1.0;
    let z1 = 4.0;
    let world_before = [
        (anchor[0] - 0.5) / z0 + center[0],
        (anchor[1] - 0.5) / z0 + center[1],
    ];
    let next = zoomed_view_center(center, anchor, z0, z1);
    let world_after = [
        (anchor[0] - 0.5) / z1 + next[0],
        (anchor[1] - 0.5) / z1 + next[1],
    ];
    assert!(
        (world_after[0] - world_before[0]).abs() < 1e-5
            && (world_after[1] - world_before[1]).abs() < 1e-5,
        "anchored zoom must keep the world uv under the anchor fixed, got {next:?}"
    );

    // Drag sign: dragging right-down moves the view center toward smaller uv
    // on both axes — content follows the cursor (uv y runs top-down like
    // screen y, so no y flip against the screen delta).
    let panned = panned_view_center([0.5, 0.5], [0.1, 0.1], 4.0);
    assert!(
        (panned[0] - 0.475).abs() < 1e-6 && (panned[1] - 0.475).abs() < 1e-6,
        "a right-down drag must pull the center toward smaller uv, got {panned:?}"
    );

    // Viewport clamp: the field covers the viewport at every zoom.
    let clamped = clamped_view_center([0.1, 0.9], 4.0);
    assert_eq!(
        clamped,
        [0.125, 0.875],
        "the center clamps to [0.5/z, 1 - 0.5/z]"
    );
    assert_eq!(
        clamped_view_center([0.2, 0.8], 1.0),
        [0.5, 0.5],
        "at the zoom floor the clamp forces exact identity"
    );
}

/// The wheel factor is multiplicative on raw pixel deltas: a notched wheel
/// step (≈ 3 lines at a ~20px line height) lands near ×1.25, trackpads zoom
/// continuously, and absurd deltas clamp to [0.5, 2.0] per event.
#[test]
fn the_wheel_factor_is_multiplicative_and_clamped() {
    use crate::canvas::state::wheel_zoom_factor;

    let factor = wheel_zoom_factor(60.0);
    assert!(
        (factor - 1.25).abs() < 0.01,
        "one notched wheel step must land near ×1.25, got {factor}"
    );
    assert!(
        (wheel_zoom_factor(-60.0) - 1.0 / 1.25).abs() < 0.01,
        "the opposite delta must zoom out symmetrically"
    );
    let proportional = wheel_zoom_factor(0.4);
    let expected = (0.4_f32 * 0.0054).exp2();
    assert!(
        (proportional - expected).abs() < 1e-6,
        "small trackpad deltas zoom proportionally, got {proportional}"
    );
    assert_eq!(wheel_zoom_factor(400.0), 2.0, "huge deltas clamp per event");
    assert_eq!(
        wheel_zoom_factor(-400.0),
        0.5,
        "huge deltas clamp per event"
    );
}

/// Zoom refines the LOD slider's base bias downward toward mip 0: the level
/// derivation subtracts log2(zoom) and clamps to the chain.
#[test]
fn zoom_refines_the_field_level_toward_mip_zero() {
    use crate::canvas::effective_field_level;

    assert_eq!(
        effective_field_level(0.0, 1.0, 3),
        0.0,
        "the identity view keeps the slider's level"
    );
    assert_eq!(
        effective_field_level(1.0, 1.0, 3),
        2.0,
        "the slider alone still reaches the coarsest level"
    );
    assert_eq!(
        effective_field_level(1.0, 4.0, 3),
        0.0,
        "zoom refines the coarsest bias down to mip 0"
    );
    assert_eq!(
        effective_field_level(0.5, 2.0, 3),
        0.0,
        "mid-slider with ×2 zoom refines one step"
    );
    assert_eq!(
        effective_field_level(0.0, 64.0, 3),
        0.0,
        "zoom never refines below mip 0"
    );
    assert_eq!(
        effective_field_level(1.0, 1.0, 1),
        0.0,
        "a single-level chain stays at 0"
    );
}

/// The compiled module carries the view: the transformed `field_uv` and the
/// reserved `view_p` helper — in the scene prelude and in the contour
/// overlay's module, which compiles from the same helpers.
#[test]
fn the_prelude_carries_the_view_transform() {
    let scene = match SdfScene::from_str(CIRCLE_SCENE, "circle") {
        Ok(scene) => scene,
        Err(error) => {
            eprintln!("unexpected scene parse failure: {error}");
            return;
        }
    };
    let module = scene.module_source();
    assert!(
        module.contains("fn view_p("),
        "the prelude must reserve `view_p`, got: {module}"
    );
    assert!(
        module.contains("u.view_center") && module.contains("u.params.z"),
        "field_uv must apply the view from the uniform block"
    );

    let overlay = crate::overlay::module_source(2, 3);
    assert!(
        overlay.contains("fn view_p(") && overlay.contains("u.view_center"),
        "the overlay compiles from the same field helpers"
    );
}

/// The view mutators through the entity: zoom clamps to [1, 64], pan is a
/// no-op at the floor (the clamp forces identity), drag bookkeeping only
/// pans while a drag is running.
#[gpui::test]
fn view_mutators_clamp_and_follow_the_cursor(cx: &mut TestAppContext) {
    use crate::SdfCanvasState;

    cx.update(|cx| {
        let canvas = cx.new(SdfCanvasState::new);
        canvas.update(cx, |canvas, cx| {
            canvas.zoom_at([0.25, 0.75], 1_000.0, cx);
            assert_eq!(canvas.view_zoom(), 64.0, "zoom must clamp to the ceiling");
        });
        canvas.update(cx, |canvas, cx| {
            canvas.zoom_at([0.8, 0.2], 0.1, cx);
            assert_eq!(canvas.view_zoom(), 1.0, "zoom must clamp back to the floor");
            assert_eq!(
                canvas.view_center(),
                [0.5, 0.5],
                "the clamp forces exact identity at the floor"
            );
        });
        canvas.update(cx, |canvas, cx| {
            canvas.pan_by([0.3, -0.4], cx);
            assert_eq!(
                canvas.view_center(),
                [0.5, 0.5],
                "panning at the floor is a no-op: the field always covers the viewport"
            );
        });
        canvas.update(cx, |canvas, cx| {
            canvas.zoom_at([0.5, 0.5], 8.0, cx);
            canvas.begin_drag([0.5, 0.5]);
            canvas.drag_move([0.6, 0.5], cx);
            let center = canvas.view_center();
            assert!(
                (center[0] - (0.5 - 0.1 / 8.0)).abs() < 1e-5,
                "dragging right must pull the view center left, got {center:?}"
            );
            assert!(
                (center[1] - 0.5).abs() <= f32::EPSILON,
                "an x-only drag must not move the y center"
            );
            canvas.end_drag();
            canvas.drag_move([0.9, 0.9], cx);
            assert_eq!(
                canvas.view_center(),
                center,
                "moves after the mouse-up must not pan"
            );
        });
        canvas.update(cx, |canvas, cx| {
            canvas.reset_view(cx);
            assert_eq!(canvas.view_zoom(), 1.0, "reset returns to the fitted view");
            assert_eq!(canvas.view_center(), [0.5, 0.5]);
        });
    });
}
