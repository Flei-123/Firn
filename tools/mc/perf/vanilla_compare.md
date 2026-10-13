# Vanilla comparison -- measurement method (NOT run yet; needs the owner's OK per run)

Rule: nothing runs on FLEI-ONE without an answer to the `project_needs` question. No Mojang files are copied or read;
Vanilla is only started as a measured object. Until numbers exist, nobody may claim "faster than Vanilla".

## Same conditions
| item | value |
|---|---|
| machine | FLEI-ONE, same power plan, nothing else running, same monitor mode |
| world | same seed + same spawn area (flat copy of tools/mc/view/fixture/world.mcw is NOT Vanilla-loadable: use a fresh world with a fixed seed, note it) |
| view distance | 8 / 16 / 32 chunks (render distance in options), simulation distance = same |
| resolution | 1280x720 windowed (the CPU harness uses 640x360: report both, never mix) |
| camera path | Vanilla: fixed spawn, hold forward + same yaw sweep for 60 s (or /tp loop by command blocks); harness: deterministic flight (docs/MC-PERF.md 1.) |
| runs | 3 per view distance, discard the first 10 s (shader/chunk warm-up) |

## Vanilla frame times
1. Start PresentMon (`PresentMon.exe -process_name javaw.exe -output_file vanilla_r<R>_<n>.csv -timed 60 -terminate_after_timed`).
2. CSV column `msBetweenPresents` -> p50 / p95 / p99 / 1 % low, same definitions as docs/MC-PERF.md (numpy linear percentile, 1 % low = mean of slowest 1 %).
3. Alternative without PresentMon: the game's own debug screen (F3) is NOT precise enough -- do not use for the table.

## Voxel client harness on the same PC
`bash tools/mc/perf/build_win.sh <dir>` builds `bench.exe` (CPU raster, single thread; cross build + wine smoke test pass on the server).
Run `bench.exe world.mcw <radius> 100 10 > out.txt` 3x per radius. Windows GL path: not built yet (no bench_gl) -> only the CPU raster can be compared,
which is NOT a fair comparison against Vanilla's GPU renderer. Mark every table "CPU raster, no GPU" until bench_gl exists.

## Report
Table per view distance: Vanilla p50/p95/p99/1 % low vs Voxel (CPU / GL), tris per frame, RSS, plus hardware line (CPU, GPU, RAM, driver).
