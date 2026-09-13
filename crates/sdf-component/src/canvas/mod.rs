pub(crate) mod presentation;
pub(crate) mod state;

// ---------------------------------------------------------------------------------------------- //

use self::{
    presentation::Presentation,
    state::{State, wheel_zoom_factor},
};
use crate::renderer::{FrameRequest, Renderer};
use gpui::{
    AnyElement, App, Bounds, Corners, Element, ElementId, Entity, GlobalElementId,
    InspectorElementId, InteractiveElement as _, IntoElement, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, RenderImage, ScrollWheelEvent,
    Size, Styled as _, Window, div, px,
};
use soul_attributes::soul;
use std::{sync::Arc, time::Instant};

/// Exponential smoothing factor for the FPS estimate of presented frames.
const FPS_EMA_FACTOR: f32 = 0.9;

/// Safety clamp so a huge window with a large scale factor cannot allocate
/// absurd textures.
const MAX_TEXTURE_EDGE: u32 = 4096;

/// A reusable SDF rendering surface for gpui.
///
/// Drop it into any view as `sdf_canvas(state.clone())` where `state` is an
/// `Entity<SdfCanvasState>`. The element lazily brings up the wgpu renderer on
/// first paint, renders the currently selected scene into a texture at device
/// resolution, and presents it through gpui's image pipeline. While a scene is
/// animated, the element requests animation frames itself; the mouse position
/// is tracked and fed to scenes through the `u.mouse` uniform.
pub struct Canvas {
    state: Entity<State>,
}

/// Constructs an `SdfCanvas` bound to `state`.
pub fn sdf_canvas(state: Entity<State>) -> Canvas {
    Canvas { state }
}

impl IntoElement for Canvas {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Canvas {
    type RequestLayoutState = AnyElement;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(ElementId::Name("sdf-canvas".into()))
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut host = div()
            .id("sdf-canvas-root")
            .size_full()
            .debug_selector(|| "sdf-canvas-root".to_string())
            .into_any_element();
        let layout_id = host.request_layout(window, cx);
        (layout_id, host)
    }

    fn prepaint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        request_layout.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        request_layout.paint(window, cx);

        self.track_mouse(bounds, window);
        self.track_view_input(bounds, window);
        self.render_and_present(bounds, window, cx);
    }
}

impl Canvas {
    fn track_mouse(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        let state = self.state.clone();
        let origin = bounds.origin;
        let size = bounds.size;
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
            if !phase.bubble() {
                return;
            }
            state.update(cx, |state, cx| {
                let width = f32::from(size.width).max(1.0);
                let height = f32::from(size.height).max(1.0);
                let relative_x = f32::from(event.position.x - origin.x) / width;
                let relative_y = f32::from(event.position.y - origin.y) / height;
                let inside = (0.0..=1.0).contains(&relative_x) && (0.0..=1.0).contains(&relative_y);
                if !inside {
                    return;
                }
                let mouse_uv = [
                    (relative_x * 2.0 - 1.0) * (width / height),
                    1.0 - relative_y * 2.0,
                ];
                if (state.mouse_uv[0] - mouse_uv[0]).abs() > f32::EPSILON
                    || (state.mouse_uv[1] - mouse_uv[1]).abs() > f32::EPSILON
                {
                    state.mouse_uv = mouse_uv;
                    cx.notify();
                }
            });
        });
    }

    /// Registers the map view's input handlers alongside `track_mouse`:
    /// scroll zooms about the cursor, left-drag pans, mouse-up ends the drag.
    /// Wheel and mouse-down gate on `state.has_data()` so non-data scenes
    /// leave the wheel alone; the mouse-up handler carries no inside-bounds
    /// check so releasing outside the canvas still ends the drag, and moves
    /// outside the bounds keep panning (clamped) while a drag is running.
    #[soul(id = "interaction.sdf.render-frame", step = "view input")]
    fn track_view_input(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        let state = self.state.clone();
        let origin = bounds.origin;
        let size = bounds.size;
        window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
            if !phase.bubble() {
                return;
            }
            let fraction = viewport_fraction(event.position, origin, size);
            if !inside_viewport(fraction) {
                return;
            }
            let delta_y = f32::from(event.delta.pixel_delta(window.line_height()).y);
            let factor = wheel_zoom_factor(delta_y);
            state.update(cx, |state, cx| {
                if !state.has_data() {
                    return;
                }
                state.zoom_at(fraction, state.view_zoom() * factor, cx);
            });
        });

        let state = self.state.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, _, cx| {
            if !phase.bubble() {
                return;
            }
            if event.button != MouseButton::Left {
                return;
            }
            let fraction = viewport_fraction(event.position, origin, size);
            if !inside_viewport(fraction) {
                return;
            }
            state.update(cx, |state, _| {
                if state.has_data() {
                    state.begin_drag(fraction);
                }
            });
        });

        let state = self.state.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
            if !phase.bubble() {
                return;
            }
            let fraction = viewport_fraction(event.position, origin, size);
            state.update(cx, |state, cx| state.drag_move(fraction, cx));
        });

        let state = self.state.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
            if !phase.bubble() {
                return;
            }
            if event.button != MouseButton::Left {
                return;
            }
            state.update(cx, |state, _| state.end_drag());
        });
    }

    #[soul(id = "interaction.sdf.render-frame", step = "paint loop")]
    fn render_and_present(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let scale = window.scale_factor();
        let mut outcome = Presentation::Idle;
        let mut animated_frame = false;
        let mut outstanding = 0usize;

        self.state.update(cx, |state, _cx| {
            if state.renderer.is_none() && state.renderer_error.is_none() {
                match Renderer::new() {
                    Ok(renderer) => state.renderer = Some(renderer),
                    Err(error) => state.renderer_error = Some(error.to_string()),
                }
            }

            let (Some(scene), Some(renderer)) = (state.scene.as_ref(), state.renderer.as_mut())
            else {
                return;
            };

            let width = physical_edge(f32::from(bounds.size.width), scale);
            let height = physical_edge(f32::from(bounds.size.height), scale);
            let time = if state.animated {
                state.start.elapsed().as_secs_f32()
            } else {
                0.0
            };
            let level = state
                .scene
                .as_ref()
                .and_then(|scene| scene.data())
                .map_or(0.0, |data| {
                    effective_field_level(state.field_level, state.view_zoom, data.level_count())
                });

            match renderer.render(
                scene,
                &FrameRequest {
                    width,
                    height,
                    time,
                    mouse: state.mouse_uv,
                    view_center: state.view_center,
                    view_zoom: state.view_zoom,
                    params: [level, state.contour_band],
                },
            ) {
                Ok(maybe_image) => {
                    state.compile_error = None;
                    state.size = (width, height);
                    state.submitted_field_level = Some(level);
                    outstanding = renderer.pending_frames();
                    if let Some(image) = maybe_image {
                        animated_frame = state.animated;
                        let now = Instant::now();
                        if let Some(last) = state.last_present {
                            let seconds = now.duration_since(last).as_secs_f32();
                            if seconds > 0.0 {
                                let instant = 1.0 / seconds;
                                state.fps = if state.fps > 0.0 {
                                    FPS_EMA_FACTOR * state.fps + (1.0 - FPS_EMA_FACTOR) * instant
                                } else {
                                    instant
                                };
                            }
                        }
                        state.last_present = Some(now);

                        let previous = state.frame.replace(image.clone());
                        outcome = Presentation::Painted { image, previous };
                    } else {
                        // The GPU is still catching up; ask for another frame
                        // so the result is presented as soon as it lands.
                        outcome = Presentation::Pending;
                    }
                }
                Err(error) => {
                    state.compile_error = Some(error.to_string());
                }
            }
        });

        let image = painted_frame(&outcome, &self.state.read(cx).frame);
        if let Some(image) = image {
            let _ = window.paint_image(bounds, bounds, Corners::all(px(0.0)), image, 0, false);
        }
        if let Presentation::Painted {
            previous: Some(previous),
            ..
        } = outcome
        {
            let _ = window.drop_image(previous.clone());
        }

        // Keep the loop alive while any submitted frame is still outstanding:
        // on a non-animated scene the presented frame may be one pipelined
        // behind the newest submission (the ring drains without resubmitting),
        // and stopping earlier would leave the newest view unpainted until
        // the next input event.
        let keep_animating = animated_frame || outstanding > 0;
        if keep_animating {
            window.request_animation_frame();
        }
    }
}

fn physical_edge(css_pixels: f32, scale: f32) -> u32 {
    let edge = (css_pixels * scale).round().max(1.0);
    edge.min(MAX_TEXTURE_EDGE as f32) as u32
}

/// The image this repaint must carry: the freshly landed frame, or — when no
/// new frame arrived — the last presented one. The window's display list is
/// rebuilt on every repaint, so a repaint that paints nothing would blank
/// the canvas region and the theme background would show through. `stored`
/// is the element state's last presented frame; on `Painted` it already
/// equals the fresh image, so the outcome wins, and on `Pending`/`Idle` the
/// stored frame keeps presenting — compile errors keep the last good frame,
/// per the scene contract.
/// through, and the scene appears to "go white" the moment the mouse leaves
/// the window or focus moves away.
pub(crate) fn painted_frame(
    outcome: &Presentation,
    stored: &Option<Arc<RenderImage>>,
) -> Option<Arc<RenderImage>> {
    match outcome {
        Presentation::Painted { image, .. } => Some(image.clone()),
        Presentation::Pending | Presentation::Idle => stored.clone(),
    }
}

/// The cursor position as a viewport fraction (the base field-uv mapping of
/// the cursor: fraction from the left, fraction from the top).
fn viewport_fraction(
    position: Point<Pixels>,
    origin: Point<Pixels>,
    size: Size<Pixels>,
) -> [f32; 2] {
    let width = f32::from(size.width).max(1.0);
    let height = f32::from(size.height).max(1.0);
    [
        f32::from(position.x - origin.x) / width,
        f32::from(position.y - origin.y) / height,
    ]
}

/// The same inside test `track_mouse` applies: both fractions within 0..=1.
fn inside_viewport(fraction: [f32; 2]) -> bool {
    (0.0..=1.0).contains(&fraction[0]) && (0.0..=1.0).contains(&fraction[1])
}

/// The mip level the renderer submits: the LOD slider's base bias across the
/// chain, refined downward by the map view's zoom — zooming in without mip
/// refinement would just magnify blocks. `raw = base * (levels - 1) -
/// log2(zoom)`, clamped to the chain.
pub(crate) fn effective_field_level(base: f32, zoom: f32, levels: u32) -> f32 {
    let max = (levels.max(1) - 1) as f32;
    (base * max - zoom.log2()).round().clamp(0.0, max)
}
