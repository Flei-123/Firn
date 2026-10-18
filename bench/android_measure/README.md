# android_measure

Repeatable on-device harness for the Firn-vs-Kotlin benchmark apps (`bench/android_kotlin`, `examples/bench_android`).
Runs on a PC with adb and a USB phone (default: Xiaomi 14T Pro `LVSGQWMN5XGYSGJZ` on HOLO-LAPTOP-3). Stdlib Python only.

    python measure.py --phase all                 # scroll, startup, cpunet, energy(kotlin), energy(firn)
    python measure.py --phase scroll --runs 5 --scroll-s 10
    python measure.py --phase energy --app firn --energy-s 300

- Needs both APKs installed (`/root/jarvis-android-builds/bench/bench-{kotlin,firn}.apk`).
- Takes `%TEMP%/phone.lock` before every phase (waits while a lock younger than 10 min exists), releases it afterwards. One phase = one slot (<= 10 min); do not touch the phone while it runs.
- Start it detached (e.g. `Win32_Process.Create`), a tool-call timeout kills child processes.
- Output: `<out>/<phase>_<app>_<time>.json` (per run + medians) and `SUMMARY` lines on stdout.

## Metrics
- scroll: SurfaceFlinger `--latency` polled every ~0.3 s and merged (ring buffer ~1 s), 5 runs x 10 s. p50/p95/p99/max of present intervals, frames > 8.5 ms (missed 120 Hz vsync; 8.33 ms + 0.2 tolerance), > 20 ms, stalls > 100 ms. Intervals across a poll gap are dropped. Kotlin also `gfxinfo` (summary + framestats); Firn is a NativeActivity, so gfxinfo has no data.
- startup: `am start -W` x5 (TotalTime), `BENCH ready`, `dumpsys meminfo` x3 (PSS/RSS/Java/Native/Graphics).
- cpunet: `BENCH cpu` (ms:checksum) + `cpu_pure`, `BENCH net`, 3 runs.
- energy: 5 min autoscroll, brightness fixed 128, `dumpsys batterystats --reset` before; battery temp/level/charge counter, thermalservice temperatures before/after and every 30 s, app CPU time from `/proc/<pid>/stat`, batterystats per-uid mAh estimate (+ screen), raw dump stored.
  While the phone is USB-charging, charge counter/current are not drain (flag `plugged`); comparable numbers are the batterystats estimate and CPU time. For true drain unplug the phone (adb over Wi-Fi).
