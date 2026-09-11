use gpui::{AppContext as _, Context, Entity, Size, TestAppContext, VisualTestContext, Window, px};
use gpui_component::ActiveTheme as _;
use gpui_component::slider::{SliderEvent, SliderValue};

use crate::RootView;

#[gpui::test]
fn initialize_pins_the_list_selection_theme(cx: &mut TestAppContext) {
    cx.update(|cx| {
        assert!(crate::initialize(cx).is_ok());

        let theme = cx.theme();
        assert_eq!(theme.list_active, theme.accent);
        assert_eq!(theme.list_active_border, gpui::transparent_black());
    });
}

struct AppHost {
    root: Entity<RootView>,
}

impl gpui::Render for AppHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        self.root.clone()
    }
}

fn app(cx: &mut TestAppContext) -> (Entity<crate::AppState>, &mut VisualTestContext) {
    cx.update(|cx| assert!(crate::initialize(cx).is_ok()));

    let app_state = cx.new(crate::AppState::new);
    let app_state_clone = app_state.clone();
    let (_host, cx) = cx.add_window_view(move |_, cx| {
        let root = cx.new(|cx| RootView::new(app_state_clone, cx));
        AppHost { root }
    });

    (app_state, cx)
}

fn redraw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        let _ = window.draw(cx);
    });
}

fn bounds(
    cx: &mut VisualTestContext,
    selector: &'static str,
) -> Option<gpui::Bounds<gpui::Pixels>> {
    cx.debug_bounds(selector)
}

#[gpui::test]
fn root_renders_the_canvas_shell(cx: &mut TestAppContext) {
    let (_state, cx) = app(cx);

    redraw(cx);

    for selector in [
        "canvas-host",
        "sdf-canvas-root",
        "control-bar",
        "status-bar",
        "navigation-scenes",
    ] {
        let found = bounds(cx, selector);
        assert!(found.is_some(), "the root layout must render {selector}");
    }

    let host = match bounds(cx, "canvas-host") {
        Some(host) => host,
        None => unreachable!("canvas-host was rendered above"),
    };
    assert!(
        host.size.height > px(100.0),
        "the canvas area should absorb the window body, got {host:?}"
    );
}

#[gpui::test]
fn scene_selection_loads_into_the_canvas(cx: &mut TestAppContext) {
    let (state, cx) = app(cx);

    let names = cx.update(|_, cx| state.read(cx).scene_names());
    assert!(
        names.len() >= 3,
        "the embedded examples must be discoverable, got {names:?}"
    );

    let (initial_active, _initial_scene) = cx.update(|_, cx| {
        let state = state.read(cx);
        let canvas = state.canvas().read(cx);
        (state.active(), canvas.scene_name().map(ToString::to_string))
    });

    // The embedded examples guarantee at least one scene on startup.
    assert!(
        initial_active.is_some(),
        "the first discoverable scene is selected at startup"
    );

    cx.update(|_, cx| {
        state.update(cx, |state, cx| state.select(1, cx));
    });
    redraw(cx);

    let (active, scene_after) = cx.update(|_, cx| {
        let state = state.read(cx);
        let canvas = state.canvas().read(cx);
        (state.active(), canvas.scene_name().map(ToString::to_string))
    });
    assert_eq!(active, Some(1), "selection must move to the clicked scene");
    let expected = names.get(1).cloned().unwrap_or_default();
    assert_eq!(
        scene_after,
        Some(expected),
        "the canvas must adopt the selected scene's name"
    );

    // A window resize keeps the shell intact.
    cx.simulate_resize(Size {
        width: px(1024.0),
        height: px(640.0),
    });
    redraw(cx);

    let host = bounds(cx, "canvas-host");
    assert!(host.is_some(), "the canvas survives a resize");
}

#[gpui::test]
fn out_of_range_scene_selection_is_a_no_op(cx: &mut TestAppContext) {
    let (state, cx) = app(cx);

    // Selecting an invalid scene must neither panic nor corrupt state.
    cx.update(|_, cx| {
        state.update(cx, |state, cx| state.select(usize::MAX, cx));
    });

    let (active, load_error) = cx.update(|_, cx| {
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
    let (state, cx) = app(cx);

    let (enabled, band) = cx.update(|_, cx| {
        let app = state.read(cx);
        (app.contour_enabled(), app.canvas().read(cx).contour_band())
    });
    assert!(!enabled, "the overlay starts off");
    assert_eq!(band, 0.0, "the overlay starts with a zero band width");

    cx.update(|_, cx| {
        state.update(cx, |state, cx| state.toggle_contour(cx));
    });

    let (enabled, band, has_data) = cx.update(|_, cx| {
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
    assert!(!has_data, "the embedded startup scene carries no game data");
}

/// The LOD and density sliders drive the canvas through the App's slider
/// subscriptions: a level change maps onto the 0..1 level position (clamped),
/// and a density change updates the band width while the overlay is on.
#[gpui::test]
fn field_level_and_density_sliders_drive_the_canvas(cx: &mut TestAppContext) {
    let (state, cx) = app(cx);

    let level = cx.update(|_, cx| state.read(cx).level_slider().clone());
    let density = cx.update(|_, cx| state.read(cx).density_slider().clone());

    cx.update(|_, cx| {
        level.update(cx, |_, cx| {
            cx.emit(SliderEvent::Change(SliderValue::from(0.5_f32)));
        });
    });
    let field_level = cx.update(|_, cx| state.read(cx).canvas().read(cx).field_level());
    assert_eq!(
        field_level, 0.5,
        "the LOD slider must drive the canvas level"
    );

    cx.update(|_, cx| {
        level.update(cx, |_, cx| {
            cx.emit(SliderEvent::Change(SliderValue::from(1.5_f32)));
        });
    });
    let field_level = cx.update(|_, cx| state.read(cx).canvas().read(cx).field_level());
    assert_eq!(
        field_level, 1.0,
        "out-of-range slider values must clamp into 0..1"
    );

    cx.update(|_, cx| {
        state.update(cx, |state, cx| state.toggle_contour(cx));
    });
    cx.update(|_, cx| {
        density.update(cx, |_, cx| {
            cx.emit(SliderEvent::Change(SliderValue::from(6.0_f32)));
        });
    });
    let band = cx.update(|_, cx| state.read(cx).canvas().read(cx).contour_band());
    assert_eq!(
        band, 6.0,
        "the density slider must drive the canvas band width"
    );
}
