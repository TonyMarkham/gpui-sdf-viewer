---
id: concept.game-install-packs
kind: concept
title: The game install and its packs
---

# The game install and its packs

## What

The layout of a Crimson Desert install: numbered paz pack directories, each
indexed by a shared `0.pamt` table. The content attribution below is scanned
first-party from a full install's pamt tables (paths, sizes, flags — no
content decoding), 2026-09-14.

## Pack map

34 packs, `0000`–`0035` with no `0018`/`0034`. Entry counts and uncompressed
sizes from the pamt tables:

| Packs | Content | Size |
|---|---|---|
| `0000` | `object/` — world props & gimmicks (pam meshes, prefabs, havok) | 37.6 GB |
| `0001` | `tree/` — vegetation | 4.2 GB |
| `0002` | `texture/` — shared/reference textures | 0.3 GB |
| `0003` | `technique/` — render techniques, `.material`s | — |
| `0004` | `sound/` — main Wwise bank pack (`.wem`/`.bnk`, per-language soundbank info) | 7.3 GB |
| `0005`, `0006`, `0035` | `sound/` — three parallel voice/audio sets | ~1.7 GB each |
| `0007` | `effect/` — VFX | 2.2 GB |
| `0008` | `gamedata/` — 134 staticinfo tables + navigation + saves | 2.2 GB |
| `0009` | `character/` — models, skeletons, animations | 49.3 GB |
| `0010` | `actionchart/` — animation action charts | 0.4 GB |
| `0011` | `miscellaneous/` — ICU data, splines, IES profiles | 0.1 GB |
| `0012` | `ui/` — the worldmap SDF tiles, map UI, css/thtml, video | 6.7 GB |
| `0013` | `aiscript/` — two `.pai` files | — |
| `0014` | `sequencer/` — cutscene/quest sequencer data | 0.5 GB |
| `0015` | `leveldata/` — level geometry/tiles | 51.9 GB |
| `0016` | a single `paconfig.txt` | — |
| `0017` | `shadercache__/` — compiled shader cache | 3.5 GB |
| `0019`–`0033` (no `0018`) | localization: 15 packs, each the same 39 `gamedata/*.paloc` tables, **all entries ChaCha20-encrypted** | ~0.02 GB each |

The two packs the app reads are `0012` (map layers, textures, UI) and `0008`
(the staticinfo tables). The map layer inventory is
[[concept.game-map-layers]].

## Encryption and compression

- **Encryption is per-entry, not per-pack.** Unencrypted: every map layer
  texture in `0012`, every `.staticinfo` table in `0008`, the `0.pamt` paths
  of the localization packs. Encrypted (ChaCha20, flags bits 20–23 = 3):
  every worldmap/minimap **layout** file in `0012` (all css/thtml/html/xml),
  the `.staticinfomisc` file and a `gamedata/` subset of `0008`, and every
  localization `.paloc`.
- **The decrypt path is proven** (2026-09-14): the key derives from the
  entry's lowercased basename; the stored pipeline is orig → lz4 → encrypt,
  so a reader decrypts first, then lz4-decompresses to the `orig` size.
  Verified end-to-end on `worldmapicon.css`, `worldmapview.css`,
  `cd_worldmap_regiontitleimage_eng.xml`, `region.paloc`, and the
  `.staticinfomisc` file — all produce valid, readable content.
- **Compression** (flags bits 16–19) across the whole install: 0 (none),
  1 (partial — the DDS-wrapped per-mip lz4 form), 2 (lz4). Fields 3/4 decode
  to named errors and never occur.
- Flags bits 0–7 select the paz file (`<pack>/<flags & 0xFF>.paz`).

## The localization packs

Identified by decrypting `region.paloc` from each and reading the text
(2026-09-14): **0020 = English** ("Continent of Pywel", "City of Pailune"),
0026 French, 0027 German, 0028 Italian, 0029 Polish, 0023 Turkish,
0024/0025/0030 the other Latin variants (es/pt family — "Continente de
Pywel"), and 0019/0021/0022/0031/0032/0033 the six CJK/Thai packs (no Latin
"Pywel" match). The paloc format is `[ASCII decimal id][UTF-8 text]`
entries.
