# MC-PERF -- Performance measurement of the Voxel client

Owner: worker mc-p3 (12.10.2026). Status: **harness + CPU baseline exist, GL / live / Windows / Vanilla comparison OPEN**.
Rule: only real measurements go into the tables; empty = not measured.

## 1. Harness (`bash tools/mc/perf/run.sh [--offline]`)
| Part | File |
|---|---|
| statistics (sort, linear percentile, mean, 1 % low, `RESULT` lines) | `lib/mc/mc_perf.fi` |
| scenario (world tiling, mesh/upload sample, load, flight path, frames, RSS) | `lib/mc/mc_perf_run.fi` |
| entry CPU raster (`mc_raster` through `mc_backend`) | `tools/mc/perf/bench.fi` |
| aggregation (median / min / max of N runs -> TSV + JSON) | `tools/mc/perf/agg.py` |
| self test against numpy (5 sample sets, 8 statistics, tolerance 1 ns) | `tools/mc/perf/stats_check.py` |

* No argument = gate (self test + smoke run radius 2, in `tools/mc/run.sh` automatically). `--offline` = full series
  (`PERF_RADII="8 16 32"`, `PERF_RUNS=3`, `PERF_FRAMES=100`), output `reports/perf_<stamp>_cpu_r<R>.tsv/.json`, server load per run.
  Start with `heavy` (shared server).
* World: the 7x7 chunks of the fixture / a live dump, tiled periodically to `(2*(R+4)+1)^2` chunks (seams every 7 chunks).
* Camera: deterministic flight (48 blocks along +x, triangle wave in z, one full turn), function of the frame number only.
  10 warm-up frames are discarded. Mesh budget 16 sections / frame (`BENCH_MAXMESH`).
* Frame = begin + `wr_render_b` + end + present. NOT included: sky, HUD, box (CPU sky alone 2.4 ms at 640x360).
* Definitions: percentile = numpy `linear`; **1 % low fps** = 1e9 / mean of the slowest ceil(n/100) frames; **p99 fps** = 1e9 / p99.
* Startup: `BENCH_T0_NS` (CLOCK_REALTIME at launch, set by run.sh) -> first image (empty world) and first world image.
* Metrics printed: frame p50/p95/p99/min/max, fps avg/p50/p99/1 % low, tris + sections per frame, build_chunk (block ids ->
  chunk column; offline stand-in for the packet parse, NOT a network decode), load e2e (parse excluded; mesh+upload+draw),
  RSS start / loaded / peak, startup.

## 2. Measurement method Vanilla comparison (NOT yet run)
* Vanilla runs on the owner's PC FLEI-ONE (Windows, FleiLauncher). Same world/seed, view distance (8/16/32), resolution,
  hardware. Frame times by PresentMon (CSV) or the game's own profiler / log. Only after the owner's OK per run
  (`project_needs`: "Darf ich auf deinem PC kurz Minecraft Vanilla zum Benchmark starten?") and only when the Windows
  harness is ready: `firnc --target=x86_64-windows -o bench.exe tools/mc/perf/bench.fi` (cross build prepared in principle,
  **not built, not run**). No Mojang files are copied or read; Vanilla is only started as a measured object.
* Alternative: published reference values WITH source, date and hardware (not collected yet; nothing is quoted without a source).

## 3. Levers (status = honest)
| Lever | Status | Expected effect | Before / after (real) | Method |
|---|---|---|---|---|
| GPU pool VBO / multidraw | p1 (docs/mc/gpu-world.md), not merged when written | far fewer draw calls | open | bench_gl (not written) |
| Frustum culling per section | on main (`camera_section_visible`) | -- | in baseline | frame time |
| Cave / occlusion culling | p2 (mc_cull) open | fewer sections drawn | open | sections_drawn |
| Greedy + AO | on main (flag) | tris/section ~ -50 % (smoke sample: 1980 -> 972) | tris measured, ms open (see 5.) | mesh_* metrics |
| Jobs / remesh budget | budget on main (`max_mesh`); threads open | no frame spikes | open | frame p99 |
| LOD | p2 (mc_lod) open | far view | open | view 16/32 |
| Incremental light | not started | -- | open | -- |
| Instanced entities | p1 open | -- | open | -- |
| Transparency sorting | p1 open | -- | open | -- |
| Upload budget | = `max_mesh` | -- | open | sections_meshed_per_frame |

## 4. Results (CPU raster, single thread; server heavily loaded, load 27 -- numbers are an UPPER bound for the time)
Single run, radius 8, 640x360, 60 frames, fixture tiled, firnc of 8 Oct, `uptime` load 26.8 (not aggregated, no spread):
| Metric | CPU raster | GL llvmpipe | Vanilla |
|---|---|---|---|
| frame p50 / p95 / p99 | 403 / 844 / 907 ms | not measured (bench_gl missing) | not measured |
| fps avg / 1 % low | 2.2 / 1.1 | -- | -- |
| tris per frame | 484 345 | -- | -- |
| load e2e (view 8, 289 chunks, mesh+draw) | 7.2 s = 40 chunks/s | -- | -- |
| RSS start / loaded | 13.8 / 151 MB | -- | -- |
| startup to first image | 107 ms (empty world) | -- | -- |
| first world image | 2.8 s | -- | -- |
Series 8 / 16 / 32 with 3 runs: started via heavy (queue), results in `reports/perf_*_cpu_r*.tsv` if finished -- otherwise OPEN.
The 8-block frame times include on-demand meshing along the flight (budget 16 sections / frame) -- that is deliberate (p99).

## 5. Known problems / open
* `BENCH_COMP=1` (mesh + upload components per section) **crashes in `mc_raster.backend_mesh_upload`** (heap block cache
  corrupted after ~290 upload/free cycles; naive/greedy meshing alone is fine, the isolated alloc/free pattern is fine). Cause not
  found -- off by default. The mesh ms numbers (naive vs greedy+AO) are therefore NOT measured here; `tools/mc/mesh/run.sh` has
  sections/s for the mesher.
* GL path: `bench_gl.fi` not written; the compiler build was blocked by the disk (89 %), the old firnc (8 Oct) has no `link_lib`.
  Once written, results must carry "NO REAL GPU (Mesa llvmpipe)".
* `--live`: join time (handshake -> last chunk) needs `t_last` + parse time printed by `demos/mcview`; not wired.
* After p1 / p2 are merged: repeat the series and fill "after".

## 6. Honesty
* No GPU exists on this server; nothing here is a GPU number. The CPU renderer is the reference, not the end goal on desktop.
* No Vanilla number exists yet. "Better than Vanilla" is NOT shown by this document.
* The server is shared (load 10-30 during the runs): treat times as noisy; the harness stores the load per run.
