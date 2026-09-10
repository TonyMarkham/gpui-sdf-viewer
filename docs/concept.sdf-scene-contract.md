---
id: concept.sdf-scene-contract
kind: concept
title: The SDF scene file contract
---

# The SDF scene file contract

## What

An SDF scene is a WGSL file the component compiles at runtime. It is spliced
between a fixed prelude of SDF helpers and a fixed rasterizer kernel.

## Where

- Files live anywhere; the app scans the `scenes/` directory at the working
  root (override with the `SDF_SCENES_DIR` environment variable).
- The component ships guaranteed-valid embedded examples (see
  `SdfScene::embedded_examples`).

## File shape

Header directives (optional, one per line):

```text
// sdf-scene: name = "Metaballs"
// sdf-scene: animated = true
```

- `name` — label shown in the navigation and status bar
- `animated` — re-render every frame with a running `time` value

The body defines one of:

```wgsl
fn scene(p: vec2f, time: f32) -> f32   // 2D SDF; negative inside
fn render(p: vec2f, time: f32) -> vec4f // full-shading escape hatch (3D raymarch, …)
```

Coordinate convention for `p`: `y` spans `[-1, 1]` bottom to top, `x` spans
`[-aspect, aspect]` left to right. `render` must return straight alpha; the
component premultiplies before presentation.

## Reserved names

The prelude reserves `u` (the uniform block: `resolution`, `time`, `aspect`,
`mouse`, `params`) and the helpers `sd_*`, `op_*`, `rot`. Scenes must not
redefine them.

## Failure modes

- Parse-time problems (missing entry point) surface when the file is loaded.
- WGSL validation errors are captured through a wgpu error scope at compile
  time and surfaced verbatim in the canvas overlay and status bar; the last
  good frame keeps being presented.

## Why

The contract is deliberately minimal: one function for evaluation, one
optional override for shading. Everything GPU-heavy stays in WGSL so a future
GPU-backend swap does not change scene files.