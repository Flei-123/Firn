# Firn on Android

A Firn program that opens its window through `lib/window/window.fi` runs as a
native Android app. No Java, no Kotlin, no Gradle, no Play Store:
`android.app.NativeActivity` is part of Android, and everything behind it is
Firn.

```sh
bash tools/android/build.sh demos/x11demo/main.fi --name "fUi Galerie" \
    --package org.firn.gallery            # -> build/android/fui_galerie/*.apk
adb install -r build/android/fui_galerie/fui_galerie.apk
```

The entry file is the one that runs under X11. `build.sh` builds it from a
shadow of its directory in which `window/backend.fi` points to
`lib/window/android.fi`, compiles with `--pic` for arm64-v8a and x86_64,
links a `.so` that exports only `firn_activity_create`, writes the manifest
and signs with a key kept in `~/.firn/android.keystore` (keep it: an update
needs the same key).

## The pieces

| File | What it does |
|---|---|
| `lib/android/activity.fi` | The host. Starts one pthread that runs `gc_init` and the program's `main`; Android's main thread only forwards window, input queue, content rectangle, configuration and lifecycle and waits for the acknowledgement where Android needs it. `args.txt` in the app's data directory becomes argv, stdout/stderr go to `stdout.txt` there. |
| `lib/android/ndk.fi` | The NDK functions and C layouts (from Certus, MPL-2.0). |
| `lib/window/android.fi` | The window backend: in host mode it pumps the app thread's looper, turns touches and keys into window events, maps Android density to design points (160 dpi = 1.0) and shows the program only the part of the window that no status bar, navigation bar or cutout covers. |
| `lib/plat/sysfont.fi` | The system face: DejaVu on Linux, Roboto on Android. |
| `tools/android/build.sh` | Any entry file -> APK. |
| `tools/android/gallery_check.sh` | The acceptance run on a device or emulator. |

## Measured (emulator, Android 15 x86_64, 23.09.2026)

`tools/android/gallery_check.sh`: 14 of 14 checks -- touch toggles switch and
checkbox, "Starten" runs the progress animation, Tab/Enter, PageDown scrolls,
rotation, home and back again, Back ends `main` and the process, no crash.
A full frame of the gallery (2072 x 1014) takes 885 ms to paint in the
emulator (median of 22). The protocol splits one frame: 884 ms fUi painting,
10 ms RGBA -> BGRX, 62 ms into the window buffer (`r_image` swaps octet by
octet; the double conversion RGBA -> BGRX -> RGBA is a known waste).

![landscape](android/gallery-used.png)

## Open

* The soft keyboard composes nothing yet (no InputConnection without a dex
  class); hardware keys and the key events Android synthesises arrive.
* A finger drag does not scroll: fUi scrolls by scroll bar, wheel and keys.
* Only tested in the emulator; the arm64 build still needs a real phone.
* x86_64 Android forbids some legacy system calls (seccomp); `dup2` hit it.
  Firn's x86_64 code uses the raw numbers, aarch64 goes through the `*at`
  forms -- phones are aarch64.
