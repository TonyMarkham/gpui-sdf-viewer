use gpui::{
    Entity, Modifiers, MouseButton, Point, ScrollDelta, ScrollWheelEvent, Size, TestAppContext,
    TouchPhase, VisualTestContext, px,
};
use gpui_component::ActiveTheme as _;
use gpui_component::slider::{SliderEvent, SliderValue};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::time::Duration;

use crate::state::offline::{ExtractionEvent, OfflineState, drain_progress};

mod app_fixture;
mod app_host;

use self::app_fixture::fixture;

// ------------------------------------------------------------------------------------------ //

const TEST_INSTALL_DIR: &str = "install";
const TEST_WORK_DIR: &str = "work";
const TEST_SCENES_DIR: &str = "scenes";
const TEST_PACK: &str = "0012";
const TEST_PAMT: &str = "0.pamt";
const TEST_SCENE_A: &str = "// sdf-scene: name = \"Probe A\"\nfn scene(p: vec2f, _time: f32) -> f32 {\n    return sd_circle(p, 0.5);\n}\n";
const TEST_SCENE_B: &str = "// sdf-scene: name = \"Probe B\"\nfn scene(p: vec2f, _time: f32) -> f32 {\n    return sd_box(p, vec2f(0.3, 0.3));\n}\n";
const TEST_DATA_SCENE: &str = "// sdf-scene: name = \"Config Probe\"\n// sdf-scene: data = \"config:sdf/manifest.json\"\n// sdf-scene: layer = \"probe\"\nfn render(p: vec2f, _time: f32) -> vec4f {\n    let raw = field_raw(p, u.params.x);\n    return vec4f(vec3f(raw), 1.0);\n}\n";
const TEST_DATA_SCENE_UNQUOTED: &str = "// sdf-scene: name = \"Config Probe\"\n// sdf-scene: data = config:sdf/manifest.json\n// sdf-scene: layer = \"probe\"\nfn render(p: vec2f, _time: f32) -> vec4f {\n    let raw = field_raw(p, u.params.x);\n    return vec4f(vec3f(raw), 1.0);\n}\n";
const TEST_DRAINED_PROGRESS: &str = "extract · probe · pack scanned";

/// Fixture I/O helpers: the workspace lints deny `panic!`/`unwrap`/`expect`,
/// so failures fail loudly through `assert!` instead.
fn ensure_dir(path: &std::path::Path) {
    let created = std::fs::create_dir_all(path);
    assert!(
        created.is_ok(),
        "create `{}` failed: {created:?}",
        path.display()
    );
}

fn write_bytes(path: &std::path::Path, contents: &[u8]) {
    let written = std::fs::write(path, contents);
    assert!(
        written.is_ok(),
        "write `{}` failed: {written:?}",
        path.display()
    );
}

fn write_file(path: &std::path::Path, contents: &str) {
    write_bytes(path, contents.as_bytes())
}

fn redraw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
}

fn bounds(
    cx: &mut VisualTestContext,
    selector: &'static str,
) -> Option<gpui::Bounds<gpui::Pixels>> {
    cx.debug_bounds(selector)
}

/// Clicks the debug-selected button, returning false when it is not on
/// screen. The click lands on the bounds' center.
fn click_button(cx: &mut VisualTestContext, selector: &'static str) -> bool {
    redraw(cx);
    let Some(found) = bounds(cx, selector) else {
        return false;
    };
    let center = Point::new(
        found.origin.x + found.size.width / 2.0,
        found.origin.y + found.size.height / 2.0,
    );
    cx.simulate_click(center, Modifiers::none());
    true
}

// ------------------------------------------------------------------------------------------ //

#[gpui::test]
fn initialize_pins_the_list_selection_theme(cx: &mut TestAppContext) {
    cx.update(|cx| {
        assert!(crate::initialize(cx).is_ok());

        let theme = cx.theme();
        assert_eq!(theme.list_active, theme.accent);
        assert_eq!(theme.list_active_border, gpui::transparent_black());
    });
}

/// The operator's first-run flow in the suite's only window: the setup panel
/// is the first view, "Extract now" runs the real pipeline composition and
/// surfaces its failure, the picker pick saves the config and hands over to
/// the shell, a scene loads into the canvas, the window survives a resize,
/// and the navigation "game-folder" button reopens the picker.
#[gpui::test]
fn the_first_run_flow_renders_setup_then_the_shell(cx: &mut TestAppContext) {
    let fixture = fixture("flow", cx);
    write_file(&fixture.scenes.join("probe_a.wgsl"), TEST_SCENE_A);
    write_file(&fixture.scenes.join("probe_b.wgsl"), TEST_SCENE_B);
    // Discovered at the post-extraction refresh; loads once the config
    // manifest exists (written below, before the shell comes up). The view
    // assertions select it (last alphabetically, so the fixture's scene
    // indices above stay stable) because only a data scene's canvas takes
    // the wheel and the drag.
    write_file(
        &fixture.scenes.join("zz_config_probe.wgsl"),
        TEST_DATA_SCENE,
    );
    let work = fixture.root.join(TEST_WORK_DIR);
    let manifest = work.join("sdf").join("manifest.json");
    let removed = std::fs::remove_file(&manifest);
    assert!(
        removed.is_ok(),
        "remove the fixture manifest failed: {removed:?}"
    );

    let config_path = fixture.root.join("config.toml");
    let mut config = sdf_offline::Config::defaults();
    config.paths.game_root = fixture.root.join(TEST_INSTALL_DIR).display().to_string();
    config.paths.data_dir = work.display().to_string();
    let offline = OfflineState::from_config(config).with_config_path(config_path.clone());
    let (state, cx) = fixture.app_with(offline, cx);
    redraw(cx);

    for selector in ["setup-panel", "status-bar"] {
        let found = bounds(cx, selector);
        assert!(
            found.is_some(),
            "the first run must render {selector} from the setup panel"
        );
    }
    assert!(
        bounds(cx, "canvas-host").is_none(),
        "the canvas must not render while setup is incomplete"
    );
    assert!(
        bounds(cx, "navigation-scenes").is_none(),
        "the scene list must not render while setup is incomplete"
    );

    let status = crate::component::capture::status_line();
    assert!(
        status.contains(crate::constants::STATUS_GAME_DATA_MISSING),
        "the first-run status line must read as missing game data, got: {status}"
    );
    let panel = crate::component::capture::setup_panel_lines();
    assert!(
        panel.iter().any(|line| {
            line.contains(TEST_INSTALL_DIR)
                && line.starts_with(crate::constants::SETUP_FOLDER_LABEL)
        }),
        "the setup panel must show the configured game folder, got: {panel:?}"
    );

    let clicked = click_button(cx, crate::constants::SETUP_EXTRACT_BUTTON_ID);
    assert!(clicked, "the setup panel must render the extract button");
    let outcome = wait_for_extraction(&state, cx);
    assert!(
        outcome,
        "the extraction started by the click must end within the bounded wait"
    );
    let failed = cx.update(|_, cx| {
        state
            .read(cx)
            .offline()
            .read(cx)
            .extraction_failed()
            .is_some()
    });
    assert!(
        failed,
        "the stub install must fail the pipeline through the drain"
    );

    redraw(cx);
    let status = crate::component::capture::status_line();
    assert!(
        status.contains(crate::constants::STATUS_EXTRACTION_FAILED),
        "the failed extraction must reach the rendered status line, got: {status}"
    );
    let panel = crate::component::capture::setup_panel_lines();
    assert!(
        panel
            .iter()
            .any(|line| line.starts_with(crate::constants::SETUP_FAILURE_PREFIX)),
        "the setup panel must surface the extraction failure, got: {panel:?}"
    );

    write_config_manifest(&work);
    let picked = click_button(cx, crate::constants::SETUP_PICK_BUTTON_ID);
    assert!(picked, "the setup panel must render the picker button");
    assert!(
        cx.did_prompt_for_paths(),
        "the picker button must open the platform prompt"
    );
    let install = fixture.root.join(TEST_INSTALL_DIR);
    cx.simulate_path_prompt_response(move |_| Some(vec![install.clone()]));
    cx.run_until_parked();

    let needs_setup = cx.update(|_, cx| state.read(cx).needs_setup(cx));
    assert!(!needs_setup, "a valid pick with data present leaves setup");
    assert!(
        config_path.exists(),
        "the pick must save the config through the injected path"
    );

    redraw(cx);
    for selector in [
        "canvas-host",
        "sdf-canvas-root",
        "control-bar",
        "status-bar",
        "navigation-scenes",
    ] {
        let found = bounds(cx, selector);
        assert!(found.is_some(), "the shell must render {selector}");
    }
    let status = crate::component::capture::status_line();
    assert!(
        status.contains(crate::constants::STATUS_GAME_FOLDER_OK),
        "the shell status line must read the game folder as ok, got: {status}"
    );
    let host = match bounds(cx, "canvas-host") {
        Some(host) => host,
        None => unreachable!("canvas-host was rendered above"),
    };
    assert!(
        host.size.height > px(100.0),
        "the canvas area should absorb the window body, got {host:?}"
    );

    cx.update(|_, cx| {
        state.update(cx, |state, cx| state.select(1, cx));
    });
    redraw(cx);
    let scene_after = cx.update(|_, cx| {
        state
            .read(cx)
            .canvas()
            .read(cx)
            .scene_name()
            .map(ToString::to_string)
    });
    assert_eq!(
        scene_after,
        Some(String::from("Probe B")),
        "selection must load the picked scene into the canvas"
    );

    cx.simulate_resize(Size {
        width: px(1024.0),
        height: px(640.0),
    });
    redraw(cx);
    let host = bounds(cx, "canvas-host");
    assert!(host.is_some(), "the canvas survives a resize");

    // The map view, driven through the canvas element's real input handlers:
    // wheel zooms about the cursor, left-drag pans, the 1:1 control resets,
    // and a scene switch re-fits. The control bar's zoom readout tracks the
    // canvas state.
    redraw(cx);
    let bar = crate::component::capture::control_bar_lines();
    assert!(
        bar.iter().any(|line| line == "1.0×"),
        "the control bar must read the fitted view as 1.0×, got: {bar:?}"
    );
    assert!(
        bar.iter().any(|line| line.contains("1:1 disabled")),
        "the 1:1 control must be disabled while the scene carries no data, got: {bar:?}"
    );

    // The has_data policy through the real handlers: on a non-data scene the
    // wheel and a left-drag must leave the map view alone, so the zoom
    // readout cannot move while the 1:1 control stays disabled. The view is
    // zoomed through the public mutator first — the wheel itself is under
    // test, and at the identity view a drag pans nothing (the clamp forces
    // exact identity at the zoom floor), so only a zoomed view makes a
    // deleted gate observable.
    cx.update(|_, cx| {
        state.update(cx, |state, cx| {
            state
                .canvas()
                .update(cx, |canvas, cx| canvas.zoom_at([0.75, 0.75], 4.0, cx));
        });
    });
    let (zoomed_center, zoomed_zoom) = cx.update(|_, cx| {
        let canvas = state.read(cx).canvas().read(cx);
        (canvas.view_center(), canvas.view_zoom())
    });
    let Some(gate_bounds) = bounds(cx, "sdf-canvas-root") else {
        unreachable!("the canvas renders in the shell");
    };
    let gate_center = Point::new(
        gate_bounds.origin.x + gate_bounds.size.width / 2.0,
        gate_bounds.origin.y + gate_bounds.size.height / 2.0,
    );
    cx.simulate_event(ScrollWheelEvent {
        position: gate_center,
        delta: ScrollDelta::Lines(Point { x: 0.0, y: 3.0 }),
        modifiers: Modifiers::none(),
        touch_phase: TouchPhase::Moved,
    });
    let zoom_after_wheel = cx.update(|_, cx| state.read(cx).canvas().read(cx).view_zoom());
    assert_eq!(
        zoom_after_wheel, zoomed_zoom,
        "a wheel event on a non-data scene must not zoom, got {zoom_after_wheel}"
    );
    cx.simulate_mouse_down(gate_center, MouseButton::Left, Modifiers::none());
    let gate_dragged = Point::new(gate_center.x + px(24.0), gate_center.y);
    cx.simulate_mouse_move(gate_dragged, None, Modifiers::none());
    let center_after_drag = cx.update(|_, cx| state.read(cx).canvas().read(cx).view_center());
    assert_eq!(
        center_after_drag, zoomed_center,
        "a left-drag on a non-data scene must not pan, got {center_after_drag:?}"
    );
    cx.simulate_mouse_up(gate_dragged, MouseButton::Left, Modifiers::none());
    cx.update(|_, cx| {
        state.update(cx, |state, cx| {
            state
                .canvas()
                .update(cx, |canvas, cx| canvas.reset_view(cx));
        });
    });

    cx.update(|_, cx| {
        state.update(cx, |state, cx| state.select(2, cx));
    });
    redraw(cx);
    let has_data = cx.update(|_, cx| state.read(cx).canvas().read(cx).has_data());
    assert!(has_data, "the config scene must load as a data scene");
    let bar = crate::component::capture::control_bar_lines();
    assert!(
        !bar.iter().any(|line| line.contains("disabled")),
        "the 1:1 control must enable once the scene carries data, got: {bar:?}"
    );

    let Some(canvas_bounds) = bounds(cx, "sdf-canvas-root") else {
        unreachable!("the canvas renders in the shell");
    };
    let canvas_center = Point::new(
        canvas_bounds.origin.x + canvas_bounds.size.width / 2.0,
        canvas_bounds.origin.y + canvas_bounds.size.height / 2.0,
    );

    cx.simulate_event(ScrollWheelEvent {
        position: canvas_center,
        delta: ScrollDelta::Lines(Point { x: 0.0, y: 3.0 }),
        modifiers: Modifiers::none(),
        touch_phase: TouchPhase::Moved,
    });
    let zoom = cx.update(|_, cx| state.read(cx).canvas().read(cx).view_zoom());
    assert!(zoom > 1.0, "a wheel-up step must zoom in, got {zoom}");
    redraw(cx);
    let bar = crate::component::capture::control_bar_lines();
    let zoom_label = format!("{zoom:.1}×");
    assert!(
        bar.contains(&zoom_label),
        "the zoom readout must track view_zoom(), got: {bar:?}"
    );

    cx.simulate_mouse_down(canvas_center, MouseButton::Left, Modifiers::none());
    let dragged = Point::new(canvas_center.x + px(24.0), canvas_center.y);
    cx.simulate_mouse_move(dragged, None, Modifiers::none());
    let center = cx.update(|_, cx| state.read(cx).canvas().read(cx).view_center());
    assert!(
        center[0] < 0.5,
        "dragging right must pull the view center left (content follows the cursor), got {center:?}"
    );
    assert!(
        (center[1] - 0.5).abs() <= 1e-4,
        "a horizontal drag must not move the y center, got {center:?}"
    );
    cx.simulate_mouse_up(dragged, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        Point::new(dragged.x - px(48.0), dragged.y),
        None,
        Modifiers::none(),
    );
    let center_after_up = cx.update(|_, cx| state.read(cx).canvas().read(cx).view_center());
    assert_eq!(
        center_after_up, center,
        "moving after the mouse-up must not pan"
    );

    // The bounds-edge policies: moves outside the canvas keep panning while
    // a drag runs (clamped), and a release outside the canvas still ends it.
    cx.simulate_mouse_down(canvas_center, MouseButton::Left, Modifiers::none());
    let Some(bar_bounds) = bounds(cx, "control-bar") else {
        unreachable!("the control bar renders in the shell");
    };
    let outside = Point::new(
        bar_bounds.origin.x + bar_bounds.size.width / 2.0,
        bar_bounds.origin.y + bar_bounds.size.height / 2.0,
    );
    let fraction_outside = [
        f32::from(outside.x - canvas_bounds.origin.x) / f32::from(canvas_bounds.size.width),
        f32::from(outside.y - canvas_bounds.origin.y) / f32::from(canvas_bounds.size.height),
    ];
    assert!(
        !(0.0..=1.0).contains(&fraction_outside[0]) || !(0.0..=1.0).contains(&fraction_outside[1]),
        "the control bar must sit outside the canvas for this probe, got {fraction_outside:?}"
    );
    cx.simulate_mouse_move(outside, None, Modifiers::none());
    let center_outside = cx.update(|_, cx| state.read(cx).canvas().read(cx).view_center());
    assert_ne!(
        center_outside, center,
        "a move outside the canvas must keep panning while the drag runs, got {center_outside:?}"
    );
    cx.simulate_mouse_up(outside, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(canvas_center, None, Modifiers::none());
    let center_after_outside_up = cx.update(|_, cx| state.read(cx).canvas().read(cx).view_center());
    assert_eq!(
        center_after_outside_up, center_outside,
        "a release outside the canvas must end the drag"
    );

    let clicked = click_button(cx, crate::constants::CONTROL_VIEW_RESET_BUTTON_ID);
    assert!(clicked, "the control bar must render the 1:1 control");
    let (center, zoom) = cx.update(|_, cx| {
        let canvas = state.read(cx).canvas().read(cx);
        (canvas.view_center(), canvas.view_zoom())
    });
    assert_eq!(zoom, 1.0, "the 1:1 control must reset the zoom");
    assert_eq!(center, [0.5, 0.5], "the 1:1 control must reset the center");

    // The zoom-aware LOD derivation through its real call site: the submitted
    // level must follow both the slider's base bias and the view zoom.
    cx.update(|_, cx| {
        state.update(cx, |state, cx| {
            state
                .canvas()
                .update(cx, |canvas, cx| canvas.set_field_level(1.0, cx));
        });
    });
    redraw(cx);
    let submitted = cx.update(|_, cx| state.read(cx).canvas().read(cx).submitted_field_level());
    if let Some(level) = submitted {
        let levels = cx.update(|_, cx| state.read(cx).canvas().read(cx).field_level_count());
        let max = levels.map_or(0.0, |levels| (levels.max(1) - 1) as f32);
        assert_eq!(
            level, max,
            "the LOD slider at 1.0 must submit the chain's coarsest level"
        );
        cx.update(|_, cx| {
            state.update(cx, |state, cx| {
                state
                    .canvas()
                    .update(cx, |canvas, cx| canvas.zoom_at([0.75, 0.75], 4.0, cx));
            });
        });
        redraw(cx);
        let refined = cx.update(|_, cx| state.read(cx).canvas().read(cx).submitted_field_level());
        let expected = (max - 4.0f32.log2()).round().clamp(0.0, max);
        assert_eq!(
            refined,
            Some(expected),
            "zooming must refine the submitted level below the slider's bias"
        );
    } else {
        eprintln!("skipping the LOD call-site assertion: the renderer never submitted a frame");
    }

    cx.simulate_event(ScrollWheelEvent {
        position: canvas_center,
        delta: ScrollDelta::Lines(Point { x: 0.0, y: 3.0 }),
        modifiers: Modifiers::none(),
        touch_phase: TouchPhase::Moved,
    });
    let zoom = cx.update(|_, cx| state.read(cx).canvas().read(cx).view_zoom());
    assert!(
        zoom > 1.0,
        "the second wheel step must zoom in again, got {zoom}"
    );

    // Resize stability: the view lives in uv fractions, so resizing the
    // window must not move it.
    let (center_before, _) = cx.update(|_, cx| {
        let canvas = state.read(cx).canvas().read(cx);
        (canvas.view_center(), canvas.view_zoom())
    });
    cx.simulate_mouse_down(canvas_center, MouseButton::Left, Modifiers::none());
    let panned_to = Point::new(canvas_center.x + px(30.0), canvas_center.y);
    cx.simulate_mouse_move(panned_to, None, Modifiers::none());
    let (center, zoom) = cx.update(|_, cx| {
        let canvas = state.read(cx).canvas().read(cx);
        (canvas.view_center(), canvas.view_zoom())
    });
    assert_ne!(
        center, center_before,
        "the pre-resize drag must move the view, got {center:?}"
    );
    cx.simulate_resize(Size {
        width: px(900.0),
        height: px(700.0),
    });
    redraw(cx);
    let (center_after, zoom_after) = cx.update(|_, cx| {
        let canvas = state.read(cx).canvas().read(cx);
        (canvas.view_center(), canvas.view_zoom())
    });
    assert_eq!(
        center_after, center,
        "a window resize must not move the view center"
    );
    assert_eq!(zoom_after, zoom, "a window resize must not change the zoom");
    cx.simulate_mouse_up(panned_to, MouseButton::Left, Modifiers::none());

    cx.update(|_, cx| {
        state.update(cx, |state, cx| state.select(1, cx));
    });
    redraw(cx);
    let (center, zoom) = cx.update(|_, cx| {
        let canvas = state.read(cx).canvas().read(cx);
        (canvas.view_center(), canvas.view_zoom())
    });
    assert_eq!(
        zoom, 1.0,
        "a scene switch must reset the zoom to the fitted view"
    );
    assert_eq!(center, [0.5, 0.5], "a scene switch must reset the center");
    let bar = crate::component::capture::control_bar_lines();
    assert!(
        bar.iter().any(|line| line == "1.0×") && bar.iter().any(|line| line.contains("disabled")),
        "the no-data scene must read 1.0× with the 1:1 control disabled, got: {bar:?}"
    );

    let reopened = click_button(cx, crate::constants::NAVIGATION_GAME_FOLDER_BUTTON_ID);
    assert!(
        reopened,
        "the navigation must render the game-folder button"
    );
    assert!(
        cx.did_prompt_for_paths(),
        "the game-folder button must reopen the platform prompt"
    );
    let install = fixture.root.join(TEST_INSTALL_DIR);
    cx.simulate_path_prompt_response(move |_| Some(vec![install.clone()]));
    cx.run_until_parked();
    let needs_setup = cx.update(|_, cx| state.read(cx).needs_setup(cx));
    assert!(!needs_setup, "a re-pick of a valid folder keeps the shell");
}

#[gpui::test]
fn out_of_range_scene_selection_is_a_no_op(cx: &mut TestAppContext) {
    let fixture = fixture("noop", cx);
    write_file(&fixture.scenes.join("probe_a.wgsl"), TEST_SCENE_A);

    let state = fixture.app_state_with_offline(fixture.offline(), cx);

    cx.update(|cx| {
        state.update(cx, |state, cx| state.select(usize::MAX, cx));
    });

    let (active, load_error) = cx.update(|cx| {
        let state = state.read(cx);
        (state.active(), state.load_error().map(ToString::to_string))
    });
    assert!(active.is_some(), "the startup selection survives a no-op");
    assert!(
        load_error.is_none(),
        "a no-op selection must not set an error"
    );
}

#[gpui::test]
fn contour_toggle_drives_the_canvas_band(cx: &mut TestAppContext) {
    let fixture = fixture("contour", cx);
    let state = fixture.app_state_with_offline(fixture.offline(), cx);

    let (enabled, band) = cx.update(|cx| {
        let app = state.read(cx);
        (app.contour_enabled(), app.canvas().read(cx).contour_band())
    });
    assert!(!enabled, "the overlay starts off");
    assert_eq!(band, 0.0, "the overlay starts with a zero band width");

    cx.update(|cx| {
        state.update(cx, |state, cx| state.toggle_contour(cx));
    });

    let (enabled, band, has_data) = cx.update(|cx| {
        let app = state.read(cx);
        (
            app.contour_enabled(),
            app.canvas().read(cx).contour_band(),
            app.canvas().read(cx).has_data(),
        )
    });
    assert!(enabled, "toggling switches the overlay on");
    assert!(
        band > 0.0,
        "toggling on binds the density slider's band width"
    );
    assert!(
        !has_data,
        "the startup scene (no scenes) carries no game data"
    );
}

/// The LOD and density sliders drive the canvas through the App's slider
/// subscriptions: a level change maps onto the 0..1 level position (clamped),
/// and a density change updates the band width while the overlay is on.
#[gpui::test]
fn field_level_and_density_sliders_drive_the_canvas(cx: &mut TestAppContext) {
    let fixture = fixture("sliders", cx);
    let state = fixture.app_state_with_offline(fixture.offline(), cx);

    let level = cx.update(|cx| state.read(cx).level_slider().clone());
    let density = cx.update(|cx| state.read(cx).density_slider().clone());

    cx.update(|cx| {
        level.update(cx, |_, cx| {
            cx.emit(SliderEvent::Change(SliderValue::from(0.5_f32)));
        });
    });
    let field_level = cx.update(|cx| state.read(cx).canvas().read(cx).field_level());
    assert_eq!(
        field_level, 0.5,
        "the LOD slider must drive the canvas level"
    );

    cx.update(|cx| {
        level.update(cx, |_, cx| {
            cx.emit(SliderEvent::Change(SliderValue::from(1.5_f32)));
        });
    });
    let field_level = cx.update(|cx| state.read(cx).canvas().read(cx).field_level());
    assert_eq!(
        field_level, 1.0,
        "out-of-range slider values must clamp into 0..1"
    );

    cx.update(|cx| {
        state.update(cx, |state, cx| state.toggle_contour(cx));
    });
    cx.update(|cx| {
        density.update(cx, |_, cx| {
            cx.emit(SliderEvent::Change(SliderValue::from(6.0_f32)));
        });
    });
    let band = cx.update(|cx| state.read(cx).canvas().read(cx).contour_band());
    assert_eq!(
        band, 6.0,
        "the density slider must drive the canvas band width"
    );
}

/// A scene whose `data` header uses the `config:` scheme loads through the
/// rewritten copy: the manifest under the app's data directory is found,
/// validated, and its payloads verified — the full first-run path after
/// extraction.
#[test]
fn a_config_scene_resolves_through_the_app_config() {
    let root = std::env::temp_dir().join(format!("sdf-ui-config-scene-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let scenes = root.join("scenes");
    let data_dir = root.join("data");
    ensure_dir(&scenes);
    ensure_dir(&data_dir.join("sdf"));

    let scene_path = scenes.join("config_probe.wgsl");
    write_file(&scene_path, TEST_DATA_SCENE);

    write_config_manifest(&data_dir);

    let library = crate::state::scene::SceneLibrary::discover_in(&scenes);
    assert_eq!(library.len(), 1, "the fixture scene must be discoverable");

    let scene = library.load(0, &data_dir);
    let _ = std::fs::remove_dir_all(&root);
    let scene = match scene {
        Ok(scene) => scene,
        Err(error) => unreachable!("the config scene must load through the app config: {error}"),
    };
    assert!(scene.data_requested(), "the loaded scene must request data");
    assert_eq!(scene.name(), "Config Probe");
}

/// The same load through the unquoted form the component's header parser
/// accepts: the rewrite must replace the bare value, and the staged copy
/// must resolve against the app's data directory.
#[test]
fn an_unquoted_config_scene_resolves_through_the_app_config() {
    let root = std::env::temp_dir().join(format!("sdf-ui-config-unquoted-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let scenes = root.join("scenes");
    let data_dir = root.join("data");
    ensure_dir(&scenes);
    ensure_dir(&data_dir.join("sdf"));

    let scene_path = scenes.join("config_unquoted.wgsl");
    write_file(&scene_path, TEST_DATA_SCENE_UNQUOTED);

    write_config_manifest(&data_dir);

    let library = crate::state::scene::SceneLibrary::discover_in(&scenes);
    assert_eq!(library.len(), 1, "the fixture scene must be discoverable");

    let scene = library.load(0, &data_dir);
    let _ = std::fs::remove_dir_all(&root);
    let scene = match scene {
        Ok(scene) => scene,
        Err(error) => {
            unreachable!("the unquoted config scene must load through the app config: {error}")
        }
    };
    assert!(scene.data_requested(), "the loaded scene must request data");
    assert_eq!(scene.name(), "Config Probe");
}

#[test]
fn an_unknown_data_scheme_is_a_named_scene_error() {
    let root = std::env::temp_dir().join(format!("sdf-ui-unknown-scheme-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let scenes = root.join("scenes");
    let config_dir = root.join("config");
    ensure_dir(&scenes);
    ensure_dir(&config_dir);

    let source = "// sdf-scene: data = \"ftp://host/manifest.json\"\n// sdf-scene: layer = \"probe\"\nfn scene(p: vec2f, _time: f32) -> f32 { return 0.0; }\n";
    let scene_path = scenes.join("bad_scheme.wgsl");
    write_file(&scene_path, source);

    let library = crate::state::scene::SceneLibrary::discover_in(&scenes);
    let failure = match library.load(0, &config_dir) {
        Err(error) => error.to_string(),
        Ok(_) => String::new(),
    };
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        failure.contains("ftp://host/manifest.json"),
        "an unknown scheme must fail naming the value, got: {failure}"
    );
}

// ------------------------------------------------------------------------------------------ //

/// Bounded wait for the extraction drain: real OS threads feed the channel,
/// so the test spins parked runs with small sleeps until the outcome lands.
/// Returns false when the outcome never lands.
fn wait_for_extraction(state: &Entity<crate::state::app::App>, cx: &mut TestAppContext) -> bool {
    for _ in 0..200 {
        cx.run_until_parked();
        let finished = cx.update(|cx| state.read(cx).offline().read(cx).extraction_finished());
        if finished {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

/// A rejected pick stays in the setup panel with the reason, saves nothing,
/// and leaves the configured game root empty.
#[gpui::test]
fn a_rejected_pick_stays_in_setup_and_names_the_reason(cx: &mut TestAppContext) {
    let fixture = fixture("pick-reject", cx);
    let config_path = fixture.root.join("config.toml");
    let offline = OfflineState::from_config(sdf_offline::Config::defaults())
        .with_config_path(config_path.clone());
    let state = fixture.app_state_with_offline(offline, cx);

    let offline_entity = cx.update(|cx| state.read(cx).offline().clone());
    let bad_root = fixture.root.join("nowhere");
    cx.update(|cx| {
        offline_entity.update(cx, |offline, cx| {
            offline.apply_game_root(bad_root, cx);
        });
    });

    let (rejection, valid, needs_setup) = cx.update(|cx| {
        let offline = state.read(cx).offline().read(cx);
        (
            offline.rejection().map(ToString::to_string),
            offline.game_root_valid(),
            offline.needs_setup(),
        )
    });
    assert!(
        rejection
            .as_ref()
            .is_some_and(|rejection| rejection.contains(TEST_PAMT)),
        "the rejection must name the missing pamt table, got: {rejection:?}"
    );
    assert!(!valid, "a rejected root must not count as valid");
    assert!(needs_setup, "a rejected pick must stay in setup");
    assert!(
        !config_path.exists(),
        "a rejected pick must not save the config"
    );

    // The status snapshot the status bar reads must carry the same
    // rejection, not the raw state's other fields.
    let status_rejection = cx.update(|cx| state.read(cx).offline().read(cx).status().rejection);
    assert!(
        status_rejection
            .as_ref()
            .is_some_and(|rejection| rejection.contains(TEST_PAMT)),
        "the status snapshot must carry the rejection, got: {status_rejection:?}"
    );
    let status_valid = cx.update(|cx| state.read(cx).offline().read(cx).status().game_root_valid);
    assert!(
        !status_valid,
        "the status snapshot must not count a rejected root valid"
    );
}

/// A valid pick saves the config (through the injected path), clears the
/// rejection state, and flips `needs_setup` off — the panel hands over to the
/// shell.
#[gpui::test]
fn a_valid_pick_saves_the_config_and_leaves_setup(cx: &mut TestAppContext) {
    let fixture = fixture("pick-apply", cx);
    let config_path = fixture.root.join("config.toml");
    let mut config = sdf_offline::Config::defaults();
    config.paths.data_dir = fixture.root.join(TEST_WORK_DIR).display().to_string();
    let offline = OfflineState::from_config(config).with_config_path(config_path.clone());
    let state = fixture.app_state_with_offline(offline, cx);

    let offline_entity = cx.update(|cx| state.read(cx).offline().clone());
    let install = fixture.root.join(TEST_INSTALL_DIR);
    cx.update(|cx| {
        offline_entity.update(cx, |offline, cx| {
            offline.apply_game_root(install.clone(), cx);
        });
    });

    let (rejection, valid, needs_setup) = cx.update(|cx| {
        let offline = state.read(cx).offline().read(cx);
        (
            offline.rejection().map(ToString::to_string),
            offline.game_root_valid(),
            offline.needs_setup(),
        )
    });
    assert!(
        rejection.is_none(),
        "a valid pick must not leave a rejection, got: {rejection:?}"
    );
    assert!(valid, "the picked install must count as valid");
    assert!(
        !needs_setup,
        "a valid pick with data present must leave the setup panel"
    );

    let reloaded = match sdf_offline::Config::load_from(&config_path) {
        Ok(config) => config,
        Err(error) => unreachable!("reload the saved config: {error}"),
    };
    assert_eq!(
        reloaded.paths.game_root,
        install.display().to_string(),
        "the pick must persist into the injected config file"
    );
}

/// Extraction failure end-to-end: a finished extraction result with an error
/// ends the drain, surfaces the failure, and refreshes the scene list through
/// the injected scenes directory. The channel is filled before the drain
/// starts, so the test is deterministic (no real pipeline thread, no timers).
#[gpui::test]
fn an_extraction_failure_completes_and_refreshes_the_scene_list(cx: &mut TestAppContext) {
    let fixture = fixture("extract-refresh", cx);
    let offline = fixture.offline();
    let state = fixture.app_state_with_offline(offline, cx);

    // Written after boot: only a post-extraction refresh can discover it.
    write_file(&fixture.scenes.join("probe_a.wgsl"), TEST_SCENE_A);

    let offline_entity = cx.update(|cx| state.read(cx).offline().clone());
    let (sender, receiver) = std::sync::mpsc::channel::<ExtractionEvent>();
    let progress_sent = sender.send(ExtractionEvent::Progress(sdf_offline::Progress {
        step: "extract",
        layer: String::from("probe"),
        message: String::from("pack scanned"),
        milestone: false,
    }));
    let failure_sent = sender.send(ExtractionEvent::Finished(Err(String::from(
        "the export manifest is missing",
    ))));
    drop(sender);
    assert!(progress_sent.is_ok(), "the progress event must be queued");
    assert!(failure_sent.is_ok(), "the failure must be queued");

    cx.update(|cx| {
        offline_entity.update(cx, |_, cx| {
            cx.spawn(async move |entity, cx| drain_progress(cx, entity, receiver).await)
                .detach();
        });
    });

    let drained = wait_for_extraction(&state, cx);
    assert!(drained, "the drain must end within the bounded wait");

    let failure = cx.update(|cx| {
        state
            .read(cx)
            .offline()
            .read(cx)
            .extraction_failed()
            .map(ToString::to_string)
    });
    assert_eq!(
        failure.as_deref(),
        Some("the export manifest is missing"),
        "the failure must surface through the drain"
    );

    // The status snapshot must carry the failure and the running flag the
    // status bar branches on.
    let (status_failed, status_running) = cx.update(|cx| {
        let status = state.read(cx).offline().read(cx).status();
        (status.extraction_failed, status.extraction_running)
    });
    assert_eq!(
        status_failed.as_deref(),
        Some("the export manifest is missing"),
        "the status snapshot must carry the extraction failure"
    );
    assert!(
        !status_running,
        "the status snapshot must not report a running extraction after failure"
    );
    let status_progress = cx.update(|cx| state.read(cx).offline().read(cx).status().progress_line);
    assert_eq!(
        status_progress, TEST_DRAINED_PROGRESS,
        "the drained progress event must land in the status snapshot's progress line"
    );

    let (names, active, running) = cx.update(|cx| {
        let state = state.read(cx);
        (
            state.scene_names(),
            state.active(),
            state.offline().read(cx).extraction_running(),
        )
    });
    assert!(
        names.contains(&String::from("probe_a")),
        "the scene list must refresh after extraction, got: {names:?}"
    );
    assert_eq!(
        active,
        Some(0),
        "the refreshed list must select the first scene"
    );
    assert!(!running, "the drain must stop once the outcome is known");
}

/// A dead extraction thread (channel closed without an outcome) ends the
/// drain with the named error instead of waiting forever.
#[gpui::test]
fn a_dead_extraction_thread_ends_the_drain_with_a_named_error(cx: &mut TestAppContext) {
    let fixture = fixture("dead-thread", cx);
    let offline = fixture.offline();
    let state = fixture.app_state_with_offline(offline, cx);

    let offline_entity = cx.update(|cx| state.read(cx).offline().clone());
    let (sender, receiver) = std::sync::mpsc::channel::<ExtractionEvent>();
    drop(sender);
    cx.update(|cx| {
        offline_entity.update(cx, |_, cx| {
            cx.spawn(async move |entity, cx| drain_progress(cx, entity, receiver).await)
                .detach();
        });
    });

    let drained = wait_for_extraction(&state, cx);
    assert!(drained, "the drain must end within the bounded wait");

    let failure = cx.update(|cx| {
        state
            .read(cx)
            .offline()
            .read(cx)
            .extraction_failed()
            .map(ToString::to_string)
    });
    assert_eq!(
        failure.as_deref(),
        Some("the extraction thread ended unexpectedly"),
        "a dead thread must surface as the named drain failure"
    );
}

// ------------------------------------------------------------------------------------------ //

// ------------------------------------------------------------------------------------------ //

/// A minimal VS-20 export manifest under `<config dir>/sdf` with its tile
/// payloads beside it — what `sdf-app extract` leaves behind.
fn write_config_manifest(config_dir: &std::path::Path) -> Vec<u8> {
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
        write_bytes(&config_dir.join("sdf").join(payload_name), payload);
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
    let body = serde_json::to_string_pretty(&manifest).unwrap_or_default();
    write_file(&config_dir.join("sdf").join("manifest.json"), &body);
    body.into_bytes()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
