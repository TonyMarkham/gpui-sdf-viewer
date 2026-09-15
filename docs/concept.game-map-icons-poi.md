---
id: concept.game-map-icons-poi
kind: concept
title: Map icons and POIs
---

# Map icons and POIs

## What

How the world map places its icons — settlements, ruins, camps, watchtowers,
shipwrecks, caves — and what art they draw from. Every number below is decoded
first-party from the game data (pack 0008 staticinfo tables + pack 0012
textures), 2026-09-14.

## Position source: `factionnode.staticinfo`

**World positions — source of truth (verified).** 1119 records; **1002 yield a
world position triple `(x, y, z)`** — x/z world coordinates, y the terrain
height. Record names are the POI identity (`Node_Her_HernandNorthGate`,
`Node_Dem_DemenissCastle`, …); positions sit at a small fixed offset after the
record's name in most records (bytes 56–84 of the payload; some records carry
additional nested triples — waypoints, spawn points — after the first).

Ruled out as a competing node-position source: `factionwaypoint.staticinfo`
(475 records) — decoded 2026-09-14; its payloads are dense polylines of world
triples (the dashed travel routes), not node positions.

The first-party container/record format is [[concept.game-staticinfo]].

Decoded samples (x, y, z — y is terrain height):

| Node | Position |
|---|---|
| `Node_Dem_DemenissCastle` | (-8062.0, 586.2, -3539.0) |
| `Node_Her_PervinFort` | (-11580, 656, -5259) |
| `Node_Her_SunsetVillage` | (-11341, 747, -7029) |
| `Node_Kwe_SteinnfellFortress` | (-11096, 857, -1128) |
| `Node_Crim_FortMusket` | (-6774.5, 583.7, -1925.6) |
| `Node_Kwe_VagueAbyssCave` | (-11409, 700, 855) |

All decoded positions fall inside the map window x ∈ [-16384, +3072],
z ∈ [-11264, +8192] ([[concept.game-map-layers]]) — and inside the land
content bounds, which is itself a consistency check on the window triple.

**Map placement — projection verified for direction, affine open.** Turning
world positions into map pixels needs the world→plane mapping:
`u = (x − x_min)/span, v = (z_top − z)/span` over the window triple
([[concept.game-map-layers]]). Verified against `REFERENCE-land-map.png`:
`Node_Dem_DemenissCastle` (-8062.0, -3539.0) projects to (u=0.428, v=0.603)
and the render's Demeniss city art sits at ≈(0.455, 0.605) — vertical agrees
to ±0.002, horizontal within eyeball error; the flipped vertical convention
collapses to chance in a land/sea image test (49.2% vs 75.0%, base rate
97.9% all-sea). What remains open is the exact affine (the render is
2848×2695 with margins; the css numbers that pin it are encrypted). Until
then: world coordinates are fact, final pixel placement is
verified-to-eyeball-precision only.

## Pixel source: the `v_bitmap_*` atlases

## Pixel source: the `v_bitmap_*` family (corrected by decode)

All four related textures are **single 8-bit channels in 16-bit containers** —
in every pixel, one of the two bytes is always zero (measured on full mip0s,
2026-09-14). The DDS headers say `bits 16, fourcc none`; effectively A8/L8
masks. There is no two-channel data to decode — that question is closed.

The family's behavior at rest (all reconstructed via the partial-decompress
path, content measured):

| Texture | Shape | Content at rest |
|---|---|---|
| `v_bitmap_factionnode` (4096², 13 mips) | quadrant-swap | **near-empty** — 341 non-zero bytes, one ~10×5 stamp at (0.455, 0.413) |
| `v_bitmap_region` (4096², 13 mips) | quadrant-swap | upper-left 2048² quadrant fully dense; rest blank |
| `v_bitmap_region_nature` (4096², 13 mips) | quadrant-swap | one ≈1031×1015 blob at (0.36–0.61, 0.35–0.60); rest blank |
| `bitmap_region` (2048², R8) | static | fully dense (all but one pixel non-zero) |
| `bitmap_region_worldmap_ui` (2048², R8) | static | dense |

**`v_bitmap_factionnode` is not a static icon atlas** — it is a runtime
target: the game streams icon images into its 2×2 2048² quadrants (the census
claim that it statically holds 1119 node icons is wrong). The static faction
art lives in the `cd_knowledgeimage_worldmapicon_*` settlement DDSs (what
[[concept.abyss-map-composition]] already uses) and possibly inside the
encrypted UI set.

## Cell selection: `bitmapposition.staticinfo`

Seven records — `Bitmap_FactionNode`, `Bitmap_Region`,
`Bitmap_Region_AutoSpawn`, `Bitmap_Region_Crime`, `Bitmap_Region_Nature`,
`Bitmap_BellKnowledge`, `Bitmap_Fish` — one per atlas consumer.
`Bitmap_FactionNode` is 120.8 KB ≈ 30,202 4-byte entries.

**Entry structure (decoded):** each 4-byte entry is two u16s — a high u16 in
10.6 fixed point stepping **+255.0 units per entry, wrapping at 1024.0**
(i.e. UV steps of ≈0.249 with wrap at 1.0: the sequence 0.249, 0.498, 0.747,
0.996, then wraps), and a low u16 that increments once per 4 entries. Four
entries per index × quarter-UV steps = **quadrant addressing**: the record
addresses the 2×2 2048² quadrants of the swap texture, matching the
`v_bitmap_*` quadrant behavior above (2048² is also the size of
`bitmap_region` and `bitmap_region_worldmap_ui`).

What this means practically: the POI→icon mapping runs through a runtime
swap-texture mechanism, not a static atlas — reproducing the game's icon
rendering from static data means using the `cd_knowledgeimage_worldmapicon_*`
DDSs directly (as the composition recipe already does), with
`bitmapposition` only needed if the goal is to mimic the game's own swap
behavior.

## Marker pixel types (from `uimaptextureinfo`)

`uimaptextureinfo.staticinfo` (2025 records) names every map marker sprite the
UI composites — `Actor_Enemy`, `Actor_Npc_*`, `Abyss_Gate`,
`AbyssTreasureBox`, `Abyss_Ruins(_Fog)(_Disabled)` states, etc. These are UI
sprites (the HUD minimap icons), separate from the `v_bitmap` swap textures;
their images live in `0012`'s UI DDS set (mostly 8-bit masks; some DXT5).

## Nested values in `factionnode` records

Records carry (measured): a **second copy of the node's world position**
later in the property bag, occasional additional position triples, and
trailing ±180.0 f32s (yaw-like facings) next to small offset pairs (e.g.
-168/-131). Reading: the second copy is likely the map-anchor variant and the
nested entries spawn/orientation data. Whether the UI consumes the nested
values is game-runtime behavior — not decidable from the data.

## Open questions

The toolkit for these items is [	ools/](../tools/README.md).

- [ ] Pin the exact render affine (scale + margins) — needs a proper
  overlay diff or the decrypted css numbers.
- [ ] Map `bitmapposition` entries to their consumers end-to-end (which
  quadrant UV lands on which node id) — only worth it if mimicking the
  game's swap behavior; the static composition recipe does not need it.
- [ ] The `factionnode` key ↔ `regioninfo` key spaces are independent
  (u32 `1000000 + k` vs u16); the binding between a node and its
  `Region_Node_*` record is **by name** (verified for HernandNorthGate,
  DemenissCastle, PailuneBeacon: regioninfo keys 41002/44000/43005 share no
  arithmetic relation to the factionnode keys).
