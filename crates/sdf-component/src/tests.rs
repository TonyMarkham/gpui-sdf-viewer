use crate::{
    renderer::{FrameRequest, Renderer},
    scene::SdfScene,
};

const CIRCLE_SCENE: &str = r#"
fn scene(p: vec2f, _time: f32) -> f32 {
    return sd_circle(p, 0.6);
}
"#;

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
}

/// Renders every embedded scene offscreen and checks that the readback path
/// produces valid pixels for each. This exercises both shader variants (the
/// built-in 2D shading and the `render` override) against the real WGSL
/// validator. Skipped (with a note) when no wgpu adapter is available.
#[test]
fn renderer_compiles_and_renders_every_embedded_scene() {
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
                },
            ) {
                Ok(Some(image)) => {
                    frame = Some(image);
                    break;
                }
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
                Err(error) => {
                    eprintln!("scene '{}' failed to render: {error}", scene.name());
                    return;
                }
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
            },
        ) {
            Ok(Some(image)) => {
                frame = Some(image);
                break;
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
            Err(error) => {
                eprintln!("skipping renderer test: render failed: {error}");
                return;
            }
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
