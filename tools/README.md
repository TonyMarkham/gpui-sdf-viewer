# Investigation toolkit

Standalone scanners used to produce every decoded fact in
`docs/concept.game-*` and `docs/concept.abyss-map-composition.md`. They read
a Crimson Desert install read-only; nothing here touches the app's crates or
the game files. The derived artifacts (CSVs, decrypted files, PGMs, exes)
are gitignored — regenerate them, never commit them.

Each open item in the docs maps to a tool below — see the mapping table at
the bottom.

## Build

Any Rust toolchain (the repo's crates are untouched; these are standalone
`rustc` one-file programs):

```sh
rustc --edition 2021 -O -o pamt-dump.exe pamt-dump.rs
rustc --edition 2021 -O -o crypt.exe crypt.rs
rustc --edition 2021 -O -o gamedata-read.exe gamedata-read.rs
rustc --edition 2021 -O -o staticinfo-parse.exe staticinfo-parse.rs
rustc --edition 2021 -O -o staticinfo-schema.exe staticinfo-schema.rs
rustc --edition 2021 -O -o anchors.exe anchors.rs
rustc --edition 2021 -O -o field-asm.exe field-asm.rs
rustc --edition 2021 -O -o fit.exe fit.rs
rustc --edition 2021 -O -o hash-test.exe hash-test.rs
rustc --edition 2021 -O -o grammar-misc-probe.exe grammar-misc-probe.rs
```

`classify-reference.ps1` needs PowerShell + System.Drawing (built into
Windows PowerShell 5+).

## Prerequisites

1. `ROOT` = the install directory (contains `0000/`…`0035/`, `bin64/`,
   `meta/`).
2. Entry CSVs for the packs you work with — `staticinfo-parse`,
   `crypt`, `gamedata-read`, and `anchors` read them:

   ```sh
   pamt-dump.exe <ROOT> 0008          # writes dump-0008.csv
   pamt-dump.exe <ROOT> 0012          # writes dump-0012.csv
   ```

   CSV columns: `path,orig,comp,flags,offset`.
   Flags: bits 0–7 = paz file index, 16–19 = compression
   (0 none / 1 partial / 2 lz4), 20–23 = crypto (3 = ChaCha20).

3. Decrypted staticinfo tables for the staticinfo tools (the decrypt +
   lz4 chain is `crypt`, see below), saved as `<prefix>-header.bin` /
   `<prefix>-body.bin`. Known-good sizes: regioninfo header 6,044 B
   (1007 records), factionnode 8,954 B (1119), uimaptextureinfo 16,202 B
   (2025), bitmapposition 16,202-ish (7), fieldinfo 66 B (8).

## Tools

Argument orders are copy-paste exact — the staticinfo tools take a file
*prefix* (e.g. `gd-ui`, resolving `<prefix>-header.bin` / `<prefix>-body.bin`),
and `staticinfo-schema` takes the mode *before* the files.

| Tool | Usage |
|---|---|
| `pamt-dump.exe` | `pamt-dump <ROOT> <pack>` → `dump-<pack>.csv`; `pamt-dump <ROOT> grep <name>` sweeps all packs |
| `crypt.exe` | `crypt dec <ROOT> <pack> <csv> <name-substr>` (head preview) / `crypt save .` (writes the plaintext next to the CSV - stays inside tools/, gitignored) |
| `gamedata-read.exe` | `gamedata-read <ROOT> <pack> <csv> <out-prefix> <path> [path…]` |
| `staticinfo-parse.exe` | `staticinfo-parse <prefix> <mode> [args]` — modes: `records`, `dump <substr> [n]`, `dumpat <i>`, `scan <substr>`, `triples`, `find <f32>` |
| `staticinfo-schema.exe` | `staticinfo-schema schema <header.bin> <body.bin> [max-off]` / `staticinfo-schema keys <header.bin> <body.bin>` |
| `anchors.exe` | `anchors <body.bin> <header.bin> <name-substr> [substr…]` |
| `field-asm.exe` | `field-asm <data_dir> <tile_prefix> <out.pgm> [threshold]` |
| `fit.exe` | after `classify-reference.ps1` (writes `ref256.cls`/`ref256.cf` next to `land256.pgm`) |
| `hash-test.rs` | `hash-test <fn-header.bin> <fn-body.bin>` — the id-is-not-a-hash falsifier |
| `grammar-misc-probe.rs` | `grammar-misc-probe` — grammar scoping across tables + `.staticinfomisc` name probe |

Worked example (smoke-tested 2026-09-14):

```sh
pamt-dump <ROOT> 0012
staticinfo-parse gd-ui triples | Select-String Abyss_Sleet
# -> Knowledge_Node_Abyss_Sleet_Isles_…_1_0   -10817.823  946.5815  -449.1921  @60
crypt save <ROOT> 0012 dump-0012.csv worldmapview.css   # decrypt + lz4
staticinfo-schema schema gd-fn-header.bin gd-fn-body.bin 12
anchors gd-ui-body.bin gd-ui-header.bin Abyss_Triangle_Ring
```

## Open item → tool mapping

| Open item (doc) | How to work it |
|---|---|
| Exact render affine (game-map-icons-poi) | `classify-reference.ps1` → `fit.exe`; the decrypted `worldmapview.css` (via `crypt save`) carries the game's own plane numbers (8192²px planes of 512px slices) |
| Sleet_Isles anchor outlier (game-map-labels) | Re-extract with `anchors.exe`, diff against the reference label position; not a css translate (verified — no per-label rules in the decrypted css) |
| Reconcile 227 textures vs the xml registry (game-map-labels) | `crypt save … regiontitleimage_eng.xml`, count `GetRect` entries, diff against the `dump-0012.csv` regiontitle list |
| paloc key ↔ region id binding (game-map-labels) | `crypt save` a `region.paloc`; ids are decimal `(regioninfo key << 32) \| small` — key 1 appears as `4294967568` |
| Property-name resolution (game-staticinfo) | `crypt save` the keyed tables (palocs decrypt cleanly; EN pack = 0020), parse `[ASCII decimal id][UTF-8 text]` entries, join ids |
| Per-field parse of `.staticinfomisc` (game-staticinfo) | `crypt save` it (0008), then `grammar-misc-probe.exe` for the name structure (755 names, ≈4.5 KB stride) |
| bitmapposition end-to-end (game-map-icons-poi) | `staticinfo-parse dump Bitmap_FactionNode` — entries are quadrant-UV pairs (10.6 fixed, +255.0 step, wrap 1024.0 + group index) |

## Verified facts these tools established

- ChaCha20 KDF: seed = lookup3(lowercased basename, 0x000C5EDE); key =
  `seed ^ 0x60616263 ^ delta` ×8 (deltas in `crypto.rs`); nonce = seed ×3;
  keystream seek = `seed × 64`. Stored pipeline: **orig → lz4 → encrypt**.
- All 15 localization packs decrypt; **EN = 0020** ("Continent of Pywel").
- The `.staticinfomisc` file decrypts to a u32-count container with 755
  embedded mission-gimmick names.
- `factionnode` post-name ids are allocated ordinals (NOT hashes — see
  `hash-test.rs`); the paloc join is `(regioninfo key << 32) | small`.
- The game's own renderer: 8192²px planes of 512px slices per layer
  (decrypted `worldmapview.css`) — the same 16×16 tile grid the pipeline
  exports.
