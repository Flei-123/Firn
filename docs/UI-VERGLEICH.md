# fUi UI comparison (harness + method)

Goal: measure honestly how fUi compares to Web (Svelte/HTML in headless Chromium), egui, GTK and Qt/PySide
for 7 reference apps: flei (launcher), chat, settings, dash, files, form, notes. Where fUi is worse, the table says so.
Everything runs on the Linux server (Xvfb, 1100x700, DPR 1, light and dark). **Windows and phone numbers: "needs laptop"** (not guessed).

Tools live in `tools/ui-compare/` (working copy with data/shots/results: `/root/jarvis/data/staff_scratch/uivgl/`, env `UIVGL_DIR`).

## Rules
- **Lines** = non-empty, non-comment-only lines (`loc.py`; .fi .rs .py .js .ts .svelte .html .css .slint .qml). Data files are read from `data/`, never embedded.
- **flei is a visual rebuild only** (fake data, no backend). The original's lines are counted as UI code only: Svelte views/components + CSS, no Rust backend.
- Median of 5 runs, min/max next to it; the server load average is stored in each result (`results/<app>-<toolkit>.json`).
- Web has an extra row "Runtime" (browser size); its RAM is the whole browser process tree.

## Metrics and how they are measured (`measure.py`)
| Metric | Method |
|---|---|
| Lines of code | `loc.py` over the app sources |
| Binary / bundle size | sum of file sizes of the binary / bundle (Web: HTML+CSS+JS without browser; runtime separately) |
| Build time | wall time of the clean build command (caller supplies it) |
| Startup | wall time from spawn until the `--shot` PNG is complete (valid PNG incl. IEND chunk) |
| RAM idle | PSS (`/proc/<pid>/smaps_rollup`) summed over the whole process tree, 5 s after start |
| CPU idle | utime+stime of the tree over 10 s (t=5..15 s), 100 % = one core |
| Input -> pixel latency | XTest event (libXtst, same private Xvfb) -> first changed pixel of the root window; polled with XGetImage (~1-3 ms per poll); baseline after 300 ms stable screen; first changed pixel counts (hover/press feedback included) |
| Scroll / resize / animation frame time | optional hook: app writes JSON to `$UIVGL_FRAMES`; otherwise `n/a` + reason |
| Visual similarity | SSIM (Gaussian 11x11, luma) + mean abs. pixel deviation vs reference (`compare.py`); flei: original Svelte rendering, others: Web version |

### Validation of the latency method (fUi `examples/fui/counter.fi`, click on "+")
5 runs: 11.1 ms median (9.1 .. 17.2 ms). Plausible; the method has a floor of one poll (~2 ms) and sees hover/press feedback.
Caveat: apps with a blinking caret or animation can show a too low value; `measure.py` flags a screen that is not idle-stable.
Idle on the same app: PSS 10.1 MB, CPU 0.0 %.

## Reproduce
```
python3 tools/ui-compare/gen_data.py          # data/ (deterministic, seed 20261021)
tools/ui-compare/run_all.sh                    # measure all apps/*/*/measure.json, compare, gallery
python3 tools/ui-compare/compare.py --table    # SSIM table only
```
App registration and data formats: `tools/ui-compare/DATA.md`.

## Results
Generated 2026-10-21 by `tools/ui-compare/finish_doc.py` from `results/*.json` (real measurements only; `-` = not measured / no result).
Server load average during the runs: 4-9 (shared server, 4 toolkits measured in parallel lanes, each with its own Xvfb).
Runs: 5 per metric (startup, first draw, latency), 3 for RAM/CPU idle (15 s each). Cells: median (min-max).

fUi apps present: dash, files, flei, form. **Missing fUi apps: chat, settings, notes** (not built within the task window).
Windows and phone numbers: **needs laptop** (offline; nothing guessed).

### Lines of code (non-empty, non-comment)
| app | original | fui | web | egui | gtk | qt |
|---|---|---|---|---|---|---|
| flei | 1106 | 62 | 533 | 177 | 126 | 107 |
| chat | - | - | 309 | 169 | 135 | 121 |
| settings | - | - | 294 | 177 | 121 | 112 |
| dash | - | 328 | 328 | 182 | 133 | 119 |
| files | - | 100 | 302 | 151 | 130 | 115 |
| form | - | 26 | 275 | 160 | 130 | 117 |
| notes | - | - | 256 | 148 | 127 | 94 |

### Binary / bundle size (MB)
| app | original | fui | web | egui | gtk | qt |
|---|---|---|---|---|---|---|
| flei | - | 2.34 | 0.08 | 6.78 | 0.01 | 0.01 |
| chat | - | - | 0.01 | 6.70 | 0.01 | 0.01 |
| settings | - | - | 0.01 | 6.85 | 0.01 | 0.01 |
| dash | - | 1.55 | 0.01 | 6.69 | 0.01 | 0.01 |
| files | - | 2.30 | 0.01 | 6.77 | 0.01 | 0.01 |
| form | - | 1.62 | 0.01 | 6.63 | 0.01 | 0.01 |
| notes | - | - | 0.01 | 6.67 | 0.01 | 0.01 |

### Build time (s, clean)
| app | original | fui | web | egui | gtk | qt |
|---|---|---|---|---|---|---|
| flei | - | 13 | - | - | - | - |
| chat | - | - | - | - | - | - |
| settings | - | - | - | - | - | - |
| dash | - | 13 | - | - | - | - |
| files | - | 15 | - | - | - | - |
| form | - | 13 | - | - | - | - |
| notes | - | - | - | - | - | - |

### First draw, interactive start (ms): spawn -> first pixel change
| app | original | fui | web | egui | gtk | qt |
|---|---|---|---|---|---|---|
| flei | - | - | 616 (510-757) | 209 (206-240) | 219 (210-314) | 371 (324-411) |
| chat | - | - | 626 (557-710) | 242 (196-359) | 389 (320-464) | 457 (336-635) |
| settings | - | - | 510 (481-568) | 208 (196-314) | 264 (255-338) | 359 (298-412) |
| dash | - | - | 563 (559-710) | 212 (201-250) | 336 (260-378) | 380 (334-438) |
| files | - | - | 649 (546-755) | 316 (304-347) | 7985 (7540-8300) | 878 (565-957) |
| form | - | - | 528 (513-658) | 196 (158-299) | 242 (199-286) | 323 (313-446) |
| notes | - | - | 524 (502-578) | 185 (168-200) | 237 (229-297) | 338 (311-357) |

### Startup to --shot PNG (ms; includes per-toolkit artificial waits, see notes)
| app | original | fui | web | egui | gtk | qt |
|---|---|---|---|---|---|---|
| flei | - | 174 (169-185) | 915 (900-932) | 172 (166-175) | 1198 (1186-1210) | 1096 (1013-1114) |
| chat | - | - | 873 (848-922) | 214 (208-219) | 1277 (1252-1354) | 1095 (1077-1102) |
| settings | - | - | 779 (739-800) | 157 (149-161) | 1234 (1226-1267) | 1000 (995-1046) |
| dash | - | 173 (170-194) | 813 (779-851) | 186 (183-190) | 1255 (1207-1267) | 1022 (1002-1077) |
| files | - | 225 (220-231) | 1233 (1170-1247) | 334 (319-346) | 8044 (7601-8713) | 1300 (1292-1323) |
| form | - | 175 (174-184) | 737 (719-791) | 166 (153-168) | 1206 (1201-1233) | 1001 (998-1089) |
| notes | - | - | 836 (774-840) | 170 (161-175) | 1217 (1198-1231) | 1002 (993-1077) |

### RAM idle PSS (MB, median)
| app | original | fui | web | egui | gtk | qt |
|---|---|---|---|---|---|---|
| flei | - | 12 (12-12) | 284 (281-289) | 47 (47-47) | 36 (34-36) | 37 (37-38) |
| chat | - | - | 283 (281-283) | 47 (47-47) | 35 (34-35) | 39 (39-40) |
| settings | - | - | 375 (374-376) | 44 (44-54) | 44 (40-62) | 38 (38-54) |
| dash | - | 14 (14-14) | 283 (283-283) | 49 (49-49) | 40 (40-40) | 38 (38-38) |
| files | - | 24 (24-24) | 425 (424-425) | 76 (76-76) | 120 (120-121) | 79 (79-79) |
| form | - | 14 (14-14) | 375 (267-383) | 44 (44-44) | 34 (34-34) | 37 (37-37) |
| notes | - | - | 382 (381-382) | 45 (45-48) | 35 (34-35) | 37 (37-39) |

### CPU idle (% of one core)
| app | original | fui | web | egui | gtk | qt |
|---|---|---|---|---|---|---|
| flei | - | 0.0 (0.0-0.0) | 0.5 (0.5-0.6) | 0.0 (0.0-0.0) | 0.0 (0.0-0.0) | 0.0 (0.0-0.0) |
| chat | - | - | 0.6 (0.5-0.9) | 0.0 (0.0-0.0) | 0.0 (0.0-0.1) | 0.0 (0.0-0.0) |
| settings | - | - | 0.6 (0.5-0.7) | 0.0 (0.0-0.0) | 0.0 (0.0-0.0) | 0.0 (0.0-0.0) |
| dash | - | 0.0 (0.0-0.0) | 0.5 (0.4-0.6) | 0.0 (0.0-0.0) | 0.0 (0.0-0.0) | 0.0 (0.0-0.0) |
| files | - | 0.0 (0.0-0.0) | 0.7 (0.4-0.8) | 0.0 (0.0-0.0) | 25.9 (20.6-26.0) | 0.0 (0.0-0.0) |
| form | - | 0.0 (0.0-0.0) | 0.8 (0.4-0.8) | 0.0 (0.0-0.0) | 0.1 (0.0-0.2) | 0.0 (0.0-0.0) |
| notes | - | - | 0.5 (0.3-0.7) | 0.0 (0.0-0.0) | 0.0 (0.0-0.0) | 0.0 (0.0-0.0) |

### Input -> pixel latency (ms)
| app | original | fui | web | egui | gtk | qt |
|---|---|---|---|---|---|---|
| flei | - | 18.6 (15.5-24.1) | 13.7 (11.5-19.0) | 16.1 (12.2-20.7) | 13.4 (10.2-15.4) | 11.6 (8.4-17.0) |
| chat | - | - | 12.9 (5.4-288.5) | 14.1 (12.1-20.5) | 19.7 (15.4-32.7) | 56.8 (43.7-76.8) |
| settings | - | - | 9.7 (7.8-13.6) | 8.0 (8.0-8.0) | 19.7 (15.8-27.8) | 11.1 (10.3-13.9) |
| dash | - | 21.3 (21.3-21.3) | 12.9 (11.2-18.6) | 15.9 (13.3-19.7) | 19.2 (16.3-20.9) | 13.7 (11.8-15.5) |
| files | - | 22.8 (21.8-47.1) | 224.2 (29.4-291.0) | 25.4 (22.8-34.6) | 19.4 (12.3-1981.1) | 12.5 (11.6-22.4) |
| form | - | 44.5 (28.5-47.4) | 10.7 (6.3-14.8) | - | 11.8 (9.0-14.8) | - |
| notes | - | - | 8.2 (5.8-29.5) | - | 21.4 (9.0-576.9) | 17.6 (8.5-72.7) |

### Lines of code: app file only (shared glue common.py / lib.rs excluded)
| app | original | fui | web | egui | gtk | qt |
|---|---|---|---|---|---|---|
| flei | 1106 | 62 | 533 | 82 | 41 | 30 |
| chat | - | - | 309 | 74 | 50 | 44 |
| settings | - | - | 294 | 82 | 36 | 35 |
| dash | - | 328 | 328 | 87 | 48 | 42 |
| files | - | 100 | 302 | 56 | 45 | 38 |
| form | - | 26 | 275 | 65 | 45 | 40 |
| notes | - | - | 256 | 53 | 42 | 17 |

### Runtime size (MB, not part of binary/bundle size): Web = Chromium, gtk = python3, qt = PySide6+shiboken
| toolkit | MB |
|---|---|
| web | 353.8 |
| gtk | 6.8 |
| qt | 249.9 |

Runtime (Chromium, /usr/lib/chromium): 354 MB (not part of the Web bundle size above).

### Visual similarity (SSIM / mean abs. deviation %) vs reference (flei: original Svelte, others: web)
| app | theme | fui | web | egui | gtk | qt |
|---|---|---|---|---|---|---|
| flei | dark | 0.823 / 2.94 | 0.943 / 0.84 | 0.690 / 6.54 | 0.685 / 6.93 | 0.687 / 6.49 |
| flei | light | - | - | - | - | - |
| chat | dark | - | - | 0.717 / 6.83 | 0.725 / 6.72 | 0.724 / 6.75 |
| chat | light | - | - | 0.774 / 6.81 | 0.780 / 6.71 | 0.784 / 6.74 |
| settings | dark | - | - | 0.859 / 3.43 | 0.884 / 2.96 | 0.864 / 3.37 |
| settings | light | - | - | 0.907 / 3.56 | 0.903 / 2.99 | 0.912 / 3.52 |
| dash | dark | 0.758 / 5.15 | - | 0.613 / 8.77 | 0.597 / 9.04 | 0.601 / 8.83 |
| dash | light | 0.776 / 5.00 | - | 0.686 / 10.58 | 0.660 / 10.67 | 0.659 / 10.60 |
| files | dark | 0.563 / 7.60 | - | 0.529 / 7.96 | 0.566 / 7.02 | 0.515 / 8.40 |
| files | light | 0.577 / 6.84 | - | 0.574 / 7.10 | 0.580 / 7.62 | 0.550 / 7.72 |
| form | dark | 0.827 / 4.13 | - | 0.854 / 3.37 | 0.850 / 3.36 | 0.881 / 2.92 |
| form | light | 0.924 / 2.57 | - | 0.910 / 2.75 | 0.900 / 2.87 | 0.892 / 2.80 |
| notes | dark | - | - | 0.700 / 5.66 | 0.651 / 7.14 | 0.707 / 5.59 |
| notes | light | - | - | 0.736 / 6.53 | 0.731 / 6.10 | 0.749 / 6.26 |


## Notes and honest caveats
- **Startup columns**: "First draw" (interactive start, spawn -> first changed pixel) is the toolkit-neutral number. "Startup to --shot PNG"
  contains per-toolkit waits of the shot mode (GTK 900 ms timer, Qt 700 ms timer, egui 6 frames, Web = Chromium cold start incl. profile creation) and is NOT comparable across toolkits.
- **Lines**: first table = app file + shared glue (gtk/qt `common.py`, egui `lib.rs` counted again for every app), second table = app file only.
  Python lines are denser (many `;`). Web counts html+css+js, never the data (read from `data/`). fUi counts only the app's `main.fi`.
  **fUi kit effort is NOT in the fUi numbers**: the kit parts written for these apps are `screen.fi` 148 + `kitapp.fi` 91 + `kitdata.fi` 48 lines (in main) plus the files-app parts (appshot, tsvtable, jsontree, skin: ~416 lines, not merged);
  the whole existing fUi library is 86442 lines (`lib/fui`). The short app files are only possible because that library exists (same holds for Qt/GTK/egui/Chromium, which are not counted either).
- **flei** = visual rebuild only (fake data, no backend). Original 1106 lines = UI code only (App.svelte, app.css, Home, Sidebar, TitleBar, AccountMenu, loaderIcons; no Rust backend, no api/stores). The original has no light theme -> no light reference for flei.
- **Size**: egui/fUi = native binary (egui strip+lto thin, fUi release default); Web/gtk/qt = source files only; runtimes (Chromium 354 MB, PySide6 250 MB, python3 binary 6.8 MB without the system GTK libs) are listed separately.
- **RAM**: PSS summed over the whole process tree (Web: all browser processes). egui runs with software GL (`LIBGL_ALWAYS_SOFTWARE=1`, llvmpipe), so its RAM/start are Xvfb numbers, not GPU numbers.
- **Latency**: XTest -> first changed pixel (includes hover/press feedback); each toolkit gets its own input spec (`apps/<app>/<toolkit>/measure.json`), so the clicked element differs a bit; `-` = that app showed no visible change for the input. Large max values are scheduler/load outliers (median is the value to read).
- **GTK files**: ~8 s start (Python filling a ListStore with 100000 rows) and 26 % CPU idle (busy after load) -> real finding, not a measurement error.
- **Build time**: fUi flei 7.5 s (clean firnc compile, one app); egui clean build of all 7 bins 69 s (worker log); Web/GTK/Qt interpreted = 0.
- **Frame times** (scroll/resize/animation): no common hook; known single values: Web files scroll 300 frames median 16.7 ms (vsync-capped), main thread 9.6 ms/frame; sort 100000 rows 139 ms; fUi files scroll (host CPU painter, 100k rows, 240 frames) median 6.0 ms, max 11.3 ms (separate benches, different methods, not same-method comparable).
- **Where fUi looks worse**: see `gaps-fa.md` / `gaps-fb.md` (scratch): DejaVu instead of Inter, no avatars/gradients/glow, active-nav style, Forge/Quilt glyphs.
- Visual shots/galleries: `gallery/index.html`, `collage.png` in the scratch dir (not in git).
