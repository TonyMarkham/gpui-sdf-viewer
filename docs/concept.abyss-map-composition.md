---
id: concept.abyss-map-composition
kind: concept
title: Composing the abyss reference map
---

# Composing the abyss reference map

## What

Every element of `REFERENCE-abyss-map.png`, traced to first-party decoded
game data. Nothing here is taken from the plan documents; each row is either
decoded (with numbers) or names the exact remaining decode step.

## Element map

| Reference element | Game data | Status |
|---|---|---|
| Base terrain (paper land + water tones) | tiled `land_sdf` / `road_sdf` / `blur_height` fields | exported by the pipeline today |
| The hex lattice | `cd_worldmap_abyss_hex_sdf` (254 real tiles, 16×16 grid) | exported today (`abyss_hex` route) |
| Dashed region outlines | contours of the hex field — the dashed segments in the reference follow hex edges exactly | a rendering of the existing field, no new data |
| Region labels: SLEET ISLES, DRY VALLEY, TRIANGLE RING, THE WANDERER'S WAY, THE PATH OF PROVIDENCE, (HYPER SPACE) | textures `cd_worldmap_regiontitle_abyss_*_eng_sdf_2048x256.dds` (unencrypted, ÷4 name caveat); anchors **extracted for all six** from `uimaptextureinfo` (`Knowledge_Node_Abyss_<Region>_ui_map_texture_1_0`) — region labels anchor at land altitude (y ≈ 400–950) ([[concept.game-map-labels]]) | anchors done; Triangle_Ring + Path_Of_Providence project onto their reference labels; Sleet_Isles outlier + per-label css translate open |
| Node icons (settlements, ruins, camps) | positions: `factionnode.staticinfo` — 1002/1119 records decoded with world `(x, y, z)` (verified; samples in [[concept.game-map-icons-poi]]); pixels: the `cd_knowledgeimage_worldmapicon_*` settlement DDSs (static art). The `v_bitmap_*` textures are **runtime quadrant-swap targets, not static atlases** (decoded: near-empty at rest; single 8-bit channel in a 16-bit container); `bitmapposition`'s `Bitmap_*` records address those quadrants (entry structure decoded) | world positions done; projection verified (DemenissCastle ±0.002 vertical); static composition uses the settlement DDSs and does not need the swap mechanism |
| Abyss stage/POI nodes | regions `Region_Node_Abyss_{Corridor, MechHeart, RecordFleet, SkyPillars, SteelSail, StreamSky}` + islands `Region_Node_Abyssone_0001..0201` in `regioninfo`; marker sprites `Abyss_Gate`, `Abyss_Ruins(_Fog)(_Disabled)`, `AbyssTreasureBox` in `uimaptextureinfo` | regions named; per-region geometry (the reference's outlined areas) needs the `regioninfo` property bag decoded |
| The vertical "…OF GREAT PAYWEI" edge label | `Region_Pywel` — regioninfo record key 1 (the world region) | named; text via the localization packs (encrypted) |
| World → map projection | `fieldinfo.staticinfo` record `MainField_UIMap_Texture`: window `(x_min, z_top, span) = (-16384, 8192, 19456)`, land content bounds `(-13350, -7120, -1050, 4070)` | decoded, first-party |
| Abyss stage interiors | `stageinfo.staticinfo` (28 MB), `levelinfo`, `specialmode`, `sublevelinfo`, one `abyssisland_XXXX_phase00_00.save` per island | out of scope for the map view |

## The composition recipe

1. Project the world window to the canvas: the window triple maps world
   (x, z) → the 8192² UI plane; the tiled fields are 32768² = 4× the plane,
   so field-uv maps directly.
2. Paint `land`/`road`/`blur_height` via the existing composite recipes.
3. Overlay the hex field (its own layer or as contours — the reference's
   dashed outlines are hex-edge contours).
4. Place node icons: world `(x, z)` → plane via the same projection; draw
   the `cd_knowledgeimage_worldmapicon_*` settlement art at those points
   (static art — the game's own `v_bitmap_*` swap mechanism is not needed).
5. Place the six label textures at their decoded world anchors
   ([[concept.game-map-labels]]), scaled by their CSS name size.

## Remaining decode work (in order of value)

1. The Sleet_Isles anchor outlier — why it sits far from its reference label
   while Triangle_Ring/Path_Of_Providence match (not a css translate — the
   decrypted css has no per-label rules; unblocks confident label placement).
2. The exact render affine (scale + margins) — the game's own css is now
   decrypted (`.worldmap-sea` = 8192²px planes of 512px slices; the render's
   margins/crop are the last unknowns).
3. `regioninfo` property bag (region polygons / child-key lists) for the
   dashed outlines' region grouping.
4. Only if mimicking the game's swap behavior: `bitmapposition` entry →
   node-id mapping (the entry structure itself is decoded — quadrant UV
   addressing of the `v_bitmap_*` swap textures).
