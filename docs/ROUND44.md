# Round 44 — the build-up phase of the collector defused

Assignment: **no pause above 4 ms any more, over the whole run including
the build-up**, aiming for ≤ 2 ms, at a throughput loss of at most 10 %.

Result up front: the longest interruption fell from **11,82 ms to 0,45 ms**
(5 s run, quiet machine) resp. **0,62 ms of pure compute time** in the
10-minute endurance run; throughput lost **2 %** with a small and **0 %**
with a large live set. The reason for the old threshold `INKR_AB = 8 MiB`
was measured correctly but attributed wrongly — the actual cause sat in
the sliced sweep and is now fixed.

## 1. Measure first: what really happens in the build-up phase

`tools/gc_meas/pause_big.fi` could not answer the question: it calls
`gc_hist_reset()` **after** the build-up, so the histogram only shows
continuous operation. New:

* **`tools/gc_meas/build.fi`** — zeroes nothing. Phase 0 is the balance
  after the build-up, phase 1 the balance over the WHOLE run; continuous
  operation is the difference. Every collection during the build-up is
  reported individually (with the heap size and node count at that point).
* **`tools/gc_meas/throughput.fi`** — fixed WORK, measured TIME (the pause
  runs measure the other way round and are no good for throughput).
* **`tools/gc_meas/ab.fi`** — A/B in the same process, see §4.

Plus four additions in the runtime (`lib/gc/gc.fi`):

* **Histogram type 7 = the whole interruption at one allocation site.**
  Types 0–5 measure individual sections (start, mark, sweep, end) and
  understate as soon as several sections run in sequence in ONE call.
  A frame does not see the slice but the sum. `gc_stop_max()`
  returns the maximum of this type.
* **Second clock `gc_set_cpu_uhr(1)`** — the same interruption a second
  time, measured with `CLOCK_THREAD_CPUTIME_ID` (`gc_chist(fach)`,
  `gc_cpu_stop_max()`). If the wall clock deviates upwards, it was the
  machine; if both agree, it was the collector. See §8 — without this clock
  this round would have run into the same wrong finding as round 40.
* **`gc_diag(0..6)`** — new chunks while idle / while marking / while
  sweeping, mark slices, sweep slices, gray allocations, allocations total.
* **`gc_set_inkr_ab(n)` / `gc_inkr_ab()`** — the switchover threshold
  settable at runtime. That is the basis of the A/B measurement in §4.

### Starting measurement (state `main` f48e51c, `INKR_AB` = 8 MiB)

`build.fi`, 120 000 live text nodes, build-up up to a heap of 9,5 MiB:

| Collection | Pause | heap at that point | nodes at that point | Kind |
|---|---|---|---|---|
| 1 | **0,88 ms** | 1,5 MiB | 13 106 | FULL (stop-the-world) |
| 2 | **2,90 ms** | 2,6 MiB | 26 214 | FULL |
| 3 | **11,81 ms** | 4,7 MiB | 52 430 | FULL |
| 4 | 0,14 ms | 8,9 MiB | 105 278 | incremental |

Four repetitions: 10,80 / 10,88 / 11,15 / 11,84 ms for run 3 — the value
is stable, not an outlier. The build-up phase lasts 65–77 ms; more than a
third of that is a single stop-the-world run.

Over the whole run (build-up + 5 s of continuous operation), histogram type
7 (whole interruption, bucket k = [2^(k−1) µs, 2^k µs)):

```
Fach  2   3   4   5   6      7    8   9  10  12  14
Zahl  1   4   6  10  54  44390  345  76   2   1   1
                                            ^   ^
                               2,05-4,10 ms |   | 8,19-16,38 ms
```

Two interruptions above 2 ms, one of them above 8 ms. `gc_stop_max` =
**11,82 ms**. That is exactly the finding that had to be eliminated.

## 2. The justification of `INKR_AB` re-examined — and refuted

`docs/ROUND38.md` justifies the threshold like this: purely incremental
cost −13 to −26 % throughput with a small heap, „pure loss"; the hybrid
costs only −8,7 % for the „phase check per allocation".

**First finding, from the source text:** the phase check runs at EVERY
allocation anyway, independently of the threshold. `__gc_alloc_raw` checks
`S_PHASE == 0 && S_SEIT >= S_GRENZE` and after that `S_PHASE != 0` — both
unconditionally. The threshold does not save it. The −8,7 % (stage 2 →
hybrid) are therefore already paid and are no argument for the threshold.
What remained to be explained were the additional −8,1 % (hybrid → purely
incremental, 7 110 000 → 6 533 000 cycles in round 38).

**Re-measurement** (`durchsatz.fi`, 2000 live nodes, 200 000 rounds,
heap 1,5 MiB — the regime that the threshold is supposed to protect):

| Variant | Work | Heap at end | Collection time | RSS |
|---|---|---|---|---|
| `INKR_AB` = 8 MiB (atomic) | 121 ms | 1,57 MiB | 26,0 ms | 1612 KiB |
| `INKR_AB` = 0 (incremental) | 163 ms (**−26 %**) | 3,15 MiB | 61,0 ms | 3148 KiB |

The loss is therefore reproducible. But the heap **doubles** in the
process, and the collection time rises by more than a factor of two. The
counters `gc_diag` show where from:

| Diagnostic | atomic | incremental |
|---|---|---|
| new chunks while idle | 119 | 6 |
| new chunks while marking | 0 | 0 |
| new chunks **while sweeping** | 0 | **113** |
| mark slices | 0 | 429 |
| sweep slices | 0 | 614 |
| gray allocations | 0 | 949 of 1 402 001 (0,07 %) |

**The cause is not the incremental method.** The suspected running costs —
gray allocation and the active insertion barrier — affect 0,07 % of all
allocations and are measurably insignificant. The costs came
from `__gc_sweep_init()`: it cleared **all free lists in one stroke**. The
sweep runs in slices, however, and between the slices the program keeps
allocating — so every allocation until the end of the sweep hit an empty
list and had to mmap a fresh chunk. 113 of 119 new chunks
came about that way. The heap grows, and because `__gc_block_von` searches
the chunk list linearly, that makes **every marking more expensive** —
hence the doubled collection time.

The counter-check that nails down the attribution: with
`SCHEIBE_SWEEP = 1 000 000` (the whole sweep in ONE slice, otherwise
unchanged incremental) the work fell from 163 to 141 ms, the heap from 3,15
to 2,36 MiB, the collection time from 61 to 47 ms. The rest sat in the
transition marking → sweeping: `sweep_init` clears the lists and the step
returns, so the triggering allocation is guaranteed to find empty lists.

## 3. The change

Three interventions in `lib/gc/gc.fi`, all in the sweep:

1. **Clear free lists per class and lazily.** `__gc_sweep_init()`
   no longer clears anything, it only sets markers (`S_SWKL`, one per size
   class). The free list of a class is cleared when the **first chunk
   of that class** is swept.
   That is safe: afterwards the list only contains blocks of already swept
   chunks of the same class. A block of a not yet swept chunk falls out of
   the list when it is cleared and only comes back in once its chunk has
   been swept — it can never be handed out twice. The return of a
   completely dead chunk stays valid as well: the jump back of the list head
   (`old_head`) still cuts off exactly its contiguous segment,
   and no pointer from the old list content can point into this chunk any
   more.
2. **Sweeping with a time budget.** The sweep now hangs on the same
   `ZEIT_BUDGET_NS` (100 µs) as the marking has since round 41, instead of
   on two chunks per slice. `SCHEIBE_SWEEP` now only says after how many
   chunks the clock is consulted. A small heap is thereby finished in ONE
   slice — and only then does the triggering allocation see full free lists
   again.
3. **`INKR_AB` = 0**, i.e. incremental from the first cycle on; settable at
   runtime with `gc_set_inkr_ab()` (the default remains the constant value).

## 4. Throughput: A/B in the same process

The difference to be measured is a few percent. Two separately built
programs already differ by more than that through code layout alone —
measured in this round: after purely diagnostic counters were built in, the
same measurement went from 121 to 104 ms, i.e. 14 % **faster**, although
more code ran. Whoever compares that way measures chance.

`tools/gc_meas/ab.fi` therefore compares in the **same process**, with
the same machine code and the same live set: `gc_set_inkr_ab()`
switches over, and the phases run interleaved (A B A B A B) so that a drift
of the machine hits both equally.

**2000 live nodes, 100 000 rounds per phase, two runs of three pairs each:**

| | atomic (A) | incremental (B) |
|---|---|---|
| work, individual values | 52, 48, 48, 52, 56, 52 ms | 52, 53, 53, 54, 61, 53 ms |
| **median** | **52 ms** | **53 ms** (−2 %) |
| heap | 1,57 MiB | 1,57 MiB (the same) |
| collection time per phase | 10,7–12,3 ms | 13,7–15,9 ms |
| longest interruption | 296–371 µs | **120–148 µs** |

**120 000 live nodes, 60 000 rounds per phase:**

| | atomic (A) | incremental (B) |
|---|---|---|
| work | 529, 514, 530 ms | 523, 525, 526 ms |
| **median** | **529 ms** | **525 ms** (+0,8 %) |

Here the heap is above the old threshold from the start; both variants
therefore run identically in continuous operation, and the difference would
only be in the build-up. That they are equally fast is the control showing
that the measurement is clean.

−26 % (before the change) became **−2 %**. The requirement of at most
−10 % is met.

## 5. Result: pauses over the whole run

`build.fi`, 120 000 live nodes, build-up included, 5 s of continuous
operation, quiet machine:

| | before (`INKR_AB` 8 MiB) | after (`INKR_AB` 0) |
|---|---|---|
| full stop-the-world runs in the build-up | **3** | **0** |
| pauses in the build-up | 0,88 / 2,90 / 11,81 ms | 0,079 / 0,113 / 0,109 / 0,140 ms |
| build-up duration | 76 ms | 74 ms |
| **longest interruption, whole run** | **11,82 ms** | **0,45 ms** |
| throughput in continuous operation | 107,7 cycles/ms | 107,0 cycles/ms (−0,7 %) |

Histogram type 7 (whole interruption) over the whole run:

```
vorher   Fach  2   3   4   5   6      7    8   9  10  12  14
         Zahl  1   4   6  10  54  44390  345  76   2   1   1
nachher  Fach  2   3   4   5   6      7    8   9
         Zahl  3   4   4  14  90  44216  314  74
```

No bucket above 9 occupied any more: **nothing above 512 µs**, over the
whole run including the build-up.

## 6. Side effect: the memory overhang shrinks

Round 38 named as an „honest side effect" of the hybrid in the phase test
`rss_ende` = 14 912 KiB against 2112 KiB in stage 2 — floating garbage and
delayed return. With the lazy clearing of the free lists,
`tools/gc_meas/run.sh` (3b, phase fragmentation) now measures **2368 KiB**.
The overhang was therefore mostly not floating garbage but the same
bug: chunks freshly mapped during the sweep.

## 7. Acceptance

| Check | Result |
|---|---|
| `bash ./test.sh` | **676/676** (673 base + new test 771, every test runs in three build stages) |
| `bash tools/self_compare.sh` | **197** identical behavior, 0 differing, 0 failing (196 base + test 771) |
| `bash tools/fixpoint.sh` | stage 2 == stage 3, character-identical (322 723 lines of assembly) |
| `bash tools/gc_meas/run.sh` | fragmentation drift +0,0 % (stable), RSS end 2632 KiB, phase test end 2368 KiB, `volle_laeufe` 0 |
| pause histogram ≥ 10 min, large live set, build-up included | §8 — wall clock max 2,14 ms, compute time max 0,62 ms, 0 full runs |
| `tools/dom_soak` (in test.sh) | consumption flat 1360 → 1360 KiB, counter-check fires |

New test `tests/771_gc_build_without_stw.fi`: 200 000 live nodes (heap
above the old threshold), and the check is deterministic — not via the
clock — that `gc_volle_laeufe() == 0`, that anything was collected at all,
that the heap stays below 2,5 times the live set (measured 1,2 times) and
that the chain is complete and in the right order. Counter-check: with
`gc_set_inkr_ab(8388608)` — the behavior up to round 43 — the same test
fails (exit 3). So the test really can fire.

## 8. The endurance run — and why two clocks are needed

**First 10-minute run** (600 s, 120 000 live nodes, build-up
included, 61,3 million cycles, 7164 collections, **0 full runs**):
wall clock maximum **8,11 ms**, and in histogram type 7 there were **712
interruptions in [2,05; 4,10) ms and 81 in [4,10; 8,19) ms**. By those
numbers the goal would have been missed.

Almost all the long values were **marking slices** (type 1: bucket 12 = 682,
bucket 13 = 80). But a marking slice has a time budget of 100 µs, and
the clock is consulted every 16 objects. 16 objects are around 20 µs of work
(16 × 8 pointer fields × ~50 chunks in `__gc_block_von`) — 4 ms of compute
work between two clock readings are simply not possible. In parallel, two
other rounds ran on the same machine; the 1-minute load was at a **median
of 2,30 and a maximum of 5,57**.

Exactly this wrong finding cost round 40 a whole round. Instead of claiming
it or talking it away, the runtime now measures it: `gc_set_cpu_uhr(1)`
books the same interruption a second time in **pure compute time of the
thread**.

**Second 10-minute run, both clocks** (600,35 s, 65,5 million cycles,
7662 collections, **0 full runs**, heap 13,0 MiB, RSS 13 404 KiB,
0 mark stack overflows; load median 1,62, maximum 2,60):

Build-up: 4 collections, **not a single full one**, pauses 82 / 110 / 133 /
134 µs, longest interruption in the build-up 162 µs (compute time 161 µs).

| Bucket (upper limit) | wall clock | compute time |
|---|---|---|
| 2 µs | 173 | 170 |
| 4 µs | 400 | 397 |
| 8 µs | 577 | 582 |
| 16 µs | 1 211 | 1 303 |
| 32 µs | 5 059 | 6 457 |
| 64 µs | — | — |
| 128 µs | 5 143 326 | 5 146 125 |
| 256 µs | 44 409 | 41 026 |
| 512 µs | 11 076 | 10 406 |
| 1,02 ms | 221 | **21** |
| 2,05 ms | 34 | **0** |
| 4,10 ms | 1 | **0** |
| above that | **0** | **0** |

* **Compute time: nothing above 1,02 ms.** So the collector itself never
  holds up the mutator for longer than a millisecond — 5 206 487
  interruptions, of which 21 (0,0004 %) above 512 µs, none above 1,02 ms.
  `gc_cpu_stop_max` = **0,62 ms**.
* **Wall clock: nothing above 4,10 ms**, a single value in [2,05; 4,10) ms.
  `gc_pause_ns_max` = **2,14 ms**. The 35 values above 1 ms are exactly the
  difference to the compute time — preemption by the parallel rounds.

With that, the hard requirement (**no pause above 4 ms**) is met by the
wall clock as well and by compute time with a factor of 6 to spare; the
aimed-for ≤ 2 ms are met in compute time with a factor of 3 to spare and in
wall clock except for a single, demonstrably machine-caused value.

For comparison, the starting state in the same format: there the
maximum was **11,82 ms**, and it did not come from the machine but from
three full stop-the-world runs that occurred reproducibly at the same spot
in every run.

## 9. Open

* **`__gc_block_von` searches the chunk list linearly.** That is the most
  expensive path of the collector: in the run with 120 000 live nodes,
  1,65 s of 1,73 s of work sit in the collector, and the marking costs grow
  with the number of chunks. A sorted chunk array with binary search or a
  page table would lower throughput cost AND slice length considerably. Not
  touched, because this round had the build-up phase as its subject and the
  change touches the allocator.
* **The time budget is only checked every 16 objects (mark) resp. every 2
  chunks (sweep).** In compute time that is enough today (nothing above
  1,02 ms), but the distance to the 4 ms limit thereby depends on the shape
  of the objects. If `__gc_block_von` gets faster, this distance should be
  measured anew as well.
* **Finalizers and `Arc[T]`** — unchanged named remaining work from
  round 38, not touched in this round.
