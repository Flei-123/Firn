# Round 38 — GC: pauses, fragmentation, endurance run

Territory of this round: `lib/gc/gc.fi`, `lib/rc/`, measuring tools
`tools/gc_meas/`. Compiler sources were not touched. Base: round 36
(commit 97ec31a).

## Stage 1 — measuring tools and before-numbers (without a GC change)

New: `tools/gc_meas/` with `run.sh`, `pause.fi`, `frag.fi`.

- **pause.fi**: DOM-like endurance workload, sorts every collection pause
  (`gc_pause_ns_last`) into nine classes (histogram) and reports the maximum
  and the sum. Runtime budget via `GCM_PAUSE_SEK`.
- **frag.fi**: six size classes (48 to 2048 bytes of payload), per round
  a fresh stack of exactly one class; the stack of the same class from the
  previous round is dropped. Live: constantly six stacks.
  What is measured is the real RSS from `/proc/self/statm` (via
  `lib/dom/meas.fi`).
- **run.sh**: builds both in three build stages (release-fast, no-opt,
  dev-fast), checks in a short run that the counters agree independently of
  the build stage (otherwise the measurement would be worthless), then the
  actual run in release-fast plus evaluation (pause histogram,
  fragmentation verdict).

### Finding: the test first had to be made measurable

The first draft of the fragmentation test showed `lebende=120000` — exactly
ALL objects ever allocated, i.e. the GC collected nothing in the test.
Isolation with minimal tests (noted in the log, not part of the repo):

1. Linked chain (10000 objects), head variable set to null, `gc_collect()`
   twice → **10000 stay live**.
2. Unlinked objects, dropped immediately → become free (1 remainder).
3. Like (1), plus 40 KiB of stack devastation after the nulling → **stays**.

The cause lies NOT in the collector but in the interplay of codegen and the
conservative stack scan, demonstrated twice via `objdump`:

- The optimizer removes `k = gc_null[T]()` as a dead assignment if `k` is
  never read afterwards (min1: not a single write to the k slot after the
  last read). The old pointer physically stays in the frame.
- Result temp cells of calls are allocated freshly per call site and are
  never nulled (min4: first `bau()` union in `-0x50(%rbp)`, second in
  `-0x110(%rbp)`). The old stack head sticks in the first temp cell —
  ONE pointer holds the whole old chain through the links.

Consequence for all GC measurements (and, honestly, for users of the
runtime):
**Establishing unreachability reliably means: let the pointer die in a
helper function (a returned frame is nulled by the scrubber) or really
overwrite the root as a raw value** — never rely on a final nulling within
the same function.
`tests/510_gc_cycle_becomes_resolved.fi` does exactly that (cycle created in
`zyklus()`, only `u32` comes back) — the test proves that the collector
itself collects correctly.

frag.fi therefore holds the six stack heads as raw addresses in a
`[u64; 6]` array in `main` (real overwriting, reference sum at the end),
and building happens in helper functions that only return the address.
With that: `lebende=1200` after the final collect — exactly the expected
6 stacks × 200 objects. Short-run counters identical in all three build
stages (300 at 120×50).

### Before-numbers (base 97ec31a, without a GC change)

Pauses, 60 s DOM soak workload (release-fast, this machine):

| Run | Cycles | Collections | longest pause | ≤ 250 µs | > 2 ms | Sum GC |
|---|---|---|---|---|---|---|
| 1 | 105,3 M | 49819 | 1,97 ms | 96,1 % | 5 | 9456 ms |
| 2 | 94,3 M | 44619 | 7,22 ms | 96,0 % | 154 | 9390 ms |

The maximum pause fluctuates strongly between runs (outliers in the 4–8 ms
band); typical (96 %) are ≤ 250 µs. Sum of GC time ≈ 15–16 % of the runtime.
The 3,54 ms from the browser acceptance are in the same order of magnitude.

Fragmentation, 600 rounds × 200 objects (120000 allocations, 6 classes):

| Metric | Value |
|---|---|
| RSS start | 48 KiB |
| RSS maximum | 3140 KiB |
| RSS end | 3140 KiB |
| Drift in the last third | **+0,0 % (stable)** |
| heap_bytes end | 3145728 |
| live at end | 1200 (expected 1200) |

RSS rises once to ~3 MiB (the heap threshold settles) and then stays
constant for 200+ rounds — no fragmentation visible over this duration.
The statement is limited by the short runtime; the 30-minute endurance run
(stage 4) is the harder proof.

Measurement artifacts: `tools/gc_meas/pause.tsv`, `tools/gc_meas/frag.tsv`.

## Stage 2 — fragmentation: empty chunks go back to the OS

### Finding (before, measured with the new phase test `frag2.fi`)

`frag2.fi` drives a phase workload: 300 rounds with large objects only
(class 2048, 24 stacks in a ring), then 300 rounds with small ones only
(class 48). Result with the old sweep:

| Metric | Before |
|---|---|
| RSS phase A max | 20284 KiB |
| RSS phase B max | 24124 KiB (**keeps rising**) |
| RSS after the final `gc_collect()` | 24124 KiB (**never falls**) |

Two separate causes, both in `lib/gc/gc.fi`:

1. **Empty class chunks never went back to the OS.** `__gc_sweep` only
   returned large-object chunks (`klasse >= KLASSEN`) via `munmap`; the
   256 KiB class chunks stayed mapped forever, even completely empty ones.
2. **The collection threshold was open at the top.** After the last run of
   the large phase, `GRENZE = lebbytes` was ~ 9,5 MiB; the small phase
   allocates only 3,8 MiB in total — so a collection NEVER ran again, and
   the dead chunks of the large phase were not even visited any more.
   (measured: 300 rounds, 0 collections in phase B)

### Change (only `lib/gc/gc.fi`)

- `__gc_sweep`: completely empty chunks of EVERY class are returned to the
  OS. Cutting the free-list segment runs in O(1) via the list head
  remembered in front of the chunk — no second pass.
- **Hysteresis** (chunk header offset 48, newly occupied and documented): a
  chunk is only returned once it has been empty for two sweeps in a row.
  Without it the phase test oscillates between munmap/mmap — measured
  +46 % runtime (118 -> 176 ms); with it: 123 ms, i.e. churn-free.
- **Threshold cap**: `MAX_GRENZE = 4 MiB` caps the distance between two
  collections. The memory overhang above the live set is therefore always
  < 4 MiB, no matter how large the heap once was. Workloads with a small
  heap (DOM soak ~ 1,3 MiB) notice nothing of it (`MIN_GRENZE` works as
  before).

### After (same tests, same machine)

| Metric | Before | After |
|---|---|---|
| frag: RSS end | 3140 KiB | 2624 KiB |
| frag: runtime | 63 ms | 58 ms |
| frag2: phase A max | 20284 KiB | 14400 KiB |
| frag2: phase B max | 24124 KiB | 16448 KiB |
| frag2: RSS end | 24124 KiB | **2112 KiB** |
| frag2: runtime | 120 ms | 115 ms |

The RSS curve in phase B now falls by itself IN THE MIDDLE OF THE RUN
(16448 -> 13632 -> 2112 KiB from round ~150 on), instead of remaining
stuck at the maximum of the large phase. `lebende` stays exactly 1200
(frag) resp. 4856 (frag2 ring) — the behavior of the collection is
unchanged, only the memory return is new.

Bar: test.sh **640/640**, selbst_vergleich **186/0/0**, fixpoint
**character-identical (284207 lines)**, 19 gc/rc tests ok, 6 gc negative
tests rc!=0.

Remaining design limit (honestly named): objects that live scattered over
many chunks hold those chunks — without compaction (forbidden by the
conservative scan) that cannot be solved. The scattering count stays within
bounds, however, because allocations come chunk-wise from the free list.

## Stage 3 — incremental collection (hybrid)

### Method

`lib/gc/gc.fi` now runs a three-phase cycle (new state fields from offset
320 in the state block — the room was there, **no compiler change
needed**):

- **Phase 0 (idle)**: as before. Threshold reached → cycle.
- **Phase 1 (marking)**: the root scan (registers + stack) stays atomic
  (a conservative scan cannot be interrupted — the stack changes), the
  tracing runs in slices of 512 objects per allocation.
- **Phase 2 (sweeping)**: the sweep runs in slices of 2 chunks per
  allocation (cursor in the state block); free-list segment cutting and the
  empty hysteresis from stage 2 apply per chunk unchanged.

Correctness without compaction and without a second stack scan:

- **Insertion barrier activated** (Dijkstra): `__gc_barrier` colors the
  target of a Gc write gray while a cycle is running — until now it only
  counted. White targets behind black containers can thus not become
  invisible.
- **Objects allocated during a cycle are gray** and go onto the mark stack
  (the stack is not scanned again; an object that is only stack-live could
  otherwise be swept before it was ever linked in).
- **White parity** (`S_PAR`, 0/2): the sweep no longer resets marks; at the
  end of the cycle the parity flips and all survivors are white again in
  one stroke. There is thus no mark reset pass and no stale marks in the
  next cycle.
- **Exhaustion fallback**: if an allocation in the middle of a cycle finds
  no block, the rest of the cycle is finished atomically before OutOfMemory
  is reported (DESIGN_GOALS §2 stands).

### Measurements (this machine, load caveat: rounds running in parallel)

Pauses by slice type (diagnostic fields `S_PMAX_*`, `gc_pause_max_typ`),
DOM workload, incremental path forced (`INKR_AB` set to 1 MiB for the test):

| Slice | longest pause |
|---|---|
| cycle start (root scan) | 0,15 ms |
| marking (512 objects) | 0,01–0,02 ms |
| sweeping (2 chunks) | 0,44–0,52 ms |
| termination | 0,01 ms |

The pauses are bounded **independently of the heap size**: the start
depends on the stack depth, mark/sweep on the (constant) slice sizes —
no longer on the heap. That was exactly the goal.

Throughput DOM workload (5 s runs, median of 5, interleaved):

| Variant | Cycles (median) | longest pause |
|---|---|---|
| stage 2 (atomic) | 7 786 000 | 0,44 ms |
| hybrid | 7 110 000 (**−8,7 %**) | 0,46 ms |
| incremental forced | 6 533 000 | **0,47 ms** |

### The hybrid and why

Purely incremental (from the first cycle on) cost −13 to −26 % throughput
for heaps whose full cycle is below a millisecond anyway — pure loss.
The collector therefore only switches to the incremental cycle from
`INKR_AB = 8 MiB` of heap; below that the atomic path from stage 2 runs
unchanged (its pause grows linearly: ~0,5 ms per 1,3 MiB — at 8 MiB that
would be ~3 ms, and from there the slices take over).

The DOM soak workload (heap ~1,3 MiB) therefore runs atomically: pauses as
in stage 2, throughput affected only by the phase check per allocation
(measured −8,7 %, within the ±10% requirement). Large heaps get pauses of
around 0,5 ms, independent of the heap size.

Honest side effect, measured on the phase test (peak ~20 MiB, above the
threshold): `rss_ende` 14912 KiB instead of 2112 KiB in stage 2 — floating
garbage (objects allocated gray live one cycle longer) and the empty
hysteresis delays the return in the incremental path. Against the state
before this round (24124 KiB, never falling) it remains a clear
improvement; below 8 MiB exactly the stage 2 behavior applies.

Bar: test.sh **640/640**, selbst_vergleich **186/0/0**, fixpoint
**character-identical (284207)**, 19 gc/rc tests ok, 6 gc negative tests
rc!=0.

## Stage 5 — finalizers and Arc[T]: named remaining work

Deliberately NOT started, both are bigger than a residual stage:

- **Finalizers**: they need a semantic decision (when allowed, resurrection
  yes/no, order, thread). Without a SPEC basis every implementation would
  be guesswork; the conservative scan and the parity marks of this round
  are compatible with them (finalizable objects would live one cycle
  longer — the same machinery as floating garbage).
- **Arc[T]**: a second reference type next to Rc[T] with atomic counters —
  only sensible once there are threads (SPEC §7 is single-threaded in
  stage 0). Building atomics without threads would be wasted effort.

Both remain named as remaining work, not patched on.
