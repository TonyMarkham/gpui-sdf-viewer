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
- Label collisions are possible: a dropped scene naming itself "Composite"
  lists twice alongside the synthetic entry. Nothing is reserved —
  selection is index-based, so both entries work.

## File shape

Header directives (optional, one per line):

```text
// sdf-scene: name = "Metaballs"
// sdf-scene: animated = true
// sdf-scene: data = "manifest.json"
// sdf-scene: layer = "coast"
// sdf-scene: layers = "coast,river,road"
```

- `name` — label shown in the navigation and status bar
- `animated` — re-render every frame with a running `time` value
- `data` + (`layer` xor `layers`) — a pair: `data` names a VS-20 export
  manifest, resolved relative to the scene file, or through the `config:`
  scheme (below); `layer` selects one self-contained entry from the
  manifest's `layers` array by `name`, `layers` selects several as a
  comma-separated list whose order is paint order (bottom → top), preserved
  everywhere (hard error when an entry is absent). Carrying only one of
  `data`/`layer` (or `data`/`layers`) is a parse error, and carrying `layer`
  and `layers` together is a parse error; an empty item in the `layers` list
  is a parse error naming the value.
- A `data` value of the form `config:<remainder>` resolves through the app
  config: `<data dir>/<remainder>` — the directory the extraction pipeline
  writes to, by default `~/.config/cd-map-offline/data`, and configurable
  via `paths.data_dir`. The host shell rewrites the directive to the
  resolved absolute path at scene-load time and parses the rewritten source
  through the component; the component itself knows nothing about schemes.
  Any other `scheme:` value is a parse error naming the value.

## Data scenes

A scene with `data`/`layer` (or `data`/`layers`) binds the game fields
exported by the map pipeline (`cd-map-sdf-field` manifest, version 1).
Loading resolves the manifest entries and verifies them structurally (tile
table covers the grid exactly once, mip chain halves from level 0, byte
lengths match level sizes); every tile payload is then read beside the
manifest, verified against its declared sha256, length-checked against the
mip table, and uploaded into one field texture per configured layer — mip 0
first, one texture level per declared mip. Stub tiles carry constant-fill
payloads and expand to their grid region at every level. Nothing is inferred
from file sizes; the manifest is the trust anchor, and the GUI parses no
DDS, ever.

The shipped `scenes/` files in the repository root point `data` at
`config:sdf/manifest.json` — the app's own extracted export. Until extraction
has run they fail to load with the missing-manifest error (documented
behavior): the setup panel is the intended first-run view, and after
extraction the scenes load machine-independently through the data directory
under the config home. Copy a shipped scene and point `data` at another
export (file-relative or absolute) to view other fields.

The field texture is a texture array: one layer per grid tile, sized by the
manifest's mip-0 tile edge. A composite scene binds one such array per
configured field; a composite's fields must share the first field's grid
and mip table, and the generated prelude emits one texture binding and one
sampling helper per field (`field_0..n`) so they sample side by side. The
prelude's field helpers decompose canvas coordinates into tile and in-tile
uv (see the reserved names below), so a scene samples `field(p)` regardless
of the geometry behind it. Scenes without `data` bind a 1×1 zero-filled
stand-in — same single-field binding shape, the helpers stay inert.

### View

Data scenes are map scenes: the canvas pans and zooms. The view lives
entirely in the prelude — `field_uv` applies it — so every map scene and the
contour overlay follow without scene-file changes:

- The view is stored in field-uv space: `view_center` (the field-uv point at
  the viewport center, default `[0.5, 0.5]`) and the zoom factor
  `u.params.z` (default 1.0, clamped to [1.0, 64.0]). Because the state is
  uv-fraction-based, window resizes re-render the same uv window instead of
  shifting it.
- The wheel zooms about the cursor and left-drag pans (content follows the
  cursor); a `1:1` control in the shell resets to the fitted view, and every
  scene switch opens fitted.
- The viewport clamp keeps `view_center ∈ [0.5/z, 1 - 0.5/z]` per axis, so
  the field always covers the viewport and the zoom floor is exactly the
  identity.
- The mip level the renderer submits is zoom-aware: the LOD slider stays the
  base bias and zoom refines downward toward mip 0.
- `view_p(p)` expresses the view back in the canvas `p` convention (the
  space `u.mouse` uses), so a scene can deliberately map screen to world —
  e.g. a cursor-anchored probe under zoom: `view_p(u.mouse)`.

`u.params.x` is the mip-level selector (0.0 = mip 0), `u.params.y` the
contour-overlay band width (0.0 = pass skipped; otherwise band width in
field bytes), and `u.params.z` the view zoom (1.0 = fitted) — see `State`.

The component-level contour overlay is a second fragment pass, not a
scene: it samples the same data texture and alpha-blends contour lines over
whatever scene is active. It runs only when a data texture is bound and
`params.y` is on, so scenes with the overlay off are untouched. On a
composite scene it contours the **first** configured layer — `field_lod`
stays bound to field 0 (documented behavior).

### Composite view

The left navbar carries one synthetic **Composite** entry, rendered after
the alphabetical file scenes, stacking the configured layers of the app
export in one frame. The layer list and the styling are owned by
`config.toml` - `[composite]`, paint order bottom → top - not by scene
files. An empty stack hides the entry entirely; each layer must name an
export route of the export.

#### Recipes

A layer's styling is its recipe: an ordered list of bands. Every layer is
`name` + `bands`; every band is `low`, `high`, `kind`, `ink`, `weight` -
exactly these fields, no optionals, for every layer and every band:

- `low`, `high` - the band's window in field bytes (0..=255)
- `kind` - one of two threshold primitives:
  - `band` - hard half-open window: `v >= low && v < high`; full ink
    inside, nothing outside. Water fills, shore outlines, the channel look.
  - `ramp` - soft edges, open above: `smoothstep(low, high, v)`; coverage
    rises from 0 at `low` to 1 at `high` and stays at 1 above. Shorelines,
    roads, elevation.
- `ink` - the band's color, straight-alpha RGB components in 0..=1
- `weight` - the mix strength in 0..=1; scales the band's coverage before
  it paints

Painting is sequential: `color` starts as PAPER, then every layer's bands
mix their ink over the previous color in listed order (layer order, then
band order within a layer), at `coverage * weight`. A recipe-less route is
written out like any other layer (see the shipped `road_wagon` block) -
there is no automatic fallback styling; a layer carries no bands is a
configuration error. `Config::check()` rejects: a layer that is not an
export route, a name listed twice, a layer without bands, a band edge
outside 0..=255, a band whose low is above its high, a weight outside
0..=1, and an ink component outside 0..=1 - each naming the offending
layer.

The styling comes from the config alone: editing a shipped scene file does
not change the composite, and no scene file is consulted for it. A
user-editable composite scene file remains the follow-up that would allow
custom shading beyond threshold bands.

Migration: configs written before per-layer recipes carried
`[composite] layers = [...]` - a string array. The new schema ignores that
key, which turns the composite off silently; replace the `[composite]`
section with the layer blocks from the shipped template (or delete the
config to regenerate, which loses `game_root`).

Every existing control keeps working: the composite is a data scene like
any other, with pan/zoom, LOD and contours contouring the first layer.

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
`mouse`, `view_center`, `params`) and the helpers `sd_*`, `op_*`, `rot`, plus
the field machinery: `t_field` (the field-0 texture array), `s_field`,
`s_field_nearest`, `field`, `field_lod`, `field_raw`, `field_coord`,
`field_uv`, `view_p`, and the `FIELD_*` constants — plus, on composite
scenes, the generated per-field names `field_0..n` and `t_field_0..n`
(any name starting with `field_` or `t_field_`). Scenes must not redefine
them.

- `field(p) -> f32` — the field value at `p`, sampled linearly at the
  effective `u.params.x` mip level; on a composite scene this samples the
  **first** configured layer
- `field_lod(p, level)` / `field_raw(p, level)` — explicit level; linear
  vs. nearest filtering (the raw-byte view for the inspect scene)
- `field_k(p) -> f32` (composite scenes, k = 0..n) — the positional helper
  sampling field *k*'s own texture array (`t_field_k` at its own binding
  slot), in config paint order

## Failure modes

- Parse-time problems (missing entry point, a lone `data` or `layer`
  directive, `layer` carried together with `layers`, an empty item in a
  `layers` list) surface when the file is loaded.
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