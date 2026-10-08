# SCRIPTS — Skripte in Firn: Shebang, `firn-run`, späterer `firn`-Treiber (r316)

Stand 08.10.2026. Vorschlag **und** das, was davon gebaut ist (alles Gemessene wurde auf dieser Maschine gemessen, siehe „Messung“).

## 1. Was jetzt geht

```sh
tools/script_port/firn-run check.fi arg1 "arg 2"     # übersetzen (einmal), starten
./check.fi arg1                                       # mit  #!/usr/bin/env firn-run  in Zeile 1 und chmod +x
tools/script_port/firn-run --no-cache x.fi            # immer neu übersetzen, nichts speichern
tools/script_port/firn-run --clean                    # Cache leeren
```

- **`#!` in Zeile 1**: beide Lexer (`compiler/src/lexer.rs`, `lib/firnc1/lexer.fi`) und der Formatter (`tools/fmt/fmt.fi`)
  behandeln `#!…` am **Dateianfang (Offset 0)** wie einen Zeilenkommentar. Zeilennummern bleiben stabil (der Zeilenumbruch bleibt).
  *Warum in jeder Datei, nicht nur in der Wurzel:* der Lexer kennt die Wurzel nicht (er bekommt nur den Text einer Datei), und ein
  Modul, das auch als Skript lauffähig ist, soll keine Sonderbehandlung brauchen. Ein `#!` an anderer Stelle (Zeile 2, eingerückt) bleibt der
  alte Fehler `expected '[' after '#', found '!'` (`tests/neg/shebang_not_first.fi`).
- **Compiler-Option `--deps-out=<datei>`** (neu, klein): schreibt die absoluten Pfade aller gelesenen Quelldateien, Wurzel zuerst. `firn-run` nimmt sie
  für den Cache-Schlüssel. Die im Compiler eingebettete GC-Runtime erscheint darin als Pseudo-Pfad `lib/gc/gc.fi` (keine Datei); `firn-run` verwirft Einträge,
  die keine Dateien sind (sie stecken im Hash des Compilers). Sauberer wäre, sie im Compiler wegzulassen (S, offen).

## 2. Cache (`tools/script_port/firn-run`)

| | |
|---|---|
| Ort | `$FIRN_CACHE`, sonst `$XDG_CACHE_HOME/firn-run`, sonst `~/.cache/firn-run` |
| `obj/<H>` | die fertige ELF-Datei (Stufe `dev-fast`, `FIRN_OPT` ändert sie). `H` = SHA-256 (40 Hex) über: Optimierungsstufe, `FIRNLIB`, Compiler-Pfad **und -Inhalt**, **Inhalt jeder** von `firnc` gelesenen Quelldatei (Skript, Nachbar-Module, `FIRNLIB`-Module) |
| `idx/<Pfad mit ,>@<Stufe>` | Schnellindex je Skript: `H`, Liste der Quelldateien, ihre `stat`-Signatur |
| Treffer (warm) | kein Hashing, **kein Prozess außer dem Programm selbst**: die Shell prüft nur `idx -nt <jede Datei>` (make-Regel) und führt `exec obj/<H>` aus |
| Änderung nur am Zeitstempel (`touch`, `git checkout`) | Inhalts-Hash neu berechnet, gleiche `H` → Binary wiederverwendet, Index erneuert |
| Schreiben | `mktemp -d` im Cache, dann `mv` (rename) → atomar; parallele Kaltstarts bauen im schlimmsten Fall mehrfach, jeder Prozess hat ein richtiges Binary; kein Lock |
| Quelle während des Baus geändert | wird erkannt (`find -newer`), das Binary läuft einmalig ungecacht |
| Aufräumen | Temp-Ordner per `trap`; Einträge, die länger als `FIRN_CACHE_DAYS` (30) nicht benutzt wurden, fliegen; bei mehr als `FIRN_CACHE_MAX` MiB (512) zuerst die am längsten ungenutzten; veraltete `tmp.*` (> 2 h) |
| Exit-Codes | Programm: unverändert (`exec`). `firn-run` selbst: **125** (Aufruf-/Compile-Fehler, Compilertext auf stderr) |
| Argumente / stdin / stdout | unverändert (`exec`, Argumente gequotet durchgereicht; `argv[0]` ist der Cache-Pfad, nicht das Skript) |
| Strenger Modus | `FIRN_STRICT=1`: Inode, Größe, Nanosekunden-mtime müssen exakt stimmen (fängt eine **ältere** Ersatzdatei, `tar -x`/`cp -p`), kostet einen Prozess mehr |
| Grenzen | Linux (`stat -c`); eine **neue** Datei, die einen vorhandenen Import überdeckt, wird erst bei Änderung des Skripts oder `--clean` bemerkt; Bau-Kommentar/Warnungen erscheinen nur beim Kaltlauf |

Tests: `bash tools/script_port/test_firn_run.sh` (Sektion 101 in `test.sh`): Treffer, Änderung Skript / lokales Modul / `FIRNLIB`-Modul,
`touch`, 8 parallele Kaltstarts, Compile-Fehler, Argumente mit Leerzeichen/leer/Anführungszeichen/Stern, stdin, `--no-cache`, `--clean`,
Altersgrenze, Größengrenze, Strenger Modus.

## 3. Messung

`bash tools/script_port/bench_firn_run.sh` (100 Starts je Block, 3 Blöcke abwechselnd, Median-Block; Maschine **unter Last**, Load 15–19 auf 20 Kernen, EPYC 7571;
zwei komplette Läufe am 08.10.2026):

| Start von | Lauf 1 | Lauf 2 |
|---|---:|---:|
| `/bin/true` | 1,25 ms | 1,19 ms |
| `sh -c :` (Shell-Start) | 1,39 ms | 1,41 ms |
| fertiges Firn-Binary direkt (`return 0`) | 0,78 ms | 0,77 ms |
| **`firn-run x.fi`** (warm) | **2,51 ms** | **2,20 ms** |
| `firn-run x.fi`, `FIRN_STRICT=1` | 4,59 ms | 4,01 ms |
| **`./x.fi`** (`#!/usr/bin/env firn-run`, warm) | **3,32 ms** | **3,01 ms** |
| `python3 -c pass` | 26,6 ms | 24,8 ms |
| `python3 x.py` (`import sys; sys.exit(0)`) | 25,7 ms | 25,8 ms |

- **Overhead warm** gegenüber dem direkten Binary: **1,4–1,7 ms** (`firn-run x.fi`), **2,2–2,5 ms** über `env`/Shebang, **3,2–3,8 ms** im strengen Modus.
  Ziel < 3 ms: erreicht für beide normalen Aufrufarten (unter Last; auf ruhiger Maschine erwartet kleiner, **nicht gemessen**).
  Der Prototyp lag bei ~6 ms Overhead (22 ms gegen 16 ms), davon `firnc --version` + `sha256sum` — beides entfällt auf dem warmen Pfad
  (make-Regel mit eingebauten `-nt`-Tests statt `stat`-Prozess; `stat` allein kostete im Test ~1,8 ms).
- **Firn-Skript gegen Python-Skript (Startzeit)**: `./x.fi` 3,0–3,3 ms gegen `python3 x.py` 25,7–25,8 ms, also **etwa 8× schneller beim Start**
  (reiner Start eines leeren Skripts; Python mit den üblichen Importen csv/json/subprocess/… ~50 ms, siehe `docs/SCRIPT-REPLACEMENT.md` §4).
- **Kalt** (leerer Cache, Mittel aus 5 Bauten): Skript ohne Imports **62–65 ms**; Skript mit 6 `std`-Imports (`rt str process fs text vec`)
  **417 ms** (`dev-fast`, der Standard), **654 ms** (`release-fast`), **709 ms** (`release-safe`). Deshalb ist `dev-fast` der Standard von `firn-run`;
  rechenintensive Skripte: `FIRN_OPT=release-fast` (die Laufzeit-Zahlen dazu: `bench/RESULTS.md`).
- Gemessen wird Wandzeit der ganzen Schleife durch `date +%s%N`; die Streuung wegen der Last ist **nicht** statistisch ausgewertet (Unterschied der beiden Läufe ≈ 10–15 %).

## 4. Vorschlag: der echte `firn`-Treiber (später)

`firn-run` ist ein Shell-Skript und bleibt Prototyp. Ein kleines Firn-Programm `firn` (Subkommandos) würde dieselbe Cache-Logik ohne
Shell-Start (~1 ms) und ohne `stat`/`sha256sum`-Prozesse haben:

| Aufruf | Bedeutung |
|---|---|
| `firn run x.fi [args]` | wie `firn-run`; `#!/usr/bin/env firn-run` bliebe als Alias (oder `#!/usr/bin/env -S firn run`) |
| `firn test x.fi` | `firnc --test` mit Cache, Ausgabe nach Wahl `--format=tap` |
| `firn fmt [-c] dateien` | `firnfmt` |
| `firn check x.fi` | nur Parser + Typprüfung (`--emit=…`), kein `as`/`ld` |
| `firn cache clean/list` | Cache verwalten |

Nutzen gegen das Shell-Skript: Start ohne `sh` (spart bei Last ~1–2 ms), Hash- und Stat-Arbeit im selben Prozess, Windows-Variante, Sperre/Doppelbau-Vermeidung
über Datei-Lock, Cache-Statistik. Aufwand: **M** (Treiber ~400 Zeilen Firn; `process`/`fs`/`hash` gibt es in `lib/std`).

## 5. Offen

- Der Cache-Schlüssel kennt **keine** neu hinzugekommene überdeckende Datei (siehe Grenzen). Behebung: `firnc --deps-out` um die Liste der *geprüften, nicht gefundenen* Pfade erweitern (S).
- Kaltstart ohne Cache (frischer CI-Checkout): jedes Skript ~0,4 s CPU; Abhilfe `FIRN_CACHE` im CI persistieren oder Skripte in **ein** Binary mit Subkommandos bauen.
- `firn-run` unter Windows/macOS: nicht gebaut (`stat -c`, `find -newer`, Shell).
- Hervorhebung (`lib/highlight`) und LSP kennen die Shebang-Zeile noch nicht gesondert (der LSP benutzt den Lexer, ist also bereits richtig; die Hervorhebung: **ungeprüft**).
- Andere Test-Läufer (`tools/aarch64`, `tools/windows`, `tools/optlevels`) lesen die Erwartung aus Zeile 1: die Shebang-Tests tragen sie deshalb **in** der Shebang-Zeile
  (`#!/usr/bin/env firn-run  // expect_exit: 7`) — das Ende der Zeile ist ein Kommentar.
- `argv[0]` ist der Cache-Pfad: ein Skript, das seinen eigenen Namen ausgeben will, braucht eine Umgebungsvariable (`FIRN_SCRIPT`) — nicht gebaut.
