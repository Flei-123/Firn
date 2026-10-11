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
* Kein Compare-and-swap in Firn (`__atomic_add`, `__atomic_swap` only) -> **kein Work-Stealing (Chase-Lev) moeglich**. Ersatz: eine Queue unter Futex-Lock,
  Chunk-Claiming (naechster Chunk der vordersten Batch), Aufrufer hilft beim Warten. Dynamische Lastverteilung wie beim Stehlen, Kosten s.u.
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
