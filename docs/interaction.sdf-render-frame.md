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
- `SdfCanvasState` (entity) — owns the renderer, scene, last frame, status,
  and the field controls (`u.params`)
- `Renderer` (wgpu) — device context, render pipeline, contour overlay
  pass, field texture, staging ring, readback
- The host shell (sdf-ui) — selects scenes, drives the overlay toggle and
  mip-level selector, displays status/failures

Scenes rendered through this flow follow [[concept.sdf-scene-contract]].

## When

- On every animation frame while the active scene is `animated = true`
- On window resizes (the texture is recreated at the new device-pixel size)
- On mouse movement over the canvas (the `u.mouse` uniform)
- When the host selects a different scene
- When the host changes a field parameter — the mip-level selector
  (`u.params.x`) or the contour overlay band width (`u.params.y`)

## Flow

1. `SdfCanvas::paint` → `SdfCanvasState.update` → `Renderer::render(scene, FrameRequest)`
2. The renderer aligns the field texture with the scene's data: a data
   scene's tiles are read, sha256-verified, length-checked, and uploaded
   into a texture array (one layer per grid tile, one mip level per declared
   mip) when the scene changed; a scene without data rebinds the inert 1×1
   stub. A failed upload is recorded with the data identity that failed and
   re-raised on every subsequent paint without re-reading the payloads —
   the failure is deterministic, so re-attempting it would only re-read and
   re-hash the layer each frame. Fixing a corrupt payload on disk therefore
   takes effect when the scene's data identity changes (the host selects a
   scene bound to a different manifest/layer), not on the next paint.
3. The renderer ensures the target texture matches the requested size, uploads
   uniforms (resolution, time, aspect, mouse, params), and submits a render
   pass — plus the contour overlay pass (alpha-blended over the scene's
   output, only while a data texture is bound and `params.y` is on) — plus a
   `copy_texture_to_buffer` into a free staging slot.
4. `map_async` completions are drained by `device.poll(Maintain::Poll)`; a
   finished slot becomes a BGRA `RenderImage` frame.
5. Back in `paint`, the new frame is drawn with `window.paint_image` and the
   previous frame is released with `window.drop_image` (keeps the atlas bounded).
6. If the scene is animated — or a submission is still in flight — the element
   calls `window.request_animation_frame()` to keep the loop running.

## Needs elaboration

- A future gpui version may expose shared-texture interop; the presentation
  step (steps 3–4) is the only place that would change.