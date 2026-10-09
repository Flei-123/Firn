# Thread-Tabelle: Limit, Kosten, join/detach

Stand: 2026-10-09 (Runde T1). Code: `lib/gc/gc.fi` (Abschnitt „The thread table"),
Test: `tests/2395_thread_table_many.fi`, Messung: `tools/thread/table_bench.fi`.

## 1. Was `THREAD_MAX` ist (belegt)

| Name | Wo | Bedeutung |
|---|---|---|
| `THREAD_MAX` | `gc.fi`, Abschnitt Thread-Tabelle | Anzahl Einträge der Tabelle (Eintrag 0 = Hauptthread). Vorher 64 → 63 Worker. Jetzt 1024 → 1023 Worker. |
| `S_THREAD_MAX` | Zustandsblock, Offset 1968 | nur eine Kopie von `THREAD_MAX`, wird in `thread_init` geschrieben und **von niemandem gelesen**. |
| `S_THREAD_HI` | Zustandsblock, Offset 2152 (neu) | High-Water-Mark: Anzahl je vergebener Einträge. |
| `FT_BYTES` | `gc.fi` | 512 Byte pro Eintrag (Thread-Block/TCB). |

Die 64 stammt aus Runde 49 (`244550c16`, 2026-08-19, damals `FADEN_MAX`); im
Quelltext und in den Docs steht **keine Begründung**, es war eine runde Zahl.

### Warum eine feste Tabelle (und nicht „wachsend")

* Die Adresse eines Eintrags ist die `fs`-Basis des Threads (`fs:0` zeigt auf
  sich selbst, `FT_SELF`; Windows: TLS-Slot mit dem Zeiger). Ein Eintrag kann
  nie umziehen → kein `realloc`, nur eine Kette von Tabellen oder ein
  **reservierter Adressbereich**.
* Der reservierte Bereich ist hier billig: `thread_init` mappt
  `THREAD_MAX * 512` Byte (32 KiB vorher, 512 KiB jetzt) anonym. Seiten werden
  erst beim ersten Schreiben echt; ungenutzte Einträge kosten nur Adressraum.

## 2. Was beim Anheben bricht (geprüft)

* **Compiler / Intrinsics:** nichts. `compiler/src/thread.rs` kennt die
  Tabellengröße nicht (`__thread_start`/`__thread_self` nur über Stapel, ctid
  und `fs:0`). Kein Platzhalter in `compiler/src/gc.rs` (Zustandsblock:
  nur neue Wörter in freien Offsets).
* **firnc1-Spiegel:** `lib/firnc1/gctext.fi` ist aus `gc.fi` erzeugt
  (`tools/gen_gctext.sh`), neu erzeugt; kein eigener Konstantenspiegel.
* **Windows:** das Limit „64 TLS-Slots" (W-T3) ist **ein** Slot pro Prozess
  für den Zeiger auf den Thread-Block, unabhängig von der Tabellengröße.
  Pro Thread bleibt der gemappte Stapel (W-T2): 1023 Threads = 1 GiB
  Adressraum/Commit bei 1 MiB Stapel (`thread_stack_set` verkleinert ihn).
* **aarch64/Android:** dieselbe `gc.fi`; nichts Tabellenabhängiges im Seam.
* **Kosten pro GC-Zyklus:** drei Schleifen liefen über **alle** `THREAD_MAX`
  Einträge (Welt anhalten `__gc_stw_an`, Wurzelscan `__gc_threads_scan`,
  lokale Listen `__thread_lists_all_back`) und `__thread_slot_new` linear.
  Bei 1024 Einträgen à 512 B wäre das eine Cache-Zeile pro Eintrag pro Zyklus
  auch ohne einen einzigen Thread → die Schleifen laufen jetzt nur bis
  `S_THREAD_HI`. Ohne Threads (`HI` = 1) ist der Aufwand wie vorher.
* **Kernel-Grenzen (Maschine):** `vm.max_map_count` 1 048 576, `threads-max`
  595 716; pro Thread 2 Mappings (Stapel + Schutzseite). Für andere Hosts
  (cgroup `pids.max`, `ulimit -u`, Container) ist `thread_start` = 0 mit
  `thread_error()` = 3 (clone) bzw. 2 (Stapel).

## 3. Entscheidung: 1024

Begründung: Tabelle kostet 512 KiB Adressraum und ab dem ersten Thread nur
`HI`-viele Einträge pro Zyklus; die echte Grenze ist der Stapel (1 MiB →
~1 GiB bei voller Tabelle). Ein dynamischer Wert (Konstante im Programm
setzen) würde einen Aufruf vor `thread_init` brauchen; er ist als
offener Punkt notiert, weil 1023 für den Thread-pro-Verbindung-Stil reicht und
darüber ohnehin `epoll`/Async (`std.async`) gedacht ist.

## 4. Messung (`tools/thread/table_bench.fi`, release-fast)

Maschine: EPYC 7571, **Last während der Messung ca. 320** (Host stark
belegt – Zahlen sind relativ zueinander zu lesen, je 5 Läufe, Median).
N Threads geparkt (blockiert in `thread_sleep`), 20 000 lebende Cells im Heap,
200 erzwungene `gc_collect()`:

| Compiler | N | `gc_collect` Mittel (µs) | `thread_start` (µs) |
|---|---|---|---|
| alt (Tabelle 64) | 0 | 698 | – |
| neu (1024) | 0 | 634 | – |
| alt (64) | 60 | 668 | 63 |
| neu (1024) | 60 | 761 (Streuung 610–838) | 61 |
| neu | 250 | 965 | 73 |
| neu | 500 | 1406 | 63 |
| neu | 1000 | 2187 | 57 |

Lesart: Bei gleich vielen lebenden Threads (0 und 60) ist **kein Unterschied
zwischen Tabelle 64 und 1024 messbar** (Streuung größer als der Unterschied);
die Pause wächst danach linear mit der Zahl **lebender** Threads
(~1,5 µs pro geparktem Thread, das ist der Stapelscan). `thread_start` bleibt
bei ~55–70 µs (clone + mmap + mprotect), unabhängig von der Tabellengröße.

## 5. `thread_wait` ist zugleich `free` (R2)

Ein beendeter Thread (`Z_DEAD`) behält Eintrag **und** Stapel, bis jemand
`thread_wait` für ihn aufruft. Wer Threads startet und nie wartet, hat nach
`thread_limit()` Starts keine Einträge mehr (das hat `demos/mcserver` in
Runde 76 bei der 64. Verbindung getroffen).

Jetzt klar benannt, rückwärtskompatibel:

| Aufruf | Bedeutung |
|---|---|
| `thread_start(kind, arg)` | Handle 1..1023 oder **0** (Tabelle voll / kein Stapel / clone fehlgeschlagen) – nie ein Absturz. |
| `thread_error()` | Grund des letzten 0 dieses Threads: 1 Tabelle voll, 2 Stapel, 3 clone. |
| `thread_limit()` | maximale Zahl gleichzeitiger Worker (1023). |
| `thread_wait(h)` | **join + free**, genau einmal pro Handle. Gibt das Ergebnis zurück. |
| `thread_detach(h)` | **detach**: nie joinen; Eintrag + Stapel werden vom nächsten `thread_start` eingesammelt, sobald der Thread wirklich zu Ende ist (TID-Wort 0). Ergebnis verworfen. |

Scharfe Kanten, die bleiben (dokumentiert im Code an `thread_wait`):
Handle ist nur Index → nach `thread_wait`/Recycling kann derselbe Index einen
**neuen** Thread meinen; zwei Threads dürfen nicht dasselbe Handle joinen;
nach `thread_detach` das Handle nicht mehr benutzen. Ungültige, freie oder
detached Handles geben bei `thread_wait` 0 zurück und ändern nichts.
Eine Generationszahl im Handle wäre die saubere Lösung, ändert aber die
Handle-Werte (offener Punkt).

## 6. Tests

* `tests/2395_thread_table_many.fi` (4 Optimierungsstufen; Wine; qemu-aarch64):
  200 gleichzeitige Threads mit GC-Ketten unter erzwungenen Collections,
  Tabelle füllen bis 0 (genau `thread_limit()` Worker, `thread_error()` = 1,
  danach wieder frei), 3000 detached Threads, Fehlbedienung.
* `tools/thread/stress.sh` mit `STRESS_THREADS=100` (Array auf 1024 erweitert).
* `tools/mcserver/run.sh` Gegenprobe A (ohne `reap()` stirbt der Server jetzt
  bei Verbindung 1024 statt 64); `MAX_CONN` der Demos 63 → 1000.

## 7. Offen

* Prozessende bei laufenden Threads: `main` endet mit `exit(2)` statt
  `exit_group(2)` → Prozess hängt, solange ein nicht gejointer Thread lebt
  (vorher schon so, belegt mit einem 12-Zeilen-Programm).
* Handles mit Generationszahl; Tabellengröße zur Laufzeit wählbar.
* Variante B (`gc_set_local(1)`) unter vielen Threads – siehe Roadmap.
