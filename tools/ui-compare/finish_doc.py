#!/usr/bin/env python3
"""Regenerate the '## Results' section of docs/UI-COMPARISON.md from results/*.json.
Usage: finish_doc.py <path to UI-COMPARISON.md>"""
import datetime
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
doc = sys.argv[1]
tables = subprocess.run([sys.executable, os.path.join(HERE, "mdtable.py")], capture_output=True, text=True,
                        env=dict(os.environ, UIVGL_DIR=os.environ.get("UIVGL_DIR", HERE))).stdout
res = os.path.join(HERE, "results")
have_fui = sorted(f[:-9] for f in os.listdir(res) if f.endswith("-fui.json") and f[:-9] in ("flei", "chat", "settings", "dash", "files", "form", "notes"))
loads = []
for f in os.listdir(res):
    if f.endswith(".json") and f not in ("compare.json", "web-perf.json"):
        try:
            loads.append(json.load(open(os.path.join(res, f))).get("load_avg", 0))
        except (ValueError, OSError):
            pass
load_s = "%.0f-%.0f" % (min(loads), max(loads)) if loads else "?"
missing = [a for a in ["flei", "chat", "settings", "dash", "files", "form", "notes"] if a not in have_fui]

head = open(doc).read().split("## Results")[0]
body = """## Results
Generated %s by `tools/ui-compare/finish_doc.py` from `results/*.json` (real measurements only; `-` = not measured / no result).
Server load average during the runs: %s (shared server, 4 toolkits measured in parallel lanes, each with its own Xvfb).
Runs: 5 per metric (startup, first draw, latency), 3 for RAM/CPU idle (15 s each). Cells: median (min-max).

fUi apps present: %s. **Missing fUi apps: %s** (not built within the task window).
Windows and phone numbers: **needs laptop** (offline; nothing guessed).

%s

## Notes and honest caveats
- **Startup columns**: "First draw" (interactive start, spawn -> first changed pixel) is the toolkit-neutral number. "Startup to --shot PNG"
  contains per-toolkit waits of the shot mode (GTK 900 ms timer, Qt 700 ms timer, egui 6 frames, Web = Chromium cold start incl. profile creation) and is NOT comparable across toolkits.
- **First draw caveat**: first changed pixel = first paint of the window; some toolkits may paint an empty window background first, so it is a lower bound of "content visible" (fUi ~27 ms looks like window + first frame; Web/Qt/GTK include their runtime start).\n- **Lines**: first table = app file + shared glue (gtk/qt `common.py`, egui `lib.rs` counted again for every app), second table = app file only.
  Python lines are denser (many `;`). Web counts html+css+js, never the data (read from `data/`). fUi counts only the app's `main.fi`.
  **fUi kit effort is NOT in the fUi numbers**: the kit parts written for these apps are `screen.fi` 148 + `kitapp.fi` 91 + `kitdata.fi` 48 lines (in main) plus the files-app parts (appshot, tsvtable, jsontree, skin: ~416 lines, not merged);
  the whole existing fUi library is 86442 lines (`lib/fui`). The short app files are only possible because that library exists (same holds for Qt/GTK/egui/Chromium, which are not counted either).
- **flei** = visual rebuild only (fake data, no backend). Original 1106 lines = UI code only (App.svelte, app.css, Home, Sidebar, TitleBar, AccountMenu, loaderIcons; no Rust backend, no api/stores). The original has no light theme -> no light reference for flei.
- **Size**: egui/fUi = native binary (egui strip+lto thin, fUi release default); Web/gtk/qt = source files only; runtimes (Chromium 354 MB, PySide6 250 MB, python3 binary 6.8 MB without the system GTK libs) are listed separately.
- **RAM**: PSS summed over the whole process tree (Web: all browser processes). egui runs with software GL (`LIBGL_ALWAYS_SOFTWARE=1`, llvmpipe), so its RAM/start are Xvfb numbers, not GPU numbers.
- **Latency**: XTest -> first changed pixel (includes hover/press feedback); each toolkit gets its own input spec (`apps/<app>/<toolkit>/measure.json`), so the clicked element differs a bit; `-` = that app showed no visible change for the input. Large max values are scheduler/load outliers (median is the value to read).
- **GTK files**: ~8 s start (Python filling a ListStore with 100000 rows) and 26 %% CPU idle (busy after load) -> real finding, not a measurement error.
- **Build time**: fUi flei 7.5 s (clean firnc compile, one app); egui clean build of all 7 bins 69 s (worker log); Web/GTK/Qt interpreted = 0.
- **Frame times** (scroll/resize/animation): no common hook; known single values: Web files scroll 300 frames median 16.7 ms (vsync-capped), main thread 9.6 ms/frame; sort 100000 rows 139 ms; fUi files scroll (host CPU painter, 100k rows, 240 frames) median 6.0 ms, max 11.3 ms (separate benches, different methods, not same-method comparable).
- **Where fUi looks worse**: see `gaps-fa.md` / `gaps-fb.md` (scratch): DejaVu instead of Inter, no avatars/gradients/glow, active-nav style, Forge/Quilt glyphs.
- Visual shots/galleries: `gallery/index.html`, `collage.png` in the scratch dir (not in git).
""" % (datetime.date.today().isoformat(), load_s, ", ".join(have_fui) or "none", ", ".join(missing) or "none", tables)
open(doc, "w").write(head + body)
print("doc written:", doc)
