# MC-RENDER -- Renderer des Voxel-Clients: Ist-Analyse und Entscheidung

Stand: 2026-10-11 (Worker mc-w7). Quellen: GDD-SDD Kap. 7/9, `FIRN-GAPS.md` (H3),
`docs/FFI.md`, Code unter `lib/`. Kennzeichnung: **[belegt]** = im Code/Test gelesen oder
gelaufen, **[ungeprueft]** = Annahme, nicht gemessen.

## 1. Entscheidung (vom Boss vorgegeben)

1. **CPU-Software-Renderer zuerst.** Laeuft ueberall (Linux, Windows, Web/Wasm, OrientOS),
   braucht keine GPU-Bibliothek, ist per Bild-Hash gegen eine Python-Referenz testbar.
2. **GPU-Backend danach als austauschbare Schnittstelle** (Backend-Interface: Mesh-Upload,
   Draw, Present). Der Software-Renderer bleibt Fallback UND Referenz (so wie `lib/fui/gpu.fi`
   es fuer fUi vormacht: "The software painter stays: FALLBACK and REFERENCE") [belegt].

Eingabe des Renderers ist schon fertig: `mc_mesh` (8-Byte-Vertices + u32-Indices pro
16^3-Sektion, AO 0..3, Tile-ID, u/v-Wiederholung) und `mc_math` (Mat4, Frustum, Raycast) [belegt].

## 2. Ist-Analyse

### 2.1 Daidalos (`/root/projects/daidalos`, pausiert) [belegt, nur gelesen]

* **Sprache/API:** C++17 + C-Header (`include/dai_render.h`, `extern "C"`), CMake/`build.sh`,
  **Vulkan 1.3 mit Dynamic Rendering**, SPIR-V-Shader in `shaders/` (mesh, sky, shadow, post,
  particle, ui, lines). Lizenz GPL-3.0-or-later + 7(b)-Namensklausel -- **nicht** mit der
  MPL-2.0-Linie von Firn mischbar, ohne die Lizenzfrage zu klaeren.
* Render-Interface ist backend-neutral gedacht ("replace one .cpp") und kann offscreen
  rendern + Pixel zurueckholen (Regressionstest per Readback).
* **Wiederverwendbar als Idee:** Offscreen-Readback-Test, Konventionen (rechtshaendig, +Y oben),
  Post-Pipeline-Aufteilung. **Nicht wiederverwendbar als Code:** C++ (Firn-Projekt braucht Firn),
  Mesh-Pfad ist fuer glTF/Szenen gebaut, nicht fuer 8-Byte-Voxel-Quads; Lizenz s.o.
* Fazit: kein Code uebernehmen. Als Referenz fuer ein spaeteres Vulkan-Backend lesen.

### 2.2 Firn (Stand main 2026-10-11)

| Baustein | Befund |
|---|---|
| `lib/paint/canvas.fi` | `Canvas`: `px` = w*h*4 Byte **premultiplied RGBA**, CPU-Pixelpuffer [belegt] |
| `lib/paint/png.fi` | `write_ppm`, `write_png` (nimmt `Canvas`), `decode_png` (alle Farbtypen, 1-16 Bit, Adam7) [belegt]. Also **PNG-Decoder + -Encoder vorhanden** |
| `lib/fui/*` (`scene.fi`, `painter.fi`, `render.fi`) | 2D-UI: Rechtecke, Text, Schatten, Glas auf der CPU. **Kein 3D**, nur 2D-Dreieck/Shape-Pfade [belegt] |
| `lib/fui/gpu.fi` + `gpu_vs/fs.glsl` | 2D-GPU-Backend fuer fUi (Canvas = GPU-Textur). Laeuft **nur Web (WebGL2) + Android (GLES3)** [belegt, Kopfkommentar]. GLSL ES 3.00 / GL 3.3-kompatibel |
| `lib/plat/gles.fi` | `extern fn gl*` gegen libGLESv3: nur fuer Programme, die **beim Bauen** gegen GLES gelinkt sind (Android). Fuer statische Linux-Programme unbrauchbar [belegt] |
| `lib/plat/webgl.fi` | WebGL2 ueber Host-Calls (`firn.js`) [belegt] |
| `lib/paint/gl.fi` | Tabelle von GL-Funktionen (`GlApi`), die der fUi-Renderer benutzt [belegt] |
| `lib/window/*` (x11, win32, android, osum) | Fenster + Pixel ausgeben: **X11** = eigenes X-Protokoll (kein Xlib), `PutImage` in 64-KiB-Baendern oder **ShmPutImage**; **Windows** = `BitBlt` (GDI); Android/Osum eigene Wege. `r_gpu_open` in `backend.fi` (X11) und `backend.windows.fi` = **ehrliches `false`** [belegt] |
| `lib/plat/fuiwin.fi` | Fensterschleife: Canvas RGBA -> BGRX, ein PutImage pro Frame [belegt]. Das ist der Praesentationsweg fuer einen CPU-Renderer |
| `lib/std/dynlib.fi` / `dynlib.windows.fi` | **`dlopen`/`dlsym`/`callx` (Linux) und `LoadLibraryA`/`GetProcAddress` + Win64-Gate (Windows) existieren** (Commit 2d1611789, 09.10.2026). Programm mit `#[link_lib(c)]` wird dynamisches ELF (+0,4 ms Start) [belegt] |
| `tools/ffi/egl_probe.fi`, `win_gl_probe.fi` | Echt gelaufen: EGL-Kontext (Pbuffer/surfaceless) + `glClear` + `glReadPixels` auf Linux (Mesa llvmpipe), `wglCreateContext` unter Wine. **Keine echte GPU, kein echtes Windows, kein Vulkan-Probe** [belegt, laut docs/FFI.md] |

### 2.3 Blocker H3 -- gilt er noch?

`FIRN-GAPS.md` (H3 "kein Desktop-GPU-Pfad", R9 "dynamisches Laden/FFI", B16 "Win32-Importliste")
ist **teilweise ueberholt**:

* **R9/B16 sind geloest** (`std.dynlib`, 09.10.): Eine Bibliothek wie `libEGL.so.1`,
  `libGL.so.1`, `libvulkan.so.1`, `opengl32.dll` ist ladbar und aufrufbar [belegt].
* **H3 bleibt offen**, aber enger: Es fehlt (laut `docs/FFI.md`, Abschnitt "What r114 still
  needs") eine `GlApi`-Tabelle ueber dynlib (`lib/plat/gldyn.fi`), `gpu_open` fuer Windows
  und besonders **X11**: das X11-Fenster ist ein eigener Socket ohne Xlib, ein EGL/GLX-Window-
  Surface braucht aber ein Xlib/xcb-`Display*`. Wege: (a) Xlib per dynlib auch fuer das Fenster,
  (b) Fenster bleibt, EGL **surfaceless/Pbuffer -> FBO -> glReadPixels -> PutImage** (GPU rechnet,
  ein Readback pro Frame; Kosten **nicht gemessen**), (c) Vulkan hat dasselbe Verbindungsproblem.
  Variante (b) ist ohne neue Fensterarbeit moeglich [ungeprueft als Ganzes, Teile belegt].
* Floats > 8 Argumente / Variadics / Callbacks unter Linux: nicht noetig fuer GL-Kern, ungetestet.
* Threads: Fremdbibliotheken nur vom Haupt-Thread aus aufrufen (Firn-Threads haben anderes
  `fs`) -- wichtig fuer ein Backend-Design (Render-Thread = Hauptthread) [belegt, Doku].

## 3. Optionen je Plattform

| Plattform | Software-Renderer | GPU heute | GPU-Weg (spaeter) |
|---|---|---|---|
| Linux | ja: Canvas -> X11 ShmPutImage [belegt, fuiwin] | nein ohne neuen Code | dynlib + EGL (surfaceless, FBO-Readback) oder Xlib-Fenster; `GlApi`-Tabelle |
| Windows | ja: BitBlt [belegt] | nein ohne neuen Code | dynlib + `opengl32`/`wglCreateContextAttribsARB` (Probe unter Wine belegt, echtes Windows **ungeprueft**); `SwapBuffers` |
| Web | ja (Wasm, Canvas-Putimage) [ungeprueft fuer Voxel-Last] | ja: WebGL2 via `webgl.fi` [belegt fuer fUi] | direkt WebGL2 (GLSL ES 3.00) |
| Android | ja | ja: GLES3 via `gles.fi` [belegt fuer fUi] | wie fUi |
| OrientOS | ja: Framebuffer/Osum-Fenster [belegt: kein GL/Vulkan, nur UEFI-FB/virtio-gpu, GAPS O2] | nein | keiner geplant; Software ist der einzige Weg |

Konsequenz: Nur der CPU-Renderer deckt **alle** Zeilen ab. Er ist auf OrientOS der einzige.

## 4. Architektur

```
 mc_world -> mc_mesh (Sektion -> Vertices/Indices)  ----\
 mc_camera (View/Proj/Frustum/Pick)  ------------------+--> RenderBackend
 mc_texture (Atlas, Tile->UV, Block->Tile-Tabellen) ---/      |
                                                     +--------+----------+
                                                  mc_raster (CPU)     gpu_gl (spaeter)
```

Backend-Interface (Entwurf, noch nicht gebaut -- Funktionen mit Zustandsstruct, kein Closure noetig):

| Aufruf | Bedeutung |
|---|---|
| `backend_upload_atlas(b, rgba, w, h)` | Atlas einmal hochladen |
| `backend_mesh_upload(b, key, verts, nverts, idx, nidx) -> handle` | Sektions-Mesh ablegen (CPU: Zeiger behalten/kopieren; GPU: Pool-Block) |
| `backend_mesh_free(b, handle)` | freigeben |
| `backend_begin(b, view, proj, w, h)` / `backend_draw(b, handle, origin)` / `backend_end(b)` | Frame; Draw mit Sektions-Ursprung (Vertices sind sektionsrelativ) |
| `backend_present(b)` | CPU: Pixel an `window`; GPU: Swap bzw. Readback |
| `backend_read_pixels(b, out)` | fuer Tests (beide Backends muessen vergleichbare Bilder liefern) |

Festlegungen, die das Interface tragen:
* Vertices bleiben das mc_mesh-Format (Position 0..16 sektionsrelativ), Sektions-Ursprung kommt als
  Uniform/Parameter -> gleicher Mesh-Puffer fuer CPU und GPU.
* Kamera-Mathe in **f64**, Upload an GPU per `mat4_to_f32`; Sektions-Ursprung **kamerarelativ** abziehen
  (f64 -> kleine f32), sonst Zittern weit vom Ursprung. Der CPU-Rasterer rechnet intern in f64.
* Backend-Wahl zur Laufzeit; Tests laufen immer gegen den Software-Pfad.

## 5. Reihenfolge, Aufwand, Messplan

Aufwand: S = <1 Tag, M = 1-3 Tage, L = Woche+. Alles Schaetzung, nicht gemessen.

| # | Schritt | Aufwand | Status |
|---|---|---|---|
| 1 | `mc_camera` (View, Projektion, Frustum, Pick-Ray) | S | dieser Worker |
| 2 | `mc_texture` (prozeduraler Atlas, Tile->UV, Block->Tile-Tabellen) | S-M | dieser Worker |
| 3 | `mc_raster` (Dreiecks-Rasterer, Tiefe, AO, PPM/PNG, Python-Referenz) | M | dieser Worker |
| 4 | Fenster-Praesentation: Rasterer -> `window` (fuiwin-Weg RGBA->BGRX, PutImage/BitBlt) | S | offen |
| 5 | Welt-Renderer: Sektionen meshen (Jobs), Frustum-Culling, Reihenfolge vorne->hinten, Mesh-Cache | M | offen |
| 6 | Optimierung CPU: Tile-/Span-Rasterung, Mehrthread-Baender, Grobtiefe (Hi-Z), SIMD falls Firn es kann (B19 offen) | M-L | offen |
| 7 | Transparenz-Durchlauf (Wasser/Glas sortiert), Ausschnitt-Texturen (Laub, alpha test), Himmel/Nebel | M | offen |
| 8 | Backend-Interface als echte Firn-API + `gpu_gl`: dynlib-`GlApi`, EGL-surfaceless-FBO (Linux), wgl (Windows), WebGL2 | L | offen, H3-Rest |
| 9 | Vulkan-Probe (`libvulkan.so.1` laden, Instanz anlegen) | S | offen |

**Messwert-Plan:** (a) Dreiecke/s und ms fuer 640x360 Testszenen (in `mc_raster`-Cross-Check),
(b) spaeter ms/Frame fuer eine echte Weltszene (Render-Distanz 8 und 12, 1280x720) -- Ziel
ist eine **Kostenschaetzung, ab welcher Szene der CPU-Weg nicht mehr reicht**, damit Schritt 8
begruendet startet; (c) Schritt 8: Readback-Kosten (`glReadPixels` 1280x720) einmal messen,
bevor Variante (b) fuer X11 gewaehlt wird. Referenzwert aus Firn: CPU-Raster-Tempo fUi 10,5/6,2 ms
bei 1240x720 (2D) [belegt in GAPS H4] -- sagt wenig ueber 3D.

## 6. Nicht belegt / Risiken

* Ob ein reiner CPU-Renderer in Firn fuer Render-Distanz 12 bei 60 FPS reicht: **ungeprueft**
  (Firn hat keine Float-Registerallokation auf `main`, keine SIMD-Multiplikation -- B3/B19).
  Daher gilt der Software-Renderer zuerst als Korrektheits-Referenz, nicht als Endziel auf Desktop.
* Echte GPU, echtes Windows, Vulkan: nie gelaufen. Nur Mesa llvmpipe und Wine.
* Texturen: keine Mojang-Dateien. `mc_texture` erzeugt ein eigenes prozedurales Standardpaket;
  eigene/freie PNGs koennen spaeter ueber `paint.png.decode_png` geladen werden.
