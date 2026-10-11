# mc_raster -- CPU triangle rasterizer for mc_mesh output

`lib/mc/mc_raster.fi`, test `tests/2752_mc_raster.fi`, cross-check `bash tools/mc/raster/run.sh`
(`RESULT mc-raster: ok`). Builds on `mc_camera` (view basis), `mc_texture` (atlas), `mc_mesh` (vertices),
`paint.canvas`/`paint.png` (PPM/PNG), `std.crypto.sha256` (image hash).

## Frame buffers
* `color`: u32 per pixel, bytes R G B A (`0xAABBGGRR`), alpha always 255. `depth`: f64 per pixel = 1/z, 0.0 = far,
  larger = nearer; a pixel is written only if its 1/z is **strictly** larger.
* `raster_hash` = SHA-256 of the colour buffer bytes (w*h*4, row major) -> `raster_hash_hex` (64 lowercase hex chars).
* `raster_write_png` / `raster_write_ppm` go through `paint.canvas` (alpha is 255, so premultiplied == straight).

## Pipeline (the contract the python reference repeats)
1. Vertex word -> world `(ox + x, oy + y, oz + z)` (f64) -> `rel = world - camera` -> view
   `(rel.right, rel.up, rel.forward)`, each a left-to-right sum of three products (no fused multiply-add).
2. Near clipping at `z = near` (Sutherland-Hodgman, 3 -> up to 4 vertices; u, v, ao linear in view space; z set to `near`).
3. `iz = 1/z`, `sx = w/2 + (x*iz) * (w/2/tan_x)`, `sy = h/2 - (y*iz) * (h/2/tan_y)`, snapped to **1/16 pixel**
   (`floor(s*16+0.5)`, i64). Everything that decides coverage from here is integer.
4. `area2 = (bx-ax)(cy-ay) - (by-ay)(cx-ax)`: **> 0 = back face** (front faces are counter clockwise seen from outside,
   y points down), culled when `cull` (default). Reverse faces are drawn with b/c swapped.
5. Edge functions at pixel centres `(16x+8, 16y+8)`; **top-left rule**: a pixel exactly on an edge belongs to the triangle
   if the edge has `dy < 0` or (`dy == 0` and `dx > 0`) -> shared edges are covered exactly once.
6. Pixel: `zinv = (l0*iz0 + l1*iz1 + l2*iz2) / area2` (l = integer edge values as f64), depth test, perspective-correct
   `u, v, ao = sum(l_i * a_i*iz_i) / sum(l_i * iz_i)`, texel = `atlas` nearest `floor(u*tile) & (tile-1)` (tile-local wrap =
   greedy quads repeat), alpha < 128 -> cut-out (no write, no depth).
7. Shading (integers): `fq = floor(100 + 52*ao)` (0..256), `total = fq*face_k >> 8`, channel `= c*total >> 8`;
   `face_k` = 204 (-x, +x), 128 (-y), 256 (+y), 153 (-z, +z) (`face_shade`).
8. Fog (optional, `raster_set_fog`): `t = clamp((1/zinv - start)/(end - start))`, `f = floor(256 t)`,
   `c = (c*(256-f) + fog*f) >> 8`.
Translucency (water, alpha 190) is **not** blended in v1: tiles with alpha >= 128 are opaque.

## API
| Function | Meaning |
|---|---|
| `raster_new(w,h)`, `raster_free`, `raster_clear(rgba)` | buffers |
| `raster_set_camera(c)`, `raster_set_atlas(a)`, `raster_set_fog`, `raster_set_cull` | state |
| `raster_draw_mesh(verts,nverts,idx,nidx,ox,oy,oz)` | one mc_mesh result (quads = 2 triangles), section origin in world blocks |
| `raster_draw_tri(a,b,c,tile,fk)` | one triangle of three 6-f64 view-space records (x,y,z,u,v,ao) |
| `raster_pixel`, `raster_hash`, `raster_hash_hex`, `raster_write_png/ppm`, `st` (Stats) | output, counters |
| `backend_upload_atlas / mesh_upload / mesh_free / begin / draw / end / present / read_pixels` | backend interface of MC-RENDER.md 4 (CPU implementation; `mesh_upload` copies the mesh, handle >= 1, slots are reused) |

## Checked (tools/mc/raster)
`dump.fi` renders 14 scenes (cube, stairs, greedy floor with repeating quads, fog, two draw orders of overlapping cubes,
near clipping floor + wall, cut-out leaves, culling from inside a block on/off, 3 random triangle soups of 300 quads
with skinny / huge / behind-camera triangles, culling on/off, fog on/off) and writes cameras (f64 bit patterns), vertex
words, hashes, framebuffers, PNG and PPM. `ref.py` renders the same data with its own code (python floats, exact
python integers, per pixel edge evaluation) and requires **byte identical** framebuffers, equal SHA-256, PNG/PPM files
that decode to the same pixels, `depth_ab == depth_ba`, culling effective. 3 build stages byte-equal; counter-check (damaged
fill rule) is detected. Not independently checked: the atlas pixels themselves (read from the same dump).

## Measured (server, release, Load ~10)
640x360, 5x5 sections of 16x16 hills (greedy + AO): ~72 ms/frame, 47 K triangles in, 25 K drawn, 345 K pixels written
(~0.63 M triangles/s incl. large ones). Tiny triangles (1 pixel): 2.9 M triangles/s (setup cost). No section culling in this scene;
see mc_render_world.
