---
id: concept.game-map-layers
kind: concept
title: The game's map field layers
---

# The game's map layers

## What

Every raster map layer the Crimson Desert install ships for the world map and
the abyss: the gridded SDF field families, the pre-assembled whole-map fields,
and the single-texture overlay layers. All of it lives in pack `0012`
(`ui/`); a name sweep across all 34 packs finds no gridded field anywhere
else. Evidence basis: full pamt-table scan + sampled DDS header probes of the
live install, 2026-09-14. Numbers below are read from the data, not from any
plan document.

## Gridded field families

Seven families share one shape — a complete 16×16 grid of tiles, each tile an
uncompressed 8-bit DDS carrying a 10-mip chain (512² mip0), plus 8² 4-mip
constant-fill stubs for empty slots. Tile names are
`<prefix>_32768x32768_<x>_<y>.dds` except `blur_height`, which drops the
size segment (`<prefix>_<x>_<y>.dds`). All unencrypted.

| Family | Real tiles (512²) | Stubs (8²) | In the repo's config |
|---|---|---|---|
| `cd_worldmap_land_sdf` | 176 | 80 | yes — `coast` + `river` routes |
| `cd_worldmap_land_2_sdf` | 134 | 122 | **no** — nowhere referenced |
| `cd_worldmap_road_sdf` | 64 | 192 | yes — `road` |
| `cd_worldmap_road_wagon_sdf` | 39 | 217 | yes — `road_wagon` (extra) |
| `cd_worldmap_mountain_sdf` | 103 | 153 | yes — `mountain` |
| `cd_worldmap_abyss_hex_sdf` | 254 | 2 | yes — `abyss_hex` |
| `cd_worldmap_blur_height` | 226 | 30 | yes — `blur_height` (extra) |

- Sampled headers: every real tile probed is `512x512, mips 10, fourcc none,
  bits 8`; every stub `8x8, mips 4, fourcc none, bits 8`.
- The real/stub split mirrors the world window: only the inhabited quadrant
  carries data (road 64/256, wagon 39/256).
- `land_2_sdf` is the discovery this repo had missed: byte-identical tile
  geometry to `land_sdf` (134 real tiles, same 512²/10-mip shape). It would
  flow through the pipeline unchanged given an include pattern and a route
  name. What it represents (a second land state? a different normalizer?) is
  open — compare its byte distributions against `land_sdf`.

## Whole-map single-file fields

Two fields ship as one pre-assembled DDS instead of tiles:

- `ui/cd_worldmap_road_sdf_32768x32768.dds` — 8192², 14 mips, R8
- `ui/cd_worldmap_blur_height.dds` — 8192², 14 mips, R8

These are ÷4 previews of two tiled fields (8192 = 32768/4; 14 mips down from
8192). No land/mountain/abyss equivalents exist. They do not fit the tiled
pipeline (no grid, 14 mips, one texture); consuming them needs a non-tiled
export route, and they are redundant with the tiled originals anyway.

## Overlay and annotation layers

Single-texture R8 layers, all unencrypted, all with a 10-mip-ish chain, all
usable by the scene contract's `data`/`layer` mechanism once a non-tiled
export route exists:

- **Region-title SDFs** — see [[concept.game-map-labels]].
- **Map-marker SDFs** — `cd_worldmap_image_*_sdf_<W>x<H>.dds`: 44 abyss-stage
  markers, 17 skill icons + 17 `_failed` variants, 12 `mountain_<deg>`
  direction symbols, animal/knowledge markers, and five per-faction **crime
  overlays** (`demeniss/heranad/delesyia/pailune/crimsondesert_crime_sdf_4096x4096`,
  actual texture 1024²) plus `proselytizing_sdf_1024x1024`.
- **Name-vs-texture caveat**: the `<W>x<H>` in the name is the CSS layout
  size; the texture is exactly ÷4 (named `1024x128` → a 256×32 DDS; named
  `4096x4096` → a 1024² DDS).

## What the pipeline can and cannot take today

- **Works unchanged** — the seven gridded families (six configured;
  `land_2_sdf` once an include + route name it).
- **Needs a new route shape** — the 8192² single-file fields and the R8
  overlay SDFs (no grid; a single-texture manifest entry would do).
- **Needs format work** — DXT1/DXT5 surfaces and the 16-bit `v_bitmap`
  swap textures violate the pipeline's uncompressed-8-bit rule:
  `cd_worldmap_sea_pattern` (512², DXT1), `cd_uitexture_worldmap_00..02`,
  `cd_uitexture_worldmap_marker_00`, `cd_worldmap_node_tooltip_contents_bg_mask`
  (1024², DXT5, 1 mip), and the 4096² 16-bit `v_bitmap_*` textures (single
  8-bit channel in a 16-bit container; runtime quadrant-swap textures, not
  static atlases — see [[concept.game-map-icons-poi]]).

## The world window (first-party)

`fieldinfo.staticinfo` (8 records, decoded directly) carries the map-plane
geometry in record `MainField_UIMap_Texture` (key 10):

- four f32s: `-13350, -7120, -1050, 4070` — the land content bounds inside
  the window (same record shape as `MainField_UIMap`, key 99999)
- three f32s: `-16384, 8192, 19456` — map window `(x_min, z_top, span)`;
  the window therefore spans x ∈ [-16384, +3072], z ∈ [-11264, +8192]
- `MainField_Multi` (key 2) carries `-20000, -20000, 20000, …` — a square
  bound of ±20000; the tiled fields' 32768² at ÷4 = 8192² maps the 19456²
  window with margin.

Cross-check: the decoded node positions in [[concept.game-map-icons-poi]] all
fall inside this window.
