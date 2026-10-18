# fUi Lückenliste vs Qt 6 / Flutter / WinUI 3 / GTK 4 / Jetpack Compose / SwiftUI / Slint / egui

Stand 17.10.2026 (Worker W12, Welle 2b). Reine Analyse: **kein Produktcode geändert, nichts gebaut, kein Testlauf wiederholt.**
Der fUi-Status stammt aus dem Code von Firn `main` (153bd3e23) und der Roadmap (project fUi, r1–r231).
Vorgänger: `docs/GAP-ANALYSE-0710.md` (07.10.). Diese Datei ersetzt sie nicht, sie zieht den Stand nach (siehe Abschnitt „Was sich seit 07.10. geändert hat“) und vergleicht erstmals Feature-für-Feature mit acht Toolkits.

## Legende

* **fUi-Status:** *vorhanden* = Code da und Prüfprogramm/Beleg genannt · *teilweise* = Kern da, Rest benannt · *fehlt* = im Code gesucht (grep, Dateinamen), nichts gefunden.
* **Vergleichsspalten** (Qt · Flutter · WinUI3 · GTK · Compose · SwiftUI · Slint · egui): `✓` eingebaut/offiziell · `~` teilweise, nur Zusatzpaket oder mit Handarbeit · `✗` nicht vorhanden · `?` unsicher. **Die Spalten stammen aus Trainingswissen (Stand ca. Mitte 2026), nicht aus frischer Recherche — sie sind Orientierung, kein Beleg.** Compose = Compose Multiplatform, WinUI3 = Windows App SDK.
* **Priorität:** **P0** = ohne das scheitern echte Apps (FleiLauncher, JARVIS-Client, OrientOS, CodeHub) · **P1** = klar spürbar, nächste Welle · **P2** = nützlich, später · **P3** = Komfort/Zukunft · `-` = vorhanden, nichts zu tun.
* **Ref `rNN`** = bestehender offener Roadmap-Punkt (dort gepflegt). **`neu`** = in dieser Runde als Roadmap-Punkt angelegt.
* **Messwert/Test-Gate:** woran man „fertig“ misst. Bei `-` gilt das vorhandene Prüfprogramm.

## Kurzbefund

* fUi ist bei **Rendering, Text-Grundlagen, Animation, Theming, Kontextmenüs, Inspector, Tests/Goldens** auf oder über Niveau der Vergleichs-Toolkits (einzigartig: Schmelzgruppe, Theme-Kontrastprüfung, Füllregel, a11y-Audit im Build).
* Die größten Lücken liegen bei **Anwendungs-Gerüst**, nicht bei Pixeln: Baumkapazität 128 Knoten (r102), `fui.app` ohne Standard-Controls (r145), Listen/Tabellen/Bäume nicht im Szenenbaum (r134), Popups nur unter X11 als Fenster (r172/r175), Schrift-Fallback nicht im Standard-Setzer (r139).
* **Plattform-Wahrheit:** Windows nur unter Wine, Android nur Emulator, Touch nur Protokolltests (r121/r122) — und OrientOS-Host noch nicht auf fUi migriert (r41).
* Wo andere Toolkits weit vorn liegen: Hot-Reload/Designer (r158), Wayland/macOS/iOS, Komplexschrift außer Arabisch, Charts, WebView/Video, OS-IME.

## Tabellen


### Layout

| Feature | fUi-Status | Beleg | Qt | Flutter | WinUI3 | GTK | Compose | SwiftUI | Slint | egui | Prio | Messwert/Test-Gate |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Flex (Richtung, wrap, grow/shrink, justify, align, gap, min/max) | **vorhanden** | lib/fui/flex.fi; tools/fui/flex_main.fi | ✓ | ✓ | ~ | ✓ | ✓ | ✓ | ✓ | ~ | - | flex_main grün |
| Grid mit Spannen, auto/fr-Spuren im Szenenbaum | **teilweise** | lib/fui/layout.fi (festes cols×rows); r148 | ✓ | ✓ | ✓ | ✓ | ~ | ✓ | ✓ | ✓ | P1 (r148) | 40 Grid-Fälle = Chromium-Referenz, 0 px Abweichung |
| Stack/Overlay/Z-Ebenen | **vorhanden** | lib/fui/layer.fi; scene.node_set_offset | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | - | layer-Prüfung in run.sh |
| Constraints: min/max (da); Seitenverhältnis, min-/max-content (fehlt) | **teilweise** | scene.node_set_bounds; kein aspect (r148) | ✓ | ✓ | ~ | ✓ | ✓ | ✓ | ✓ | ~ | P2 (r148) | aspect/fit-content-Fälle in layout_main |
| Responsive: Media-/Container-Queries, Breakpoints, Size-Classes | **teilweise** | lib/fui/sizeclass.fi (Größenklassen 600/900 dp, Hysterese, Spalten, Container-Abfrage); sizeclass_main (36 Checks); noch nicht im Szenenbaum verdrahtet (r148) | ~ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | ✗ | P1 (r148) | App schaltet bei 600/900 px Layout um; Test an 3 Breiten |
| Scrollen (Rad, Touch-Fling, Overscroll, dünne Leiste) | **vorhanden** | lib/fui/viewport.fi; glide_main (30 Checks) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | - | glide_main |
| Scroll-Snap, Pull-to-Refresh, verschachteltes Scrollen | **fehlt** | nicht gefunden | ~ | ✓ | ~ | ✗ | ✓ | ✓ | ✗ | ✗ | P3 | - |
| Virtuelle Liste/Tabelle/Baum (100k Zeilen) | **teilweise** | lib/fui/vlist.fi + vlistview.fi (Sortierpfeil, Spaltenziehen, Mehrfachauswahl, Typeahead); nicht im Szenenbaum/fui.app (r134, r145) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | P0 (r134, r145) | 100k Zeilen in fui.app: Frame ≤ 16 ms |
| Knotenkapazität des Baums (SCENE_MAX 128, KIDS_MAX 64) | **teilweise** | lib/fui/scene.fi:166/171; r102 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | P0 (r102) | App mit 5000 Knoten: Bau+Malen ≤ 16 ms |
| RTL-Spiegelung | **vorhanden** | kit.kit_set_rtl; tools/fui/kitrtl_main.fi | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | ✗ | - | kitrtl_main |
| Layout-Animation (Umordnen gleitet) | **fehlt** | r149 | ~ | ✓ | ✓ | ✗ | ✓ | ✓ | ~ | ✗ | P2 (r149) | - |
| Safe-Area/Insets (Notch, Tastatur, Systemleisten) | **teilweise** | lib/fui/wave8m.fi: Insets (max/add/start/end, RTL), insets_lift (Feld ueber Tastatur); Host-Leser fehlen | ~ | ✓ | ~ | ✗ | ✓ | ✓ | ~ | ✗ | P2 | Android-Emulator: Feld bleibt über Tastatur sichtbar |
| Inkrementelles Layout (Schmutzbits, Memo) | **vorhanden** | scene.fi memo; r100/r109 (1009 Knoten 0.9 ms) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | - | Baumarbeit ≤ 2 ms |

### Widgets

| Feature | fUi-Status | Beleg | Qt | Flutter | WinUI3 | GTK | Compose | SwiftUI | Slint | egui | Prio | Messwert/Test-Gate |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Button (primary/secondary/subtle), Label, Link | **vorhanden** | wave2.button_variant; kit.button_draw; richtext (Link-IDs) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | - | fluent_main (58 Maße) |
| Checkbox, Radio, Toggle (animiert) | **vorhanden** | wave2.draw_checkbox/radio/toggle; appctl.checkbox/toggle/radio | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | - | glide_main |
| Slider, Progress (bestimmt) | **vorhanden** | wave2.draw_slider/progress; kit.progress_draw | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | - | fluent_main |
| Busy-Ring/Spinner, unbestimmter Fortschritt als Widget | **teilweise** | **vorhanden-Modell** wave8.spinner_* (12 Punkte, Reduced-Motion = ruhiger Ring), wave8_main | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | P2 | animbench: Idle-Ring 60 Hz ohne Voll-Dirty |
| Range-Slider (2 Griffe), NumberBox/Spinbox | **teilweise** | appctl.spinbox da; Range-Slider fehlt (r146) | ~ | ✓ | ✓ | ✗ | ✓ | ✓ | ✓ | ~ | P2 (r146) | - |
| Einzeiliges Textfeld (Auswahl, Undo, Passwort, Secret-Schutz) | **vorhanden** | lib/fui/editor.fi; textbuf.fi; r99/r108 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | - | editor_main |
| Mehrzeiliges Textfeld | **teilweise** | lib/fui/textarea.fi (LINES_MAX 2048); r127 (Shift+Klick, bidi-Cursor, >100 KB) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | P1 (r127) | textarea_main + 1 MB Text ≤ 16 ms/Frame |
| Combobox editierbar + Autovervollständigung | **teilweise** | lib/fui/combofilter.fi (Filter+Ranking+Highlight+Tastatur, 10k Einträge 5,5 ms Release); combo_main; Zeichnen/Popup im Szenenbaum fehlt (r146) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | P1 (r146) | Tippen filtert 10k Einträge ≤ 16 ms |
| Dropdown/Select | **vorhanden** | appctl.dropdown; wave2.draw_dropdown | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | - | appctl-Prüfung |
| Liste einfach (Tastatur Auf/Ab/Enter, Mehrfachauswahl, Umbenennen) | **teilweise** | listrow.fi, wave3.draw_list; vlist hat Tastatur, Szenenbaum nicht (r134) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | P0 (r134) | listrow_main: Tastatur + Strg/Umschalt-Auswahl |
| Tabs/TabBar/Pivot | **vorhanden** | appctl.tabs; kit.tabbar_draw; wave2.draw_tab | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | - | kit_main |
| NavigationView/Sidebar (Kollaps, Hamburger) | **teilweise** | kit.nav_* (Seitenleiste); kein Kollaps/Hamburger | ~ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | ✗ | P2 | kit_main: 3 Breitenstufen |
| Expander/Accordion | **teilweise** | appctl.expander (einzeln); wave8m.Accordion (exklusiv/frei, Tasten, 64 Abschnitte); noch nicht in appctl | ~ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | ✓ | P2 | - |
| Splitter (ziehbarer Trenner) | **vorhanden** | lib/fui/splitter.fi; splitter_main, splitnode_main; appctl.split_* | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | P1 (neu) | Zug/Tastatur/a11y-separator-Test, Mindestgrößen |
| Docking (Panels andocken, Layout speichern) | **teilweise** | lib/fuishell/dock.fi (875 Z., JSON-Layout); nicht in fui.app | ✓ | ✓ | ✗ | ✗ | ~ | ✗ | ✗ | ~ | P2 | dock-Layout round-trip Test |
| Stepper/Wizard, Breadcrumb | **teilweise** | wave8.Crumbs (Faltung in "...", Fokuslauf, RTL) + wave8.Steps (Stepper, Tasten, RTL); wave8_main; nicht in appctl/a11y | ~ | ~ | ~ | ✗ | ~ | ✗ | ✗ | ✗ | P3 | - |
| Rating, Chips/Tags, Carousel/PageView, Segmented, Skeleton/Shimmer | **vorhanden** | lib/fui/wave5.fi; wave5_main, appw5_main (Carousel/Pager, Segmented, Skeleton) | ~ | ✓ | ✓ | ~ | ✓ | ✓ | ~ | ✗ | P2 (r45) | - |
| Badge, Avatar, Card/Tile, Kachelraster | **vorhanden** | kit.draw_badge/avatar_draw/grid_*; wave3.draw_card | ~ | ✓ | ✓ | ✗ | ✓ | ✓ | ~ | ✗ | - | kit_main |
| DatePicker/Calendar | **vorhanden** | wave3.draw_datepicker (Monat/Jahr); kein Bereich | ✓ | ✓ | ✓ | ✓ | ~ | ✓ | ✓ | ~ | - | wave3-Prüfung |
| TimePicker, Bereichs-Datum | **teilweise** | wave8.Time (24h/12h, Pfeile/Home/End/Ziffern/Rad, Hit, Zeichnen); Bereichs-Datum fehlt | ✓ | ✓ | ✓ | ~ | ✓ | ✓ | ✓ | ✗ | P2 (r146) | - |
| ColorPicker (Feld+Regler+Hex) | **vorhanden** | lib/fui/colordlg.fi; wave3.draw_colorpicker | ✓ | ~ | ✓ | ✓ | ~ | ✓ | ✗ | ✓ | - | colordlg_main |
| TreeView (Tastatur, Drag-Umordnen, Lazy-Load) | **teilweise** | wave3.draw_tree; vlist-Baum; Tastatur/Lazy unbelegt | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | P1 (r145) | Baum 100k Knoten lazy ≤ 16 ms |
| TableView/DataGrid (Sortieren, Spaltenbreite) | **teilweise** | vlist: Sortierpfeil/Spalten ziehen; wave8m.CellEdit (Cursor, Enter/F2/Tippen, Tab, Esc, Nur-Lese-Spalten; Modell, noch nicht in vlistview); Spalten umordnen, Fixierung fehlen | ✓ | ~ | ✓ | ~ | ~ | ✓ | ✗ | ~ | P1 (r146) | vlist_main + Zell-Edit-Test |
| Property-Grid/Inspector-Form | **vorhanden** | lib/fui/wave6.fi; wave6_main, appctl.propgrid | ~ | ✗ | ~ | ✗ | ✗ | ✗ | ✗ | ~ | P3 | - |
| Toolbar/CommandBar/Menüleiste (mit Überlauf) | **teilweise** | wave2.draw_toolbar/menubar; appctl.menubar; wave8m.Toolbar (toolbar_fit: Prioritaet, Chevron-Platz; Modell, Zeichnen offen) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | P2 | - |
| Tooltip/Flyout/Popover/TeachingTip | **teilweise** | wave2.tooltip; fuipopup.fi (X11-Toplevel); Flyout/Popover fehlen; r175 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | P1 (r175) | Popup ragt über Fensterrand, 3 Backends |
| Modaler Dialog, Toast, InfoBar | **teilweise** | kit.modal_*/toasts_*; msgdlg.fi; wave8.InfoBar (4 Schweregrade, Aktion, Schliessen, RTL, 3:1-Kontrast) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | P2 | kit_main |
| Bottom-Sheet/Drawer | **vorhanden** | lib/fui/wave6.fi; appctl.sheet_* | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | P2 | - |
| Markdown-/RichText-Ansicht, QR-Code, Kamera-Vorschau, Syntaxfarben | **vorhanden** | markdownview.fi (2289 Z.), richtext.fi, qrview.fi, camview.fi, syntaxcolor.fi | ~ | ~ | ~ | ✗ | ~ | ~ | ✗ | ~ | - | mdview_main, richtext_main, qrview_main |
| Editierbarer RichText (WYSIWYG) | **fehlt** | richtext.fi nur Anzeige | ✓ | ~ | ✓ | ~ | ~ | ✓ | ✗ | ✗ | P3 | - |
| Charts (Linie/Balken/Sparkline) | **fehlt** | nicht gefunden; Basis: Painter+viewport | ✓ | ~ | ~ | ~ | ~ | ✓ | ✗ | ✓ | P2 | Sparkline 1000 Punkte ≤ 1 ms |
| PDF-Ansicht | **fehlt** | lib/pdf erzeugt, zeigt nicht an | ✓ | ~ | ✓ | ~ | ~ | ✓ | ✗ | ✗ | P3 | - |

### Text, IME, RTL, Schrift, i18n

| Feature | fUi-Status | Beleg | Qt | Flutter | WinUI3 | GTK | Compose | SwiftUI | Slint | egui | Prio | Messwert/Test-Gate |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Rasterer: AA, Kerning (kern + GPOS-Paare), Gewichte, Kursiv, Tracking | **vorhanden** | lib/font/ttf.fi; painter.run_set_*; r13 (408/408 Paare) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | - | textbench; gposkern_check.py |
| Subpixel-Positionierung + Gamma-Kurve | **vorhanden** | lib/fui/textcurve.fi; Abweichung 11.2 Oktette; FreeType 1.3 % | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | - | text_main |
| Hinting | **fehlt** | lib/font/ttf.fi:29 'No hinting' | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | ✗ | P3 | - |
| LCD-Subpixel-AA (optional) | **fehlt** | r191 | ✓ | ~ | ✓ | ✓ | ~ | ✗ | ~ | ✗ | P3 | - |
| Variable Fonts (fvar/gvar) + echte Gewichte | **fehlt** | nur Default-Instanz; r141, r192 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | ✗ | P2 (r141, r192) | Inter wght 400/500/600/700 gegen FreeType ≤ 1 % |
| Fallback-Kette (Latein → CJK → Symbole → Emoji) im Standard-Setzer | **teilweise** | lib/font/fallback.fi (Gate 135) vorhanden; Painter nutzt sie nicht (r139) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | P0 (r139) | Chat mit CJK+Emoji: 0 Kästchen |
| Farb-Emoji (COLR v0, CBDT) | **teilweise** | lib/font/colr.fi, cbdt.fi; sbix/COLRv1/SVG-OT fehlen; r140 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | ✗ | P1 (r140) | Emoji-Zeile in Painter = Referenz, Diff ≤ 1 |
| Zeilenumbruch UAX #14 (CJK, Bindestrich, NBSP, Zahlen) | **vorhanden** | lib/str/linebreak.fi; uax14_main 7654/7654 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | ~ | P1 (neu) | LineBreakTest.txt 100 % bestanden |
| Silbentrennung | **fehlt** | nicht gefunden | ✓ | ✓ | ~ | ~ | ✓ | ✓ | ✗ | ✗ | P3 | - |
| Komplexschrift Arabisch (Formen, Ligaturen) | **teilweise** | lib/font/gsub.fi (Typ 1/4/7); Kontext 5/6 + Mark-Positionierung fehlen (r13) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | P2 (r13) | Arabische Referenztexte gegen HarfBuzz |
| Komplexschrift Indisch, Thai, Khmer, Hebräisch-Niqqud | **fehlt** | nur Arabisch; r13 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | P2 | HarfBuzz-Referenz, 20 Texte |
| Bidi UAX #9 | **vorhanden** | lib/fui/bidi.fi (91707/91707, 770241/770241) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | - | bidiconf_main |
| IME eigene Tabellen (Romaji, Hangul, Pinyin) | **teilweise** | lib/fui/ime.fi; Pinyin Zeichen + 634 Woerter (pywords.fi), keine Satz-Umwandlung | ~ | ~ | ~ | ~ | ~ | ~ | ~ | ✗ | P2 (r16) | - |
| IME OS-Komposition (TSF/IMM32, ibus/fcitx, Wayland text-input) | **fehlt** | win32.fi/x11.fi nur Zeichen; r142 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | P1 (r142) | Echter Test: Windows-Pinyin + fcitx |
| Selektierbare Labels / Auswahl über Widgets | **teilweise** | nur markdownview (mdv_select_*), Labels nicht | ~ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ | P2 | - |
| Rechtschreibung/Autokorrektur | **fehlt** | nicht gefunden | ~ | ✓ | ✓ | ~ | ✓ | ✓ | ✗ | ✗ | P3 | - |
| Suche im Text (Strg+F) in Textarea/Markdown | **fehlt** | nur kit.search (Suchfeld) | ✓ | ~ | ✓ | ~ | ~ | ✓ | ✗ | ✗ | P2 | - |
| Zahlen/Datum/Plural (CLDR) als Bibliothek | **vorhanden** | lib/i18n/human.fi; 8016 Fälle gegen ICU 72 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | ~ | - | i18n-Gate |
| fUi-Standardtexte lokalisierbar (OK/Cancel, Dialoge, Picker) | **teilweise** | lib/fui/uistr.fi: alle 119 tr_-Texte (filedlg/colordlg/fontdlg/msgdlg/appctl) über uistr.t, Deutsch komplett; tools/fui/uistr_audit.py 0 Befunde; Arabisch fehlt | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | P1 (neu) | Audit: 0 feste UI-Strings; de/en/ar-Dump |

### Animation

| Feature | fUi-Status | Beleg | Qt | Flutter | WinUI3 | GTK | Compose | SwiftUI | Slint | egui | Prio | Messwert/Test-Gate |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Implizite Übergänge (Hover/Press/Zustand) | **vorhanden** | lib/fui/anim.fi transreg; animbench; Hover ≤ 1 Frame | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | - | animbench_main |
| Federn, Easing, Fling | **vorhanden** | anim.easing_spring/bezier; viewport | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | - | anim_main |
| Explizit: Keyframes/Timeline/Sequenz/Stagger | **teilweise** | lib/fui/timeline.fi (seq/par/stagger/repeat/alternate/reverse/speed, reproduzierbar, Reduced-Motion); timeline_main (36 Checks); keine Verdrahtung mit Widgets (r149) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | P1 (r149) | Timeline mit 5 Spuren pixelgenau reproduzierbar |
| Shared-Element/Hero | **fehlt** | nicht gefunden | ✗ | ✓ | ✓ | ✗ | ✓ | ✓ | ✗ | ✗ | P2 | - |
| Seitenübergänge/Navigation-Stack | **fehlt** | r149 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | ✗ | P2 (r149) | - |
| Lottie/Vektor-Animation | **fehlt** | uianim.fi nur GIF/WebP | ✓ | ~ | ✓ | ~ | ✓ | ~ | ✗ | ✗ | P3 | - |
| Reduzierte Bewegung (System) beachten | **teilweise** | syspref.motion_* (gsettings/SPI/Android-Skala) + anim.transreg_set_reduced + timeline.set_reduced; Host-Leser fehlen | ✗ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | P2 | Schalter → 0 Animationen |
| Frame-Pacing, Idle 0 % CPU | **vorhanden** | lib/fui/pacing.fi; looppump.fi | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | - | animbench_main |

### Theming

| Feature | fUi-Status | Beleg | Qt | Flutter | WinUI3 | GTK | Compose | SwiftUI | Slint | egui | Prio | Messwert/Test-Gate |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Tokens, Hell/Dunkel, Akzent-Ableitung (OKLab) | **vorhanden** | lib/fui/theme.fi; themefile.fi | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | - | themefile_main |
| Theme-Dateien mit Kontrast-/Füllregel-Prüfung | **vorhanden** | themefile.fi; themelist.fi (6 Themes) | ✗ | ~ | ~ | ~ | ~ | ~ | ~ | ✗ | - | themefile_main |
| Fluent-Preset (Radien, Dichte, Schatten, Maße) | **vorhanden** | theme.shape_fluent; fluent_main (58 Maße, 0 Abw.) | ~ | ✓ | ✓ | ~ | ✓ | ✓ | ~ | ✗ | - | fluent_main |
| System-Akzent/Dark-Mode live folgen | **teilweise** | lib/fui/syspref.fi (Parser+Theme-Anbindung, syspref_main); Host-Leser (DBus/Registry) fehlen (r150) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | P1 (r150) | Systemwechsel → Frame in ≤ 1 s |
| High-Contrast des OS erkennen (Forced Colors) | **teilweise** | lib/fui/syspref.fi hc_from_flags/portal_contrast -> Theme 7:1; Host-Leser fehlen | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | P1 (r150) | Windows-Kontrastmodus → Theme gewechselt |
| Dichte compact/comfortable | **teilweise** | theme_set_scale, 3 Shapes; wave8m.density_* (compact 80 %, Touch bleibt comfortable); nicht im Theme verdrahtet | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | P2 | - |
| Mica/Acrylic/Blur-Behind | **fehlt** | r151; In-Fenster-Glas effect.fi (~100 ms/Dialog) | ~ | ~ | ✓ | ✗ | ~ | ✓ | ~ | ✗ | P2 (r151) | Blur ≤ 8 ms @1240x720 |
| Icons (Lucide 93 + SVG) / Fluent-Icons | **teilweise** | icons.fi, lucide.fi; Fluent/Material-Satz fehlt | ✓ | ~ | ✓ | ✓ | ✓ | ✓ | ~ | ✗ | P2 | - |
| Design-Tokens exportieren (css/json/rs) | **fehlt** | r161 | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | P2 (r161) | Drift-Prüfung 0 Abweichungen |
| Schatten/Elevation | **vorhanden** | render.draw_elevation_at; r197/r208 | ~ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | - | fluent_main |

### Barrierefreiheit

| Feature | fUi-Status | Beleg | Qt | Flutter | WinUI3 | GTK | Compose | SwiftUI | Slint | egui | Prio | Messwert/Test-Gate |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| a11y-Baum (Rollen, Name, Zustand, Wert, Secret) | **vorhanden** | lib/fui/a11y.fi; audit.fi (0 unbenannt) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | - | audit.sh |
| AT-SPI-Brücke (Linux) | **teilweise** | lib/fui/atspi.fi 26/26; Events/Text-Interface fehlen (r219), kein Orca-Test (r220) | ✓ | ✓ | ✗ | ✓ | ✓ | ✗ | ✓ | ✓ | P1 (r219, r220) | Orca liest Showcase-Fenster |
| UIA-Brücke (Windows) | **fehlt** | r19, r152 (Geometrie) | ✓ | ✓ | ✓ | ✗ | ✓ | ✗ | ✓ | ✓ | P1 (r19, r152) | Narrator/Inspect.exe sieht 100 % der Knoten |
| ARIA-Spiegel (Web) | **fehlt** | r103 | ~ | ✓ | ✗ | ✗ | ~ | ✗ | ~ | ✓ | P1 (r103) | axe-core 0 Verstöße auf Galerie |
| Tastaturbedienung: Tab-Ring, Fokusmodell, Mnemonics, Shortcut-Registry | **teilweise** | control.fi; action.fi (Kürzel, act_mnemonic); r143, r221 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | P1 (r143, r221) | Alle Kit-Controls ohne Maus bedienbar (Skript) |
| Fokusring, Kontrast-Audit im Build | **vorhanden** | kit.focus_ring; contrast_main; themefile | ~ | ~ | ~ | ~ | ~ | ~ | ✗ | ✗ | - | contrast_main |
| Textskalierung nach System-Einstellung | **teilweise** | wave8m.textscale_* (85-200 %, 2,5-%-Raster); Host-Leser fehlen | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | ✗ | P2 | - |
| Voll-Zugriff A11Y_MAX/SCENE_MAX 128 | **teilweise** | a11y.fi:105; r222 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | P1 (r222) | Brücke zeigt 2000 Knoten |

### Eingabe, Drag&Drop, Zwischenablage

| Feature | fUi-Status | Beleg | Qt | Flutter | WinUI3 | GTK | Compose | SwiftUI | Slint | egui | Prio | Messwert/Test-Gate |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Maus, Tastatur, Rad, Cursorformen | **vorhanden** | window.fi; event.fi | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | - | event_main |
| Touch/Gesten (Tap, Doppel, Lang, Pan, Fling, Pinch) | **teilweise** | event.fi; kittouch.fi; nur Emulator/Protokoll (r121, r122) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | P1 (r121, r122) | Echtgerät: Touchpad-Beispiel auf Handy + Touchscreen |
| Stift (Druck, Neigung, Radierer) | **teilweise** | pointerType pen; kein Druck | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | ~ | P3 | - |
| Gamepad-/Fokus-Navigation (D-Pad) | **teilweise** | lib/input/pad*.fi (Eingabe), keine fUi-Anbindung | ~ | ~ | ✓ | ~ | ✓ | ✓ | ✗ | ✗ | P2 | Launcher per Controller bedienbar |
| Drag & Drop: Datei als Ziel | **vorhanden** | x11 XDND; win32 WM_DROPFILES | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | - | desktop-Tests |
| Drag & Drop: Quelle/innerhalb der App | **teilweise** | lib/fui/dnd.fi (Zustandsmaschine, Effekt-Regeln, Listen-Slot, Rand-Scroll, Tastatur); dnd_main (56 Checks); keine Host-/Szenenanbindung (r144) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | ✓ | P1 (r144) | Ziehen Zeile→Zeile, Text→Feld in 3 Hosts |
| Zwischenablage Text/Bild/Formate (Desktop) | **vorhanden** | window.clipboard_*; X11/Win32; Web webclip.fi | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | - | desktop-Tests |
| Zwischenablage OrientOS/Android | **fehlt** | osum.fi:990, android.fi:1652 (interner Fallback) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | P1 (neu) | Kopieren in App A, Einfügen in App B (Text+PNG) |
| Tastaturlayouts, AltGr, Totasten | **teilweise** | x11.fi keymap_*; Totasten unbelegt | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | P2 | Test mit de/fr/ru-Layout |
| Bildschirmtastatur | **teilweise** | Android (Emulator); Web; Windows/OrientOS fehlt | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | P3 | - |

### Dialoge

| Feature | fUi-Status | Beleg | Qt | Flutter | WinUI3 | GTK | Compose | SwiftUI | Slint | egui | Prio | Messwert/Test-Gate |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Datei-Dialog (fUi, 5000 Einträge 14.9 ms) | **vorhanden** | lib/fui/filedlg.fi (3733 Z.) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | ~ | - | filedlg_main |
| Native Datei-/Ordner-Dialoge (IFileDialog, XDG-Portal) | **fehlt** | r153 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | ~ | P2 (r153) | - |
| Farb-/Schrift-Dialog | **vorhanden** | colordlg.fi; fontdlg.fi (CFF/.ttc nicht, r182) | ✓ | ~ | ✓ | ✓ | ~ | ✓ | ✗ | ✗ | - | colordlg_main, fontdlg_main |
| Meldung/Bestätigung/Eingabe | **vorhanden** | msgdlg.fi; appctl.message/confirm/input | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | - | msgdlg_main |
| Druck-Dialog/Vorschau | **fehlt** | lib/print (IPP) ohne Widget | ✓ | ~ | ✓ | ✓ | ~ | ✓ | ✗ | ✗ | P3 | - |

### Menüs, Shortcuts, Popups

| Feature | fUi-Status | Beleg | Qt | Flutter | WinUI3 | GTK | Compose | SwiftUI | Slint | egui | Prio | Messwert/Test-Gate |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Kontextmenü (Provider-Kette, Untermenüs, Plugins, Bus) | **vorhanden** | ctxmenu.fi; menuview.fi; ctxstd.fi; docs/CONTEXT-MENUS.md | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | ~ | - | ctxmenu_main |
| Menüleiste | **vorhanden** | appctl.menubar; wave2.draw_menubar | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | - | menuview_main |
| Popups als eigene Toplevel-Fenster | **teilweise** | lib/plat/fuipopup.fi X11 fertig; Win32/OrientOS offen (r172), Dropdown/Tooltip/Toast (r175) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | P0 (r172, r175) | Menü über Fensterrand auf allen 3 Desktop-Hosts |
| Shortcut-Management (Konflikte, Rebinding-UI) | **teilweise** | action.fi Registry; act_conflict/act_rebind (w16_main) + keycapture.fi (Aufnahme-Zustandsmaschine); Settings-UI fehlt | ✓ | ✓ | ✓ | ✓ | ~ | ✓ | ~ | ~ | P2 (r143) | - |
| Fensterleiste/Titelleiste eigene (Drag/Min/Max/Close) | **teilweise** | lib/fui/titlebar.fi; Hosts nicht verdrahtet (r229) | ~ | ~ | ✓ | ~ | ~ | ~ | ~ | ~ | P1 (r229) | Drag/Max/Close auf X11+Win32 |

### Grafik/Canvas

| Feature | fUi-Status | Beleg | Qt | Flutter | WinUI3 | GTK | Compose | SwiftUI | Slint | egui | Prio | Messwert/Test-Gate |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Rect/Rund/Ring/Linie/Schatten/Blur/Glas | **vorhanden** | painter.fi; effect.fi | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | - | effect_main |
| Gradient linear/radial (Rechteck/Text) | **teilweise** | painter.grad_rect/round_rect_radial; style background:gradient; konisch + Pfad fehlen | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | P2 | - |
| Vektorpfade (Bezier, Füllregeln, Stroke Cap/Join/Dash) als Zeichner-API | **teilweise** | svg/path.fi, paint/stroke.fi, icons.fill_path vorhanden; kein offener Painter-Pfad-Aufruf | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | P1 (neu) | 30 Pfade gegen resvg ≤ 0.5 % Pixel |
| Clip (Rechteck/Rund/Pfad/Maske) | **teilweise** | canvas.clip_push_rect/mask; Pfad-Clip fehlt | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | P2 | in Pfad-API |
| Blend-Modes | **teilweise** | canvas BLEND_* (8 Modi); nicht im Stil/GPU | ✓ | ✓ | ~ | ~ | ✓ | ✓ | ~ | ~ | P3 | - |
| Filter (Farbmatrix, Blur, Drop-Shadow, Backdrop) | **vorhanden** | effect.cm_*/backdrop_blur/glass_*; Blur ~100 ms/Dialog (r151) | ✓ | ✓ | ✓ | ~ | ✓ | ✓ | ~ | ✗ | - | effect_main |
| Shader/Custom-Effekte | **teilweise** | gpu_fs.glsl nur Web/Android (r71) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ | P3 | - |
| Schmelz-/Metaball-Effekt | **vorhanden** | lib/fui/merge.fi; 11.4 ms | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | - | merge_main |
| 2D-Transform (Rotate/Scale/Skew) | **vorhanden** | lib/fui/transform.fi; Perspektive/3D fehlt | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | - | transform_main |
| Bilder PNG/JPEG/WebP/GIF/SVG | **vorhanden** | uiimage*.fi; uisvg.fi; APNG nur 1. Bild | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | - | image_main, uisvg_main |
| GPU-Pfad Desktop | **fehlt** | gpu.fi nur Web/Android; r156 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | P2 (r156) | 1080p Hover/Scroll ≤ CPU-Wert; 4K-Messung |

### Medien/Einbetten

| Feature | fUi-Status | Beleg | Qt | Flutter | WinUI3 | GTK | Compose | SwiftUI | Slint | egui | Prio | Messwert/Test-Gate |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Video-Widget | **fehlt** | r157 | ✓ | ~ | ✓ | ✓ | ~ | ✓ | ? | ✗ | P2 (r157) | 720p 30 fps im Widget |
| WebView/Certus-Glass einbetten | **fehlt** | r157 | ✓ | ~ | ✓ | ~ | ~ | ~ | ✗ | ✗ | P2 (r157) | - |
| Audio-Ausgabe (Bibliothek) | **vorhanden** | lib/audio | ✓ | ~ | ✓ | ✓ | ~ | ✓ | ~ | ~ | - | Audio-Gates |
| 3D-Szene | **fehlt** | lib/gpu hat Minecraft-Renderer, kein fUi-Widget | ✓ | ~ | ✓ | ✓ | ~ | ✓ | ✗ | ~ | P3 | - |

### Entwicklung, Tests, Doku

| Feature | fUi-Status | Beleg | Qt | Flutter | WinUI3 | GTK | Compose | SwiftUI | Slint | egui | Prio | Messwert/Test-Gate |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Hot-Reload/Live-Preview | **fehlt** | r158 | ✓ | ✓ | ✓ | ~ | ✓ | ✓ | ✓ | ~ | P1 (r158) | Änderung → Bild in ≤ 2 s ohne Zustandsverlust |
| Reaktiver Zustand/Datenbindung, Closures für Handler | **teilweise** | Closures in Sprache (docs/CLOSURES.md, by-value); fUi-Handler weiter benannte fn; r158 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | P1 (r158) | Beispiel-App ohne ud-Zeiger |
| Deklaratives Markup | **fehlt** | r158; nur Stilblatt | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | P2 (r158) | - |
| Designer/GUI-Builder | **fehlt** | r158 | ✓ | ~ | ✓ | ✓ | ~ | ✓ | ✓ | ✗ | P3 | - |
| Inspector (Element wählen, Stil+Siegerregel, Live-Edit) | **vorhanden** | inspect.fi; query.fi; r97 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | ~ | - | apptree_main |
| Widget-Tests headless (Pixel, scene_query) | **vorhanden** | tools/fui/*_main.fi (~100); query.fi | ✓ | ✓ | ✓ | ~ | ✓ | ✓ | ✓ | ✓ | - | run.sh |
| Golden-Bilder/Parität X11-wasm-Wine | **vorhanden** | showcase.sh; tools/fui/golden; parity.sh | ~ | ✓ | ~ | ✗ | ✓ | ✓ | ✓ | ~ | - | showcase.sh |
| Test-Recorder/Replay, Treiber für fremde Apps | **vorhanden** | lib/fui/recorder.fi; recorder_main, rechook_main | ✓ | ✓ | ✓ | ~ | ✓ | ✓ | ✓ | ~ | P1 (r159) | Aufzeichnung läuft 10× identisch ab |
| Fuzz/Soak (Zufallsereignisse, Zufallsbäume) | **vorhanden** | tools/fui/fuzz_main.fi (W-Fuzz) | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | P1 (neu) | 10^6 Ereignisse + 10^4 Bäume: 0 Absturz/OOB |
| Doku: API-Referenz, Rezeptbuch, Tutorials | **teilweise** | docs/fui-quickstart.md (217 Z.), fui-kit.md, APP-TREE.md; r160 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | P1 (r160) | Jede pub fn mit Doku; 5 Rezepte laufen |
| Beispiele/Galerie/Demo-Apps | **vorhanden** | examples/fui (7), fui_controls, gallery1-10, codehub | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | - | run.sh |
| Perf-Gate + Baseline | **vorhanden** | perfgate.py; perfbench; perf_baseline.json | ~ | ✓ | ~ | ✗ | ✓ | ✓ | ✗ | ✗ | - | perfgate |

### Plattform-Hosts, Fenster, Packaging

| Feature | fUi-Status | Beleg | Qt | Flutter | WinUI3 | GTK | Compose | SwiftUI | Slint | egui | Prio | Messwert/Test-Gate |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Linux X11 (eigenes Protokoll) | **vorhanden** | lib/window/x11.fi | ✓ | ✓ | ✗ | ✓ | ✓ | ✗ | ✓ | ✓ | - | x11live |
| Wayland nativ | **fehlt** | nur XWayland (extras.fi:26); r155 | ✓ | ✓ | ✗ | ✓ | ✓ | ✗ | ✓ | ✓ | P2 (r155) | Wayland-Sitzung: Fenster, IME, Dateien |
| Windows | **teilweise** | lib/window/win32.fi (2700 Z.); nur Wine; r121/r138 | ✓ | ✓ | ✓ | ✗ | ✓ | ✗ | ✓ | ✓ | P0 (r121) | Echter Windows-Lauf der 7 Beispiele |
| macOS | **fehlt** | nur Stubs (docs/APPKIT.md) | ✓ | ✓ | ✗ | ✗ | ✓ | ✓ | ✓ | ✓ | P3 | - |
| iOS | **fehlt** | nicht gefunden | ✓ | ✓ | ✗ | ✗ | ✓ | ✓ | ~ | ~ | P3 | - |
| Android | **teilweise** | lib/window/android.fi; Emulator; r122 | ✓ | ✓ | ✗ | ✗ | ✓ | ✗ | ✓ | ~ | P1 (r122) | Echtes Handy: Tastatur, Touch, Lebenszyklus |
| Web/WASM | **vorhanden** | lib/plat/web.fi; appcheck.py 0 px | ✓ | ✓ | ✗ | ✗ | ~ | ✗ | ✓ | ✓ | - | appcheck |
| OrientOS | **teilweise** | lib/window/osum.fi; Migration r41/r42/r61/r170 | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | P0 (r41) | wlib ersetzt; Explorer+Editor laufen auf fUi |
| Mehrfenster | **teilweise** | fuiwin.fi kann; fui.app 1 Fenster (r89) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | P1 (r89) | 2 Fenster in fui.app, getrennte Eingabe |
| Tray + Benachrichtigungen in der fUi-App | **teilweise** | lib/desktop/tray.fi, notify.fi (SNI, Shell_NotifyIcon) als Bibliothek; keine fui.app-API | ✓ | ~ | ~ | ✗ | ✓ | ✓ | ✗ | ~ | P1 (neu) | Beispiel mit Tray-Menü unter SNI-Mock + Wine |
| Benachrichtigungen mit Aktionsknöpfen/Bildern | **teilweise** | Windows nur Balloon; r154 | ✓ | ~ | ✓ | ✓ | ~ | ✓ | ✗ | ~ | P2 (r154) | - |
| Mehrmonitor-DPI, Refresh-Rate | **teilweise** | r155, r212; X11 ein Xft.dpi | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | P2 (r155, r212) | 2 Monitore 100 %/150 % pixelgenau |
| Packaging, Installer, Auto-Update, Signatur | **teilweise** | tools/pack (deb/rpm/AppImage/Setup.exe), appkit.update (Ed25519); Signatur offen (r154) | ~ | ✗ | ✓ | ~ | ✓ | ✓ | ✗ | ✗ | P2 (r154) | SmartScreen ohne Warnung |

### Performance

| Feature | fUi-Status | Beleg | Qt | Flutter | WinUI3 | GTK | Compose | SwiftUI | Slint | egui | Prio | Messwert/Test-Gate |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| CPU-Raster 1240x720 10.5/6.2 ms; Hover 0.63 ms | **vorhanden** | perf_baseline.json; Kennzahlen | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | - | perfgate |
| Kalter Text / Glyph-Cache | **teilweise** | 340 µs kalt/100 µs warm (r190) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | P2 (r190) | kalt ≤ 150 µs |
| Hohe Dichte (1080x2400 Voll-Neumalen 13.2 ms) | **teilweise** | r83 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | P2 (r83) | ≤ 8 ms |
| Speicher je Knoten (760 B) | **teilweise** | r102 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | P0 (r102) | ≤ 200 B/Knoten |

## Was sich seit GAP-ANALYSE-0710 geändert hat (belegt)

* **Erledigt/gebaut seit 07.10.:** GPOS-Paarkerning (r13), Schrift-Fallback-Kette + Graphem-Cluster + CBDT/COLR v0 als Bibliothek (`lib/font/fallback.fi`, `docs/FONT-FALLBACK.md`, Gate 135), Fluent-Preset (`theme.shape_fluent`, `themelist` fluent-light/dark, 58 Maßtests), AT-SPI-Brücke (26/26 Knoten), Kontextmenü-System (r162–r168), Popup-Toplevel unter X11 (`fuipopup.fi`), Dirty-Rect/Teil-Present (W4/W6), Overscroll/Federn/Transition-Damage (W7), Titelleisten-Modell (`titlebar.fi`), Dialoge Datei/Farbe/Schrift/Meldung (r177–r184), Windows-Host existiert (`win32.fi`, nur Wine), Closures in der Sprache (`docs/CLOSURES.md`, by-value).
* **Weiter offen und weiter der Kern:** SCENE_MAX 128 (r102), `fui.app`-Controls (r145), Listen im Szenenbaum (r134), Fallback im Standard-Setzer (r139), OS-IME (r142), UIA (r19), Hot-Reload/State (r158), echte Geräte (r121/r122), OrientOS-Migration (r41).
* **Neu gefunden (nicht in der alten Analyse/Roadmap):** keine UAX-#14-Umbrüche (CJK), fUi-Standardtexte fest englisch, kein offener Pfad-Aufruf im Painter, kein Splitter im Szenenbaum, kein Tray/Notify in `fui.app`, System-Zwischenablage fehlt auf OrientOS/Android, kein Fuzz/Soak für fUi.

## Priorisierung (kurz begründet)

Maßstab: Wirkung für die vier Ziel-Apps gegen Aufwand (S ≤ 3 Tage · M ≤ 3 Wochen · L darüber; Schätzung, nicht gemessen).

* **P0 = Baustein, ohne den eine echte App nicht gebaut werden kann.** r102 (Knotenlimit: jede Listen-/Mail-/Chat-App bricht bei >128 Knoten, M–L) · r145 (`fui.app` ohne Checkbox/Slider/Menü/Dialog/Tabelle: Apps müssen Low-Level schreiben, M) · r134 (Listen mit Tastatur/Mehrfachauswahl/Virtualisierung im Szenenbaum, M) · r139 (Kästchen statt CJK/Emoji in jedem Chat, M, Bausteine fertig) · r172/r175 (Menüs/Dropdowns werden unter Win32/OrientOS am Fensterrand abgeschnitten, M) · r121 (Windows-Behauptung ist ungeprüft, S, braucht Justin) · r41 (OrientOS-Nutzen von fUi ist erst mit Migration real, L).
* **P1 = nächste Welle, hoher Hebel.** OS-IME (CJK-Nutzer), UIA/AT-SPI-Rest/ARIA (Screenreader, Firmen-Apps), Shortcuts/Mnemonics, Drag-Quelle, Mehrfenster, Titelleiste, Touch/Android am echten Gerät, Hot-Reload + State, Test-Recorder, Doku, Fuzz/Soak (feste Arrays machen Überläufe wahrscheinlich), System-Akzent/Kontrast, Farb-Emoji, TextArea-Rest, Grid/Responsive, Timeline-Animation; neu: UAX-#14, i18n-Texte, Pfad-API, Splitter, Tray/Notify, Zwischenablage OrientOS/Android.
* **P2/P3 nur in der Tabelle:** Charts, Shared-Element, Docking in `fui.app`, Wayland nativ, Video/WebView, GPU-Desktop, Designer, macOS/iOS, Lottie, Druck-Dialog. Begründung: kein Ziel-App-Blocker, großer Aufwand (L) oder fremde Zielplattform.

## Neue Roadmap-Punkte aus dieser Runde (Gruppe „Lücken“)

1. **Painter-Pfad-API** — Bezier/Füllregeln/Stroke/Pfad-Clip als offener Aufruf für eigene Zeichner und Charts; Gate: 30 Pfade gegen resvg ≤ 0.5 % Pixeldiff. (Bausteine: `lib/svg/path.fi`, `lib/paint/stroke.fi`, `icons.fill_path`.)
2. **UAX-#14-Zeilenumbruch** — CJK, Bindestrich, NBSP, Zahlen in `run_draw_wrapped` und Textarea; Gate: `LineBreakTest.txt` 100 %.
3. **fUi-Standardtexte lokalisierbar** — OK/Cancel/Dialoge/Picker über `i18n`/`appkit.texts`; Gate: Audit 0 feste UI-Strings, de/en/ar-Dump.
4. **Splitter im Szenenbaum + fui.app** — aus `fuishell/dock.fi` `split_*`; Gate: Zug/Tastatur/a11y-Test, Mindestgrößen.
5. **Tray/Notify/Autostart in fui.app** — `lib/desktop` anbinden; Gate: Beispiel unter SNI-Mock + Wine.
6. **Zwischenablage OrientOS/Android** — heute interner Fallback (`osum.fi:990`, `android.fi:1652`); Gate: Kopieren App A → Einfügen App B (Text+PNG).
7. **Fuzz/Soak für fUi** — Zufallsereignisse + Zufallsbäume durch Router/Szene/Textarea/vlist; Gate: 10^6 Ereignisse, 0 Absturz/OOB, Abschnitt in `run.sh`.

Alle anderen P0/P1-Zeilen verweisen auf **bestehende offene Punkte** (r19, r89, r102, r103, r121, r122, r127, r134, r139–r146, r148–r150, r152, r158–r160, r172, r175, r219–r222, r229) — dort keine Dopplung angelegt.

## Top-15 (Reihenfolge der Abarbeitung)

| # | Punkt | Ref | Paket | Warum zuerst |
|---|---|---|---|---|
| 1 | Knotenkapazität + Arena (SCENE_MAX/KIDS_MAX, A11Y_MAX) | r102, r222 | A | Blockiert alles Größere |
| 2 | Standard-Controls in `fui.app` | r145 | A | Schließt API-Lücke, Checkbox/Slider/Menü/Dialog/Tabelle |
| 3 | Fallback-Kette im Standard-Setzer | r139 | B | Bausteine fertig, nur Verdrahtung, sichtbar in jedem Chat |
| 4 | Popups Win32/OrientOS + Dropdown/Tooltip/Toast | r172, r175 | C | Menüs über Fensterrand, Windows/OrientOS |
| 5 | Listen im Szenenbaum: Tastatur, Mehrfachauswahl, Virtualisierung | r134 | A | Explorer, Dateiliste, Chatliste |
| 6 | OrientOS: Vendor-Sprung + wlib → fUi | r61, r41, r42 | C | Nutzen von fUi für OrientOS |
| 7 | Fuzz/Soak | neu | E | Feste Arrays → Überlaufrisiko, billig |
| 8 | Zwischenablage OrientOS/Android | neu | C | Copy/Paste zwischen Apps |
| 9 | UAX-#14-Umbruch | neu | B | CJK-Text bricht sonst falsch |
| 10 | UIA-Brücke + Geometrie | r19, r152 | D | Windows-Screenreader |
| 11 | Shortcuts/Mnemonics/Fokusmodell | r143, r221 | D | Tastatur-Apps, Barrierefreiheit |
| 12 | Splitter + Tabellen-Zellbearbeitung/Combobox | neu, r146 | A | Explorer/Editor-Layouts |
| 13 | Painter-Pfad-API | neu | E (nach B-Merge) | Charts, eigene Zeichner |
| 14 | Tray/Notify/Autostart in fui.app | neu | C | Launcher/Client im Hintergrund |
| 15 | Test-Recorder + generierte API-Doku | r159, r160 | E | Dritt-Apps, Einarbeitung |

Parallel, kein Paket: **r121/r122** echter Windows-/Touch-/Handy-Test (braucht Justin am Gerät).

## 5 parallele Pakete (Dateibereiche ohne Überlappung)

* **A „Baum & fui.app“** — `scene.fi`, `app.fi`, `appctl.fi`, `vlist*.fi`, `listrow.fi`, `textarea.fi`, `layout.fi`, `flex.fi`. Reihenfolge: r102 → r145 → r134 → Splitter/r146 → später r148 Grid.
* **B „Text“** — `lib/font/*`, `lib/str/*`, `painter.fi` (Textteile), `bidi.fi`, `ime.fi`, `richtext.fi`. Reihenfolge: r139 → UAX-#14 → r140 → r13.
* **C „Hosts & Plattform“** — `lib/window/*`, `lib/plat/*`, `lib/@*/fui/apphost.fi`, `fuipopup.fi`, `fuiwin*.fi`, OrientOS-Repo. Reihenfolge: r172/r175 → r61/r41/r42 → Zwischenablage → Tray (Glue nur in neuer Datei, `app.fi`-Haken über A).
* **D „a11y & Eingabe“** — `a11y.fi`, `atspi.fi`, neue `uia.fi`, `kita11y.fi`, `action.fi`, `event.fi`, `control.fi`. Reihenfolge: r19/r152 → r143/r221 → r219/r220 → r103. Änderungen an `scene.fi` nur über Paket A; die Konstanten A11Y_MAX gehören zu A (r222).
* **E „Qualität & Dev“** — nur neue Dateien: `tools/fui/*` (Fuzz/Soak, Recorder), `docs/`, neue `lib/fui/vpath.fi`. Reihenfolge: Fuzz → Doku (r160) → Recorder (r159) → Pfad-API erst **nach B-Merge** (Verdrahtung in `painter.fi`).

Konfliktregel: `painter.fi` gehört B, `scene.fi`/`app.fi` gehören A, `window/*` gehört C; wer dort etwas braucht, stellt eine Mini-Anfrage an den Besitzer statt zu editieren.

## Unsicherheiten / was nicht geprüft wurde

* Vergleichsspalten: Trainingswissen, keine frische Recherche (Websuche nicht genutzt); Slint/egui/Compose-Desktop-Details besonders unsicher (`?`).
* fUi-Status per grep/Dateilesen; **keine Testläufe wiederholt**. „vorhanden“ heißt: Code + Prüfprogramm existieren, nicht „heute grün“ (bekannte Altfehler: r48, r194).
* Tree-/Tabellen-Fähigkeiten von `vlist.fi` (Tastatur, Lazy-Load) sind im Code nur teils belegt — Zeile „TreeView“ ist deshalb „teilweise“.
* Das Layout „absolute Positionierung“ wurde nicht einzeln geprüft (nur `node_set_offset`, `layer.fi`).


## Nachtrag 18.10.2026 (W26-d): Priorität-1-Zeilen

Neu in main (alles reine Modelle mit Prüfprogramm in `tools/fui/run.sh`, Gerüst für die Szenenbaum-Anbindung folgt):

* **#5 Responsive** `lib/fui/sizeclass.fi`, `sizeclass_main` (Klassen compact/medium/expanded bei 600/900 dp, Hysterese 16 dp, Höhenklassen, `columns`, Container-Abfragen).
* **#21 Combobox** `lib/fui/combofilter.fi`, `combo_main` (Rangfolge exakt > Präfix > Wortanfang > Teilstring > Teilfolge, Highlight-Maske, Pfeil/Page/Home/End, Enter/Esc; 10 000 Einträge: 5,5 ms Release-Build, 30 ms Dev-Build).
* **#67 Timeline** `lib/fui/timeline.fi`, `timeline_main` (Tracks, seq/par/at/stagger, Wiederholen/Pingpong, Rückwärts, Tempo ohne Drift, Reduced-Motion springt ans Ende; 16-ms- und 1-ms-Schritte liefern bitgleiche Werte).
* **#96 Drag & Drop in der App** `lib/fui/dnd.fi`, `dnd_main` (Schwelle 4/10 px, Effekt Move/Copy/Link mit Strg/Umschalt/Alt, Listen-/Baum-Einfügeslot, Rand-Scroll, Tastaturweg).
* **#64 Standardtexte** `lib/fui/uistr.fi` + `tools/fui/uistr_audit.py`: 119 Schlüssel, Deutsch komplett, Englisch unverändert (alle Dialog-Goldens/Läufe grün).
* **#71 Reduzierte Bewegung**: `syspref.motion_*`, `anim.transreg_set_reduced`.
* Veraltete Zeilen nachgezogen: UAX #14, Fuzz, Recorder, Splitter, wave5/6, Syspref (Stand W25).

**Offen:** Verdrahtung in Szenenbaum/`fui.app` (Grid-Spuren r148, Combobox-Popup r146, Timeline-Widgets r149, DnD-Hosts r144), Host-Leser für System-Einstellungen, Arabisch in `uistr`.

## W28-d (18.10.2026): Wave 8 + Modelle
* `lib/fui/wave8.fi`: Spinner, TimePicker, Breadcrumb, InfoBar, Stepper (Sofortmodus, ein Geometrie-Pfad, Theme-Tokens, RTL); `lib/fui/wave8m.fi`: Accordion, Toolbar-Ueberlauf, Tabellen-Zellbearbeitung, Insets, Dichte, Textskalierung; `lib/fui/keycapture.fi`: Tastenkuerzel aufnehmen (Konflikt/Ersetzen). Test `tools/fui/wave8_main.fi` (227 Checks, hell+dunkel), in `run.sh`.
* **Offen (r45-Rest):** appctl-/a11y-(kita11y)-Anbindung der wave8-Elemente, Golden fuer wave8, Host-Leser (Insets, Textskalierung), Bereichs-Datum, Zeichnen des Toolbar-Chevrons, Cell-Edit in vlistview.
