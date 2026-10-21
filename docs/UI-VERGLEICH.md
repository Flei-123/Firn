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
RESULTS_PLACEHOLDER
