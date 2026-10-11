# mc_math -- vectors, boxes, rays, matrices, frustum

`lib/mc/mc_math.fi`, `import mc.mc_math`. CPU only. Tests: `tests/2680_mc_math.fi`
(`bash tools/mc/testmc.sh math`), cross-check `bash tools/mc/math/run.sh`
(`RESULT mc-math: ok`), throughput `tools/mc/math/bench.fi`.

## Decisions
* **f64 everywhere.** Measured (20M iterations of cross+add+dot): f64 by value 1.86 s, f32 by
  value 2.44 s (f32 is *slower*: no float register allocation, SPEC 14.1.f64 F1), f64 through
  pointers 0.25 s. f64 also keeps block coordinates exact (30M blocks = 25 bits > f32's 24).
  GPU upload: `mat4_to_f32`.
* **Pointer / scalar signatures on hot paths.** Struct values are passed through memory; the
  by-value helpers (`v3_add`, ...) are ~7x slower than the `*_to` forms. Raycast, sweep and
  frustum take scalars and pointers only.
* Column-major `Mat4` (`m[c*4 + r]`), right-handed, camera looks down -z; depth range OpenGL
  `[-1,1]` or Vulkan/D3D `[0,1]` (`zero_to_one` flag).
* The solid test is a caller table: `BlockGrid` = dense box of u16 ids + `u8` table `id -> solid`
  (ids >= table size are passable) + `outside_solid` flag. A world lookup callback is not used
  (keeps the hot loop free of indirect calls); assemble the box from the chunk store.

## API
* Scalars: `floor_i(f64)->i32` (saturates, NaN->0), `floor_div/floor_mod(a,b)` (b>0),
  `chunk_of`, `local_of`, `local_index(x,y,z)` ((y*16+z)*16+x), `section_of_y(y,min_y)`, `deg_to_rad`.
* `Vec3`/`IVec3`: `v3, v3_add/sub/scale/dot/cross/len/normalize/lerp`, pointer forms
  `v3_*_to`, `iv3, iv3_add/sub/floor/chunk/local`.
* `Aabb` [min,max): `aabb_new, aabb_block, aabb_overlap` (touching = no), `aabb_union_to,
  aabb_contains, aabb_offset, aabb_expand`; `ray_aabb(o, d, box) -> t>=0 | -1.0`.
* `grid_raycast(g, o, dir, maxd, out) -> bool`: Amanatides-Woo DDA; `RayHit{x,y,z,nx,ny,nz,dist,id}`;
  dir is normalised (dist in blocks); start inside a solid block = hit, dist 0, normal 0; zero or
  non-finite direction = miss. Ties on edges/corners step x, then y, then z.
* `grid_move(g, box, dx,dy,dz, out)`: axis order y, x, z; stops at block faces; `MoveResult{dx,dy,dz,
  hit_x,hit_y,hit_z}`; blocks already overlapped do not block; perpendicular overlap < 1e-9 is ignored
  (so a box that landed on a floor with a 1-ulp error still slides). Unit-cube blocks only.
* `Mat4`: `identity, mul(o,a,b) (o may alias), translation, scaling, rotation_x/y/z, perspective,
  look_at, transpose, invert -> bool, transform_point (returns w), transform_dir, to_f32`.
  `invert` returns false for pivot < 1e-13 * max element (cond ~ 1e13+) or NaN and leaves `o` alone.
* `Frustum`: `frustum_from_matrix(f, vp, zero_to_one)` (Gribb-Hartmann, normalised), `frustum_test_box /
  _aabb / _sphere / _section` -> 0 outside, 1 intersecting (conservative), 2 inside. Never 0 for a
  box with a point inside; 2 iff all 8 corners are inside.

## Error cases
No function aborts on NaN/inf input except `as` casts you do yourself; `floor_i` saturates;
`grid_raycast`/`grid_move` return a miss / zero move for non-finite input; `floor_div` aborts on
b == 0; `mat4_perspective` does not validate near/far/fov.

## Cross-checks (what is really compared)
numpy: matrix product, transpose, inverse (relative to cond), projection depth/edge properties, look_at
properties (eye -> origin, centre on -z, orthonormal), planes vs the clip-space test on 4000 random
points per camera, boxes (corner projection + 1500 samples), spheres. Brute force: raycast against the
slab test of every solid cell (+ sampling along the ray), sweep against the same documented algorithm over
all cells + no-overlap + maximality properties. Three build stages must write byte-identical dumps.
Damaged dumps (5 kinds) are rejected.

## Measured (this server, release-fast, single thread, 32^3 world 20 % solid)
raycast reach 6 blocks ~2.5 M rays/s; player box sweep ~1.2 M moves/s; frustum section test ~23 M tests/s;
mat4 mul+invert ~1.17 M pairs/s.
