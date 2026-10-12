# lib/jobs -- thread pool, parallel_for, wait groups, futures

Code: `lib/jobs/jobs.fi`. Tests: `tests/2764_jobs.fi` (A-J: 1M-item loop exact, heavy items,
200 futures, two groups in flight, nested waits on 2 workers, `create(0)`, GC allocation in
chunks while collections run, 3000 stress loops, 40x create/destroy). Measurement: `tools/jobs/scale.fi`.

## API (import jobs.jobs; wire `__thread_work` once, see the file head)
| call | meaning |
|---|---|
| `create(workers)` / `destroy(&j)` | 0..255 workers; the caller helps while it waits, so T threads of work = T-1 workers |
| `parallel_for(&j, begin, end, chunk, body, ctx)` | `body(ctx, lo, hi)` for disjoint chunks, returns when all are done |
| `parallel_each(&j, n, body, ctx)` | one item per chunk |
| `group_new / group_go / group_wait / group_free` | wait group: many loops in flight, one wait |
| `spawn(&j, f, arg)` / `join(&j, t)` / `task_done(t)` | future with a `u64` result; `join` frees the handle |

## Befund R1 / R2 (FIRN-GAPS)
* **R1 (63 Threads) ist erledigt**: `THREAD_MAX` = 1024 -> 1023 Threads (`docs/THREAD_TABLE.md`, `tests/2395`). `create` nimmt bis 255 Worker.
* **R2 (`thread_wait` == free)**: bestaetigt, aber entschaerft. `thread_wait` gibt den Eintrag frei; ein zweites `thread_wait` auf denselben Handle
  liefert 0, ohne zu blockieren (`tests/2395` Teil D); `thread_detach` + `thread_wait` ebenso. Gefahr bleibt: Handle nach dem Join weiterbenutzen,
  wenn der Eintrag schon neu vergeben ist. `jobs.join` kapselt das: das Handle ist ein eigener Record, `join` genau einmal, danach ungueltig.
* **Korrektur 12.10.2026: Firn HAT Compare-and-swap**: `__atomic_swap(p, expected, new) -> old` (compiler/src/thread.rs, x86-64 + aarch64). Was fehlt, ist nicht das CAS,
  sondern die Speicher-Freigabe fuer ein Chase-Lev-Deque / eine verkettete Lock-free-Queue (Batch-Records, die ein Thread liest, waehrend ein anderer sie frei gibt: Hazard
  Pointer oder Epochen). Default bleibt darum die Queue unter Futex-Lock mit Chunk-Claiming (Aufrufer hilft beim Warten, Kosten s.u.); `create_lf` setzt einen
  **lock-freien Ring** davor (J8, unten).
* Futex-Woerter sind 32 Bit: Zaehler < 2^32 (dokumentiert, J6). Windows: `WaitOnAddress` ueber die Naht.

## Messung (`tools/jobs/scale.fi`, release-fast, EPYC 7571, 20 Kerne, Last 7-10 durch andere Laeufe, Median von 7)
Last: 2048 Items zu je ~70 us (xorshift), Chunk 8; Ergebnis-Pruefsumme == sequentiell in allen Laeufen.

| Threads T | Zeit ms | Speed-up | Wirkungsgrad |
|---|---|---|---|
| 1 (create(0), inline) | 151.2 | 1.00 | 100 % |
| 2 | 89.2 | 1.69 | 85 % |
| 4 | 44.8 | 3.38 | 84 % |
| 8 | 22.8 | 6.63 | 83 % |
| 12 | 23.1 | 6.5 | 55 % (fremde Last auf dem Host) |
| 16 | 18.5 | 8.2 | 51 % |
| 20 | 13.8 | 11.0 | 55 % |

Queue-Kosten (T=8): 329 ns pro Item bei Chunk 1 (Lock-Contention), **5 ns pro Item bei Chunk 64**; Rundlauf eines fast leeren `parallel_for`
(Wecken der Workers + Rueckweckung) 10 us bei T=8, 38 us bei T=20; `spawn`+`join` 3 us. Regel: Chunk so waehlen, dass er >= 20 us Arbeit hat.
Ab T=12 begrenzt vor allem die fremde Host-Last (heavy-Slots, Load 7-10), nicht die Pool-Logik; 8-Thread-Wirkungsgrad 83 %.

## Offen
* Kein Stehlen / keine Affinitaet; Prioritaeten fehlen (FIFO). Fuer Meshing reicht es; falls Chunk < 1 us noetig: Batching im Aufrufer.
* Wartende Worker schlafen auf EINEM Futex (Wake-N); bei 255 Workern waere ein Futex je Gruppe besser.

## Lock-freier Ring (`jobs.create_lf`, J8, 12.10.2026)
Gleiche API, gleiche Ergebnisse; nur der Entnahmeweg aendert sich. Ein beschraenkter MPMC-Ring (Vyukov: ein Sequenzwort je Slot, Positionen per `__atomic_swap`-CAS,
1024 Slots, ein einziger Block, Slots werden nie frei gegeben -> keine Reclamation-Frage) nimmt **Chunk-Descriptoren** auf. `group_go` legt je Chunk einen Descriptor
ab (Batch mit mehr als 512 Chunks oder voller Ring: der Rest geht wie bisher in die gelockte Queue), `spawn` einen Task. Wer Arbeit sucht, probiert zuerst den Ring (kein
Lock, kein Futex), dann die gelockte Queue. Sequenzwoerter werden mit gelocktem Add gelesen/geschrieben (Vollbarriere, auch auf aarch64; aarch64 nicht getestet).

Tests: `tests/2764_jobs.fi` (unveraendert, gruen) und `tests/2791_jobs_lockfree.fi` (gleiche Faelle A-J mit `create_lf` + K: Batch > 512 Chunks, Ueberlauf in die gelockte Queue,
3000 Futures durch den Ring), beide Exit 0.

Messung (`tools/jobs/scale.fi T 5 LF`, release-fast, Host-Last 4-10 durch andere Laeufe, 2 Durchgaenge, Median von 5):

| | T=4 heavy ms | T=8 heavy ms | wake us (T=8) | spawn+join us | tiny ns (Chunk 1, T=8) |
|---|---|---|---|---|---|
| gelockt | 46 / 45 | 30 / 23 | 12 / 12 | 3-4 | 343 / 434 |
| lock-frei (Ring) | 43 / 40 | 25 / 23 | 13 / 23 | 3-5 | 494 / 425 |

**Ehrlich: kein messbarer Gewinn.** Die schweren Laeufe haben nur 256 Chunks je Aufruf und die Queue-Kosten sind dort < 1 % der Zeit; `tiny`/`chunk64` haben > 512 Chunks und
gehen ohnehin durch die gelockte Queue. Ein Vorteil des Rings waere erst bei sehr vielen kleinen Batches (<= 512 Chunks) auf vielen Kernen zu erwarten; nicht gemessen.
Default bleibt `create`. Offen: Work-Stealing-Deque (braucht Reclamation), Ring-Messung mit vielen kleinen Batches auf >= 16 Kernen, aarch64.
