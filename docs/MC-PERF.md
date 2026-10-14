# MC-PERF -- Performance measurement of the Voxel client

Owner: worker mc-p3 (12.10.2026). Status: **harness + CPU baseline exist; Windows cross build works (build_win.sh); GL / live / Vanilla comparison OPEN**.
Rule: only real measurements go into the tables; empty = not measured.

## 1. Harness (`bash tools/mc/perf/run.sh [--offline]`)
| Part | File |
|---|---|
| statistics (sort, linear percentile, mean, 1 % low, `RESULT` lines) | `lib/mc/mc_perf.fi` |
| scenario (world tiling, mesh/upload sample, load, flight path, frames, RSS) | `lib/mc/mc_perf_run.fi` |
| entry CPU raster (`mc_raster` through `mc_backend`) | `tools/mc/perf/bench.fi` |
| entry GL back end on EGL surfaceless = Mesa llvmpipe (software, NO real GPU), `PERF_BACKEND=gl` | `tools/mc/perf/bench_gl.fi` |
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

## 2. Measurement method Vanilla comparison (NOT yet run) -- full method: tools/mc/perf/vanilla_compare.md
* Vanilla runs on the owner's PC FLEI-ONE (Windows, FleiLauncher). Same world/seed, view distance (8/16/32), resolution,
  hardware. Frame times by PresentMon (CSV) or the game's own profiler / log. Only after the owner's OK per run
  (`project_needs`: "Darf ich auf deinem PC kurz Minecraft Vanilla zum Benchmark starten?") and only when the Windows
  harness is ready: `firnc --target=x86_64-windows -o bench.exe tools/mc/perf/bench.fi` (cross build prepared in principle,
  built: `bash tools/mc/perf/build_win.sh <dir>` (bench.exe, wine smoke test ok 13.10.), NOT run on FLEI-ONE). No Mojang files are copied or read; Vanilla is only started as a measured object.
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

### 4b. Series 8 / 16 / 32 (14.10., 640x360, fixture tiled, main 8c7eb2abc; NO GPU: CPU raster vs Mesa llvmpipe GL, both software; Vanilla NOT measured)
Single runs per radius (r8: 2 runs; 30 frames, warm 8; r8 40 frames), server load 13-19 (`uptime` at the start of each run). bench.fi has its own scenario (not mc_stream): the stream changes of 14.10. are not in these numbers.
| radius (columns) | back end | frame p50 / p99 | tris per frame | sections drawn | load (end to end) | RSS loaded |
|---|---|---|---|---|---|---|
| 8 | CPU raster | 431 / 738 ms and 313 / 386 ms (2 runs) | 468,943 | 491 | 5.3-5.5 s | 151 MB |
| 8 | GL llvmpipe | 102 / 130 ms and 84 / 129 ms | 468,943 | 491 | 3.0-3.1 s | 303-306 MB |
| 16 | CPU raster | 1157 / 1602 ms | 1,994,822 | 2185 | 22.8 s | 463 MB |
| 16 | GL llvmpipe | 331 / 385 ms | 1,994,822 | 2185 | 13.3 s | 630 MB |
| 32 | CPU raster | 3550 / 4149 ms | 7,433,688 | 8088 | 156 s | 1634 MB |
| 32 | GL llvmpipe | 1218 / 1511 ms | 7,433,688 | 8088 | 119 s | 1722 MB |
llvmpipe uses all cores for rasterising, the CPU raster is one thread: the ratio (3-4x) says nothing about a real GPU. Frame times include on-demand meshing (budget 16 sections / frame).
The 3-run series through `heavy` (reports/perf_20261014-0521*) FAILED to produce frame data: `BENCH_COMP=1` (component probe) segfaults intermittently in `perf_components` (R 8: 2 of 5 direct runs, every run under heavy) -> see section 5;
run.sh now leaves the probe off (`PERF_COMP=1` enables it). The TSVs of that run only hold startup / build_chunk metrics (build_chunk p50 ~1.6-2.0 ms, ~490-620 chunks/s).

### 4a. Parallel snapshot (r103, 14.10., main 8c7eb2abc; tests/2915 async load from empty, 4 workers, 189 sections, server load ~30 -> noisy)
| | before (serial pad fill on the render thread) | after (pad filled by workers / end-of-frame barrier) |
|---|---|---|
| snapshot+submit phase, sum over the load (median of 6 runs) | ~84 ms (0.44 ms / section) | ~36 ms (0.19 ms / section) |
| worst submit phase of one frame (range of 6 runs) | 2.6 - 6.1 ms | 1.8 - 4.7 ms |
| time at the end-of-frame barrier (sum over the load) | -- | 1.6 - 3.8 ms |
Not a clean A/B (the load of the server changes between runs); the direction (about 2x less on the render thread) held in every pair. Frame budget test: >= 90 % of the frames inside `snap_budget_us` + 3 ms, no frame > 250 ms (CPU raster draw included).

## 5. Known problems / open
* **perf_components segfault: FIXED (14.10., u1).** Cause: `mc_mesh.MAX_QUADS` = 13056 assumed one visible face per neighbour pair; sections of non-opaque blocks (leaves, glass, water) show up to 6 faces per block = 24576 quads -> vertex / index buffers overflowed (heap corruption, crash far from the cause: after `world_chunks`, timing dependent). Fix: MAX_QUADS 24576; regression test tests/2950 (checkerboard of two non-opaque blocks, exit 2 on the old bound). Before: 12 of 12 runs `BENCH_COMP=1 BENCH_COMPUPLOAD=1` (R 8 / 16, 100 frames) rc 139; after: 10 of 10 completed runs (R 8 / 16, 20 frames, with and without upload probe) rc 0. `run.sh` runs the probe by default now (`PERF_COMP=0` switches it off). The old hypothesis (backend_mesh_upload heap cache) was wrong.
* `BENCH_COMP=1 BENCH_COMPUPLOAD=1` (mesh + upload components per section): the reported crash in `mc_raster.backend_mesh_upload` does
  **NOT reproduce** on main (13.10.): 915 upload/free cycles at radius 8 and 16, with and without free (`BENCH_NOFREE=1`), with both firnc builds
  (8 Oct and w-r5), no crash. Regression test tests/2862_mc_upload_cycles.fi (6000 cycles, random sizes, 300 live handles, slot table growth, content
  patterns + all-zero check of every allocation). The original crash was probably the pre-q1 build / a fixed environment problem; code reading found
  no size mismatch in alloc/free pairs (slot table, verts, idx). `run.sh --offline` now runs the components. Measured (R 8, 915 sections, server load ~15, 640x360 not involved):
  naive+AO p50 738 us, naive no AO 695 us, greedy+AO p50 727 us / p95 2607 us, upload+free p50 146 us per section (single runs, noisy).
* GL path: `bench_gl.fi` exists since 14.10. (`PERF_BACKEND=gl bash tools/mc/perf/run.sh --offline`, needs FIRNC with link_lib, e.g. /root/firn-w-r5); results carry "NO REAL GPU (Mesa llvmpipe)".
* `--live`: join time (handshake -> last chunk) needs `t_last` + parse time printed by `demos/mcview`; not wired.
* After p1 / p2 are merged: repeat the series and fill "after".

## 6. Honesty
* No GPU exists on this server; nothing here is a GPU number. The CPU renderer is the reference, not the end goal on desktop.
* No Vanilla number exists yet. "Better than Vanilla" is NOT shown by this document.
* The server is shared (load 10-30 during the runs): treat times as noisy; the harness stores the load per run.

## Messserie 14.10. (u1, PERF_COMP an, 3 Laeufe, Median [min..max], 640x360, Fixture-Welt, Server-Load ~14-16)
Note: llvmpipe = software GL, NOT a real GPU. Vanilla NOT measured.

| Backend | Radius | Frame p50 ms | p99 ms |
|---|---|---|---|
| CPU-Raster | 8 | 336 [319..339] | 531 [378..539] |
| CPU-Raster | 16 | 1040 [989..1070] | 1205 [1090..1208] |
| GL llvmpipe | 8 | 86 [82..87] | 117 [105..127] |
| GL llvmpipe | 16 | 347 [341..367] | 540 [407..561] |
| CPU-Raster | 32 | 3453 [3367..3584] | 4853 [4591..4872] |
| GL llvmpipe | 32 | 1235 [1211..1552] | 1583 [1501..2695] |

Mesher (915 sections, p50): naive+AO 823 us, naive without AO 780 us, greedy+AO 807 us (CPU run). Upload/Free p50 130 us/section.
Radius 32 (v1, PERF_FRAMES=30, 3 Laeufe, reports/perf_20261014-085359_cpu_r32.tsv + perf_20261014-085901_gl_r32.tsv):
* Scaling CPU p50: r8 336 -> r16 1040 (3.1x) -> r32 3453 ms (3.3x); GL p50: 86 -> 347 (4.0x) -> 1235 ms (3.6x). Tris/frame 0.46M -> 2.0M -> 7.4M, sections 485 -> 2185 -> 8088: time ~ linear in tris, no culling/LOD gain visible.
* GL r32 Streuung gross (p50 max 1552, p99 max 2695): Server-Load lief 16-24 (r8/r16: 13-17) -> r32 GL-Zahl pessimistisch/verrauscht. CPU r32 stabiler (+-3%).
* Weltaufbau r32: load_e2e CPU 144 s, GL 118 s (5329 Chunks, 76670 Sektionen), RSS-Peak 1.6-1.7 GB (r16: 0.46-0.63 GB, r8: 0.15-0.3 GB).
* Classification: llvmpipe/CPU, NOT a real GPU, vanilla NOT measured. No statement about vanilla/a real GPU is possible.

## 7. GL face buckets + llvmpipe cost split (g2, 14.10., llvmpipe = software GL, NO real GPU, Vanilla NOT measured)
* **Face buckets** (`gl_mc.glbe_set_facecull`, default off; bench_gl `BENCH_GL_FACECULL` default 1; mcplay `MCPLAY_FACECULL` default 1): opaque meshes are stably index-sorted by face direction at upload (7 buckets), a section in MULTI/INDIRECT mode draws only the directions whose plane the camera can be in front of (conservative 0.01). `mc_backend.mesh_class` marks translucent/LOD meshes (`MESH_TRANS`) = order kept, full range. Same image as GL back-face culling (frame hash of 60 frames identical, test 3070 over 14 camera positions).
* r8 p50 (60 frames, Median of 3 interleaved A/B, load 30-40): buckets off 87.5 ms [82.8..89.6], on 79.0 ms [78.7..86.0] = about -10 %, noisy. Process CPU time per frame unchanged (~238 ms): the gain is on the single-threaded vertex stage.
* Cost split r8 (BENCH_GL_RDISCARD=1 = vertex shading + setup only): ~28 ms wall = 28 ms CPU on ONE thread (buckets off ~37 ms; legacy per-section draws ~38 ms, so multi draw is not the issue). The other ~50 ms wall / ~200 ms CPU is rasterisation + fragment on 4 threads.
* Tried, no effect: shader without `discard`, `early_fragment_tests` (both within noise), resolution (noisy), MODE_POOL/LEGACY/MULTI/INDIRECT. Back-face culling off: 2x slower (so tiny triangles cost per triangle, not per pixel).
* Conclusion: 16 ms at r8 is NOT reachable on llvmpipe with the same image: ~470k small triangles; per-triangle bin/setup dominates. Levers left: fewer triangles (LOD / occlusion culling that changes the image only transiently), real GPU measurement. Occlusion culling was not built (open-terrain sections stay visible through the cave graph; Hi-Z needed).
* Knobs: `BENCH_GL_MODE`, `BENCH_GL_CULL`, `BENCH_GL_FACECULL`, `BENCH_GL_RDISCARD`, `BENCH_HASH=1` (frame_hash), new RESULT `cpu_ms_per_frame`.
