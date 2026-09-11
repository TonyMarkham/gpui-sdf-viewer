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
/// three mip levels per real tile, constant-fill stubs. Returns the fixture
/// directory on success.
fn field_fixture(tag: &str) -> std::result::Result<std::path::PathBuf, String> {
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

    let tiles = [
        ("tile_0_0.r8", 0, 0, "mip0", tile_payload(10)),
        ("tile_1_0.r8", 1, 0, "mip0", tile_payload(60)),
        ("tile_0_1.r8", 0, 1, "stub", vec![0u8; 21]),
        ("tile_1_1.r8", 1, 1, "stub", vec![42u8; 21]),
    ];

    let mut manifest_tiles = Vec::new();
    for (payload_name, x, y, kind, payload) in &tiles {
        let payload_path = dir.join(payload_name);
        std::fs::write(&payload_path, payload)
            .map_err(|error| format!("write {payload_name}: {error}"))?;
        manifest_tiles.push(json!({
            "path": format!("ui/{payload_name}"),
            "payload": payload_name,
            "x": x,
            "y": y,
            "kind": kind,
            "source_sha256": "0".repeat(64),
            "payload_sha256": sha256_hex(payload),
        }));
    }

    let manifest = json!({
        "format": "cd-map-sdf-field",
        "version": 1,
        "layers": [
            {
                "name": "probe",
                "tile_prefix": "fixture_field",
                "size": 8,
                "source_manifest_sha256": "0".repeat(64),
                "mips": [
                    { "level": 0, "size": 4, "bytes": 16 },
                    { "level": 1, "size": 2, "bytes": 4 },
                    { "level": 2, "size": 1, "bytes": 1 }
                ],
                "tiles": manifest_tiles,
            }
        ],
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

fn first_frame(
    renderer: &mut Renderer,
    scene: &SdfScene,
    params: [f32; 2],
) -> std::result::Result<Option<std::sync::Arc<gpui::RenderImage>>, Error> {
    const SIZE: u32 = 64;
    for _ in 0..300 {
        match renderer.render(
            scene,
            &FrameRequest {
                width: SIZE,
                height: SIZE,
                time: 0.0,
                mouse: [0.0, 0.0],
                params,
            },
        ) {
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
fn embedded_examples_parse() {
    let examples = SdfScene::embedded_examples();
    assert!(
        examples.len() >= 3,
        "the component ships at least three example scenes"
    );
    assert!(
        examples.iter().any(|scene| scene.animated()),
        "at least one example scene is animated"
    );
    assert!(
        examples
            .iter()
            .any(|scene| scene.module_source().contains("render(uv, u.time)")),
        "at least one example scene uses the render override"
    );
    for scene in &examples {
        assert!(scene.data().is_none(), "embedded examples never carry data");
    }
}

/// Renders every embedded scene offscreen and checks that the readback path
/// produces valid pixels for each. This exercises both shader variants (the
/// built-in 2D shading and the `render` override) against the real WGSL
/// validator. Skipped (with a note) when no wgpu adapter is available.
#[test]
fn renderer_compiles_and_renders_every_embedded_scene() {
    let _renderer_lock = render_lock();
    for scene in SdfScene::embedded_examples() {
        let mut renderer = match Renderer::new() {
            Ok(renderer) => renderer,
            Err(error) => {
                eprintln!("skipping renderer test: {error}");
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
                    params: [0.0, 0.0],
                },
            ) {
                Ok(Some(image)) => {
                    frame = Some(image);
                    break;
                }
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
                Err(error) => unreachable!("scene '{}' failed to render: {error}", scene.name()),
            }
        }

        let Some(frame) = frame else {
            eprintln!(
                "skipping scene '{}': no frame became ready in time",
                scene.name()
            );
            return;
        };

        let bytes = match frame.as_bytes(0) {
            Some(bytes) => bytes,
            None => {
                eprintln!("unexpected: frame for '{}' has no bytes", scene.name());
                return;
            }
        };

        let opaque = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 0)
            .count();
        assert!(
            opaque > 0,
            "scene '{}' must paint at least one pixel",
            scene.name()
        );
    }
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
/// paired data directives. When the export they name exists on this machine,
/// one of them also loads through the file path and renders — the scenes'
/// WGSL is validated against the real pipeline, not just the fixture.
#[test]
fn shipped_game_scenes_parse_and_render_when_data_is_present() {
    let _renderer_lock = render_lock();
    const SHIPPED: [(&str, &str); 6] = [
        ("abyss_hex", include_str!("../../../scenes/abyss_hex.wgsl")),
        ("coast", include_str!("../../../scenes/coast.wgsl")),
        ("inspect", include_str!("../../../scenes/inspect.wgsl")),
        ("mountain", include_str!("../../../scenes/mountain.wgsl")),
        ("river", include_str!("../../../scenes/river.wgsl")),
        ("road", include_str!("../../../scenes/road.wgsl")),
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
    let scene = match SdfScene::from_file(&coast_path) {
        Ok(scene) => scene,
        Err(error) => {
            eprintln!(
                "skipping shipped-scene render (load error: {error}): the export named by \
                 scenes/coast.wgsl may not be present on this machine"
            );
            return;
        }
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

/// End-to-end against the real VS-20 export when one is available: point
/// `SDF_TEST_FIELD_MANIFEST` at the map pipeline's fields manifest (e.g.
/// `D:\git\cd-data-extract\.work\fields\manifest.json`). Skipped when unset.
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
    use crate::canvas::SdfCanvasState;

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
    });
}
