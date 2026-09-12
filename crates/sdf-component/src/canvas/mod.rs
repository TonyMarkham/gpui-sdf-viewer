pub(crate) mod presentation;
pub(crate) mod state;

// ---------------------------------------------------------------------------------------------- //

use self::{presentation::Presentation, state::State};
use crate::renderer::{FrameRequest, Renderer};
use gpui::{
    AnyElement, App, Bounds, Corners, Element, ElementId, Entity, GlobalElementId,
    InspectorElementId, InteractiveElement as _, IntoElement, LayoutId, MouseMoveEvent, Pixels,
    Styled as _, Window, div, px,
};
use soul_attributes::soul;
use std::time::Instant;

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

    #[soul(id = "interaction.sdf.render-frame", step = "paint loop")]
    fn render_and_present(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let scale = window.scale_factor();
        let mut outcome = Presentation::Idle;

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
                    (state.field_level * (data.level_count().max(1) - 1) as f32).round()
                });

            match renderer.render(
                scene,
                &FrameRequest {
                    width,
                    height,
                    time,
                    mouse: state.mouse_uv,
                    params: [level, state.contour_band],
                },
            ) {
                Ok(maybe_image) => {
                    state.compile_error = None;
                    state.size = (width, height);
                    if let Some(image) = maybe_image {
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
                        outcome = Presentation::Painted {
                            image,
                            previous,
                            animated: state.animated,
                        };
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

        if let Presentation::Painted {
            ref image,
            ref previous,
            ..
        } = outcome
        {
            let _ = window.paint_image(
                bounds,
                bounds,
                Corners::all(px(0.0)),
                image.clone(),
                0,
                false,
            );
            if let Some(previous) = previous {
                let _ = window.drop_image(previous.clone());
            }
        }

        let keep_animating = match outcome {
            Presentation::Painted { animated, .. } => animated,
            Presentation::Pending => true,
            Presentation::Idle => false,
        };
        if keep_animating {
            window.request_animation_frame();
        }
    }
}

fn physical_edge(css_pixels: f32, scale: f32) -> u32 {
    let edge = (css_pixels * scale).round().max(1.0);
    edge.min(MAX_TEXTURE_EDGE as f32) as u32
}
