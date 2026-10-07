# GAP-ANALYSE-0710 — Was fehlt fUi und OrientOS für den Alltag?

Stand 07.10.2026. Auftrag von Justin. Geprüft am Code von Firn `main` (64397869b, 05.10.2026)
und OrientOS/Osum `main` (9467c5da, 07.10.2026), plus beide Roadmaps. **Nichts davon
wurde neu gebaut oder auf Hardware gemessen — es ist eine Lesung des Codes, der Docs und
der Testwerkzeuge.** Am Dell wurde nichts geändert.

**Regeln dieser Datei**

* Status: **vorhanden** = Code da und ein Test/Beleg genannt · **teilweise** = Kern da, Rest benannt ·
  **fehlt** = im Code gesucht und nichts gefunden (Suchbegriffe stehen beim Beleg) ·
  **unbekannt** = konnte ich nicht belegen — das ist keine Vermutung für "fehlt".
* "Test grün" heißt hier nur: so steht es in den Docs/Roadmaps. Ich habe **keinen** Testlauf
  wiederholt (Platte/Last, und der Auftrag war eine Analyse).
* Aufwand (Schätzung, nicht gemessen; ein Entwickler mit KI-Hilfe): **S** ≤ 3 Tage · **M** ≤ 3 Wochen · **L** > 3 Wochen.
* Nutzen für typische Apps: **5** = ohne das gibt es keine ernsthafte App · **1** = Komfort.
* Beleg-Kürzel: `lib/…` = Firn-Repo, `k/…` = OrientOS-Repo (`kernel/…`), `r123` = Roadmap-Punkt des jeweiligen Projekts.

---

# TEIL 1 — fUi gegenüber Win32/WPF/WinUI 3, Qt, Flutter, Compose, Tauri/Web

## 1.0 Was fUi ist (Ausgangslage, belegt)

* 46 804 Zeilen in `lib/fui` (57 Dateien), 67 Prüfprogramme `tools/fui/*_main.fi`, Plattformschichten X11
  (`lib/window/x11.fi`, eigenes Protokoll, ohne Xlib), Win32 (`lib/window/win32.fi`, 2700 Z.), Android
  (`lib/window/android.fi`), Web/WASM (`lib/plat/web.fi`), OrientOS (`lib/window/osum.fi`).
* Zwei Modelle nebeneinander: (a) **Szenenbaum** (`scene.fi`: Box/Text/Bild/SVG/Widget, Stilblatt `sheet.fi`,
  Flex `flex.fi`, Ereignisweg capture/target/bubble `event.fi`, a11y `a11y.fi`) und (b) **Immediate-Mode-Teile**
  (`kit.fi`, `wave2.fi`, `wave3.fi`, `markdownview.fi`), die der Aufrufer selbst pro Bild zeichnet und trifft.
* Hochebene `fui.app` (`lib/fui/app.fi`, 10 Zeilen "Hello Window"): **nur** label, button, entry, textarea,
  list/list_item, row/column/spacer (Funktionsliste in `app.fi`). Alles andere ist heute Low-Level.
* **Die Sprache hat keine Closures** (`docs/fui-quickstart.md`: "Firn has no closures, so Close gets a named function") —
  jeder Handler ist eine benannte `fn` plus `ud`-Zeiger.
* Zahlen aus der Roadmap (gemessen von den Workern, hier nicht wiederholt): Rasterer 1240×720 10,5 ms ohne / 6,2 ms mit
  Cache; Baumarbeit 1009 Knoten 0,9 ms; Audit "unbenannte Bedienelemente" 0.
* **Stand der Windows-Schicht (Roadmap veraltet!):** r24 ("Win32-Plattformschicht") und r86 ("fui.app Win32-Host") stehen
  offen, aber `lib/window/win32.fi` (2700 Z.), `lib/@windows/fui/apphost.fi` und `tools/pack/installer/ui.fi`
  (fUi-Installer, 635 Z.) **existieren** (Commit d317249f3, 04.10.). Getestet ist das **nur unter Wine**
  (`docs/PACKAGING.md`, `docs/DESKTOP.md`, `docs/APPKIT.md`: "not on a real Windows machine"). → r24/r86 sollten
  auf "gebaut, nur Wine" umgeschrieben werden, offen bleibt der echte Windows-Test (r121).

## 1.1 Layout

| # | Punkt | Status | Beleg | Aufwand | Nutzen |
|---|---|---|---|---|---|
| L1 | Flexbox (Richtung, wrap, grow/shrink in Tausendstel, justify, align, gap, min/max) | **vorhanden** | `lib/fui/flex.fi` (856 Z.), `tools/fui/flex_main.fi`, r3/r76 | — | 5 |
| L2 | Grid (Spalten/Zeilen, Spans, auto/fr-Spuren) | **teilweise**: nur fest `Grid{cols,rows}` mit Rand/Abstand; im Szenenbaum kein Grid | `lib/fui/layout.fi` 296 ff.; `grep grid scene.fi` → nur Kommentar | M | 3 |
| L3 | Constraints: min/max-Größe | **vorhanden** (`node_set_bounds`, `item_set_bounds`); **Seitenverhältnis, min-/max-content, fit-content**: **fehlt** (grep `aspect|fit.content` → 0 Treffer) | `scene.fi`, `flex.fi` | S–M | 3 |
| L4 | Scrollen (Rollbalken, Rad, Touch-Fling) | **vorhanden** | `lib/fui/viewport.fi` (828 Z.), `kittouch.fi`, r5 | — | 5 |
| L5 | **Baumgröße / Virtualisierung langer Listen** | **fehlt — Blocker.** `SCENE_MAX = 128` Knoten, `KIDS_MAX = 64`; fui.app: 128 Knoten, 8 Textfelder, 256 Oktett je Text; "about 24 rows fit". Virtualisierung nur von Hand. | `scene.fi:166/171`, `docs/fui-quickstart.md` "Limits today", `docs/fui-list-rows.md`, r102, r134 | L | **5** |
| L6 | Responsive (Media-/Container-Queries, Breakpoints) | **teilweise**: fließende Schrift `clamp()` (r82), flex-wrap; **keine** Media-/Container-Queries (grep → 0) | `style.fi:284` | M | 3 |
| L7 | Mehrfach-Schichten/Overlay im Fenster, Ink-Culling | **vorhanden** (`layer.fi`, Culling r101) | `lib/fui/layer.fi` | — | 3 |
| L8 | Layout-Animation (Elemente gleiten beim Umordnen) | **fehlt** (Übergänge nur an Stilwerten) | `anim.fi`, r110/r126 | M | 3 |
| L9 | Inkrementelles Layout (Schmutzbits) | **teilweise**: Messgedächtnis + Stil-/Layout-Memo (r100/r109), kein echtes Dirty-Flagging je Knoten | `scene.fi` memo_* | M | 3 |

## 1.2 Controls

| # | Punkt | Status | Beleg | Aufwand | Nutzen |
|---|---|---|---|---|---|
| C1 | Button, Label, Checkbox, Radio, Toggle, Slider, Progress, Dropdown, Tabs, Toolbar, Menü(leiste), Tooltip, Trenner, Gruppe | **vorhanden** als Zeichner+Zustand (`widget.fi` KIND_* 1–22) — **aber nicht in `fui.app`** (dort nur Label/Button/Entry/Textarea/Liste) | `wave2.fi` (1299 Z.), `widget.fi`, `app.fi`, r89 | M (app-API) | 5 |
| C2 | Texteingabe einzeilig (Auswahl, Undo, Schreibmarke, Passwort-Maske, Kopierverbot) | **vorhanden** | `editor.fi`, `textbuf.fi`, r88/r99/r108 | — | 5 |
| C3 | Mehrzeilig (Textarea) | **teilweise**: `textarea.fi` (771 Z.), `LINES_MAX 2048`; fui.app 4096 Oktett; Shift+Klick, bidi-Cursor, Layout-Cache >100 KB fehlen | r127, r132 | M | 5 |
| C4 | RichText anzeigen/auswählen/kopieren | **vorhanden** (Spans, Links, Auswahl) — **editierbarer RichText: fehlt** | `richtext.fi` (1235 Z.), `docs/UI_EXTRAS.md` | L (Editor) | 3 |
| C5 | Combobox (editierbar, Autovervollständigen) | **teilweise**: `draw_dropdown` (nicht editierbar); Rolle `combobox` nur in a11y | `wave2.fi:719`, `a11y.fi` | M | 4 |
| C6 | Liste | **teilweise**: `draw_list`, `listrow.fi`; **Tastatur (Pfeile/Enter), Mehrfachauswahl, Umbenennen, Virtualisierung fehlen** | r134, `docs/fui-list-rows.md` "Open" | M | 5 |
| C7 | Tabelle (Spaltenkopf, Zeilen, Zellen) | **teilweise**: Zeichner da (`draw_table*`); Spalten ziehen, Sortieren, Zelle bearbeiten, Virtualisierung: **nicht gefunden** (grep `sort|resize` in wave3 → 0) | `wave3.fi` | M–L | 5 |
| C8 | Baum | **teilweise**: `draw_tree/draw_twisty/draw_tree_row`; Tastaturnavigation, Drag-Umordnen, Lazy-Load: nicht gefunden | `wave3.fi` | M | 4 |
| C9 | Tabs / Seitenleiste / Kacheln / Avatar / Suchfeld / Toast / modaler Dialog mit Fokusfalle | **vorhanden** | `kit.fi` (2242 Z.), `docs/fui-kit.md`, `kit_main.fi` | — | 4 |
| C10 | Menüleiste, Menü, Kontextmenü, Untermenü-Pfeil | **teilweise**: Zeichnen vorhanden (`draw_menubar/menu/menu_item`, `submenu`-Marke). **Beschleuniger/Mnemonics (Alt+Buchstabe), Shortcut-Registry**: nicht gefunden. **Popups nur innerhalb des eigenen Fensters** (kein eigenes Toplevel-Popup gefunden) | `wave2.fi:889 ff.` | M | 5 |
| C11 | Dialoge | **teilweise**: Dialograhmen + Scrim + Knöpfe + Datei-/Farb-/Datumsdialog als **fUi-eigene** Zeichner; **native** Dialoge fehlen (siehe P1) | `wave3.fi`, `lib/fuishell/filechooser.fi` | M | 4 |
| C12 | Tooltip | **vorhanden** (Platzierung, ohne grauen Kasten) | `wave2.fi` `tooltip_place` | — | 3 |
| C13 | Datum | **vorhanden** (`draw_datepicker`); **Zeit-Picker**, Bereich: **fehlt** (grep → 0) | `wave3.fi` | S | 3 |
| C14 | Slider, Progress | **vorhanden**; Range-Slider (2 Griffe) nicht gefunden; Progress mit Text + unbestimmt: `kit.progress_draw` | `wave2.fi`, `kit.fi` | S | 3 |
| C15 | Splitter / Docking | **teilweise**: `fuishell/dock.fi` (875 Z., 4 Bereiche, Tabs, Splitter, Layout als JSON) + `Split`; Modell ohne Szenenbaum-/fui.app-Anbindung | `lib/fuishell/dock.fi` | M | 4 |
| C16 | Mehrfenster | **teilweise**: Schicht kann es (`fuiwin.fi window_step/window_wait_many`); **fui.app: ein Fenster** (r89) | `docs/fui-quickstart.md` | M | 4 |
| C17 | Drag & Drop **Datei auf Fenster** | **vorhanden** (X11 XDND v5, Win `WM_DROPFILES`) — Windows-Pfad nur Teil unter Wine bewiesen | `docs/DESKTOP.md` | — | 4 |
| C18 | Drag & Drop **aus der App heraus / innerhalb** (Elemente, Text, Inhalte) | **fehlt** (nur Ziel, keine Quelle; Dock hat eigenes Tab-Ziehen) | `x11.fi` dnd_* nur Ziel; grep `DoDragDrop` → 0 | M | 4 |
| C19 | Zwischenablage | **teilweise**: Text, `CF_HDROP`, PNG, eigene Formate (X11 + Windows); Android/OrientOS-Schicht `false` laut Doc; Web `webclip.fi` | `docs/DESKTOP.md`, `lib/plat/webclip.fi` | S–M | 5 |
| C20 | Weitere WinUI-Elemente: Spinner/NumberBox, Expander/Accordion, Breadcrumb, Pagination, InfoBar, Stepper, Rating, Karussell, TeachingTip, CommandBar, Ribbon | **fehlt** (Namenssuche in `lib/fui` + `fuishell` → 0 Treffer; nur `nav`/`tabbar`/`toasts` in kit). Roadmap r45 nennt "15 fehlende Elemente" — die Liste steht nur im Chat, **nicht einzeln gegengeprüft** | `grep` | M je Gruppe | 3 |
| C21 | Gitter-/Spreadsheet-/Chart-Steuerung | **fehlt** (kein Chart-Widget; `viewport` + Painter bieten die Basis) | — | L | 3 |

## 1.3 Eingabe

| # | Punkt | Status | Beleg | Aufwand | Nutzen |
|---|---|---|---|---|---|
| E1 | Unicode/UTF-8/WTF-8 im Text | **vorhanden** | `textbuf.fi:236` | — | 5 |
| E2 | **Emoji (Farbe)** | **fehlt** (kein COLR/CPAL/sbix/CBDT-Leser; Treffer nur Kommentar in `plat/sysfont.fi`) | `grep -ril 'colr\|cbdt\|sbix' lib` | L | 4 |
| E3 | **OS-IME / Komposition** | **teilweise**: eigene Eingabetabellen Romaji→Kana (176), Hangul, Pinyin (3883 Zeichen) im Feld (`ime.fi`, 1514 Z.); Android-Bildschirmtastatur (Emulator); Web: verstecktes `<textarea>`. **Windows: kein WM_IME_*/IMM/TSF** (`win32.fi`: nur WM_CHAR), **X11: kein XIM/ibus/fcitx** (grep → 0). Pinyin nur Einzelzeichen (r16) | `ime.fi`, `win32.fi:1194`, `docs/fui-real-devices.md` | M (Win), M (Linux) | 4 (CJK) / 2 (DE/EN) |
| E4 | Bidi/RTL, Spiegelung | **vorhanden** (UAX#9: BidiCharacterTest 91707/91707, BidiTest 770241/770241); bidi-Cursorbewegung im Textarea fehlt | r10, r11, r127 | S | 3 |
| E5 | Tastaturnavigation | **teilweise**: Tab/Shift+Tab, Enter/Leertaste, Pfeile im Kit, Fokusfalle im Modal; **Fokusmodell für Listenzeilen fehlt (r134)**; Shortcuts/Mnemonics nicht gefunden; Esc schließt Fenster nicht (Doc) | `control.fi`, `kit.fi` | M | 5 |
| E6 | Touch/Gesten (Tap, Doppel, Lang, Pan+Fling, Pinch/Drehen, Arena) | **vorhanden im Kern, nur mit künstlichen Strömen/Emulator geprüft** — echte Geräte (X11-Touch, Win `WM_POINTER`, Handy) **offen** | `event.fi`, `kittouch.fi`, `pointers.fi`, r121/r122 | S (Test) | 4 |
| E7 | Stift (Druck, Neigung, Radierer) | **teilweise**: `pointerType pen` im Ereignis; Druck/Neigung: nicht gefunden | r95 | M | 2 |
| E8 | Tastaturlayouts/AltGr/Totasten im Fenster | **unbekannt** (X11-Keymap-Code vorhanden `keymap_*`; Totasten nicht geprüft) | `x11.fi` | — | 4 |

## 1.4 Barrierefreiheit

| # | Punkt | Status | Beleg | Aufwand | Nutzen |
|---|---|---|---|---|---|
| A1 | a11y-Baum (32 Rollen, Name, Zustand, Wert, Tab-Reihenfolge, Secret-Schutz) | **vorhanden** | `a11y.fi` (1357 Z.), `audit.fi`, `kita11y.fi`, r17/r98/r99 | — | 5 |
| A2 | **Brücke zu UIA (Windows) / AT-SPI (Linux) / ARIA-Spiegel (Web)** | **fehlt** — "a screenreader still cannot read the window". Der Baum hat **keine Geometrie** und nur Push-Plan (r103) | `docs/fui-kit.md` "Honest limits", `kita11y.fi:46`, r19, r103 | L | **5** (Pflicht für Windows-/Behörden-Apps) |
| A3 | heading-level, Zeiger-Pfad für Panel | **fehlt** | r20 | S | 3 |
| A4 | High-Contrast | **teilweise**: Theme "High Contrast" (7:1) eingebaut; **Erkennung des Windows-Kontrastmodus / Forced Colors: nicht gefunden** | `themelist.fi` T2 | S | 4 |
| A5 | Textskalierung nach Systemeinstellung, reduzierte Bewegung | **unbekannt** (nicht gefunden) | — | S | 3 |

## 1.5 Theming, Animation, Icons

| # | Punkt | Status | Beleg | Aufwand | Nutzen |
|---|---|---|---|---|---|
| T1 | Tokens, Hell/Dunkel, Akzent-Ableitung, Theme-Dateien mit Kontrast-/Füllregel-Prüfung | **vorhanden** (4 eingebaute: Nord, Solarized Light, High Contrast, Classic) | `theme.fi`, `themefile.fi`, `themelist.fi`, r62/r118 | — | 5 |
| T2 | **Fluent-/Win11-Schnitt als fertiges Theme** (Radien 4/8 px, Segoe-Schrift, Fluent-Farben, Fokusring, Karten) | **fehlt** als Preset. Bausteine da (Radien/Schatten/Glas/Stil-Tokens); OrientOS hat einen eigenen Win11-Look (`win11bar`) im eigenen Token-System | `themelist.fi` (4 Themes, keines Fluent) | S–M | **5** (für "sieht aus wie Win11") |
| T3 | System-Akzent und Dark-Mode **folgen** | **teilweise**: Windows `ImmersiveColorSet`/SystemUsesLightTheme + Dark-Titelleiste (DWM 20/19) + Caption-Farbe (Win11 35) im Code; X11 `Xft.dpi`/Dunkelmodus: teils; Windows-Akzentfarbe: nicht gefunden | `win32.fi:521–537, 884, 1308` | S | 4 |
| T4 | Mica/Acrylic-Fensterhintergrund (OS-Backdrop) | **fehlt** (`DWMWA_SYSTEMBACKDROP_TYPE` nicht gefunden). Eigener **In-Fenster-Glas**-Effekt vorhanden (`effect.fi`; Blur auf CPU ~100 ms je Dialog laut `kit.fi`) | `effect.fi`, `kit.fi` | M | 3 |
| T5 | Animation: Tween, Federn (9 Phasen), Easing, Hover-Übergänge, `fui.app` gleitet (r126) | **vorhanden** (`ANIM_MAX 32`) | `anim.fi`, `tools/fui/transgc_main.fi` | — | 4 |
| T6 | Keyframes/Timeline/Seitenübergänge/animierter Hintergrund | **fehlt** (grep `keyframe` → 0); Zeitgeber fehlt (r66) | r66 | M | 3 |
| T7 | Vektor-Icons | **teilweise**: Lucide 74 + 19 (`icons.fi`, `lucide.fi`), SVG-Bilder (`uisvg.fi`). **Segoe Fluent Icons / Fluent System Icons: fehlt** (Lizenz prüfen) | `docs/fui-kit.md` | S–M | 4 |

## 1.6 Schrift

| # | Punkt | Status | Beleg | Aufwand | Nutzen |
|---|---|---|---|---|---|
| S1 | TrueType-Rasterer mit Antialiasing, Kerning (`kern`-Tabelle), Fett/Kursiv/Tracking/Zeilenhöhe | **vorhanden** | `lib/font` (3249 Z.), `painter.fi` | — | 5 |
| S2 | **Hinting** | **fehlt** ("No hinting: the instructions … are skipped") | `lib/font/ttf.fi:29` | M | 2 |
| S3 | Subpixel/LCD-Glättung | **fehlt** (nur Graustufen; grep → nur Viertelpixel-Platzierung) | `painter.fi:528` | M | 2 |
| S4 | **GPOS-Kerning, GSUB-Kontext 5/6**, Texte > 4096 Zeichen im LTR-Pfad | **teilweise/fehlt** ("modern fonts put kerning in GPOS … this reader finds none") | `ttf.fi:21–27`, r13 | M | 4 |
| S5 | **Variable Fonts** | **fehlt**: nur die Default-Instanz wird gelesen (Doc-Kommentar "right by accident") | `lib/plat/sysfont.fi` | M | 3 |
| S6 | **Schrift-Fallback-Kette** (Latein → CJK → Symbole → Emoji) | **fehlt**: ein `FontSet`, ein Font je Painter; kein Fallback | `painter.fi:1813`, `font/metrics.fi` | M | **5** (jede mehrsprachige App) |
| S7 | Systemschriften wählen/auflisten | **teilweise**: feste Pfade je OS (Linux DejaVu; Windows Segoe UI→Arial→Tahoma; Android Roboto) — keine Aufzählung, keine Auswahl durch den Nutzer | `sysfont.fi` | S | 3 |
| S8 | Web: Schriften subsetten (1,8 MB TTF) | **fehlt** | r84 | S | 2 |

## 1.7 HiDPI, Fenster, GPU

| # | Punkt | Status | Beleg | Aufwand | Nutzen |
|---|---|---|---|---|---|
| H1 | DPI/Skalierung | **teilweise**: Windows `SetProcessDpiAwarenessContext`, `GetDpiForWindow`, `WM_DPICHANGED` im Code (nur Wine geprüft); X11 `Xft.dpi` / Millimeter-Maße (**ein** Wert, **kein RandR/Mehrmonitor**: grep `randr` → 0); Android Dichte | `win32.fi:37,111,449`, `x11.fi` (`resman_dpi`, `dpi_from_mm`) | M | 4 |
| H2 | Eigener Rahmen / Snap / Dark-Titel (Windows) | **vorhanden im Code** (`frame_own`, `WM_NCHITTEST`, "Aero Snap remains") — Wine-only | `win32.fi:421 ff.` | — | 3 |
| H3 | **GPU-Pfad auf dem Desktop** | **fehlt**: GPU nur Web (WebGL2) + Android (GLES3/EGL); "X11, Windows and Osum answer no" — kein Weg zur System-GL (statisch gelinkt) | `lib/fui/gpu.fi:40–50`, `window/*.fi r_gpu_open` | L | 3 (CPU reicht bei 1240×720; **4K/Effekte unbekannt, nicht gemessen**) |
| H4 | CPU-Tempo | **vorhanden**: 10,5/6,2 ms (Ziel ≤ 16), Schmelzgruppe 11,4 ms | Projektkennzahlen | — | 4 |
| H5 | Wayland nativ | **fehlt** (nur X11 eigenes Protokoll → unter Wayland via XWayland; **ob das sauber läuft: nicht geprüft = unbekannt**) | `grep -i wayland lib/` → nur Kommentare | L | 3 |
| H6 | macOS | **fehlt** (Stubs, "Firn has no macOS target") | `docs/APPKIT.md` | L | 1 (Ziel: Windows/OrientOS) |

## 1.8 Medien, Einbetten

| # | Punkt | Status | Beleg | Aufwand | Nutzen |
|---|---|---|---|---|---|
| M1 | Bilder PNG/JPEG/WebP/GIF/SVG, animierte GIF/WebP | **vorhanden** (APNG nur 1. Bild) | `uiimage*.fi`, `uianim.fi`, `docs/UI_EXTRAS.md` | — | 5 |
| M2 | Eigenes Zeichnen (Canvas/Painter/`node_set_draw`) | **vorhanden** | `painter.fi`, `scene.fi` | — | 5 |
| M3 | Audio-Ausgabe | **vorhanden** (Pulse, waveOut; MP3) — Windows nur Wine-Datei-Test | `docs/DESKTOP.md` | — | 3 |
| M4 | **Video-Widget** | **fehlt** (kein fUi-Widget; Decoder existieren anderswo: Certus 720p 32 Bilder/s, OrientOS H.264 640×480) | Projektkennzahlen, `docs/RUNDE-H264T.md` | L | 3 |
| M5 | **WebView / Glass einbetten** | **fehlt** (grep `webview|glass` in `fui/plat/window/fuishell` → nur Effekt-Glas). Certus' Glass-API (43/43 ohne Oberfläche) existiert getrennt | Certus-Kennzahlen | L | 4 (Tauri-Weg) |
| M6 | Drucken | **teilweise**: IPP/CUPS (Linux/macOS), PDF-Erzeugung `lib/pdf`; Windows-Spooler: **fehlt** | `lib/print/print.fi` | M | 3 |

## 1.9 Entwicklung: Zustand, Hot Reload, Designer, Tests, Doku

| # | Punkt | Status | Beleg | Aufwand | Nutzen |
|---|---|---|---|---|---|
| D1 | **Datenbindung / reaktiver Zustand** | **fehlt** (imperativ: `app.set_text/number`; Neuaufbau nach Schlüssel r92) | grep `bind|observable|signal` → nur Host-/Panel-Bindung | L | 4 |
| D2 | **Closures / Lambda-Handler** | **fehlt (Sprache)** | `docs/fui-quickstart.md` | L (Compiler) | 4 |
| D3 | Deklaratives Markup (XAML/QML/JSX) + **Hot Reload** | **fehlt** (Stilblatt nur in Code; "hot reload" nur als Wunsch in `DESIGN_GOALS.md`) | `docs/APP-TREE.md` §3, `docs/MODULE_REPORTS.md:804` | L | 4 |
| D4 | **Designer/GUI-Builder** | **fehlt** für fUi (OrientOS-"GUI-EDITOR" ist ein Texteditor) | `k/docs/RUNDE-GUI-EDITOR.md` | L | 3 |
| D5 | Inspektor (Stil mit Siegerregel, Live-Bearbeiten, a11y-Dump), Abfrage-API, Änderungssätze | **vorhanden** | `inspect.fi`, `query.fi`, r93/r96/r97 | — | 4 |
| D6 | UI-Tests | **teilweise**: Pixel-Tests ohne Fenster, Browser = nativ **0 px** (`tools/wasm/appcheck.py`), Xvfb+xdotool (`kitlive.py`), Audit im Build. **Fehlt:** Aufzeichnen/Abspielen, Screenshot-Diff-Werkzeug für fremde Apps, Test-Treiber à la Appium/UIA | `tools/fui/run.sh`, `audit.sh` | M | 4 |
| D7 | Lokalisierung | **vorhanden**: Plural-/Zahl-/Zeit-/Größenformate gegen ICU 72 (8016 Fälle, 8 Sprachen), RTL-Spiegelung, `appkit.texts` (.opmsg). Offen: Zeitzonennamen, Sprachen > 8, Sortierung/Collation (**unbekannt**) | `lib/i18n/human.fi`, `docs/UI_EXTRAS.md` §7 | S–M | 4 |
| D8 | Doku/Beispiele | **teilweise**: Quickstart, Kit-Doku, Listen-Doku, `APP-TREE.md` (1022 Z.), 7 Beispiele, Galerie, CodeHub-Demo. **Fehlt:** generierte API-Referenz, Rezeptbuch (Einstellungsseite, Dialog, Menü), Windows-Designrichtlinien | `docs/`, `examples/fui` | M | 4 |

## 1.10 Plattform-Integration (Desktop)

| # | Punkt | Status | Beleg | Aufwand | Nutzen |
|---|---|---|---|---|---|
| P1 | **Native Datei-/Ordner-Dialoge** (IFileDialog, XDG-Portal) | **fehlt** (nur fUi-eigener Dialog; `GetOpenFileName`/`IFileDialog` → 0; Doc: X11/Windows/Osum "honestly say no") | `lib/window/window.fi:909`, `docs/ANDROID.md:160` | M | 4 |
| P2 | Tray-Icon + Menü | **vorhanden** (Linux StatusNotifierItem + dbusmenu, Windows `Shell_NotifyIconW`); Wine/libdbus-Gegenstelle | `docs/DESKTOP.md` | — | 4 |
| P3 | Benachrichtigungen | **teilweise**: Linux Spec; Windows klassisches Balloon (**keine WinRT-Toasts, keine Aktionsknöpfe/Bilder**) | `docs/DESKTOP.md` | M | 4 |
| P4 | Autostart, einzelne Instanz, Dateiwächter, Konfig, Log, Crashberichte | **vorhanden** | `lib/desktop`, `lib/appkit` | — | 4 |
| P5 | **Installer + Pakete** (setup.exe + Deinstaller, deb, rpm, AppImage, tar, Portable-Zip, NSIS-Skript, dmg-Aufbau) | **vorhanden**; Windows nur Wine; macOS nur Struktur; **Signieren (Authenticode/SmartScreen) fehlt** | `docs/PACKAGING.md` | M (Signatur) | 5 |
| P6 | **Auto-Update** (Ed25519-signiert, Rückfall, Start-Bestätigung) | **vorhanden** (60 Checks Linux+Wine; Android 30) | `docs/APPKIT.md` | — | 5 |
| P7 | Protokoll-/Dateizuordnung, Jumplists, Taskleisten-Fortschritt, Teilen | **fehlt** (nicht gefunden) | grep | S–M | 3 |
| P8 | **Echter Windows-Test** | **fehlt**: alles bisher unter Wine/Emulator; "FLEI-ONE cannot reach this server"; mehrere Fragen an Justin stehen offen (Kits liegen im Chat) | `docs/APPKIT.md`, `docs/DESKTOP.md` "Honest limits", r121 | S (Justin muss testen) | **5** |

## 1.11 Top-10-Lücken fUi (nach Nutzen für typische Apps)

| Rang | Lücke | Warum | Aufwand |
|---|---|---|---|
| 1 | **Knotenlimit 128 + keine Virtualisierung** (L5/C6/C7) | Jede Datei-, Mail-, Chat-, Tabellen-App scheitert an Listen > 24 Zeilen | L |
| 2 | **fui.app kennt fast nichts** (C1/C16: Checkbox, Slider, Menü, Dialog, Tabelle, Baum, Mehrfenster nur Low-Level) | "App wie unter Windows" braucht die Standardsteuerungen in der 10-Zeilen-API | M |
| 3 | **Echttest Windows/Touch/Handy** (P8/E6) | Alles Gebaute ist unter Wine/Emulator bewiesen, **nichts** auf echtem Windows — ohne das ist "Windows-App" eine Behauptung | S (Justin) |
| 4 | **Schrift: Fallback-Kette, Emoji, GPOS** (S6/E2/S4) | Jede mehrsprachige oder Chat-App zeigt sonst Kästchen / falsches Kerning | M–L |
| 5 | **Windows-Look: Fluent-Preset + Segoe/Fluent-Icons + System-Akzent/Kontrast** (T2/T7/T3/A4) | Billigster Hebel für "sieht aus wie Win11" | S–M |
| 6 | **OS-IME** (E3: Windows TSF/IMM, Linux ibus/fcitx) | Ohne das nicht für CJK-Nutzer; Totasten/Komposition auch Europa unklar | M |
| 7 | **a11y-Brücke UIA/AT-SPI** (A2/A3) | Screenreader liest nichts; Voraussetzung für Behörden-/Firmen-Apps | L |
| 8 | **Popups als Toplevel + native Dialoge + Shortcuts/Mnemonics** (C10/C11/P1) | Menüs/Dropdowns dürfen den Fensterrand nicht abschneiden; Datei-Dialog soll der des Systems sein | M |
| 9 | **Zustandsbindung, Closures, Hot Reload** (D1–D3) | Entwicklerproduktivität; ohne sie bleibt jede App Handarbeit | L |
| 10 | **Desktop-GPU + Video + WebView/Glass** (H3/M4/M5) | Nötig für Spiele-Launcher, Hybrid-Apps, 4K | L |

**Vorgeschlagene Reihenfolge:** 3 (parallel, kostet nur Justins Zeit) → 5 (sichtbarer Erfolg, S) → 2 (API-Lücke schließen) →
1 (Arena + Virtualisierung r102/r134) → 4 → 8 → 6 → 7 → 9 → 10.

---

# TEIL 2 — OrientOS als Alltags-Betriebssystem

## 2.0 Ausgangslage (belegt)

* `README.md` (Stand 16.09.): 344 119 Zeilen Firn (ohne `vendor/`), 1 445 Commits seit 19.08.; ~342 000 Zeilen in `kernel/*/*.fi`
  (gemessen mit `wc` am 07.10.). Bei den Zahlen in README/KOMPATIBILITAET ist das Datum der Messung zu beachten.
* Kern x86-64 (Multiboot, BIOS+UEFI), SMP, ein Adressraum je Prozess, Linux-x86-64-Syscall-Nummern (**193** `SYS_*` in `k/sys/sys.fi`),
  SMEP/SMAP (`guard.fi`), NX für Ring 3, OOM-Behandlung (K-002).
* **Der Fensterserver läuft im Kern (Ring 0)**: `kernel/ui/wm.fi` 14 250 Z., `kgui.fi` 9 255 Z. (`docs/FUI-WLIB-PLAN.md`). Sicherheits- und
  Stabilitätsrisiko (ein Fehler dort = Kernel-Panik).
* **Es gibt schon eine Alltags-Lückenliste:** `docs/DAILY-DRIVER.md` (02./03.10.). Diese Analyse prüft sie gegen den Code und ergänzt.
  Die Stufen dort (5 = ohne das kein Alltag) übernehme ich als Gegenprobe.
* **Echte Hardware ist erst einmal gemessen:** Justins PC (AMD, NVIDIA GA106, Ultrawide) UEFI-Stick startet (`docs/BLECH-BEREIT.md`);
  der Dell 9020 zeigt Panik, kein DHCP, keine Maus (`DAILY-DRIVER` A4, r153/r182/r189/r206/r210). **Alles andere in diesem Teil ist QEMU/KVM.**

## 2.1 Hardware

| # | Punkt | Status | Beleg | Aufwand | Alltag |
|---|---|---|---|---|---|
| O1 | Boot UEFI/BIOS, Installer auf SATA/NVMe, Dual-Boot neben Windows | **vorhanden** (QEMU: 70 + 49 Checks; Dual-Boot ohne Änderung der Fremd-Partitionen); echter Rechner: Stick-Boot ja, **Installation am Gerät offen**. Installer nur **UEFI+GPT** (r203) | `docs/ROUNDINSTALL.md`, `RUNDE-DUALBOOT.md`, r203 | S–M | Blocker (am Gerät prüfen) |
| O2 | Grafik: UEFI-Framebuffer (GOP), virtio-gpu (VM), Software-3D | **teilweise** — **kein** NVIDIA/AMD/Intel-Treiber, kein OpenGL/Vulkan, kein echtes Vsync | `docs/RUNDE-GPU3D.md`, `DAILY-DRIVER` B5, r33/r36 | L–XL | Schön zu haben (Framebuffer genügt für Büro); Blocker für Spiele/flüssiges Video |
| O3 | Mehrere Monitore, Skalierung | **teilweise**: `vmode.fi` mit EDID, Modusliste, Drehen, Skalieren, Gamma (`docs/DISPLAY.md`, 1 Bildschirm); **mehrere Ausgänge: fehlt** (DISPLAY §8.4) | r32 | L | Schön zu haben (Blocker bei Dock/2. Monitor) |
| O4 | WLAN | **fehlt (Treiber)** — Protokollstapel (WPA, CCMP, Beacon) gegen simuliertes Gerät gemessen; kein Funkchip-Treiber; hängt an der Frage nach dem Chip | `lib/wlan/*`, `docs/WLAN-TREIBER.md`, r21/r118 | L je Chip | **Blocker für Laptop** |
| O5 | Bluetooth | **fehlt** (grep `bluetooth|btusb` → nur Erwähnungen in `netdev.fi`/`s3dev.fi`) | r6 | L | Schön zu haben |
| O6 | USB-Klassen | **teilweise**: xHCI + EHCI, Hub, Tastatur/Maus/HID, Massenspeicher; **fehlt:** UVC-Kamera (r7), USB-Audio, CDC/ECM/RNDIS (USB-Ethernet/Tethering), Drucker-Klasse (nur IPP über Netz). EHCI-Maus ohne xHCI: nein (r168) | `usb/usb.fi` (`DRV_KBD/MOUSE/MSC/HIDGEN/HUB`), `ehci.fi` | M je Klasse | Blocker nur Massenspeicher (da); Rest schön |
| O7 | Audio | **teilweise**: Intel HDA (2602 Z.) + AC97, Mischer, MP3/WAV; **HDMI/DP-Ton: fehlt** ("digitale Knoten übersprungen"); USB-Audio fehlt | `drv/snd/hda.fi`, `docs/AUDIO.md:115,309` | M | Blocker für Ton über HDMI-Monitor (Justins PC hat NVIDIA-HDMI-Ton) |
| O8 | Drucker | **teilweise**: IPP-Druck (K-008) ohne Treiberwelt; keine Oberfläche in Einstellungen | `drucke.fi`, `DAILY-DRIVER` C3/B9 | M | Schön |
| O9 | Kamera | **fehlt** (UVC) | r7 | M–L | Schön (Blocker für Videocall) |
| O10 | Tastatur-Layouts | **teilweise**: **nur `us` und `de`** (`LAYOUTS = 2`); H-001 schnelles Tippen vertauscht Tasten (r157) | `drv/hid/kbd.fi:43,87` | S je Layout | Blocker für Nicht-DE/US-Nutzer |
| O11 | Touchpad | **teilweise**: I²C-HID (Designware/LPSS) nur **aus der Spezifikation** gebaut, QEMU hat keinen solchen Regler → **auf echter Laptop-Hardware ungemessen** | `drv/hid/i2chid.fi`, `docs/BLECH-BEREIT.md` | S (Test) | Blocker für viele Laptops |
| O12 | Akku, Deckel, Energie | **teilweise**: ACPI/AML (4 024 Z.), Akku (`batt.fi`), CPU-Takt/C-States; **Hibernate (S4): fehlt** ("kein S4") | `pwr/*`, `acpi/*`, `docs/ROUNDAML.md:58` | M–L | Blocker für Laptop |
| O13 | Standby S3 | **teilweise**: in QEMU mit 1–4 Kernen und Geräte-Aufwachen (xHCI, NVMe, AHCI, e1000, HDA) belegt (`s3` 55/0, `s3smp`, `s3dev` 24/0); **Deckel→Standby, UEFI/OVMF, echtes Gerät, GPU nach S3: offen** (r144/r145) | `DAILY-DRIVER` B4 | M | Blocker für Laptop (am Gerät) |
| O14 | PCI/NVMe/AHCI/virtio, Netzkarten e1000 (8254x/I217/I219), RTL8168 | **vorhanden**; Dell-I219: Link/DHCP offen (r155/r182/r189) | `drv/blk`, `drv/net` | M (Fehlersuche) | **Blocker** (Dell kein Netz) |

## 2.2 Speicher

| # | Punkt | Status | Beleg | Aufwand | Alltag |
|---|---|---|---|---|---|
| S1 | Eigenes OFS mit Journal, fsck, Stromausfall-Test | **vorhanden** (QEMU; 4 Schreiber auf 4 Kernen byteweise geprüft 11/0); SMP-Zustände ohne Sperre noch offen (r211–r213) | `docs/OFS-JOURNAL.md`, `tools/fsrobust` | M | Blocker (Datenverlust) |
| S2 | FAT lesen+schreiben | **vorhanden**; Kopieren innerhalb einer Partition ~3,5 KB/s (r208) | `fs/fat.fi` | S–M | Blocker für USB-Sticks (zu langsam) |
| S3 | ext4/NTFS | **teilweise: nur lesen** (131/0 SHA-Prüfung gegen echte Werkzeug-Abbilder); **schreiben fehlt**; exFAT fehlt (nicht gefunden) | `docs/RUNDE-FREMDFS.md`, `fs/ext4.fi`, `fs/ntfs.fi` | L (schreiben) / M (exFAT) | exFAT = Blocker für USB-Platten/Kameras; Schreiben schön |
| S4 | Papierkorb, Backup, Schnappschüsse | **vorhanden** (Papierkorb je Datenträger, `/bin/backup`, 61+ Zusagen); **Zeitplan fehlt, Wiederherstellung auf leeres Blech fehlt** (r60) | `tools/vault`, `DAILY-DRIVER` A8 | M | Blocker (Restore) |
| S5 | **Plattenverschlüsselung** | **teilweise**: Prototyp (XTS, Argon2, Schlüsselplatz) 61/0 — **nicht im Installer/Start verdrahtet** | `docs/RUNDE-KRYPTO.md`, `DAILY-DRIVER` B2 | M | Blocker für Laptop |
| S6 | Datenträgerverwaltung (GUI) | **fehlt** (r59) | r59 | M | Schön |
| S7 | Netzwerkfreigaben (SMB/NFS) | **fehlt** (grep → 0) | r24 | L | Schön |
| S8 | Swap/Auslagerung | **fehlt** (kein Swap-Code; Speicherdruck/OOM-Killer vorhanden) | `docs/ROUNDMEM.md`, `docs/SPEICHERDRUCK.md` | L | Blocker bei < 4 GB RAM; sonst schön |

## 2.3 Netzwerk

| # | Punkt | Status | Beleg | Aufwand | Alltag |
|---|---|---|---|---|---|
| N1 | TCP/IP, DHCP, ARP, ICMP, DNS-Resolver | **vorhanden** (gegen Linux gemessen: 75 Checks) | `net/inet.fi`, `lib/libc/dns.fi`, README | — | Blocker (da) |
| N2 | **IPv6** | **fehlt**: Treffer nur in `host.fi`, WireGuard-/Socks-/WLAN-Erwähnungen, kein Stack | `grep -ril ipv6 kernel lib` | L | Schön (heute meist Dual-Stack) |
| N3 | **TLS** | **teilweise**: OrientOS' `lib/tls/tls.fi` ist ein **TLS-1.3-Client**; **Firn hat inzwischen TLS 1.2 + Server** (`lib/tls/prf12.fi`, `tls_server.fi`, `docs/TLS12.md`) → Vendor-Sprung r61 holt es **vermutlich** (nicht geprüft). Wurzelzertifikate altern (r25); Schlüsselerneuerung ungemessen (r23) | `lib/tls/*` beide Repos | S (nach Vendor-Sprung) | **Blocker** (Seiten nur mit 1.2) |
| N4 | VPN | **teilweise**: WireGuard (`net/wg.fi`, `user/vpn.fi`, `docs/TUNNEL.md`); kein OpenVPN, keine Oberfläche | `net/wg.fi` | M (GUI) | Schön |
| N5 | **Firewall/Paketfilter** | **teilweise**: `netview` = Sicht/Regel **je Prozess** ("not a packet filter"), eBPF-Haken (`net/ebpf*.fi`); **kein klassischer Paketfilter** | `docs/NETVIEW.md:41`, `net/ebpfhook.fi` | M | Schön (Heimnetz) / Blocker für Server |
| N6 | Proxy | **vorhanden** (`user/proxy.fi`, SOCKS5 `lib/socks`) | `kernel/user/proxy.fi` | — | Schön |
| N7 | SSH-Server, sntp, Netzwerkanzeige | **vorhanden** (OpenSSH-Client loggt ein; SNTP 39/0) | README, `docs/TIME.md` | — | — |
| N8 | Zeitzonen | **teilweise**: EU-Sommerzeitregel + feste Versätze; **keine Zeitzonen-Datenbank** (r20); RTC lokal/UTC Annahme (r79) | `docs/TIME.md` | M | Blocker außerhalb der EU |
| N9 | USB-Ethernet/Tethering, Handy-Hotspot | **fehlt** (keine CDC-Klasse, siehe O6) | `usb/usb.fi` | M | Ersatz für WLAN |

## 2.4 Sicherheit

| # | Punkt | Status | Beleg | Aufwand | Alltag |
|---|---|---|---|---|---|
| X1 | Benutzer/Rechte (uid/gid, rwx, setuid, `perm.fi` als einzige Prüfstelle), Mehrbenutzer, PBKDF2-Anmeldung, Sperrbildschirm | **vorhanden** (k13 99/0, Login 31/0, Sperre 43/0) | `ipc/perm.fi`, `STATUS-MULTIUSER.md` | — | Blocker (da) |
| X2 | Anmeldung wie Windows Hello (PIN, Passkey, Fingerabdruck) | **fehlt** | r253 | M–L | Schön |
| X3 | **Programm-Sandbox** (Prozess-Isolation) | **fehlt**; Rechte laufen über Aktions-Bus/Handles, Container (`ctr.fi`) vorhanden aber kein Sandbox-Modell je App | r47, `docs/RUNDE-CONTAINER.md` | L | Schön (Blocker bei Fremd-Apps) |
| X4 | **Fensterserver im Kern** | **Risiko**: Kernel-Absturz/Rechteausweitung bei jedem wm-Fehler | `docs/FUI-WLIB-PLAN.md` (14 250 Z. `wm.fi`) | L | Schön (Architektur) |
| X5 | Härtung | **teilweise**: SMEP/SMAP, NX Ring 3; **W^X im Kern, Wachseiten, Stapel-ASLR: nicht in main** (Zweig `haertung`, "Portierung nötig"); "kein NX" im Kernbereich | `docs/ALTZWEIGE-AUGUST.md:27,390` | M | Schön |
| X6 | **Secure Boot** | **fehlt** (Limine unsigniert; Secure Boot muss aus sein) | `docs/USBSTICK.md` | L | Blocker für Firmen-Laptop; sonst schön |
| X7 | TPM | **fehlt** (nur Kommentare) | `docs/BLECH-BEREIT.md:65` | L | Schön |
| X8 | **Signierte Updates** (Ed25519, Rückschrittsschutz, Rückfall, Wachhund) | **vorhanden** (OTA 150/0); **Kern nicht im A/B** (r104); Store-Auslieferung wird nicht automatisch nachgezogen (r178) | `docs/OTA.md`, `DAILY-DRIVER` Teil C | M | Blocker (Kern-Update) |
| X9 | Verschlüsselte Kommunikation / E2E (FirnChat-Relay) | **vorhanden** im Social-Layer | `docs/SOCIAL.md` | — | — |

## 2.5 Pakete, Prozesse, Mehrbenutzer

| # | Punkt | Status | Beleg | Aufwand | Alltag |
|---|---|---|---|---|---|
| K1 | Paketverwaltung/Store | **teilweise**: `.opk`, signierte Kataloge, Store-Fenster, 14 Programme; **keine Abhängigkeiten** (r105), Installation nur Administrator (Entscheidung offen), `opk` nur auf dem Wirt (r106) | `docs/STORE-OBERFLAECHE.md`, r105/r106 | M | Blocker (Programme installieren) |
| K2 | Prozess-/Speicherverwaltung: SMP, Faden/Futex, mmap/brk, 64 GiB gebootet, OOM | **vorhanden** (QEMU); SMP-Rennen offen (r211–r214) | `docs/ROUNDMEM.md`, `ROUNDK*` | M | Blocker (Stabilität) |
| K3 | Ressourcenlimits (rlimit), cgroups | **teilweise**: nur NOFILE-Grenze sichtbar (`rlimit` nur in `sys.fi:307` als Kommentar) | `k/sys/sys.fi` | M | Schön |
| K4 | Container im LXC-Stil | **vorhanden** (`ctr.fi`, Zweig-Doku) | `docs/RUNDE-CONTAINER.md` | — | Schön |
| K5 | Mehrere Benutzer gleichzeitig / schneller Benutzerwechsel | **teilweise** (Konten, Sperre; Wechsel ohne Abmelden: **unbekannt**) | `docs/ROUNDMULTIUSER.md` | M | Schön |

## 2.6 Shell, Entwicklung, Kompatibilität

| # | Punkt | Status | Beleg | Aufwand | Alltag |
|---|---|---|---|---|---|
| D1 | Shell (`sh` mit if/for/while/case/Funktionen), ~100 Programme, Terminal, Pipes | **vorhanden** (k16, Terminal-Abnahme) | `kernel/user/sh.fi`, README | — | Blocker (da) |
| D2 | Skripting/Sprachen | **teilweise**: `sh`, Lua/QuickJS/SQLite als fremde Binaries; **Python: fehlt** (nicht gefunden) | `tools/foreign/README.md` | L | Schön |
| D3 | Entwicklungsumgebung auf OrientOS | **teilweise**: `.fi` per Doppelklick übersetzen (firnc → `fas` → starten, `firun.fi`), Editor `nedit`; **Debugger fehlt** (kein ptrace/gdb-Stub gefunden), kein Paket-Manager für Entwicklerbibliotheken | `kernel/user/firun.fi`, `nedit.fi` | L (Debugger) | Schön |
| D4 | **Linux-Kompatibilität (statisch)** | **vorhanden**: Lua, SQLite, QuickJS, busybox 1.36.1 (Pipes, 4 musl-Fäden, echte Dateisperren) laufen unverändert | `STATUS-FREMDLAND.md` | — | Schön |
| D5 | **Linux dynamisch (`ld.so`)** | **teilweise**: Weg B gewählt = echter `ld-musl` als Interpreter; `PT_INTERP` in `ldr/elf.fi`; Abdeckung echter Programme **unbekannt** (F-007/F-009 "Syscall-Lücken nicht systematisch vermessen") | `docs/RUNDE-DYNLADER.md`, `KOMPATIBILITAET.md` | L | Schön, aber der Hebel für Fremd-Apps |
| D6 | Wayland-Server (Linux-GUI-Programme) | **teilweise**: `wayd.fi`, `weston-simple-shm` zeigt Fenster (45/0) — **Firefox/LibreOffice: nicht gezeigt** | `docs/RUNDE-WAYLAND.md` | L | Schön |
| D7 | Windows-Programme (Wine) | **fehlt, bewusst** (Entscheidung FREMDSOFTWARE 08.09.: kein Win32/X11) | r243-Text | — | — |
| D8 | **Browser (Certus) auf OrientOS** | **teilweise**: läuft als Ring-3-Programm mit wlib-Leinwand, aus dem Laden installierbar; Glass-Konformität lückenhaft (r120–r124), **Video nicht angebunden**; Hauptmetriken: Seiten vs Chromium 90,5 %, test262 72,9 % | `STATUS-CERTUS.md`, Certus-Kennzahlen | L (laufend) | **Blocker** (Web) |

## 2.7 Programme im Alltag

| # | Punkt | Status | Beleg | Alltag |
|---|---|---|---|---|
| G1 | Explorer (Kopieren, Papierkorb, Suche, ZIP, USB, Drag&Drop) | **vorhanden** | `explorer.fi`, `DAILY-DRIVER` B12 | Blocker (da) |
| G2 | Texteditor, Rechner, Notizen, Terminal, Einstellungen (15 Reiter), Task-Manager, Store | **vorhanden** | `kernel/user`, `assets/apps` | — |
| G3 | PDF-Betrachter (Suche, Serifen; keine Auswahl/Kopieren, keine Schattierungen) | **vorhanden/teilweise** (111/0 gegen poppler) | `docs/PDFVIEW.md` | Schön |
| G4 | Bild-, Medien-Betrachter, MP3/WAV, H.264 **640×480 mit 25 Bildern/s** | **teilweise**; kein Videoplayer-Fenster (r56), kein AAC/Container | `docs/RUNDE-H264T.md` | Blocker für Video |
| G5 | **Office** (Text, Tabelle, Präsentation) | **fehlt** (r64) | r64 | Blocker im Büro (Ersatz: Browser-Office) |
| G6 | **Mail** (IMAP/SMTP) | **fehlt** (grep `imap|smtp|pop3` → 0) | grep | Blocker im Alltag (Ersatz: Webmail im Browser) |
| G7 | Chat/Freunde (Fleitec-Konto, E2E) | **vorhanden** (`social`, `freunde`, 98/0) | `docs/SOCIAL.md` | Schön |
| G8 | Kalender/Kontakte | **fehlt** (r58) | r58 | Schön |
| G9 | Bildschirmtastatur, Sprachausgabe/Screenreader, Emoji-Werkzeug | **fehlt** (r57/r63/r62) | Roadmap | Schön (Touch: Blocker) |

## 2.8 Logs, Diagnose, Barrierefreiheit

| # | Punkt | Status | Beleg | Alltag |
|---|---|---|---|---|
| Z1 | Kernel-Log, Panik-Bericht mit Symbolen (`/var/crash`), Absturzbericht | **teilweise**: vorhanden; `/bin/log` fehlt auf dem Stick, `/proc` dort nicht eingehängt (r187), Panik über Reset retten (r210) | `diag/crash.fi`, `klog.fi` | Blocker für Fehlersuche am Gerät |
| Z2 | Diagnose-Programme (powermon, netmon, hwid, taskmgr) | **vorhanden** | `tools/powermon` u. a. | — |
| Z3 | A11y-Baum, Lupe/Kontrast (Konzept), Einrastfunktion | **teilweise**: A11y-Abnahme 58/0, Aktions-Bus; **Bildschirmleser fehlt** (r63) | `docs/A11Y.md`, r44/r45 | Schön |
| Z4 | Sprache: Oberfläche de/en (`locale`), Umlaute | **vorhanden**; weitere Sprachen: **fehlt** | `locale/de, en` | Blocker nur außerhalb DE/EN |

## 2.9 Top-10 OrientOS

**Blocker für den Alltag (in dieser Reihenfolge):**

| Rang | Punkt | Warum | Aufwand |
|---|---|---|---|
| B1 | **Echte Geräte stabil**: Dell-Panik (r206/r210), Netz/DHCP (r182/r189/r155), Maus (r153), Tastaturreihenfolge (r157) | Ein System, das auf dem einzigen echten Testgerät nicht netzt, ist kein Alltag | M (Fehlersuche, braucht Justin am Gerät) |
| B2 | **WLAN-Treiber für den Laptop** (Chip erfragen, r21) | Ohne Funk kein Laptop | L |
| B3 | **Browser** vollständiger (Certus: Video, Konformität) + **TLS 1.2** per Vendor-Sprung | Web ist 80 % des Alltags | L |
| B4 | **Dateisysteme am Gerät**: exFAT, FAT-Kopiertempo (r208), Restore aus Backup (r60) | USB-Platten/Kamera/Rettung | M |
| B5 | **Laptop-Energie**: S3 am Gerät (r144/r145), Deckel, Hibernate S4, I²C-Touchpad am Gerät | Ohne das ist es nur ein Desktop-System | M–L |
| B6 | **Kern im A/B-Update** (r104) + Store-Nachzug (r178) | Ein Update, das den Kern kaputtmacht, braucht Rückfall | M |
| B7 | **Plattenverschlüsselung im Installer** (K-019 verdrahten) | Laptop-Diebstahl | M |
| B8 | **Mail-Programm + Office-Ersatz** (oder klare Browser-Strategie) | Ohne die zwei gibt es keinen Büroalltag | L |
| B9 | **Zeitzonen-Datenbank, Tastaturlayouts, Sprachen** | Alltag außerhalb von DE/EU | M |
| B10 | **Mehrmonitor + Skalierung** (r32) und **HDMI-Ton** | Standard-Arbeitsplatz | M–L |

**Schön zu haben:** IPv6, Bluetooth, Kamera/UVC, Firewall (Paketfilter), SMB, Sandbox je Programm, Secure Boot/TPM, Windows-Hello-PIN,
eigener GPU-Treiber/Vsync, Datenträger-GUI, Kalender/Kontakte, Bildschirmtastatur/Screenreader, Swap (bei viel RAM), Debugger,
ld.so-Abdeckung für fremde Linux-Programme, Wayland für Firefox, `ntfs/ext4` schreiben.

---

# TEIL 3 — Wie sehen und fühlen sich Apps auf OrientOS wie Win11-Apps an? (5 Sätze)

1. **Ein Fluent-Preset in fUi bauen** (`themefile` mit Fluent-Farben, Radien 4/8 px, 1-px-Linien, Akzent aus dem System, Segoe-artige Schrift — OrientOS hat schon Inter und `win11bar`) und **alle** OrientOS-Programme darauf stellen, statt dass OrientOS ein eigenes Token-System neben fUi pflegt (r64/r120).
2. **Die Standardsteuerungen in `fui.app` freischalten und aufs Fluent-Verhalten bringen** (Checkbox/Toggle/Slider/Combobox/Menü mit Beschleunigern/Kontextmenü/Dialog/Tabelle/Baum) — erst wenn sie dieselben Hover-/Druck-/Fokus-Übergänge wie WinUI haben (120 ms ease-out, Fokusring, Kartenschatten), fühlt es sich richtig an.
3. **Das Listen-/Knoten-Limit aufheben** (Arena + Virtualisierung r102/r134) und Tastatur/Mehrfachauswahl in Listen und Tabellen bauen, denn Explorer, Einstellungen, Store und Mail sind alle Listen — ohne sie bleibt die "App wie Windows" bei 24 Zeilen stehen.
4. **Popups, Menüs, Tooltips und Toasts als eigene Fenster-Schichten** (nicht im Fenster abgeschnitten) plus Mica-/Acrylic-ähnlicher Hintergrund über den vorhandenen Glas-Effekt, Schatten für Fenster (offener Dell-Test) und die neue Leiste im Win11-Schnitt als Gegenstück.
5. **Erst auf echten Geräten prüfen** (Dell/Justins PC für OrientOS, FLEI-ONE für Windows-Apps, Handy für Touch) und die Screenshots gegen echte Win11-Apps legen (Kontrast, Abstände, Schrift), bevor weiter polieren — heute ist fast alles nur in QEMU/Wine/Emulator belegt.

---

# Anhang: Neue Roadmap-Punkte (aus dieser Analyse)

Wurden in die Projekt-Roadmaps fUi bzw. OrientOS eingetragen (Gruppe "Lücken 07.10."), soweit nicht schon vorhanden.
Bestehende Punkte, die zu Lücken passen: fUi r13, r16, r19, r20, r24–r26, r45, r66, r83, r86, r89, r102, r103, r121, r122, r127, r134;
OrientOS r6, r7, r20, r21, r24, r32, r33, r34, r36, r47, r56–r64, r104, r144, r145, r153, r155, r157, r182, r187, r189, r203, r206, r208, r210, r253.

**Korrekturhinweis:** fUi r24/r25/r86 (Win32-/Android-Schicht, Windows-Host) sind im Code **schon gebaut** — nur der echte Test fehlt (r121).

# Was ich NICHT belegen konnte (bewusst "unbekannt")

* Verhalten unter Wayland/XWayland, Totasten/AltGr in fUi-Fenstern, Textskalierung/"reduzierte Bewegung" (fUi).
* Fensterleistung bei 4K / mit Blur-Effekten auf dem Desktop (nur 1240×720 und 1996×1211 gemessen).
* Ob ein Vendor-Sprung (r61) OrientOS' TLS automatisch auf 1.2 bringt (nicht ausprobiert).
* Wie viele echte Linux-Programme der dynamische Lader trägt (F-009 nicht vermessen).
* Schneller Benutzerwechsel ohne Abmelden; Zahlen der Hardware-Abdeckung außerhalb der zwei gemessenen Rechner.
* Die Einzelliste der 15 "fehlenden Elemente" von fUi r45 (steht nur im Chat "fUi WELLE 2-4").
