use gpui::{AppContext as _, Context, Entity, Size, TestAppContext, VisualTestContext, Window, px};
use gpui_component::ActiveTheme as _;

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
