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
  root (override with the `SDF_SCENES_DIR` environment variable), sorted
  alphabetically — the first file scene is the app's startup selection.
- There are no embedded scenes. An empty scenes directory shows the empty
  list; on a machine where setup is incomplete, the setup panel is the first
  view instead. Discovery lists every `.wgsl` file it finds without parsing
  it — a scene that fails to load stays in the list, and selecting it
  surfaces the named load error in the status bar.

## File shape

Header directives (optional, one per line):

```text
// sdf-scene: name = "Metaballs"
// sdf-scene: animated = true
// sdf-scene: data = "manifest.json"
// sdf-scene: layer = "coast"
```

- `name` — label shown in the navigation and status bar
- `animated` — re-render every frame with a running `time` value
- `data` + `layer` — a pair: `data` names a VS-20 export manifest,
  resolved relative to the scene file, or through the `config:` scheme
  (below); `layer` selects one self-contained entry from the manifest's
  `layers` array by `name` (hard error when absent). Carrying only one of
  the two is a parse error.
- A `data` value of the form `config:<remainder>` resolves through the app
  config: `<data dir>/<remainder>` — the directory the extraction pipeline
  writes to, by default `~/.config/cd-map-offline/data`, and configurable
  via `paths.data_dir`. The host shell rewrites the directive to the
  resolved absolute path at scene-load time and parses the rewritten source
  through the component; the component itself knows nothing about schemes.
  Any other `scheme:` value is a parse error naming the value.

## Data scenes

A scene with `data`/`layer` binds the game fields exported by the map
pipeline (`cd-map-sdf-field` manifest, version 1). Loading resolves the
manifest entry and verifies it structurally (tile table covers the grid
exactly once, mip chain halves from level 0, byte lengths match level
sizes); every tile payload is then read beside the manifest, verified
against its declared sha256, length-checked against the mip table, and
uploaded into the field texture — mip 0 first, one texture level per
declared mip. Stub tiles carry constant-fill payloads and expand to their
grid region at every level. Nothing is inferred from file sizes; the
manifest is the trust anchor, and the GUI parses no DDS, ever.

The shipped `scenes/` files in the repository root point `data` at
`config:sdf/manifest.json` — the app's own extracted export. Until extraction
has run they fail to load with the missing-manifest error (documented
behavior): the setup panel is the intended first-run view, and after
extraction the scenes load machine-independently through the data directory
under the config home. Copy a shipped scene and point `data` at another
export (file-relative or absolute) to view other fields.

The field texture is a texture array: one layer per grid tile, sized by the
manifest's mip-0 tile edge. The prelude's field helpers decompose canvas
coordinates into tile and in-tile uv (see the reserved names below), so a
scene samples `field(p)` regardless of the geometry behind it. Scenes
without `data` bind a 1×1 zero-filled stand-in — same bind group shape, the
helpers stay inert.

`u.params.x` is the mip-level selector (0.0 = mip 0) and `u.params.y` the
contour-overlay band width (0.0 = pass skipped; otherwise band width in
field bytes) — see `State`.

The component-level contour overlay is a second fragment pass, not a
scene: it samples the same data texture and alpha-blends contour lines over
whatever scene is active. It runs only when a data texture is bound and
`params.y` is on, so scenes with the overlay off are untouched.

## Recipe scenes

The game fields are raw coverage bytes, not normalized SDFs — the built-in
`scene_shaded` gradient lighting is meaningless over them. Game-data scenes
therefore use the `render` override, sample `field(p) * 255.0`, and
hardcode their threshold recipe (the map pipeline's `config.toml` `[sdf]`
stays authoritative; the export manifest deliberately carries no recipe
copy). Styling — colors, ink treatment — is the scene's own.

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
`mouse`, `params`) and the helpers `sd_*`, `op_*`, `rot`, plus the field
machinery: `t_field` (the data texture array), `s_field`,
`s_field_nearest`, `field`, `field_lod`, `field_raw`, `field_coord`,
`field_uv`, and the `FIELD_*` constants. Scenes must not redefine them.

- `field(p) -> f32` — the field value at `p`, sampled linearly at the
  `u.params.x` mip level
- `field_lod(p, level)` / `field_raw(p, level)` — explicit level; linear
  vs. nearest filtering (the raw-byte view for the inspect scene)

## Failure modes

- Parse-time problems (missing entry point, a lone `data` or `layer`
  directive) surface when the file is loaded.
- Data-scene problems surface at load (manifest missing or malformed,
  unknown `layer` key, structurally invalid tile or mip tables) or at first
  render (payload missing, sha mismatch, length not matching the declared
  mip sizes, device limits exceeded) — each names the manifest, the tile,
  its grid slot, or the limit involved.
- WGSL validation errors are captured through a wgpu error scope at compile
  time and surfaced verbatim in the canvas overlay and status bar; the last
  good frame keeps being presented.

## Why

The contract is deliberately minimal: one function for evaluation, one
optional override for shading. Everything GPU-heavy stays in WGSL so a future
GPU-backend swap does not change scene files.