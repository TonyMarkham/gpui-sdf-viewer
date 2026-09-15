---
id: concept.game-staticinfo
kind: concept
title: The staticinfo table format
---

# The staticinfo table format

## What

The container format of every `gamedata/*.staticinfo{header,body}` pair in
pack 0008 — 134 tables of game facts (map geometry, regions, faction nodes,
UI markers, items, quests, …). Decoded first-party from the extracted bytes,
2026-09-14. This is the format all the map-placement docs
([[concept.game-map-layers]], [[concept.game-map-icons-poi]],
[[concept.game-map-labels]]) read through.

## Container

- **header**: `[u16 record count]`, then `count` index entries sorted by
  body offset. Entry stride is per-table: 6 bytes (`u16 key + u32 offset`)
  or 8 bytes (`u32 key + u32 offset`). 6-byte tables: regioninfo (1007
  records), fieldinfo (8). 8-byte tables: factionnode (1119),
  uimaptextureinfo (2025), bitmapposition (7). Distinguish by
  `header.len() == 2 + count × stride`.
- **body**: concatenated records; each record's extent runs to the next
  index offset (last runs to end of body).

## Record

    [u32 key][u32 nameLen][name bytes][NUL][property bag…]

- `key` is the stable id (regioninfo: 1, 2, 12, 14000…; factionnode:
  1000000+k; uimaptextureinfo: 5001+).
- The property bag is a TLV-ish sequence: property keys in the `0x0F____`
  range for the map tables, values typed (u32, f32, f32-triples, strings,
  nested names). The per-property tag table is not fully enumerated — every
  decoded fact so far comes from float-pattern scanning within record
  extents, which is sufficient for positions and bounds.

## Known record shapes

- **fieldinfo** (8 records): after name + a constant 4-byte tag
  (`6D D0 F6 04` in all records), four f32 bounds, then the map-window
  triple. `MainField_UIMap_Texture` (key 10): `-13350, -7120, -1050, 4070`
  then `-16384, 8192, 19456`. `MainField_Multi` (key 2): `±20000`.
- **factionnode** (1119): first world `(x, y, z)` triple within the first
  ~90 payload bytes in most records (1002/1119 yield one); extra nested
  triples (spawn points/waypoints) follow in some.
- **uimaptextureinfo** (2025): marker sprite records
  (`Actor_*`, `Abyss_*`, …); the abyss label records open with the world
  triple like factionnode. Records may embed nested names
  (`Knowledge_Node_…_ui_map_texture_3_0` variants) — the `1_0`/`3_0`
  suffix appears to distinguish map levels.
- **bitmapposition** (7): `Bitmap_FactionNode` etc.; dense cell tables,
  entry structure decoded in [[concept.game-map-icons-poi]].

## Schema skeleton (measured)

Per-offset byte histograms across every record of each table
(2026-09-14, offsets relative to the payload after name + NUL):

- **fieldinfo** — constant 4-byte schema tag `6D D0 F6 04` in **8/8**
  records, then the four f32 bounds at +4..20. A per-table schema id.
- **regioninfo** — constant 5-byte tag `11 10 01 00 00` in **1007/1007**
  records. At +9 a `0x0F`-high byte in 82% of records (a property id at a
  near-fixed position); at +13..+27 an ASCII digit run (a per-record
  timestamp like `202340204282128`) in most records.
- **bitmapposition** — constant 10-byte header
  `01 00 08 00 00 00 08 00 00 01` in **7/7** records before the entry data.
- **factionnode** — **no constant tag**: payload opens with a per-record
  `0x0F____` id (the +3 byte is `00` in 1119/1119, +2 is `0F` in 1046), then
  a variable property bag; nested sub-record names appear ~16 records deep
  in some.
- **uimaptextureinfo** — no tag: records open **directly with the world
  triple** (the +3 byte is the float's high byte: `C6`/`C5`/`C4` across
  1428 records — the x coordinate).

So the post-name area is **table-specific**: some tables carry a constant
schema tag, others a per-record id, others raw data. There is no universal
tag.

## The property id space

Property ids are u32s of the form **`0x0F0000 + u16`** — a 16-bit id space
prefixed with `0x0F` — and they sit at **byte-aligned (not word-aligned)**
positions inside the record. A 4-aligned scan misses them entirely (the
label-record ids sit at payload offsets like +37); scan every byte.

Measured inventories (byte-unaligned scan):

- **factionnode**: 10,630 hits; strongly recurring ids — `0x0F6955` in 936
  records (84%), `0x0F6956` (656), `0x0F6959` (251), `0x0F4240` (270),
  `0x0FA000` (264), `0x0F42C0` (205) — a real shared vocabulary on top of
  per-record ids.
- **uimaptextureinfo**: 1,290 hits; ids shared by at most ~5 records —
  mostly per-record.
- **regioninfo**: 2,465 hits, dominated by `0x0F0000` aliasing (the constant
  tag's `0F 00` fragment); the rest unique or ×2–4.

**The ids are allocated ordinals, not hashes.** Tested 2026-09-14 against
all 1119 factionnode (name, id) pairs: lookup3 (the repo's own variant) and
FNV-1a, raw and lowercased, hi/lo halves — zero matches. What the data shows
instead: alphabetically adjacent names get adjacent ids
(`HellwoodOuterGate`/`HellwoodSlaveCamp` = 0x4AB8/0x4AB9;
`HernandCastle`/`HernandEastGate` = 0x4250/0x4251) — sequential allocation
over grouped name lists (~63% ascending under name sort, with per-group
jumps). Recurring property ids are shared property-name entries in the same
table.

The table is keyed and localized, and the join is proven: the decrypted
`region.paloc` entries carry decimal ids of the form
`(regioninfo key << 32) | small` — key 1 (`Region_Pywel`) appears as
`4294967568` with text "Continent of Pywel" (EN, pack 0020) /
"Continente di Pywel" (IT, pack 0028).

Practical consequence: **a consumer never computes an id — it reads it from
the record.** To resolve an id to a name, join against the decrypted
keyed-string tables; to find a property, byte-scan a record for its id
(which is how every decoded fact in the map docs was produced).

## Record grammar, scoped

The `[u8 count][s32 −1][u32 id]` sequence observed in the abyss label
records is **uimaptextureinfo-specific**: 1,266 of 2,025 records contain the
`[-1][0x0F____]` pattern, and zero records of regioninfo, factionnode,
bitmapposition, or fieldinfo contain it. Do not treat it as a container
rule — it is a marker-record family convention.

## Reading it

Extraction is `extract` + the gamedata include patterns; the bodies are
uncompressed/lz4/partial DDS-wrapped like any entry
([[concept.game-data-pipeline]]). A reader needs only: parse the header
index, slice records, scan/parse property bags. No DDS, no paz access.
Note: the repo's `dds/partial.rs` reconstruct assumes 1 byte per pixel and
cannot read the 16-bit surfaces (see [[concept.game-map-icons-poi]]) — a
reader that tries bpp ∈ {1, 2, 4} handles everything. Encrypted entries
decrypt first (basename KDF), then lz4-decompress to `orig`.

## The `.staticinfomisc` variants

Exactly **one** exists in the entire install:
`gamedata/levelgimmicksceneobjectinfo_misc.base.staticinfomisc` (3.4 MB,
pack 0008). It decrypts (ChaCha20, proven 2026-09-14) and lz4-decompresses
cleanly to its `orig` size — and the container is a **related variant, not
the same container**: a `u32` count (164) followed by a flat sequence of
property-id-keyed blocks with embedded names — **755 names**, all
mission-gimmick records (`Mission_PororinVillage_Bell_All_Calphade`,
`…_Delesyia`, `…_Demeniss`, `…_Varnia`, …), average stride ≈ 4.5 KB. No
u16-key index, no `[u32 key][nameLen]` record headers. The per-field layout
inside a block is unparsed.

## Open items (buildable, not unknowns)

The toolkit for these items is [	ools/](../tools/README.md).

- [ ] Property-name resolution: build the id → name table by decrypting the
  remaining keyed-string tables (method proven; work, not discovery).
- [ ] Per-field parse of the `.staticinfomisc` blocks (container and domain
  characterized above).
- Cross-patch stability of the schema tags is a stated limitation —
  unknowable with a single install, not actionable here.
