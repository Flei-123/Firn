# Benchmark results (really measured)

## Rust suite: Firn against `rustc -O`, 37 pairs (30.10.2026, firn main e314dc53f)

Instrument for the block "A: Schneller als Rust". Run `/root/jarvis/bin/heavy bash bench/rust_suite.sh`; options are in the header of `bench/rust_suite.py`.

* default: instructions (callgrind, exact) AND wall clock (CPU time, median 5, interleaved); JSON to `bench/.work/rust_suite.json`
* `MODE=instr JOBS=6` instructions only (main instrument, about 6 min); `MODE=wall JOBS=1` clock only; `ONLY=a,b` subset; `FIRNC_BIN=<path>` other compiler; `NO_SAFE=1` skip release-safe
* Rust instruction counts are cached in `bench/.work/rust_cache.json` (key = source hash + flags): an A/B run of a new compiler only recounts Firn. `NO_CACHE=1` forces a recount
* A/B: `python3 bench/rust_suite_cmp.py old.json new.json [fast_instr|safe_instr|fast_wall]`; committed baseline: `bench/rust_suite_baseline.json`
* every pair prints one value (the vec* pairs only the exit code); Firn release-fast, release-safe and Rust must agree, otherwise the pair is marked MISMATCH and gets no factor
* pairs: 11 old (bitmap branchy bubblesort bytecount fib jsonscan matmul memstride sieve statemachine xxhash), new vec_sort, sqrt, vecsum, vecstore, vecsub, vecforeign, plus 20 from `bench/firn_vs_c` (Firn sources reused, new Rust twins; `fib40`, `matmul700`, `jsonscan28` are the firn_vs_c variants of fib/matmul/jsonscan)
* Rust twins from 30.10.: idiomatic safe Rust (bounds checks ON, Vec/slices, wrapping ops, `black_box` on sizes and result); the 11 older twins use unchecked pointers
* rustc 1.100.0-nightly (a36d05efa 2026-09-09), flags `-O`; CPU AMD EPYC 7571; instruction counts include process start (static Firn vs dynamic Rust, < 0.1 %)
* wall clock was measured at host load 22-23 (shared server): informational only; instructions are the metric

* instructions release-fast / rustc -O: median **1.251x**, geomean 1.351x, max **6.02x** (vecforeign)
* instructions release-safe / rustc -O: median **1.464x**, geomean 1.689x, max **6.55x** (vecforeign)
* wall release-fast / rustc -O (load 22): median **1.119x**, geomean 1.296x, max **6.91x** (vecforeign)
* wall release-safe / rustc -O (load 22): median **1.264x**, geomean 1.393x, max **6.71x** (vecforeign)

Pairs above 1.2x (instructions, release-fast): vecforeign 6.02, vecsub 4.09, vecsum 3.60, matmul 2.18, memstride 2.15, vecstore 2.12, jsonscan 2.08, fannkuch 2.06, memcpy_loops 1.93, fnv_hash 1.59, vec_sort 1.57, nbody 1.55, xxhash 1.48, crc32 1.43, mandelbrot 1.38, qsort 1.35, wordcount 1.29, sqrt 1.25, bitmap 1.25, floatloop 1.24.

| pair | Firn fast (instr) | Firn safe (instr) | Rust (instr) | fast/rust | safe/rust | wall fast/rust | wall safe/rust |
|---|---|---|---|---|---|---|---|
| bitmap | 525074173 | 577671392 | 419868196 | **1.25x** | 1.38x | 1.73x | 1.62x |
| branchy | 1320000486 | 1380000639 | 1215291614 | **1.09x** | 1.14x | 1.12x | 1.06x |
| bubblesort | 216598297 | 414908537 | 198901466 | **1.09x** | 2.09x | 1.80x | 1.94x |
| bytecount | 1595439993 | 1596509579 | 1476693241 | **1.08x** | 1.08x | 1.07x | 1.06x |
| fib | 101623667 | 148032497 | 155374694 | **0.65x** | 0.95x | 0.58x | 0.72x |
| jsonscan | 697000305 | 997500449 | 335300258 | **2.08x** | 2.97x | 1.66x | 1.70x |
| matmul | 376774823 | 584596459 | 173046211 | **2.18x** | 3.38x | 1.64x | 2.48x |
| memstride | 203907997 | 204424346 | 94637790 | **2.15x** | 2.16x | 1.04x | 1.05x |
| sieve | 179580915 | 212002523 | 186013976 | **0.97x** | 1.14x | 0.89x | 0.84x |
| statemachine | 600624598 | 607335600 | 607635450 | **0.99x** | 1.00x | 1.57x | 1.36x |
| xxhash | 981468052 | 1249903743 | 662994073 | **1.48x** | 1.89x | 1.03x | 1.08x |
| vec_sort | 334060013 | 583119626 | 212150735 | **1.57x** | 2.75x | 1.28x | 1.22x |
| sqrt | 48044265 | 84024400 | 38337384 | **1.25x** | 2.19x | 0.90x | 1.30x |
| vecsum | 86596075 | 96604512 | 24033159 | **3.60x** | 4.02x | 2.19x | 2.02x |
| vecstore | 106596505 | 116605166 | 50281555 | **2.12x** | 2.32x | 3.17x | 3.30x |
| vecsub | 65595317 | 71603684 | 16033370 | **4.09x** | 4.47x | 2.90x | 2.90x |
| vecforeign | 112965830 | 122974807 | 18768544 | **6.02x** | 6.55x | 6.91x | 6.71x |
| binary_trees | 1805216767 | 2815310523 | 2450124770 | **0.74x** | 1.15x | 1.25x | 1.68x |
| bitops | 8800000434 | 10800000664 | 8200293303 | **1.07x** | 1.32x | 0.96x | 1.08x |
| crc32 | 2415932323 | 2415932463 | 1694792958 | **1.43x** | 1.43x | 0.99x | 1.00x |
| fannkuch | 1676291235 | 2305383943 | 815053736 | **2.06x** | 2.83x | 1.42x | 1.79x |
| floatloop | 2100000227 | 2100000351 | 1700288746 | **1.24x** | 1.24x | 1.00x | 0.99x |
| hashmap | 159208945 | 181209165 | 155938325 | **1.02x** | 1.16x | 1.09x | 1.25x |
| mandelbrot | 1150275460 | 1154115576 | 836473566 | **1.38x** | 1.38x | 1.05x | 1.07x |
| memcpy_loops | 975213385 | 975213615 | 505470176 | **1.93x** | 1.93x | 1.53x | 1.43x |
| nbody | 3700000819 | 3700000965 | 2380298227 | **1.55x** | 1.55x | 1.09x | 1.12x |
| primes | 308931260 | 355559710 | 526459780 | **0.59x** | 0.68x | 1.00x | 1.01x |
| qsort | 554922693 | 1044858127 | 410395597 | **1.35x** | 2.55x | 1.13x | 1.23x |
| sha256 | 1308889851 | 1786518266 | 1256754411 | **1.04x** | 1.42x | 1.22x | 1.39x |
| soa_access | 803000828 | 803001068 | 1048295632 | **0.77x** | 0.77x | 1.04x | 1.00x |
| spectral_norm | 1350563318 | 2250923457 | 1171107475 | **1.15x** | 1.92x | 1.06x | 1.38x |
| strbuild | 824354841 | 1241412647 | 936479712 | **0.88x** | 1.33x | 1.46x | 1.56x |
| wordcount | 1414141998 | 1602821040 | 1094828467 | **1.29x** | 1.46x | 1.24x | 1.26x |
| fnv_hash | 2214592947 | 2214593163 | 1392804752 | **1.59x** | 1.59x | 1.01x | 0.99x |
| fib40 | 2450317037 | 3642763411 | 3248650124 | **0.75x** | 1.12x | 0.82x | 1.00x |
| matmul700 | 3112004439 | 4829454669 | 2968553110 | **1.05x** | 1.63x | 1.05x | 1.58x |
| jsonscan28 | 1804879864 | 2447598109 | 1760774531 | **1.03x** | 1.39x | 1.35x | 1.48x |

Worst offenders and first suspicions (asm read with `firnc --emit=asm`):

* vecforeign 6.0x, vecsub 4.1x, vecsum 3.6x, vecstore 2.1x: LLVM vectorises the Vec loops (`paddq` on xmm); Firn emits one scalar element per iteration and reloads the Vec header (len, ptr) in every iteration
* matmul 2.2x: the k loop is 8 instructions per element (index `lea` recomputed, no strength reduction of the row pointer, no unrolling, no vectorisation)
* memstride 2.15x, jsonscan 2.1x: extra address `lea`/moves in loop bodies; scanner loops branch per byte without unrolling
* fannkuch 2.06x: `static` arrays stay in memory (every access a load/store); Rust keeps the 10-element arrays in registers/stack after unrolling the short copy loops
* memcpy_loops 1.9x: Rust turns the byte/word copy loops into memcpy/memset (loop idiom recognition), Firn copies element-wise
* fnv_hash 1.6x, crc32 1.4x: no general loop unrolling (lea/cmp/jb overhead per byte) and a leftover copy `mov` in the fill loop (coalescing)
* vec_sort 1.57x: generic introsort in Firn against the pdqsort-style `sort_unstable` of Rust
* nbody 1.55x: positions/velocities in `static` arrays (memory traffic); Rust scalarises them into xmm registers


Produced by `bench/run.sh` (`bench/bench.py`), 9 runs per program, **median**.
Every benchmark exists twice -- `bench/firn/<name>.fi` and `bench/rust/<name>.rs` -- and both print their result; the outputs have to match, otherwise the measurement stops.
The Rust side uses `std::hint::black_box` and the same unchecked pointer accesses as the Firn side, so that the same work is measured.

* CPU: AMD EPYC 7571 32-Core Processor
* system: Linux 7.0.14-5-pve x86_64
* rustc 1.100.0-nightly (a36d05efa 2026-09-09)
* Firn: its own code generator, no external crates

| benchmark | Firn `release-fast` | Firn `release-safe` | Firn `dev-fast` (default) | `rustc -O` | factor fast | factor safe | factor devf | result |
|---|---|---|---|---|---|---|---|
| fib | 0.052 s | 0.051 s | 0.051 s | 0.028 s | **1.83x** | **1.79x** | **1.82x** | 4356618 |
| sieve | 0.039 s | 0.052 s | 0.135 s | 0.044 s | **0.87x** | **1.17x** | **3.04x** | 697026 |
| matmul | 0.042 s | 0.099 s | 0.257 s | 0.024 s | **1.77x** | **4.15x** | **10.74x** | 8291727 |
| bytecount | 0.269 s | 0.194 s | 0.820 s | 0.205 s | **1.31x** | **0.95x** | **4.01x** | 1604208 |
| bubblesort | 0.069 s | 0.107 s | 0.205 s | 0.037 s | **1.85x** | **2.89x** | **5.51x** | 12021846167 |
| statemachine | 0.138 s | 0.129 s | 0.233 s | 0.096 s | **1.43x** | **1.35x** | **2.43x** | 6710880 |

Median Firn `release-fast` against `rustc -O`: **1.60x** (range 0.87x - 1.85x).
Median Firn `release-safe` against `rustc -O`: **1.57x** (range 0.95x - 4.15x).
Median Firn `dev-fast` (default) against `rustc -O`: **3.52x** (range 1.82x - 10.74x).

`release-fast` is the like-for-like comparison: all passes, and integer arithmetic unchecked exactly as `rustc -O` leaves it. `release-safe` runs the same passes and CHECKS every integer operation, so it is Firn doing strictly more work than Rust. `dev-fast` is what a plain `firnc` gives you: checked, and without the one pass that would make the call stack unreadable.

## Round TEMPO 14 (24.09.2026) — before and after

The table above is the state after TEMPO 1-13 (register allocator, `xmm-ra`)
merged onto `main` plus TEMPO 14 (full unrolling of short counted loops,
`docs/TEMPO14.md`). "Before" is `main` at `893695bd`, measured with the
same script on the same machine in the same hour, twice each, alternating.
The machine was shared (two full test runs of other trees at the same time,
load 15-20 on 20 cores), so the wall clock scatters by 10-20 % and the pairs
are the honest statement:

| median against `rustc -O` | before, pass 1 | before, pass 2 | after, pass 1 | after, pass 2 |
|---|---:|---:|---:|---:|
| `release-fast` | 1.83x | 1.83x | 1.51x | 1.60x |
| `release-safe` | 1.71x | 1.65x | 1.59x | 1.57x |
| `dev-fast` | 4.60x | 4.02x | 3.92x | 3.52x |

The exact instrument is the count of executed instructions (callgrind, Firn
against the `rustc -O` build of the same program, outputs identical):

| median of the instruction factor | before (`main`) | after merge | after TEMPO 14 |
|---|---:|---:|---:|
| these six programs, `release-fast` | 1.37x | 1.09x | 1.09x |
| these six programs, `dev-fast` | 4.77x | 4.47x | 4.47x |
| all eleven with a Rust twin, `release-fast` | 1.51x | 1.45x | 1.45x |
| all eleven with a Rust twin, `dev-fast` | 5.07x | 4.90x | 4.90x |

So the step in this table is the register allocator of TEMPO 1-13. The
unrolling does not touch these programs (none of them has a short counted
loop in its hot path; `gc_barrier`, which has one and no Rust twin, went
-16 %); its target was the MP3 decoder: 124.0 -> 106.8 million instructions.

`matmul` is the largest factor left (2.66x in instructions): the inner loop
costs Firn 11 instructions per pass, among them an `imul` for `k * n`, and
Rust 3.75 (unrolled four times, the pointer stepped by `4n`).

## Round SPEED (27.08.2026) — the two passes, and what changed

The table above is pass 2. The machine is shared, so a second pass is
printed next to it; the honest statement is the pair, not one of them.

| benchmark | factor `release-fast` pass 1 | pass 2 |
|---|---:|---:|
| fib | 1.91x | 1.64x |
| sieve | **0.85x** | **1.01x** |
| matmul | 2.15x | 2.01x |
| bytecount | 1.30x | 1.30x |
| bubblesort | 1.82x | 1.84x |
| statemachine | 1.71x | 1.71x |
| **median** | **1.76x** | **1.67x** |

`release-safe`: **1.55x** / **1.52x** median. `dev-fast`: 4.14x / 4.21x.

### Against the state this round started from

`bench/RESULTS.md` said 2.08x median and `sieve` 4.16x, and
`orientos/ROADMAP.md` point 4.9 quoted those numbers. **Round 1 of
`docs/ROUNDSPEED.md` found that they were measured at `dev-fast`** — the
default level since round 72, which checks every integer operation and does
not inline — while the Rust side was `rustc -O`. The comparison Rust does
with Rust is `release-fast`, and this file now names the level in every
column.

| benchmark | old table (`dev-fast`, called "Firn") | now `release-fast` |
|---|---:|---:|
| fib | 1.52x | 1.64x |
| bytecount | 1.43x | 1.30x |
| statemachine | 1.99x | 1.71x |
| bubblesort | 2.17x | 1.84x |
| matmul | 2.90x | 2.01x |
| **sieve** | **4.16x** | **1.01x** |
| **median** | **2.08x** | **1.67x** |

Two things did the work, and they are separate. The **naming** of the build
level is what moves `sieve` from 4.16x to about 1x — that number was never a
codegen result, it was a checked, uninlined build measured against an
unchecked, inlined one. The **optimiser rounds 2 to 11** are what moved the
rest: block layout, exact loop depth, loops laid out in one piece, division
by a constant without `div`, the range analysis that removes checks which
provably cannot fire, `lea` for scaling, the bool cells of `&&` / `||` that
had been stuck in memory since round 92, and the same threading through a
bool phi. Every one of them is in `docs/ROUNDSPEED.md` with its own before
and after.

**The target of this round was median under 1.5x and `sieve` under 2.5x.**
`sieve` is at **1.01x**, so that one is met with room to spare. The median
is **1.67x** at `release-fast` and **1.52x** at `release-safe` — close, and
not there. `matmul` at 2.01x is the one that now carries it, and its cause
is named and not guessed: no vector instructions are emitted, and the
register allocation is linear rather than colouring.

**A note on the conditions.** These two passes were measured on a machine
that was doing other work at the same time (load average 14 to 17 on
12 cores) and with the root file system at 98 %. That widens the scatter of
BOTH sides — `fib` at `release-safe` came out faster than at `release-fast`
in pass 1, which cannot be true of the code and is the clearest available
statement about what this wall clock can resolve. For differences under
about 5 %, `tools/bench90/icount.py` (executed instructions, exact) is the
instrument, not this table.
