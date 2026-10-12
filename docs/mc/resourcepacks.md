# Resource packs (mc_assets)

Answer: **yes, supported.** `lib/mc/mc_assets.fi`, test `tests/2812_mc_assets.fi`, `bash tools/mc/assets/run.sh` (`RESULT mc-assets: ok`).

## Format
ZIP with `pack.mcmeta` (JSON, `{"pack":{"pack_format":N,"description":...}}`) and `assets/minecraft/textures/block/<name>.png`
(also `.../item`, `.../entity`, `models/`, `blockstates/`, `sounds.json` + `sounds/**.ogg`, `lang/` -- later). Only block tiles are read today.

## Clean room / licence
* The PLAYER supplies packs locally at run time (file path / drag-and-drop). Nothing from any pack is committed, bundled or downloaded by us.
* We read no Mojang files and ship none. Tests use packs built in-process from own pixel patterns. Default look = our own `mc_texture` tiles.
* File/path names are a public format convention; the `tile_file` name table contains names only.

## Load flow (implemented)
1. `pack_open_file`/`pack_open_mem` -> ZIP reader (`lib/zip`, bounded, unsafe names refused) -> `pack.mcmeta` must exist and hold a number `pack_format`.
2. `stack_push` in priority order: **first pushed = top, wins** (max 8). Error codes: IO -1, ZIP -2, NO_MCMETA -3, BAD_MCMETA -4, FULL -5; garbage never crashes.
3. `assets_apply(stack, atlas)`: for every default tile with a pack name (ids 1..40) the top-most pack with a decodable PNG wins
   (`lib/paint/png`, limit 4096x4096); otherwise the default tile stays (fallback). A broken PNG in a high pack falls through to the next pack.
4. Hash tiles 41..56, unknown, test: no pack name, always default. Unmapped blocks use the hash tiles today (see texture.md).

## Atlas / tile sizes
* Atlas has ONE tile size (power of two, caller's choice). Each source image is nearest-resampled (first frame, `w x w`) to it.
* Recommended: tile size = max size over the loaded packs (clamped e.g. 64). Default tiles exist at 16 only -> for larger atlas they should be
  upscaled (nearest) when no pack provides a tile (NOT done yet: `atlas_new(32,...)` leaves non-pack tiles transparent).
* Later: multi-layer texture array (one layer per size class, 16/32/64/...), mipmaps, per-tile avg colour update (`atlas_avg`).

## pack_format per version
Source: NOT in the local minecraft-data (checked: no `pack_format` in `pc/*`). Values below are from general knowledge, NOT verified locally;
only 1.20.1 matters now. The loader treats `pack_format` as informational (no rejection), so a wrong table cannot break loading.
| MC | pack_format (resource) | status |
|---|---|---|
| 1.20 - 1.20.1 | 15 | unverified (own memory) |
| 1.19.4 | 13 | unverified |
| 1.20.2 | 18 | unverified |
| 1.21.x, 26.x | ? | gap -- take from a real server/launcher JSON or the wiki, then put in versions.json as data |
Plan: add `resource_pack_format` to `tools/mcgen/versions.json` per version once a source is confirmed; show "pack made for older/newer" as a warning only.

## Later
* **Animated textures:** `<tile>.png.mcmeta` (`animation`: frametime, frames, interpolate); strip PNG (height = n * width); needs per-frame atlas upload or layer cycling.
* **Models / blockstates:** JSON models (parents, elements, rotations, UV) -> mesher input; replaces the "hash tile" fallback and handles non-cubes.
* **Sounds:** `sounds=synth|original|both` in `mc_audio`: `synth` = our bank (default), `original` = pack `sounds.json` + OGG (needs Vorbis decoder, not present),
  `both` = pack overrides event by event, synth fills the rest.
* Pack list UI / persistence (order file in the user dir), reload without restart, `.zip` vs folder packs, `lang`, GUI textures (HUD font).

## Effort (rough)
Done: block tiles + priority (this). Size classes/upscale default + format table: 0.5 d. Animation: 1-2 d. Models/blockstates: 4-6 d.
OGG + sound.json + mc_audio modes: 3-4 d. UI + persistence: 1-2 d.
