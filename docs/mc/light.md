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

## Not done yet (roadmap)
* mc_render_world / mc_stream still mesh without F_LIGHT: stream snapshots need a light pad per job (from Chunk.light / a LightVol per loaded region), `raster_set_light(day)` from mc_sky.daylight.
* Live check of the light arrays against a real 1.16.5 / 1.20.1 server (tools/mcclient/compare); update_light for 1.14/1.15/1.17 variants.
* GL rendering of the lit scenes is not compared with the CPU image (only the default light).
