---
id: concept.game-data-pipeline
kind: concept
title: From game data to app data
---

# From game data to app data

## What

The one-way transition that turns the raw Crimson Desert install into the data
the app renders. The extraction pipeline reads the game's paz packs, pulls the
worldmap SDF tile families out as DDS files, converts each layer to raw
mip-chain payloads, and writes a verified export — a versioned manifest plus
`.r8` tile payloads — into the app's data directory. Scene files then bind that
export by name and never touch the game again.

## The two data domains

**Game data** — the install directory (`paths.game_root`): paz archives under
`pack/<n>/`, indexed by a shared `0.pamt` table. Untrusted, read-only input:
nothing in the app writes there, and nothing parses it beyond what the pipeline
needs to make progress.

**App data** — the data directory (`paths.data_dir`, default
`<home>/.config/cd-map-offline/data`; relative values resolve against the
config dir): everything the pipeline produces. Machine-independent and fully
regenerable from game data + config; deleting it loses time, not information.

```text
<home>/.config/cd-map-offline/
  config.toml              # all tuning: packs, includes, routes, recipes
  data/
    dds/                   # intermediate: extracted game tiles, byte-verbatim
      manifest.json        # extract manifest: per-file provenance (sha256,
                           #   paz file, offset, sizes, flags)
    sdf/
      manifest.json        # the export manifest — the app-side trust anchor
      *.r8                 # per-tile raw mip chains, DDS header stripped
```

The config home is rooted at the OS home (`~/.config/cd-map-offline`),
deliberately not `dirs::config_dir()`. The extract manifest records game-data
provenance but is **not trusted**; the export manifest **is** the trust anchor —
"nothing is inferred from file sizes" holds on both sides.

### The install, and what lives in it

The install's pack map — which of `0000`–`0035` holds what, the per-entry
encryption reality, and the localization packs — is
[[concept.game-install-packs]]. What the game's map layers, icons, and labels
are, and how far each is decoded, lives in [[concept.game-map-layers]],
[[concept.game-map-icons-poi]], [[concept.game-map-labels]], and
[[concept.game-staticinfo]] (the gamedata table format the placement data
reads through). What it would take to compose the reference abyss map is
[[concept.abyss-map-composition]].

## Who

- `sdf-offline` — the whole pipeline as a library: `extract_run` + `field_run`,
  a pure function of config + game root → data-directory contents (re-runs
  overwrite; there is no incremental state).
- The GUI shell (sdf-ui `OfflineState`) — owns the config, validates the
  install, runs the pipeline on a thread, watches for completion.
- The CLI (`cd-map-offline extract`) — the identical pipeline headless.
- Scenes + the component (sdf-component `SceneData`) — the consumers; they
  verify the export structurally at load and per-payload at upload.

## When

- Setup gates everything: until `paths.game_root` validates (every configured
  pack carries its `0.pamt`) **and** the export manifest exists
  (`data_present`), the setup panel is the first view.
- Extraction is user-triggered (GUI button or CLI command), not automatic.
- After a GUI extraction completes, the scene library is re-discovered and the
  composite rebuilt; scene selection opens on the first file scene.

## Flow

### 1. Config and install validation

Config lives at `<config home>/cd-map-offline/config.toml`; a missing file
yields the defaults (the shipped template is written on first save, as a
format-preserving edit so hand-tuned sections survive). The pieces that steer
the transition:

- `[extract] packs = ["0012"]` — the map-screen pack. Its worldmap tile
  families are unencrypted (verified per-entry); the pack's worldmap *UI* files
  are not (see the scan above).
- `[extract] includes` — glob patterns (case-insensitive, matched against the
  full entry path or the file name) selecting the worldmap tile families.
- `[sdf.tiles]` + `[sdf.export.extra]` — the export routes: layer name →
  tile-name prefix. A prefix may back several layers (coast and river share the
  land field); an `extra` name colliding with a `tiles` key is a config error.
- `Config::check()` runs before any pipeline I/O: non-empty includes, every
  route prefix covered by an include, route-name collisions, composite layer
  sanity.

### 2. Extract — paz → dds

Per pack: parse `pack/<pack>/0.pamt` (folder prefix + node tree + 20-byte
records: node ref, offset, comp size, orig size, flags; the flags' low byte
selects the paz file, bits 16–19 compression, bits 20–23 crypto). Entries
matching an include glob are read — seek into the paz, optional ChaCha20
decrypt (key derived from the file name; dormant for the extracted families —
the map tiles in 0012 and the staticinfo tables in 0008 are all unencrypted),
optional decompress (lz4 block, or the DDS-specialized "partial" multi-chunk
lz4 reconstruction) — and the verbatim bytes land at
`<data_dir>/dds/<entry path>` (path-traversal-guarded join). One extract-manifest
record per file: path, paz file, offset, sizes, flags, sha256 of the extracted
bytes.

### 3. Field — dds → r8

Per unique route prefix, from the extract manifest's records:

1. Select `<prefix>*.dds`, parse tile coordinates from the trailing `_x_y`
   (the last two `_`-separated integers; coordinate-less names are ignored with
   a progress note), and require the 16×16 grid covered exactly once (missing
   slots are listed in the error).
2. Per tile: re-verify the file's sha256 against the extract manifest, census
   the DDS header (uncompressed 8-bit only: fourcc zero, bit count 8), classify
   by size — 512² = mip0 tile, 8² = stub (its payload must be constant-fill) —
   and read the mip-chain length from the first mip0 tile's declared count.
   Every mip0 tile in the layer must declare the same count; stubs carry their
   own 1..=4 range.
3. Strip the header (`[extract] dds_header`, 128); the remaining payload must
   be exactly `chain_len(edge, mip_count)` bytes.

Output per prefix: the layer's mip count plus one payload per grid slot —
name = DDS stem + `.r8`, x, y, kind, source sha256, raw bytes.

### 4. Export — manifest + payloads + round trip

`<data_dir>/sdf/` gets the `.r8` payloads and `manifest.json`:
`format: "cd-map-sdf-field"`, `version: 1`, one layer per route — `name`,
`tile_prefix`, `size` (16·512 = 8192, the field edge), `source_manifest_sha256`
(sha256 of the extract manifest — provenance back to the exact game read),
`mips` (`level`, `size = 512 >> level`, `bytes = size²`), `tiles` (`path`,
`payload`, `x`, `y`, `kind`, `source_sha256`, `payload_sha256`).

Before declaring success the pipeline **round-trips its own output**: the
just-written manifest is re-loaded and cross-checked layer-by-layer against the
in-memory fields (grid size, mip sizes, mip count, tile count, payload names,
mip0 payload length = Σ mip bytes), then every payload is re-read from disk and
byte-compared — the first differing byte is named in the error. The pipeline
rehearses its own consumption before reporting success.

### 5. Consumption — `config:` scheme → SceneData

The GUI's `data_present` is literally "the export manifest exists". Scenes bind
the export by name:

```text
// sdf-scene: data = "config:sdf/manifest.json"
// sdf-scene: layer = "coast"          (or: layers = "coast,river,road")
```

The host shell rewrites `config:<remainder>` to the resolved absolute path
under the data directory before the component parses the source (Windows drive
paths are not schemes; any other scheme is a parse error naming the value).
`SceneData::load` then re-verifies the manifest structurally — format tag,
version, tile table covering the grid exactly once, mip chain halving from
level 0, byte lengths matching level sizes, shared geometry across a scene's
fields — and the renderer verifies every payload's sha256 and length at upload.
The GUI parses no DDS, ever. The scene side of this hand-off is
[[concept.sdf-scene-contract]]; the render side is [[interaction.sdf.render-frame]].

The CLI drives the same consumption independently: `cd-map-offline extract
--game-root … --data-dir … [--save-config]` runs the identical pipeline with
flag-overrides applied over the config.

## The verification chain

Every hop between domains is checked; a corruption fails at the hop with a
named error, not downstream with a blank map:

| Hop | Check | Where |
|---|---|---|
| paz bytes → extracted DDS | sha256 recorded in the extract manifest | extract writes it; field re-verifies |
| DDS → payload | header census (uncompressed 8-bit), constant-fill stubs, per-layer mip-count agreement, exact chain length | field |
| payload → disk | byte-for-byte round trip after write | export |
| manifest → loader | structural validation (format, version, tables, shared geometry) | `SceneData::load` |
| payload → GPU | sha256 + declared-length check | renderer upload |

## Why

- **A hard schema boundary between a closed game and an open app.** The game
  formats (paz, pamt, partial DDS) are reverse-engineered and untrusted; the
  export manifest is small, versioned, and self-describing. Only the pipeline
  knows game formats — the app side consumes JSON + raw bytes, so a game-format
  surprise can never break scene loading directly.
- **Trust by verification, not assumption.** Each hop re-checks the previous
  one, and the pipeline round-trips its own output before reporting success —
  extraction-time errors name the file, layer, slot, or byte involved.
- **Regenerability over caching cleverness.** `dds/` is a pure intermediate;
  the export is derived deterministically from game + config, so there is no
  incremental state to corrupt and re-runs are safe.
- **Machine independence.** Manifest payload names are directory-relative; the
  data dir moves with the config home, and scenes resolve through `config:` at
  load time — no absolute paths baked into scene files.

## Needs elaboration

- [x] The ChaCha20 decrypt path is **proven against real encrypted entries**
  (2026-09-14): `worldmapicon.css`, `worldmapview.css`,
  `cd_worldmap_regiontitleimage_eng.xml`, `region.paloc`, and
  `levelgimmicksceneobjectinfo_misc.base.staticinfomisc` all decrypt to
  valid, readable content. The stored-entry pipeline is
  **orig → lz4 → encrypt** (decrypt first, then lz4-decompress to the
  `orig` size); the `orig` field is the decompressed size, which is why it
  can exceed `comp_size`. The key derives from the entry's lowercased
  basename.
- [x] Which localization pack is which language: identified by decrypting
  `region.paloc` from all 15 packs and reading the text — **EN = 0020**
  ("Continent of Pywel", "City of Pailune"), 0026 FR, 0027 DE, 0028 IT,
  0029 PL, 0023 TR, 0024/0025/0030 the other Latin variants, and six
  CJK/Thai packs (0019, 0021, 0022, 0031, 0032, 0033). (2026-09-14.)
- [x] Compression fields 3 (`Custom`) and 4 (`Zlib`) decode to named errors
  but never occur: a full-install scan (2026-09-14) observed only 0 (none),
  1 (partial), and 2 (lz4) across every pack.
- [ ] No cancel and no incremental extraction: a re-run redoes everything. Is
  that acceptable at the full install's scale, or will the extract step need a
  skip-when-sha256-matches shortcut?
- [ ] The extract manifest's provenance fields (paz file, offset, sizes,
  flags) are recorded but nothing downstream consumes them yet — keep until a
  debugging need proves otherwise?

Layer- and placement-specific open questions live in the per-subject docs

The scanners that produced every decoded fact — and the per-item commands for working what remains — live in [	ools/](../tools/README.md).
([[concept.game-map-layers]], [[concept.game-map-icons-poi]],
[[concept.game-map-labels]], [[concept.game-staticinfo]],
[[concept.abyss-map-composition]]).
