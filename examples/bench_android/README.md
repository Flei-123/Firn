# bench_android (Firn + fUi benchmark app)

Same UI, tests and checksums as the Kotlin app `bench/android_kotlin` (spec: bench_spec.md, CPU_SPEC.md).
Tabs: List (1000 cards, filter, Add/Clear, dark toggle), Lab (Anim / Text / Net / CPU), Settings (theme radios, slider, version).

- `main.fi`   the app (host: X11 window 411x914; Android: NativeActivity via tools/android/build.sh)
- `bcpu.fi`   CPU benchmark, nine kernels (primes sort mandel hash json strbuild matmul hashmap objsort) on ONE worker thread (std.pool); checksums in EXPECTED.txt, definition in CPU_SPEC.md, reference `cpu_ref.py`. The program must define `fn __thread_work(kind, arg) -> u64 { return bcpu.work(kind, arg) }`.
- `cpu_main.fi` host test: `FIRNLIB=lib firnc --opt-level=release-fast -o x examples/bench_android/cpu_main.fi && ./x` -> nine checksums
- `smoke.sh`  emulator run (AVD benchf, ports 5586/5093): screenshots + BENCH log lines -> shots/ (use via /root/jarvis/bin/heavy)
- Log lines (logcat tag BENCH, stderr on the desktop, `lib/plat/blog.fi`): ready, fps, cpu, cpu2, net.
- Test hooks: Intent extras (Android) / env INTENT_<KEY> (desktop): tab=list|lab|settings, lab=anim|text|net|cpu, theme=dark|light, autorun, autoscroll (`lib/plat/intentx.fi`).

Build:
    FIRNC=compiler/target/release/firnc bash tools/android/build.sh examples/bench_android/main.fi \
      --name "Bench Firn" --package com.fleitec.bench.firn --abi both --permission android.permission.INTERNET \
      --out /root/jarvis-android-builds/bench/bench-firn.apk
Desktop run: `FIRNLIB=lib firnc -o bench examples/bench_android/main.fi && INTENT_TAB=lab ./bench`.
Semantics: Add appends one item (Item 1001, ...), Clear empties the filter text.
