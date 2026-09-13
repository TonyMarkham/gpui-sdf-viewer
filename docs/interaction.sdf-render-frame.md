---
id: interaction.sdf.render-frame
kind: interaction
title: Render an SDF frame into a gpui window
---

# Render an SDF frame into a gpui window

## What

How a gpui window paints one frame of a signed-distance-field scene: from the
[`Canvas`](crates/sdf-component/src/canvas.rs) element's paint cycle, through
the wgpu renderer, to gpui's image atlas.

## Why

gpui 0.2.2 exposes no public API for injecting an external GPU texture into its
atlas, so the component renders offscreen with its own wgpu pipeline and
presents finished frames as CPU `RenderImage`s. The design keeps every GPU-side
computation in wgpu (scene evaluation, shading) while the only CPU work per
frame is an async staging-buffer copy plus a channel swizzle.

## Who

- `Canvas` (gpui element) — drives the per-frame loop from `paint`
- `State` (entity) — owns the renderer, scene, last frame, status,
  the field controls (`u.params`), and the map view (pan/zoom in field-uv
  space, applied by the prelude's `field_uv`)
- `Renderer` (wgpu) — device context, render pipeline, contour overlay
  pass, field texture, staging ring, readback
- The host shell (sdf-ui) — selects scenes, drives the overlay toggle and
  mip-level selector, resets the view, displays status/failures

Scenes rendered through this flow follow [[concept.sdf-scene-contract]].

## When

- On every animation frame while the active scene is `animated = true`
- On window resizes (the texture is recreated at the new device-pixel size)
- On mouse movement over the canvas (the `u.mouse` uniform)
- On wheel scroll or left-drag over a data scene's canvas (the map view
  uniforms: `view_center`, `params.z`) — non-data scenes leave the wheel
  alone
- When the host selects a different scene (the view resets to the fitted
  identity)
- When the host changes a field parameter — the mip-level selector
  (`u.params.x`), the contour overlay band width (`u.params.y`), or the
  view reset control

## Flow

1. `SdfCanvas::paint` → `SdfCanvasState.update` → `Renderer::render(scene, FrameRequest)`.
   Alongside the paint loop, the element registers the map view's input
   handlers: the wheel zooms about the cursor (multiplicative `exp2` on the
   raw pixel delta, per-event factor clamped to [0.5, 2.0]), a left-drag
   pans with the cursor, and the mouse-up ends the drag wherever it released.
   The view state lives on `State` in field-uv space and clamps so the field
   always covers the viewport.
2. The renderer aligns the bound field textures with the scene's data: a
   data scene's tiles are read, sha256-verified, length-checked, and
   uploaded into one field texture array per configured layer (one layer
   per grid tile, one mip level per declared mip; a composite binds one
   texture per configured layer, field 0 at binding 1) when the scene
   changed; a scene without data rebinds the inert 1×1 stub. The
   bind-group layout and both pipelines recompile whenever the scene's
   field count differs from the layout's. A failed upload is recorded with
   the data identity that failed and
   re-raised on every subsequent paint without re-reading the payloads —
   the failure is deterministic, so re-attempting it would only re-read and
   re-hash the layer each frame. Fixing a corrupt payload on disk therefore
   takes effect when the scene's data identity changes (the host selects a
   scene bound to a different manifest/layer), not on the next paint.
3. The renderer ensures the target texture matches the requested size, uploads
   uniforms (resolution, time, aspect, mouse, view, params), and submits a
   render pass — plus the contour overlay pass (alpha-blended over the scene's
   output, only while a data texture is bound and `params.y` is on) — plus a
   `copy_texture_to_buffer` into a free staging slot. The submitted mip level
   is zoom-aware: the LOD slider's base bias refined downward by
   `log2(zoom)`, clamped to the chain.
4. `map_async` completions are drained by `device.poll(Maintain::Poll)`; a
   finished slot becomes a BGRA `RenderImage` frame.
5. Back in `paint`, the frame is drawn with `window.paint_image` — the new
   frame when one landed, otherwise the last presented one (the display
   list is rebuilt on every repaint, so a repaint that paints nothing would
   blank the canvas; compile errors keep presenting the last good frame) —
   and the replaced frame is released with `window.drop_image` (keeps the
   atlas bounded).
6. If the scene is animated — or a submission is still in flight — the element
   calls `window.request_animation_frame()` to keep the loop running.

## Needs elaboration

- A future gpui version may expose shared-texture interop; the presentation
  step (steps 3–4) is the only place that would change.