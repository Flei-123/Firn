# Round 40 — regalloc attack on realweb, GC endurance run made honest

Base: merge commit `415e8b4` (rounds 37+38+39). Territory:
`compiler/src/regalloc.rs` and `tools/gc_meas/`. Two separate tracks, both
in this file.

## Track A — register allocation

Starting point after round 37: html5lib 1,69×, realweb 4,34× against
html5ever. Documented lever: 7 391 reg→reg `mov`s and 445 store/reload
pairs in the hot path of `dekodiere`/`tokenize`.

### A1 — register descriptor deletes store→reload (commit `07dab2d`)

Peephole on value slots in the emission path: a reload directly after the
store of the same slot reads the register on instead. Effect:
realweb 4,38× → 4,23×, html5lib 1,73× → 1,70×.

### A2 — cell alias for loads (commit `f6895ea`)

`d = load c`, where the cell `c` lies in a register and all
uses of `d` are in the same block before the next write of the cell:
`d` gets no place of its own, the uses read the cell register directly
(`Alloc::ort()`), the load is dropped. That deletes the three
`mov` copies per loop iteration in the hottest loop of `dekodiere`
(33,5 million iterations in the realweb run).

**The trap that knocked over three tests** (211_generic_struct,
430_ct_select, 416_fehler_ausgabe): with 8/16/32 bit loads the load
extracts the relevant bits via `movzx`/32 bit `mov`; the cell register
still contains leftovers in the upper part. The alias is therefore
**only permitted for full 64 bits**.

**Measurement — deterministic instead of wall clock.** The wall clock
measurement (`durchsatz.sh`) fluctuates by ±30 % and did not show the gain
(with alias 10,47 MB/s, without 10,72 MB/s — pure noise). What is
dependable is the instruction count with callgrind on the same realweb
corpus:

| State | I refs (realweb) |
|---|---|
| without alias (`FIRN_NO_ALIAS=1`) | 2 334 236 911 |
| with alias | 2 136 489 667 |
| | **−8,47 %** |

Lesson for coming rounds: **always** back optimizations with callgrind, and
use `durchsatz.sh` only to check the order of magnitude.

Verification A2: `test.sh` 649/649, `selbst_vergleich` 188/0/0,
fixpoint 289 096 lines character-identical.

## Track B — the GC endurance run measured the wrong path

The 30-minute endurance run from round 38 (`tools/gc_meas/pause.fi`) ran
through cleanly, but the honest evaluation shows:

- 30,0 min, 1 394 549 collections, 2 948 276 000 cycles, 0 missed
  multiple collections
- RSS: start 0,82 MiB, maximum 1,33 MiB, end 0,83 MiB — **no drift**
  (first tenth 1,16 MiB, last tenth 1,16 MiB): no leak, no
  fragmentation over time
- mean pause 127 µs, longest pause 3,853 ms
- time in pauses: 177,3 s of 1800 s = **9,85 %**

**But:** `heap_bytes` stayed at 786 432 bytes the whole time. The
incremental cycle only switches on from `INKR_AB = 8 MiB` (`lib/gc/gc.fi:109`)
— so it was **not used a single time** in this run. The run proves the
stability of the non-incremental path; about the „~0,5 ms, heap-independent"
advertised in round 38 it says nothing.

### B1 — new run with a large live set (`tools/gc_meas/pause_big.fi`)

Deliberately keeps a lot alive: one root with `KINDER` (default 120 000)
text nodes as a sibling chain, held in a frame cell of `main`,
and at the end demonstrably still reachable via `dom_kinder_zaehlen`. In
parallel the same garbage stream as in `pause.fi` runs. With that the heap
stands permanently at 13 MiB, and every collection has to mark 120 000 live
objects — the case for which incremental collection was built. Hooked into
`run.sh` as stage 2b (`GCM_GROSS_SEK`, `GCM_KINDER`), with its own
evaluation.

Measurement (6 s budget, 120 000 live nodes, heap 13,0 MiB, 82 collections):

| Class | Count | cumulative |
|---|---|---|
| ≤ 500 µs | 29 | 36,7 % |
| ≤ 1 ms | 50 | 100,0 % |

So **all pauses of the measuring loop below 1 ms**, despite a 13 MiB heap
and 120 000 live objects — the goal < 2 ms is actually held by the
incremental path, and held independently of the heap. Slice maxima: type 0
159 µs, type 1 719 µs, type 2 183 µs, type 3 6 µs.

**Open point for round 41:** `pause_max_ns` reports 12,0 ms. This value
comes from the **build-up phase**, in which the heap is still growing and
collection is still non-incremental. A 12 ms hiccup during heap growth
is too much for a 16 ms frame budget: the transition into incremental
mode has to take effect earlier, or the growth itself has to run
incrementally.

## Track C — the actual breakthrough was not in the compiler

After A2 the callgrind profile of the realweb run (2,14 billion Ir) showed:

| Share | Ir | Function |
|---|---|---|
| 26,1 % | 557 993 466 | `dekodiere` (UTF-8 → code points) |
| 23,9 % | 510 486 268 | `eingabe_pruefen` |
| 23,1 % | 492 598 522 | `tokenize` (state machine) |
| 15,6 % | 332 932 201 | `main` (catch-all: everything without its own symbol) |

That made it clear: the most expensive part is not the state machine but
the preprocessing per character. Three experiments, each measured with
callgrind and counter-checked with octet-identical output:

### C1 — `eingabe_pruefen`: one range test instead of twelve comparisons

`0x20..0x7E` is the normal case on real pages and **never** an error of the
input stream. A range test placed in front ends the function immediately.

**2 136 489 667 → 1 793 235 323 Ir (−16,1 %)**

### C2 — `dekodiere`: reserve in advance, write ASCII directly

A code point costs at least one byte, so `len` slots are always enough:
`cp_reserve(out, len)` once, then write directly into the raw memory
instead of `cp_push` per character (call + capacity test). Plus an
ASCII fast path (`c0 < 0x80 && c0 != CR`) that skips the four
width comparisons. New in `lib/html/mem.fi`: `cp_ptr`, `cp_set_len`
(exported).

**1 793 235 323 → 1 427 485 223 Ir (−20,4 %)**

### C3 — fast path at the call site

The plain function call of `eingabe_pruefen` costs as well; the same
range test directly in `tokenize` saves it for ~95 % of all characters.

**1 427 485 223 → 1 297 240 896 Ir (−9,1 %)**

### C4 — refuted: „text run in one piece" in the data state

Hypothesis: an inner loop that pushes harmless text characters into the
character buffer in one piece without a state branch (what
html5ever gets out of `memchr`) saves the `match` dispatch per character.

Measured: **1 297 240 896 → 1 297 140 701 Ir (−0,008 %)** — i.e. nothing. The
`match` over the state is already a real jump table
(`jmp *(%rdx,%rax,8)` in the disassembly), the dispatch costs practically
nothing. The change was **rejected**: more code without a return.

### Result of track C

| Corpus | before round 40 | after round 40 | Goal |
|---|---|---|---|
| html5lib (pathological) | 1,69× | **1,33×** | ≤ 2× ✅ |
| realweb (real pages) | 4,34× | **2,68×** | ≤ 3× ✅ (stretch), ≤ 2× open |

Instructions realweb in total: 2 334 236 911 → 1 297 240 896 = **−44,4 %**.
html5lib conformance unchanged 6810/6810 (error messages 6809/6810),
`test.sh` 649/649, `selbst_vergleich` 188/0/0, fixpoint 289 096
character-identical.

### Lesson

The wall clock comparison did **not** show the first gain (A2) and it would
almost have been rejected as „brings nothing"; callgrind showed −8,47 %. And
the biggest lever was in two library functions, not in the optimizer. Order
for round 41: profile first, then optimize — and back every optimization
with an instruction count, never with the clock.

### Open for round 41

- `tokenize` is now the biggest item with 492 million Ir (38 %): ~100
  instructions per character in the state machine, predominantly slot
  traffic in a function with 8 200 lines of assembly → interval splitting
  in the regalloc.
- 52 comparisons in `tokenize` load their constant from a frame slot
  (`cmp -0x270(%rbp),%r9d`): `immediate_consts` discards a constant
  globally as soon as **one** of its uses does not permit an immediate.
  Fix: clone the constant at the problematic spot instead of giving up
  everywhere.
- `tok_attr_value_push` 105 million Ir (8 %) — not yet examined.

## Track B2 — the 30-minute endurance run WITH a large live set

Caught up with `pause_big.fi` (120 000 live nodes, heap 13 MiB,
1800 s). Raw data: `tools/gc_meas/duration30_big.tsv`.

- 23 840 collections, 202 453 000 cycles, 3 missed multiple collections
- RSS 12,9–13,9 MiB the whole time, end 12,89 MiB — **no drift**,
  not even with a permanently large live set
- pause histogram:

| Class | Count | Share | cumulative |
|---|---|---|---|
| ≤ 500 µs | 11 472 | 48,1 % | 48,1 % |
| ≤ 1 ms | 11 663 | 48,9 % | 97,1 % |
| ≤ 2 ms | 295 | 1,2 % | 98,3 % |
| ≤ 4 ms | 372 | 1,6 % | 99,9 % |
| ≤ 8 ms | 25 | 0,10 % | 99,99 % |
| ≤ 16 ms | 9 | 0,04 % | 99,996 % |
| > 16 ms | 1 | 0,004 % | 100 % |

- longest pause **19,34 ms**, and that in a slice of type 1 (not only
  in the build-up phase, as the short run still suggested). The maxima grew
  over the course: 12,05 ms → 15,68 ms → 19,34 ms.
- sum of all pauses 1689,9 s of 1800 s runtime — that is **93,9 %**.
  With 120 000 permanently live objects and ongoing garbage production
  the collector therefore works almost continuously.

### Verdict

The incremental path holds the **normal case** clearly below 1 ms (97,1 %),
but it does not hold a **bound**: 0,15 % of the pauses lie above 4 ms, single
ones at 19 ms. For a 16 ms frame budget that is a visible stutter every few
minutes. Together with the pause share of 93,9 % that is the clearest open
point of the GC — ahead of finalizers and `Arc[T]`.

**Task for round 41:** find out why a type 1 slice can become 19 ms long
(unbounded amount of work per slice? re-marking at the end of the
cycle?), and tie the slice size to a real time budget instead of to
an object count.

## Addendum (round 41, preliminary work) — the 19 ms were not the collector

The diagnosis of the GC was incomplete: the **full stop-the-world run**
(`__gc_collect_now`) booked its duration only into `S_PAUSE_MAX`, but into
**no** slice class. Because of that the global maximum (11,8 ms) was larger
than every type maximum (3,6 ms) and nobody could see where it came from.
Fixed: the full run is now **type 4**, plus a counter `gc_volle_laeufe()`.

Measured with that (`pause_big.fi`, 120 000 live nodes, 13 MiB heap):

| Run | Collections | of those full | longest slice (type 0–3) | Type 4 |
|---|---|---|---|---|
| 30 s | 434 | 3 | 1,89 ms | 11,23 ms |
| 120 s | ~1600 | **3** | 3,39 ms | 11,93 ms |
| 600 s (quiet) | 7 967 | **3** | 3,12 ms | 11,67 ms |

**It always stays exactly three full runs** — all in the build-up phase,
as long as the heap is still below `INKR_AB` (8 MiB). In continuous
operation **only** the incremental path runs. The mark stack never
overflows (`ueberlaeufe = 0`), so the expensive re-marking does not occur
at all.

Pauses in the quiet 10-minute run (longest slice per cycle):

| Class | Count | cumulative |
|---|---|---|
| ≤ 500 µs | 3 356 | 42,1 % |
| ≤ 1 ms | 4 484 | **98,4 %** |
| ≤ 2 ms | 98 | 99,7 % |
| ≤ 4 ms | 26 | 100 % |
| > 4 ms | 0 | — |

**Correction to section B2:** the 19,34 ms reported there and the 0,15 %
above 4 ms come from a run that ran **at the same time as `test.sh` and
callgrind** on the same machine. Without foreign load nothing lies above
4 ms. Lesson: measure pauses only on a quiet machine, and note foreign load
in the protocol.

**Not established:** smaller marking slices (`SCHEIBE_TRACE` 512 → 128)
showed no clean gain (type 1 maximum 1,07 ms → 1,87 ms — within the noise of
the individual maxima). Reset to 512. The tool for this is missing first: a
histogram of the **individual slices** instead of only the longest slice per
cycle. That is the first task of round 41.
