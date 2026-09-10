---
id: interaction.sdf.render-frame
kind: interaction
title: Render an SDF frame into a gpui window
---

# Render an SDF frame into a gpui window

## What

How a gpui window paints one frame of a signed-distance-field scene: from the
[`SdfCanvas`](crates/sdf-component/src/canvas.rs) element's paint cycle, through
the wgpu renderer, to gpui's image atlas.

## Why

gpui 0.2.2 exposes no public API for injecting an external GPU texture into its
atlas, so the component renders offscreen with its own wgpu pipeline and
presents finished frames as CPU `RenderImage`s. The design keeps every GPU-side
computation in wgpu (scene evaluation, shading) while the only CPU work per
frame is an async staging-buffer copy plus a channel swizzle.

## Who

- `SdfCanvas` (gpui element) — drives the per-frame loop from `paint`
- `SdfCanvasState` (entity) — owns the renderer, scene, last frame, and status
- `Renderer` (wgpu) — device context, render pipeline, staging ring, readback
- The host shell (sdf-ui) — selects scenes, displays status/failures

Scenes rendered through this flow follow [[concept.sdf-scene-contract]].

## When

- On every animation frame while the active scene is `animated = true`
- On window resizes (the texture is recreated at the new device-pixel size)
- On mouse movement over the canvas (the `u.mouse` uniform)
- When the host selects a different scene

## Flow

1. `SdfCanvas::paint` → `SdfCanvasState.update` → `Renderer::render(scene, FrameRequest)`
2. The renderer ensures the target texture matches the requested size, uploads
   uniforms (resolution, time, aspect, mouse), and submits a render pass plus a
   `copy_texture_to_buffer` into a free staging slot.
3. `map_async` completions are drained by `device.poll(Maintain::Poll)`; a
   finished slot becomes a BGRA `RenderImage` frame.
4. Back in `paint`, the new frame is drawn with `window.paint_image` and the
   previous frame is released with `window.drop_image` (keeps the atlas bounded).
5. If the scene is animated — or a submission is still in flight — the element
   calls `window.request_animation_frame()` to keep the loop running.

## Needs elaboration

- A future gpui version may expose shared-texture interop; the presentation
  step (steps 3–4) is the only place that would change.