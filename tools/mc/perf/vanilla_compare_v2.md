# Vanilla 1.20.1 vs mcplay (GL / Vulkan) -- Laptop HOLO-LAPTOP-3, Hoechstleistung, 23.10.2026 (v2)

Status: partial (Zeitlimit: 640x360 nur n=1-2 statt 3; Vanilla r16 1280x720 n=2, ein PresentMon-CSV fehlt). Optional Fabric+Sodium und Timer-Experiment nicht gemacht.

## Bedingungen
- Geraet: Surface Laptop 4, i7-1185G7 (4C/8T), Iris Xe 32.0.101.7085, 16 GB, Win 10.0.26300, AC (BatteryStatus 2, 100 %).
- Energieplan: Hoechstleistung (Duplikat des High-Performance-Schemas, da keines vorhanden war), vorher "Ausbalanciert" 381b4222-... -> am Ende zurueckgesetzt, Duplikat geloescht.
- Hintergrund per CloseMainWindow geschlossen: firefox (Hauptfenster), ms-teams, OneNote, PowerPoint, Word (zeigt noch "Dokument1 - Automatisch wiederhergestellt"), Notepad (hotel_crash.log), FxSound (Tray), VirtualBox Manager, certus. Nicht schliessbar (kein Fenster/Tray): Firefox-Kindprozesse, ms-teams-WebView, SpotifyLauncher, ClockWidgets, WidgetService, FxSound-Tray. Ein anderer Worker nutzte den Laptop gleichzeitig (C:\winidle, Certus-Idle-Messung, 22:30-22:47): wurde NICHT unterbrochen, meine Messreihe startete erst danach (22:47).
- Server: Vanilla 1.20.1 lokal auf dem Laptop, Seed 4242, view 16, sim 4, creative, peaceful, spawnRadius 0, randomTickSpeed 0, online-mode false, enforce-secure-profile false. Jar per public/<zufall> + curl.exe, SHA256 gleich, public/ sofort geleert. Welt von einem Vanilla-r16-Warm-up vorgeneriert.
- Clients (alle 1280x720 bzw. 640x360 Fenster, Clientflaeche geprueft): (a) Vanilla pur (JVM 17, -Xmx2G, G1, Fancy-Defaults, vsync aus, maxFps 260, Ton aus); (b) mcplay GL (MCPLAY_GL=1 VSYNC=0 FPS=0); (c) mcplay Vulkan (MCPLAY_GFX=vk, Swapchain IMMEDIATE/MAILBOX via neuem MCPLAY_VSYNC=0, siehe unten). mcplay-Build: main 84e471c78 + dieser Branch (nur VK-Vsync-Schalter), als mcplay-bench.exe neben dem installierten Voxel Client.
- Pfad: am Spawn stehen, Settle 10 s nach Join, dann 1 Yaw-Umdrehung in 32 s (rcon tp alle 50 ms). PresentMon 2.6.0 (--process_id, ohne Admin), MsBetweenPresents, erste 10 s verworfen (22 s ausgewertet), numpy-lineare Perzentile, 1 % low = Mittel der langsamsten 1 %. Laeufe abwechselnd V -> GL -> VK je Wiederholung.
- Je Lauf zusaetzlich: Prozess-CPU-Zeit / Wandzeit im Messfenster (in % EINES Kerns; 8 logische Kerne), PeakWorkingSet + Private Bytes des Client-Prozesses (Vanilla: nur Client-JVM; Server-JVM Peak ~1.5 GB getrennt, nicht in den Zahlen), Startzeit.

## Ergebnis (Median der Laeufe; ms pro Frame)
| res | r | client | n | p50 | p95 | p99 | 1%low | avg fps | cpu %core | peak WS MB | private MB | t_window ms | t_join ms | p50 per run |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1280x720 | 8 | vanilla | 3 | 2.14 | 3.74 | 5.32 | 5.70 | 436 | 132 | 1809 | 2112 | 5736 | 9649 | 2.18 / 2.12 / 2.14 |
| 1280x720 | 8 | mcplaygl | 3 | 15.66 | 16.53 | 17.17 | 22.63 | 64 | 29 | 485 | 513 | 298 | 1072 | 15.66 / 15.59 / 15.69 |
| 1280x720 | 8 | mcplayvk | 3 | 15.64 | 16.51 | 17.08 | 20.28 | 64 | 9 | 624 | 667 | 306 | 1074 | 15.61 / 15.64 / 15.79 |
| 1280x720 | 16 | vanilla | 2 | 4.28 | 7.25 | 8.75 | 10.13 | 239 | 134 | 1884 | 2212 | 5678 | 10619 | 3.43 / 5.12 |
| 1280x720 | 16 | mcplaygl | 3 | 15.63 | 17.07 | 19.30 | 24.29 | 64 | 35 | 637 | 667 | 307 | 1090 | 15.67 / 15.56 / 15.63 |
| 1280x720 | 16 | mcplayvk | 3 | 15.61 | 16.61 | 17.91 | 21.29 | 64 | 28 | 776 | 819 | 303 | 1084 | 15.61 / 15.57 / 15.62 |
| 640x360 | 8 | vanilla | 2 | 1.73 | 3.83 | 6.73 | 7.74 | 506 | 166 | 1864 | 2134 | 6809 | 11956 | 1.97 / 1.50 |
| 640x360 | 8 | mcplaygl | 2 | 15.61 | 16.55 | 17.09 | 20.48 | 64 | 22 | 462 | 494 | 299 | 1104 | 15.60 / 15.61 |
| 640x360 | 8 | mcplayvk | 2 | 15.61 | 16.58 | 17.01 | 19.24 | 64 | 21 | 602 | 646 | 303 | 1124 | 15.61 / 15.60 |
| 640x360 | 16 | vanilla | 1 | 5.00 | 8.49 | 9.76 | 10.67 | 201 | 133 | 1720 | 2047 | 6027 | 10741 | 5.00 |
| 640x360 | 16 | mcplaygl | 1 | 15.59 | 16.79 | 17.46 | 18.85 | 64 | 42 | 613 | 647 | 306 | 1110 | 15.59 |
| 640x360 | 16 | mcplayvk | 1 | 15.62 | 16.96 | 21.62 | 43.17 | 63 | 30 | 754 | 798 | 303 | 1116 | 15.62 |


Spalten: n = Anzahl Laeufe; cpu = % eines Kerns; t_window = Prozessstart bis Fenster (Vanilla inkl. Classpath-Aufbau ~1 s); t_join = Prozessstart bis "joined the game" im Serverlog (NICHT "Welt sichtbar": Chunk-Ladezeit wurde nicht getrennt gemessen).

## Befunde
- Vanilla mit Hoechstleistung + ohne Hintergrundlast ist ~2x schneller als im Vorlauf (r8 1280x720: p50 3.99 -> ~2.1 ms). Vorlauf (Ausbalanciert, Firefox/Teams offen) ist damit NICHT mit dieser Reihe vergleichbar.
- mcplay GL UND Vulkan stehen in jeder Zelle bei p50 ~15.6 ms (= 64 Hz, exakt der Windows-Timer-Tick 15.625 ms), unabhaengig von Radius, Aufloesung und Backend. Gleichzeitig: world_draw nur ~1.4 ms (eigene Statistik), Prozess-CPU 7-42 % eines Kerns. => Der Frame wartet ~14 ms; die Zahlen sind eine Wartegrenze, KEINE Renderkosten. Das ist die Ursache der "bimodalen" r8-Werte im Vorlauf (je nach Timer-Zustand 7.5 oder 15.6 ms; hier mit Hoechstleistung/ohne Hintergrund durchgehend 15.6).
- Verdacht (nicht bewiesen): 2-ms-Socket-Recv-Timeout in der Frame-Schleife (mc_codec.wire_timeout(2)) wird auf den 15.6-ms-Tick aufgerundet; Windows 11 hat Timer-Aufloesung pro Prozess, mcplay ruft kein timeBeginPeriod. Roadmap r277.
- mcplay Vulkan war im ausgelieferten Build auf FIFO (60 fps) festgenagelt (zwei Probelaeufe: p50 16.67 ms, ein Present pro VSync). Neu: MCPLAY_VSYNC=0 waehlt IMMEDIATE/MAILBOX (Probelauf 15.63 ms = dieselbe 64-Hz-Wartegrenze wie GL).
- RAM: Vanilla-Client-JVM ~1.7-2.1 GB Peak WS (2.0-2.4 GB Private), mcplay GL ~0.46-0.64 GB, VK ~0.60-0.78 GB (siehe Tabelle).

## Nicht vergleichbar / Grenzen
- Gleiche Welt/Chunk-Inhalte/Server, aber Vanilla rendert Shader, Licht, Entities, Wetter, Himmel, Partikel, Biom-Tint, Fancy-Grafik; mcplay einen einfacheren Renderer (kein Ton bei beiden). Kein Aussagewert "schneller/langsamer" fuer Renderer-Qualitaet; KEIN "schneller als Vanilla".
- Ein Standpunkt (Spawn), eine Maschine, 22 s je Lauf, 2-3 Laeufe. 640x360 hat weniger Laeufe (Zeitlimit). Vanilla-Streuung (r16 3.4 vs 5.1 ms zwischen Laeufen) zeigt Rest-Rauschen (Hintergrunddienste, Boost/Thermik).
- Ladezeit "Start bis Welt sichtbar" nur als Start->Fenster und Start->Join; keine Chunk-fertig-Zeit. Fabric+Sodium nicht gemessen (Zeit).
- CPU-% aus Prozess-CPU-Zeit im Messfenster (Vanilla JVM: alle Threads, GC/Chunk-Meshing inklusive).

## Aufraeumen
- Server-JVM gestoppt, %TEMP%\mcvan, Public\mcvan_*.ps1, mcplay-bench.exe geloescht; Plan Ausbalanciert wieder aktiv, Hoechstleistung-Duplikat geloescht; PresentMon-ETW-Session gestoppt; Firefox neu gestartet (andere geschlossene Programme NICHT neu gestartet: Teams, OneNote, PowerPoint, Word, Notepad, FxSound, VirtualBox, certus).
- Hinweis: Mein Batch wurde beim Zeitlimit abgebrochen (PowerShell-Prozess beendet); der fremde winidle-Prozess 'big.exe' (Certus) laeuft weiter, nicht angefasst.

## v3 (23.10., after r277) -- NOT measured, blocked
- Laptop HOLO-LAPTOP-3 was on the Windows lock screen (LogonUI running, secure desktop): PresentMon sees no presents at all (even system-wide), so no CSV. Unlock needed; nothing can be done remotely.
- The Windows dist rebuild (`heavy bash tools/mc/dist.sh`) stayed queued for >20 min (server load 25-43 > heavy limit 24); the job was cancelled, rebuild it when load allows.
- Scripts prepared (merged): `mcvan_run.ps1 -Fps` (0 = uncapped MCPLAY_FPS=0, -1 = default 60 fps cap, N), `mcvan_batch.ps1` client spec `mcplay:gl:cap` / `mcplay:vk:cap`, `mcvan_agg.py` orders the capped cells.
- Resume: unlock laptop + disable lock timeout (Hoechstleistung plan: VIDEOCONLOCK 0), dist -> public/<random> -> curl.exe + SHA256 -> `setup.exe /S`, server jar `/root/.w5/jars/server-1.20.1.jar` -> %TEMP%\mcvan\srv, PresentMon 2.6.0 from GitHub, then batch (Secs 30 / Settle 20 keeps a 3-rep matrix of 720p r8/r16 + 360p r8 at ~40 min).

## v3 (24.10., after r277 fix, MEASURED) -- Laptop HOLO-LAPTOP-3, Hoechstleistung plan, 1280x720, uncapped, PresentMon
- Setup: main 304af0917 dist (mcplay-console.exe), `MCPLAY_FPS=0 MCPLAY_VSYNC=0` (GL: `MCPLAY_GL=1`, VK: `MCPLAY_GFX=vk`), Vanilla 1.20.1 maxFps unlimited + VSync off, same local server/world/spawn, view distance 8 and 16, 30 s window after 20 s settle, first 10 s dropped. Medians of 3 valid runs (Vanilla r8: runs n3-n5; n1 had no world loaded, n2 overlapped the power plan switch).
- Windows Defender flagged the unmodified dist exe as `Trojan:Win32/Bearfoos.B!ml` (false positive) and deleted it at first start; a copy padded with 64 KiB zero bytes ran. No admin rights for an exclusion.

| res | r | client | n | p50 ms | p95 | p99 | 1%low | avg fps | cpu %core | peak WS MB | private MB | t_window ms | t_join ms |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1280x720 | 8 | vanilla | 3 | 2.55 | 5.14 | 6.45 | 7.19 | 350 | 136 | 1637 | 1902 | 6078 | 10784 |
| 1280x720 | 8 | mcplay GL | 3 | 2.12 | 3.42 | 4.78 | 5.90 | 422 | 169 | 485 | 514 | 305 | 1087 |
| 1280x720 | 8 | mcplay VK | 3 | 1.84 | 2.97 | 4.08 | 6.95 | 495 | 108 | 620 | 657 | 310 | 1096 |
| 1280x720 | 16 | vanilla | 3 | 5.97 | 9.60 | 11.21 | 12.54 | 165 | 137 | 1664 | 1960 | 6368 | 11983 |
| 1280x720 | 16 | mcplay GL | 3 | 5.10 | 7.47 | 9.03 | 11.23 | 184 | 92 | 646 | 676 | 563 | 1095 |
| 1280x720 | 16 | mcplay VK | 3 | 5.18 | 6.47 | 7.33 | 7.84 | 188 | 102 | 771 | 809 | 312 | 1090 |

- p50 per run: Vanilla r8 2.05/2.61/2.55, GL r8 2.05/2.22/2.12, VK r8 1.78/1.84/1.89; Vanilla r16 5.95/5.97/6.17, GL r16 5.18/5.10/5.00, VK r16 5.18/5.14/5.26.
- Result: after the r277 fix mcplay is at or slightly under Vanilla frame time (r8 ~0.4-0.7 ms, r16 ~0.8-1.0 ms lower p50), 1% lows similar, RAM 2.5-3.4x lower, start-to-join ~10x faster. Spread between Vanilla runs (2.05-2.61) is as large as the r8 gap: treat r8 as "on par", r16 as "mcplay ~15% faster".
- Not comparable: Vanilla renders shaders/light/entities/weather/particles/fancy graphics, mcplay a simpler renderer; one spot, one machine, 30 s per run, n=3. 1%-low outliers (41 ms) in one GL r16 and one VK r16 run.
- 360p and capped (default 60 fps) variants not run (time).

## v4 (24.10., profile + 2x hunt) -- Laptop HOLO-LAPTOP-3, Hoechstleistung, 1280x720, uncapped, PresentMon
- New opt-in profile: `MCPLAY_PROFILE=1` (+ `MCPLAY_PROFILE_FROM=<s>`) prints at exit `mcplay: profile phase=<name> n p50_us p95_us mean_us` for frame, net_tick, draw_cpu, world_cpu (stream_frame), cull, submit, upload, stream_draw, present, plus per-frame counters draw_calls / tris / gl_calls / visible and the GL timer passes (terrain, sky, hud, present). Scripts pass it on (`mcvan_run.ps1`). Lib: `lib/mc/mc_prof.fi`.
- Runs: n=1-2 per cell (time), sweep 30 s after 20 s settle, same world as v3. Vanilla re-run in the same session (r8 2.44/2.51, r16 5.74/5.97 -> v3 numbers reproduce; a 4.15 ms r16 value was a world-generation warm-up and is invalid).

| config | PresentMon p50 ms | note |
|---|---|---|
| Vanilla r8 | 2.44 / 2.51 | n=2 |
| Vanilla r16 | 5.74 / 5.97 | n=2 |
| mcplay GL r8 | 2.04 / 2.05 | |
| mcplay VK r8 | 1.74 / 1.76 | |
| mcplay GL r16 | 5.02 / 4.97 | |
| mcplay VK r16 | 5.08 | |
| GL view 8 + LOD 16 (= Vanilla r16 distance) | 2.26 | `MCPLAY_LOD=16` |
| GL view 6 + LOD 16 | 1.88 | |
| GL view 4 + LOD 16 | 1.36 | |
| GL view 4 + LOD 8 (= Vanilla r8 distance) | 1.30 | |
| GL r16, `MCPLAY_FACECULL=0` | 5.53 | terrain GPU 2.7 -> 7.2 ms |

- Profile GL r8 (median per frame, us): frame 2040 = net_tick 350 + draw_cpu 1370 (world_cpu 1180 = cull 345 + submit 190 + draw loop 630) + present 270. GPU terrain 0.87 ms (sky 0.1, rest < 0.05). 643 visible, 537 drawn sections, 277k submitted tris.
- Profile GL r16: frame 5040 = world_cpu 3900 (cull 1890 + submit 280 + draw loop 1670) + net_tick 390 + present 260. GPU terrain 2.7 ms. 3865 visible, 2184 drawn, 1.18M tris.
- Verdict: CPU-bound on the render thread (stream_frame: cull BFS, schedule scan, per-section draw collection), identical on GL and VK (VK r8 world_cpu 1050, r16 4240). GPU is ~40-55 % of the frame. Fixed overhead ~0.8 ms even with almost nothing drawn (view 4 + LOD 8: world_cpu 520 of 1300).
- Tried and dropped: cull micro-opts (closed-form pair bit, Ciura gap sort, memset of the window, removing the known-count pass): r16 cull 1894 -> 1939 us = no gain, reverted. Face-direction buckets confirmed (off = slower).
- Result: the renderer alone is NOT 2x Vanilla (r8 1.2x, r16 1.1-1.2x). With LOD at equal view distance mcplay is 2.5-2.6x faster at the Vanilla r16 distance (view 8 + LOD 16: 2.26 vs 5.74-5.97 ms) and ~1.9x at the r8 distance (view 4 + LOD 8: 1.30 vs 2.44-2.51). The far terrain is then simplified LOD, and mcplay still has a simpler renderer than Vanilla (no shaders/entities/particles/weather), so this is not like-for-like.
- Next levers (CPU): sub-profile net_tick (350 us) and the draw loop (1.2 us per drawn section), cache cull result across frames / thread the cull + draw-list build, reduce the 0.8 ms fixed per-frame overhead. GPU at r16 (2.7 ms, 1.18M tris) needs fewer triangles (LOD default for r > 8).
