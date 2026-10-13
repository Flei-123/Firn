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

## Not done yet (roadmap)
* Light from packets (update_light, chunk light arrays) -> padded light arrays.
* mc_light.fi: incremental BFS light (add / remove), fuzz against full recompute, dirty sections for mc_stream.
* mc_render_world / mc_stream still mesh without F_LIGHT (no light source yet).
