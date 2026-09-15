---
id: concept.game-map-labels
kind: concept
title: Map labels (region titles)
---

# Map labels

## What

Every text rendered on the world map — region names, territory names, the six
abyss region names — and where it comes from. First-party evidence, 2026-09-14.

## Region hierarchy: `regioninfo.staticinfo`

1007 records; the first-party record layout is [[concept.game-staticinfo]].
Record names cover the full region tree:

- `Region_Pywel` (key 1 — the world; "…OF GREAT PAYWEI" on the map edge is
  this region's title) → `Region_Kweiden` (2), `Region_Pailunese_Territory`
  (12), `Region_Pailunese` (14000), …
- The four factions/kingdoms and their territories:
  `Region_Hernand` / `Region_Hernandian_Territory`,
  `Region_Demeniss` / `Region_Demenissian` / `Region_Demenissian_Territory`,
  `Region_Delesyia` / `Region_Delesyian_Territory`,
  `Region_Pailunese` / `Region_Pailunese_Territory`
- Abyss: `Region_Abyss`, six stage regions
  `Region_Node_Abyss_{Corridor, MechHeart, RecordFleet, SkyPillars, SteelSail,
  StreamSky}`, twelve island regions `Region_Node_Abyssone_0001..0201`, and
  `Region_Node_Abyssone_End_*`
- `Region_Crimson_Desert` and the ~200 landscape regions the reference land
  map titles.

Each record is a property bag; the child-key lists (u16 arrays) inside the
records define the hierarchy. Resolving the full tree is mechanical once the
property tags are enumerated.

## Label textures

227 `ui/cd_worldmap_regiontitle_<id>_eng_sdf_<W>x<H>.dds` files (221 numeric
region ids + 6 abyss areas), all `_eng`, all R8 mip chains. The name's
`<W>x<H>` is the CSS layout size; the texture is exactly ÷4 — named
`1024x128` is a 256×32 DDS, named `2048x2048` is a 512² DDS. The `_eng`
suffix plus the absence of any other language's DDS suggests other languages
are font-rendered at runtime (the localization packs carry no DDS).

## Positioning source of truth

**Abyss labels: `uimaptextureinfo.staticinfo` — anchors extracted.** Every
`Knowledge_Node_Abyss_*_ui_map_texture` record carries its world anchor
`(x, y, z)` at a fixed offset immediately after the record name (byte
`8 + nameLen + 1`; the container format is [[concept.game-staticinfo]]).
All twelve are extracted:

| Record | Anchor (x, y, z) | Altitude band |
|---|---|---|
| `…Abyss_Sleet_Isles…` | (-10817.8, 946.6, -449.2) | land |
| `…Abyss_Triangle_Ring…` | (-9195.5, 706.9, -2462.8) | land |
| `…Abyss_Wanderers_Way…` | (-5740.6, 558.2, -2281.8) | land |
| `…Abyss_Path_Of_Providence…` | (-9707.8, 528.2, -4481.9) | land |
| `…Abyss_Hyper_Space…` | (-4994.9, 407.0, -4704.2) | land |
| `…Abyssone_0002…` (and every island, both `1_0`/`3_0`) | (-10583, 1802, -3781) | abyss |
| `…Abyss_MechHeart…` | (-4310.0, 1797.0, -4521.0) | abyss |
| `…Abyss_SteelSail…` | (-3506.0, 1889.0, -5461.0) | abyss |
| `…Abyss_SkyPillars…` | (-4822.0, 1953.0, -5233.0) | abyss |
| `…Abyss_StreamSky…` | (-5719.0, 1874.0, -4028.0) | abyss |
| `…Abyss_RecordFleet…` | (-3699.2, 2028.8, -4566.3) | abyss |

Two clean altitude bands: the six **region** labels anchor at land altitude
(y ≈ 400–950); the five **stage** nodes and the twelve islands anchor at
abyss altitude (y ≈ 1797–2053). The region labels therefore anchor where
their zone meets the land map, not at abyss elevation — which also explains
why the region-label records exist only as `1_0` (one map level) while the
stage/island records carry both `1_0` and `3_0`.

**Vertical convention: settled (z_top = screen top).** Projecting under
`u = (x − x_min)/span, v = (z_top − z)/span`:
- land map: `Node_Dem_DemenissCastle` (-8062.0, -3539.0) → (u=0.428, v=0.603);
  the reference render's Demeniss city art sits at (≈0.455, ≈0.605) — the
  vertical agrees to ±0.002.
- abyss map: `Triangle_Ring` → (0.369, 0.547) vs its reference label at
  ≈(0.345, 0.50); `Path_Of_Providence` → (0.343, 0.650) vs ≈(0.30, 0.70) —
  within eyeball precision of the render.
- the flipped convention collapses to chance in a land/sea image test
  (49.2% vs 75.0% for A on the reference render's confident ocean cells;
  base rate 97.9% all-sea — so A is the only hypothesis that beats both the
  flip and the trivial baseline).

**Land labels: consumable (decrypted 2026-09-14).**
`cd_worldmap_regiontitleimage_eng.xml` decrypts and decompresses to a clean
`<Texture Name=… Filename=… GetRect="0,0,w,h"/>` registry — and its `GetRect`
values confirm from the game's own metadata that the drawn rect is exactly
the ÷4 texture size (named `2048x2048` → `GetRect 0,0,512,512`; named
`2048x256` → `0,0,512,64`). The label TEXT comes from pack 0020's
`region.paloc` (EN, decrypted; format `[ASCII decimal id][UTF-8 text]`),
keyed to the region ids. The worldmap css decrypts too — the game's own
renderer is `8192×8192px` planes of **512px slices** per layer (`.worldmap-sea`,
`.worldmap-bg-slice`, `.worldmap-mountain-slice`, `.worldmap-road-slice`,
`.worldmap-height-line-slice`, `.worldmap-abyss-background-hex` — the same
16×16 tile grid the pipeline exports), which independently confirms the
window mapping.

**Remaining for pixel-exact placement:** the Sleet_Isles outlier (below) and
any per-label translate the html applies. There is no per-label css rule for
the region titles (checked the decrypted css).

## The localization table

Label text is not in the texture names — the `_eng` SDFs are baked glyph
rasters keyed by region id. The spoken/written names live in the
`region.paloc` table of the language packs (0019–0033), all of which are
ChaCha20-encrypted; the EN pack is not identifiable from the pamt tables
alone (see [[concept.game-data-pipeline]]).

## Open questions

The toolkit for these items is [	ools/](../tools/README.md).

- [ ] Why does `Sleet_Isles` anchor at (-10817.8, 946.6, -449.2) — far from
  its reference render position — while `Triangle_Ring` and
  `Path_Of_Providence` project onto their labels? Not a css translate (the
  decrypted css has no per-label rules); the anchor semantics of that one
  record are unexplained.
- [ ] Reconcile the 227 title textures with the `regiontitleimage_eng.xml`
  entries now that it is decrypted (count the registry and diff against the
  texture list).
- [ ] The paloc key ↔ region id binding: the localization ids and the
  regioninfo keys should join — one worked example to confirm.
