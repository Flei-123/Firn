# bench_android (Worker A, Firn+fUi) -- PARTIAL
Done: bcpu.fi (sliced CPU benchmark, checksums verified == EXPECTED.txt on host), cpu_ref.py (Python reference), CPU_SPEC.md, EXPECTED.txt.
Lib: lib/plat/intentx.fi + lib/@android/plat/intentx.fi (+ lib/android/intentextra.fi): intent extras over JNI; vlist card rows (VRow.sub/tint/px, vrow_sub);
app.frame_stat FS_IV_P95_US / FS_IV_MAX_US (frame intervals).
NOT done: main.fi (UI: List/Lab/Settings, BENCH logs, hooks), build script, APK, emulator run, shots.
Semantics decided for the UI (spec leaves open): "Add" appends one item (Item 1001, ...), "Clear" empties the filter text.
CPU method: runs on the UI loop in 8 ms slices (bcpu.cpu_step), logs wall ms (BENCH cpu) and pure ms (BENCH cpu_pure).
