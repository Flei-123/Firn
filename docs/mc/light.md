# Vertex light (mc-r2)

## Vertex format (8 bytes, unchanged size)
word0 = x | y<<8 | z<<16 | face<<24 | ao<<27 (unchanged)
word1 = tile(14) | u<<14 (5) | v<<19 (5) | sky<<24 (4) | block<<28 (4)

* tile: 1..16383 (`MAX_TILE`); larger ids from the tables are clamped. GL_MAX_ARRAY_TEXTURE_LAYERS is 256 min / 2048 typical, so 14 bit is never the limit.
* u, v: 0..16 in a greedy quad (5 bit). mc_lod cuts quads longer than 31 into strips.
* Decision 8 B vs 12 B: 8 B kept. A 12 B vertex = +50 % VBO (terrain section: 48 KB -> 72 KB) for nothing; the nibbles fit.
* Measured (tools/mc/mesh/run.sh, terrain section, greedy+AO): without light 754 quads / 42224 B, with smooth light gradient 854 quads / 47824 B (less merging); b50 random: 7710 -> 7734 quads. Meshing speed terrain 1128 -> 936 sections/s (llvmpipe/CPU, noisy).

## Mesher input
`mesh_padded_lit(m, pad, lpad, tables, flags | F_LIGHT)`: lpad = u8 [5832], same index as the padded blocks, sky<<4 | block. `LIGHT_DEFAULT` = 0xF0 (sky 15). Without F_LIGHT every corner is sky 15 / block 0.
Corner light = average of the non-opaque cells of the 2x2 patch in front of the face touching the corner (front cell, 2 side cells, diagonal; diagonal skipped when both sides opaque; unknown cells skipped), sky and block separately, rounded half up. AO stays separate.
Greedy merge key = tile + (ao, sky, block) of all 4 corners; merging along an axis needs equal corner values along it (same rule as AO). Quad diagonal: brighter pair by max(sky, block) * (100 + 52 ao).

## Shading (mc_raster, mc_fs.glsl, glweb_fs.glsl, ref.py: identical integer arithmetic)
s256 = floor(sky * day / 15 + 0.5), b256 = floor(block * 256 / 15 + 0.5); per channel lum = max(s256, b256 * tint_c >> 8), lf = 24 + (lum * 232 >> 8); total_c = (fq * face_k >> 8) * lf >> 8.
Uniforms: day 0..256 (sky light multiplier, e.g. mc_sky daylight), tint r g b 0..256 (default 256 224 176 = warm block light). `raster_set_light`, `glbe_set_light`, `glweb_set_light`. day 256 + sky 15 + block 0 gives lf = 256: old images are byte identical.

## Light engine (lib/mc/mc_light.fi)
Dense volume (`LightVol`, local cell coordinates, per cell op 0..15 / emit 0..15 / sky / block). The light is the unique fixpoint
`L(c) = max(base(c), max_n L(n) - max(1, op(c)))`; block base = emit, sky base = 15 for SKY-DIRECT cells (op 0 for the cell and everything above it in the column; open sky above the volume). Nothing flows in from outside the volume.
`light_set_block(v, x, y, z, op, emit)`: removal pass (cells derived from the old value are cleared or fall back to their own base, the brighter rest becomes a source) then add pass (queue flood fill). A sky-direct flip re-lights the air column below the cell. `light_full` = oracle / initial load. `light_take_dirty` returns the 16^3 sections whose padded 18^3 light may have changed (cell + neighbour sections on a section face); `light_pad_section` writes the u8 [5832] for `mesh_padded_lit` (outside the volume = 0xF0).
Tests: tests/2831_mc_light_fuzz.fi (6 seeds x 120 steps of 1-3 random edits on a 32^3 volume, incremental == full recompute, dirty covers every changed padded section, hand-computed values), counter-check in `bash tools/mc/light/run.sh` (`RESULT mc-light: ok`).
Measured (llvmpipe box, 64^3 terrain, release): full recompute 47 ms (262144 cells); single break/place near the surface ~6 us (~18 cells written); torch place + remove ~1.6 ms per pair.

## Light from packets (lib/mc/mc_lightpkt.fi)
`lightdata_decode_118` (update_light 1.18+ with position; also the light tail of a 1.18+ map_chunk without position), `lightdata_decode_114` (1.14 - 1.16.5 update_light, int masks, no count prefix). Mask bit i = section i-1 (bit 0 = section below the world), arrays 2048 octets, even cell index in the low nibble. `lightdata_apply(vol, ld, cell_x0, cell_z0)` copies the arrays into a LightVol (volume section s = bit s+1; masked-empty = 0; absent sections untouched) and marks sections dirty. `mc_chunk.scratch_set_keep_light(sc, true)` keeps the light of a 1.18+ column in `Chunk.light` (default off: counts + hash only, memory). Test tests/2832_mc_light_packet.fi (hand-built packets; no live server fixture yet).

## Light in Stream / Render (lib/mc/mc_wlight.fi, mc-s1)
`mc_wlight.WorldLight` = light source of a chunk based world. Per padded cell the first source that knows it wins:
1. **region volume** (`wl_region_load(wl, world, cx0, cz0, ncx, ncz, from_packets)`): a `mc_light.LightVol` over ncx x ncz columns, cell data from the block ids (`wl_set_tables` / `wl_set_tables_registry`: filterLight / emitLight per state id), light = `light_full`, then (from_packets) the `Chunk.light` arrays overwrite the computed values. 5 octets per cell (7x7 columns x 6 sections = 1.5 MB, 9x9 x 24 sections = 40 MB: size the region to the near ring).
2. **Chunk.light** (`LightData` of a 1.18+ map_chunk with `scratch_set_keep_light`, or filled from update_light): bit q+1 = section q; array missing + not in the empty mask = sky 15 / block 0, empty mask = 0.
3. unknown = 0xF0 (sky 15, block 0): column not loaded, no light data, section out of range.
`wl_pad(wl, chunks[9], cx, sy, cz, out)` builds the u8 [5832] pad and returns false when nothing but defaults was seen: stream / wr then mesh WITHOUT F_LIGHT (identical vertices, cheaper) - so a world without light data renders byte identical to before (test D).
* Stream: `stream_set_light(s, true, &wl)` (remeshes loaded columns), the snapshot (render thread) fills `R_LPAD` of the job record, `mesh_one` calls `mesh_padded_lit(.. | F_LIGHT)` when `R_LIT`. `stream_set_daylight(s, b, day)` -> `Backend.set_light` (CPU raster_set_light, GL glbe_set_light / glweb_set_light) each frame with `mc_sky.daylight`; no remesh.
* mc_render_world: `wr_set_light(wr, &wl)`, `wr_set_daylight(b, day)`, `wr_mark_section`, `wr_light_sync`.
* Edits: `wl_edit(wl, world, x, y, z, id)` = `mc_blockset.world_set_block` + `light_set_block` (incremental); then `stream_mark_block` (block shell) and `stream_light_sync(s)` / `wr_light_sync(wr)` (dirty light sections from `wl_take_dirty`) before the next frame.
* Tests: tests/2885_mc_stream_light.fi (stream inline / 4 workers, wr, Chunk.light only, no light data, edit; all images == reference built from `light_pad_section` + `mesh_padded_lit`; fixed day / night hashes; incremental volume == full recompute after an edit).
* Live: `MC_KEEP_LIGHT=1` makes demos/mcclient keep Chunk.light and print `LIGHTKEPT ok=N bad=M` (kept arrays hash == packet octets hash; the dump hash itself is compared with prismarine by tools/mcclient/run.sh).

## update_light wiring + moving region (mc-t3)
* 1.14 - 1.16.5: `mc_world.world_update_light(w, chunk_format, protocol, body)` decodes one update_light and merges it into `Chunk.light` (`chunk_apply_light_update`: an array replaces the one of the same section bit, an empty bit drops it). The vanilla server sends update_light BEFORE the map_chunk of the column, so light for a column that is not stored yet is kept in `World.pend` (64 columns, merged per column, result `ULIGHT_PENDING`) and attached by `world_put`. A chunk sent again starts without light (the server sends a new update_light first). demos/mcclient: `MC_KEEP_LIGHT=1` + stderr `UPDATELIGHT applied/pending/orphan/bad`; `ref_dump.cjs` has the same pending logic (own parser). Test tests/2906.
* Moving region: `mc_wlight.wl_region_recenter(wl, w, pcx, pcz, ncx, ncz, margin, from_packets)` keeps the region while the player column is `margin` columns inside it, else reloads it centered (0 kept / 1 moved / 2 first load). Memory stays `ncx*ncz*nsec*4096*5` octets (`wl_region_bytes`). After 1 the caller remeshes (`stream_set_light` again; border columns changed source). Columns outside the region use Chunk.light or the sky-15 default. Test tests/2907 (volume after a move == fresh load, edits survive).
* GL vs CPU lit scenes: tools/gpu/dump.fi `lit_day`, `lit_night`, `lit_night_fog` (llvmpipe PSNR 41.0 - 41.5 dB, threshold 38).

## Not done yet (roadmap)
* The stream does not call `wl_region_recenter` by itself (the play client has to, then remesh); no image test of the follow-up remesh.
* No live image check against a real client; llvmpipe/CPU only. GL lit scene: llvmpipe only, no real GPU.
