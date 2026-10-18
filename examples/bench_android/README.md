# bench_android (Firn + fUi benchmark app)

Same UI, tests and checksums as the Kotlin app `bench/android_kotlin` (spec: bench_spec.md, CPU_SPEC.md).
Tabs: List (1000 cards, filter, Add/Clear, dark toggle), Lab (Anim / Text / Net / CPU), Settings (theme radios, slider, version).

- `main.fi`   the app (host: X11 window 411x914; Android: NativeActivity via tools/android/build.sh)
- `bcpu.fi`   sliced CPU benchmark (8 ms slices on the UI loop); checksums primes=110730338 sort=605536303 mandel=23077341 (EXPECTED.txt)
- `smoke.sh`  emulator run (AVD benchf, ports 5586/5093): screenshots + BENCH log lines -> shots/ (use via /root/jarvis/bin/heavy)
- Log lines (logcat tag BENCH, stderr on the desktop, `lib/plat/blog.fi`): ready, fps, cpu, cpu_pure, net.
- Test hooks: Intent extras (Android) / env INTENT_<KEY> (desktop): tab=list|lab|settings, lab=anim|text|net|cpu, theme=dark|light, autorun, autoscroll (`lib/plat/intentx.fi`).

Build:
    FIRNC=compiler/target/release/firnc bash tools/android/build.sh examples/bench_android/main.fi \
      --name "Bench Firn" --package com.fleitec.bench.firn --abi both --permission android.permission.INTERNET \
      --out /root/jarvis-android-builds/bench/bench-firn.apk
Desktop run: `FIRNLIB=lib firnc -o bench examples/bench_android/main.fi && INTENT_TAB=lab ./bench`.
Semantics: Add appends one item (Item 1001, ...), Clear empties the filter text.
