# mc_input -- input events -> camera, movement, block pick, selection box

`lib/mc/mc_input.fi`, test `tests/2755_mc_input.fi`, cross-check `bash tools/mc/input/run.sh` (`RESULT mc-input: ok`).
Pure data: the caller fills `InputEvents` (mouse delta px, key mask `K_FORWARD/BACK/LEFT/RIGHT/JUMP/SNEAK/SPRINT`, edges
`toggle_fly`, `toggle_noclip`, `slot_delta`) once per 20 Hz tick. No window dependency.

## Rules (the contract the python reference repeats)
* Look: degrees = px * ((s*0.6+0.2)^3 * 8) * 0.15 (s = sensitivity, 0.5 -> 0.15 deg/px); yaw wraps to [-180,180), pitch +-90.
* Speeds in blocks/tick: walk 0.21585, sprint 0.2806 (forward only, not sneaking), sneak 0.0648, fly 0.546 (x2 sprint),
  fly up/down 0.375; diagonals x0.7071. Ground: instant horizontal speed (no inertia), jump vy 0.42, then `vy = (vy-0.08)*0.98`.
  Box 0.6 x 1.8 (sneak 1.5), eye 1.62 (sneak 1.27). `flying`: no gravity, collision on; `noclip`: no collision.
* Collision: 9^3 snapshot of the world around the player (unloaded chunk = solid) + `mc_math.grid_move` (y, x, z order).
  No water / ladders. Block shapes: full unit cubes, plus partial shapes (slabs, stairs incl. corner shapes, carpets, snow layers) when the
  `InputTables.solid` entry is 128 + k (`mc_math.shape_box`; `mc_shapes.shapes_fill(&reg, solid, n)` fills them from the registry, mcplay
  does it). Step-up follows vanilla 1.20.1 `Entity.collide` with stepHeight 0.6 (`move_step`): partial blocks up to 0.6 high are walked
  up without a jump; full blocks (1.0) still need a jump / Auto-Jump. Since s5 also modelled (codes K_FENCE.. in mc_math): fences / walls /
  fence gates (1.5 high: no jump clears them), doors, trapdoors (open + top / bottom), beds (slab + legs), chests (single + double halves,
  trapped / ender). Every state is checked against minecraft-data 1.20 (`bash tools/mc/shapes/run.sh`, exact 1/16 voxels). `grid_move` /
  `grid_overlaps` scan one cell below the box (tall shapes reach 1.5). Not modelled: glass panes, iron bars, anvils, cauldrons, hoppers, ...
* Sneak edge (vanilla `maybeBackOffFromEdge`): sneaking (not flying) on the ground, or < 0.6 into a fall with a floor within 0.6, cuts the
  horizontal move in 0.05 steps (x, then z, then both) while the box moved 0.6 down would touch nothing -> no walking off a ledge; a drop
  of <= 0.6 is not an edge. `Player.fall` = fall distance (reset on ground / fly / noclip).
* Pick: `input_pick(world, min_y, tables, o, d, reach, &Pick)` DDA over `mc_world` (one-chunk cache); `InputTables.pick/solid`
  are u8 per block id. Hit = block, entered face normal (0,0,0 if the origin is inside), distance. Reach `player_reach`: 5 / creative 6.
* Selection box `input_draw_box(raster, x, y, z, rgba)`: 12 edges (+-0.002), near clip, 1/16-pixel snap, integer line stepper,
  pixel drawn if `1/z * 1.002 > depth`; depth is not written. `player_set_camera` copies position (+eye) and angles to mc_camera.

## Checked
Mouse look bit identical (3 sensitivities x 12 steps); 7 physics scenarios (wall, sprint jump, fall, fly+noclip, unloaded chunk,
sneak diagonal + hotbar wrap, fly with collision) vs a python re-simulation (error <= 1e-9, flags exact); 600 random rays vs a
brute-force slab test (384 hits: same block, face, distance <= 1e-9); 4 box framebuffers pixel exact (incl. near clip, occlusion);
3 build stages; 3 counter-checks (pick normal, gravity, line stepper) detected.
Measured: input_pick ~2.0 us/call, player_tick ~170 us (256 chunks; linear `world_find` dominates).

## s8 (15.10.)
* The `InputTables.solid` / `BlockGrid.solid` tables are u16 per state now (codes 0, 1..127 full, 128 + k shape k; room for 65407 shapes; the player's
  class table `Player.cls` has `CLS_N` = 65536 entries). `mc_shapes.shapes_fill` writes u16. New shapes (panes, bars, anvil, cauldron, hopper, low blocks) still to add (r211).
* Sneak edge live: `bash tools/mc/play/run_edge.sh [MC_VERSION]` -- 5x5 platform 5 blocks up, sneaking W / W+D stops at z <= 5.30 / x >= -2.30, never falls, client == server
  position, no "moved wrongly"; control without sneak falls. 1.20.1 green (z 5295, x -2295 mm).
* Sneak packets (measured against real servers, `tools/mcclient/sneak_probe.sh`): <= 1.21.5 Entity Action 0 / 1, 1.21.2+ also Player Input (shift), 1.21.6+ Player Input only.
  1.21.4+ ignores both until Player Loaded (0x2A on 1.21.4) arrived (or ~3 s passed): `mc_play.send_loaded` after the first chunks.

