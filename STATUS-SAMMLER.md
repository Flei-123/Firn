# ROUND SAMMLER — the collector of Firn

Branch `sammler`, worktree `/root/firn-sammler`, base `4af14c3e`.
Seven commits. Not merged, not pushed.

## What triggered this round

Round KERN-3 (Certus, JS interpreter) measured with callgrind: an empty
interpreter loop costs ~9 µs per round, and **~35 % of the running time sits
in the collector**, named as `__gc_ld64` and `__gc_block_of`.

## The first finding: the same two names mean two different things

The brief asked to attack `__gc_ld64` and `__gc_block_of`. Both answers are
true, and which one holds depends on how big the program is:

* **In a small program `__gc_ld64` does not exist at run time.** It is a one
  line pointer dereference and the inliner embeds it everywhere. In
  `tools/gc_bench/deuter.fi` it has a symbol in the binary and **zero**
  instructions attributed to it.
* **In the JavaScript engine it is 21.07 %** — the single largest item in
  the profile. `compiler/src/inline.rs` stops after `MAX_INLINES = 2000`
  embeddings, and `lib/js/run_main.fi` exhausts that budget long before the
  collector is reached. A one instruction function then costs a full call:
  33.0 million calls in one run.
* **`__gc_block_of` is 3.89 % in the small bench and 10.59 % in the engine.**
  It is also the biggest single *caller* of `__gc_ld64`: 20.1 of those 33.0
  million calls, six per invocation, four of which read the same word.

So the round was run against **two** benches: a synthetic one that isolates
the allocation path, and the real `lib/js/run_main.fi` — the same engine
Certus uses. Several changes help one and cost the other; both numbers are
reported for each.

The tooling had to be built first: callgrind cannot resolve the symbols of a
firnc binary (static, no DWARF, RWX LOAD segment) and prints raw addresses.
`tools/gc_bench/prof.sh` reads the raw callgrind file, sums the SELF cost per
function and maps the addresses through `nm`.

## The benches

* `tools/gc_bench/deuter.fi` — the load an interpreter puts on the collector:
  many small short lived objects (24 bytes, size class 0), pointer chasing
  over a fresh chain and an environment chain, deep recursive stacks for the
  conservative scan, old→young stores into a live working set. Deterministic
  (fixed LCG, no clock in the load).
* an allocation microbench — 480,000 allocations of one 24 byte class and
  nothing else, to isolate the allocation path.
* **`lib/js/run_main.fi`** — the real JavaScript engine, driven with an
  allocating loop (`{a: i, b: i+1}` in a 20,000/120,000 round loop). This is
  the bench that stands in for Certus.
* a 937 step soak with a **full `gc_collect()` between every step** and a
  checksum over a live tree — the shape Certus uses.

Everything at `--opt-level=release-fast`. The machine carried a load average
of 7–17 from six other rounds throughout, so **instruction counts (callgrind,
deterministic) are the primary measure**; wall clock is always an interleaved
A/B (BASE, NEU, BASE, NEU …) so the load hits both sides equally.

## Profile before / after — `tools/gc_bench/deuter.fi`, 20,000 rounds

### BEFORE (base `4af14c3e`) — total 178,160,854 Ir

| % | Ir | function |
|---:|---:|---|
| 39.37 | 70,134,643 | `__gc_alloc_raw` |
| 15.11 | 26,918,393 | `__gc_sweep_step` |
| 11.85 | 21,120,412 | `__gc_get_block` |
| 11.52 | 20,523,708 | `main` |
| 5.39 | 9,600,220 | `__gc_classes_bytes` |
| 4.54 | 8,085,675 | `__gc_trace` |
| 3.89 | 6,936,265 | `__gc_block_of` |
| 3.14 | 5,601,384 | `__gc_mark` |
| 1.85 | 3,289,171 | `__gc_cycle_step` |
| 1.32 | 2,351,808 | `__gc_mark_push` |
| 0.62 | 1,099,878 | `deep` |
| 0.25 | 440,634 | `__gc_now_ns` |
| 0.24 | 433,965 | `__gc_roots_rescan` |
| 0.24 | 431,844 | `__gc_scrub_deep` |
| 0.23 | 401,646 | `__gc_cycle_start` |
| 0.20 | 360,978 | `__gc_chunk_new` |
| 0.16 | 292,899 | `__gc_hist_record` |
| 0.04 | 73,170 | `__gc_pause_record_h` |
| 0.02 | 31,500 | `__gc_stw_an` |
| 0.01 | 16,815 | `__gc_stack_bottom_maps` |

Collector share **87.86 %**.

### AFTER (`sammler`) — total 152,240,252 Ir  (**−14.55 %**)

| % | Ir | function |
|---:|---:|---|
| 45.12 | 68,698,360 | `__gc_alloc_raw` |
| 17.69 | 26,929,482 | `__gc_sweep_step` |
| 13.48 | 20,523,741 | `main` |
| 5.34 | 8,129,977 | `__gc_trace` |
| 5.31 | 8,088,057 | `__gc_block_of` |
| 3.72 | 5,657,511 | `__gc_mark` |
| 3.15 | 4,800,080 | `__gc_class_for` |
| 2.18 | 3,323,145 | `__gc_cycle_step` |
| 1.55 | 2,364,696 | `__gc_mark_push` |
| 0.72 | 1,099,878 | `deep` |
| … | | (`__gc_get_block` and `__gc_classes_bytes` have left the profile) |

Collector share **85.80 %**.

The share barely moves *in this bench* and that is not a failure: this bench
is allocation and collection and almost nothing else, so the collector is
~86 % of it by construction. The number that transfers is the cost per
allocation.

## Profile before / after — `lib/js/run_main.fi` (the real engine)

### BEFORE — total 1,326,698,096 Ir, collector share **60.95 %**

| % | Ir | function |
|---:|---:|---|
| 21.07 | 279,551,007 | `__gc_ld64` |
| 10.59 | 140,443,696 | `__gc_block_of` |
| 9.45 | 125,343,757 | `interp__eval_node` |
| 5.01 | 66,527,249 | `__gc_alloc_raw` |
| 4.83 | 64,076,013 | `__gc_st64` |
| 2.71 | 35,938,776 | `__gc_sweep_step` |
| 1.81 | 24,069,627 | `__gc_ld32` |
| 1.68 | 22,247,875 | `__gc_mark` |
| 1.38 | 18,294,825 | `__gc_get_block` |
| 1.24 | 16,500,000 | `interp__add_values` |
| 1.18 | 15,619,824 | `gcvec_read_raw` |
| 1.17 | 15,550,092 | `__gc_as_raw` |
| 1.15 | 15,295,249 | `__gc_trace` |
| 1.14 | 15,080,466 | `gcmap_get` |
| 1.05 | 13,946,409 | `__gcslots_addr` |
| 1.05 | 13,942,341 | `__gcmap_slot` |
| 1.05 | 13,940,527 | `ast__ast_a` |
| 0.96 | 12,760,407 | `val__val_tag` |
| 0.95 | 12,600,513 | `__gc_st32` |
| 0.92 | 12,185,008 | `__gc_classes_bytes` |

### AFTER — total 1,281,081,504 Ir (**−3.44 %**), collector share **59.56 %**

| % | Ir | function |
|---:|---:|---|
| 20.50 | 262,664,001 | `__gc_ld64` |
| 10.12 | 129,693,347 | `__gc_block_of` |
| 9.78 | 125,343,757 | `interp__eval_node` |
| 6.49 | 83,185,745 | `__gc_alloc_raw` |
| 5.36 | 68,688,504 | `__gc_st64` |
| 2.80 | 35,927,482 | `__gc_sweep_step` |
| 1.87 | 24,015,105 | `__gc_ld32` |
| 1.63 | 20,921,899 | `__gc_mark` |
| 1.29 | 16,500,000 | `interp__add_values` |
| 1.22 | 15,619,824 | `gcvec_read_raw` |
| 1.21 | 15,550,092 | `__gc_as_raw` |
| 1.19 | 15,241,793 | `__gc_trace` |
| 1.18 | 15,080,466 | `gcmap_get` |
| 1.09 | 13,946,409 | `__gcslots_addr` |
| 1.09 | 13,942,341 | `__gcmap_slot` |
| 1.09 | 13,940,527 | `ast__ast_a` |
| 1.00 | 12,760,407 | `val__val_tag` |
| 0.98 | 12,590,244 | `__gc_st32` |
| 0.95 | 12,160,560 | `ast__ast_kind` |
| 0.94 | 12,100,440 | `interp__ctx_of` |

`__gc_get_block` and `__gc_classes_bytes` have left the top 20 entirely.
`__gc_alloc_raw` rises because step 5 moved the free list pop *into* it —
`alloc_raw + get_block` together went 84,822,074 → 83,185,745 Ir.

## The numbers that count

| measurement | before | after | change |
|---|---:|---:|---:|
| **JS engine, allocating loop (Ir)** | 1,326,698,096 | 1,281,081,504 | **−3.44 %** |
| JS engine, collector share | 60.95 % | 59.56 % | −1.39 pp |
| deuter, 20k rounds (Ir) | 178,160,854 | 152,240,252 | **−14.55 %** |
| allocation microbench, 480k allocs (Ir) | 140,018,126 | 112,717,058 | **−19.50 %** |
| per allocation: `alloc_raw` + `get_block` + `class_for` | 203 Ir | 153 Ir | **−24.6 %** |
| **deuter wall clock**, median of 16 A/B pairs | 1255 ns/round | 1142.5 ns/round | **−8.96 %** |
| **JS engine wall clock**, median of 9 A/B pairs | 1453 ms | 1447 ms | **−0.43 %** |

**The honest reading of the last two lines.** The allocation heavy bench got
**9 % faster in wall clock and won 15 of 16 interleaved pairs** — that is a
real, repeatable gain. The JS engine did **not** get measurably faster
(−0.43 %, faster in 4 of 9 pairs) although it executes 3.4 % fewer
instructions. The interpreter is bound by memory latency and branch
misprediction in `interp__eval_node` and the `gcmap`/`gcvec` accessors, not
by the instruction count of the allocation path. Removing instructions from
code that is waiting on cache misses does not make it finish sooner. Saying
otherwise would be inventing a result.

## The seven steps that were kept

**Step 1 — `053edd83` — the size classes as arithmetic instead of two branch
chains.** `__gc_classes_bytes` was a chain of 13 comparisons and
`__gc_class_for` called it in a **loop**. The classes 0..10 follow
`(4 + 2*(k&1)) << (k/2 + 3)` exactly; 11 and 12 break the sequence and stay
branches. Verified exhaustively for every k = 0..12 and every size 0..8999.
−3,204,092 Ir (−1.80 %).

**Step 2 — `a8792106` — the block size per class out of a table.** Three
sites recomputed `HEADER + __gc_classes_bytes(class)` per allocation — a
constant of the class. `S_BLOCKTAB` (13×8 bytes at state offset 2152) holds
it; `gc_init` fills it once. `__gc_get_block` 55.0 → 42.0 Ir per allocation.
−6,239,460 Ir (−4.57 %).

**Step 3 — `527a71e5` — null the payload of the small classes without a
loop.** Measured with `--dump-instr`: the nulling loop cost four instructions
per eight bytes — 20 of the 138 instructions of an allocation, for three
words. `size <= 32` now writes four words straight line, which is exactly the
32 byte payload of class 0. −7,299,206 Ir (−5.60 %).

**Step 4 — `023c2e38` — the allocation counters use the state pointer that is
already there.** `__gc_diag_inc` re-fetches `__gc_state()` and the code
generator materialises that address anew at every call site.
−955,982 Ir (−0.78 %).

**Step 5 — `31be1ebb` — the free list pop straight in the allocating frame.**
`__gc_get_block` cost 42 Ir per allocation, of which the actual pop is
**four**: load the head, load its link, store the head back, done. The rest
is prologue, epilogue, ten saved registers and a 1,888 byte frame.
`__gc_alloc_raw` now does the pop itself under exactly the conditions
`__gc_get_block` checks first anyway (size class, `S_LOCAL == 0`, non empty
list); everything else still goes through it. −9,434,463 Ir (**−7.72 %**),
the largest single step.

**Step 6 — `12569958` — `__gc_block_of` reads the block size once instead of
four times.** The finding that only shows up in a big program: 20.1 of the
33.0 million `__gc_ld64` calls of a JS run come out of this one function, and
four of its six loads read the *same* word. JS engine −26,710,149 Ir
(−1.99 %).

**Step 7 — `feb28922` — a second chunk cache entry in `__gc_block_of`.** The
one entry cache misses whenever the program alternates between two chunks,
which an interpreter does constantly; measured 0.83 chunks of the list still
walked per call after step 6. Both entries are invalidated when a chunk is
handed back to the kernel — missing that on the second entry would leave a
pointer into unmapped memory. JS engine −33,689,926 Ir (**−2.56 %**).

**Step 7 is a deliberate trade.** It costs the synthetic bench
+1,629,720 Ir (+1.08 %, because there the one entry cache already hit almost
always and the second check is pure overhead) and saves the real engine
−2.56 %. It was kept because the real engine is the target.

## Ideas that were tried and thrown away — with the number that killed them

**Division by a magic reciprocal in `__gc_block_of`.** The brief's own
suggestion and the textbook answer: the block index came from a division by a
run time value (20–40 cycles) on every marked pointer. Implemented properly —
a per chunk `(M, S)` pair in the chunk header at offset 56, with the
exactness condition `(M·b − 2^S)·n_max ≤ 2^S` **verified against `n / b` for
all thirteen classes over every offset 0..262,079** and for large blocks up
to 50 MB.

| | division | magic | |
|---|---:|---:|---|
| deuter, ns/round, median of 14 pairs | 1342.5 | 1325.0 | −1.30 % |
| marking bench, ms, median of 12 pairs | 69.0 | 70.0 | **+1.45 % (worse)** |
| `__gc_block_of` (Ir) | 6,936,265 | 7,953,445 | **+14.7 % (worse)** |

A first attempt was also **wrong**: a guessed fixed shift (`sh >= 22`)
produced 40,022 wrong indices across nine of the thirteen classes. It was
caught by the exhaustive check before it ever ran, which is why that check
exists. The corrected version is exact and still not faster — the extra load
plus multiply and shift costs about what the divider costs, and the divider
is not on the critical path here. **Reverted.**

**Carrying the block address along in `__gc_sweep_step`.** Replacing
`data + i * block` with a running pointer made it **worse**: 22,492,451 →
25,244,824 Ir (+12.2 %). The extra live value costs more in a function that
already spills 445 of 550 values than the multiplication it saves.
**Reverted.**

**Short circuiting classes 1 and 2 in `__gc_class_for`.** Verified correct
(exhaustive, 0 mismatches) and measured **exactly zero** change — the bench
allocates 24 byte objects, which already take the `size <= 32` early return.
Complexity without a number. **Reverted.**

**Raising `MAX_INLINES` from 2000 to 60000** so `__gc_ld64` would be inlined
in the JS engine. This is the *right* fix for the 21 % item, and it is a
compiler change, not a collector one. Compile time for
`lib/js/run_main.fi` went from **8.7 s to over 4 minutes** (killed at that
point) because `inline::inline_module` rescans from function 0 after every
single embedding — it is O(n²) in the number of inlines. Not viable without
rewriting that loop first. **Reverted, and written down as the next lever.**

**Removing the diagnostic counters from the allocation path.** `gc_diag(6)`
is read by `tools/gc_meas/*`, `lib/css/soak_style.fi`,
`lib/browser/soak_tree.fi` and `lib/layout/*`. Deleting it would break other
rounds' measuring tools. Step 4 made it cheap instead.

## The bigger finding, not fixed in this round

**The single threaded program never gets the lock free allocation path.**
`__gc_alloc_local` (round 49, variant B: handout from a per thread free list
without a lock) is guarded by `S_MULTI`, and `S_MULTI` becomes 1 only when a
second thread starts. In an interpreter — one thread, millions of small
objects — **all 480,008 allocations went through the slow path**, verified by
call counts in the callgrind file. The fast path exists and is unreachable
for exactly the workload that needs it most.

**`__gc_alloc_raw` has a 7,760 byte stack frame** (`sub $0x1e50, %rsp`),
`__gc_sweep_step` 4,448, `__gc_block_of` 1,200. `FIRN_RA_STATS=1` reports
`values=964 regs=283 spilled=574 maxlive=29` with `noiv=520` — 520 of those
values are **never touched at all** (dead code no pass removed). They cost no
instructions but they inflate the frame, and the frame is what the
conservative stack scan has to walk and scrub.

## What the next lever is

1. **Make `inline::inline_module` not O(n²), then raise `MAX_INLINES`.** This
   is the biggest single number on the table: `__gc_ld64` is 20.50 % of the
   JS engine *purely because the inliner runs out of budget*. A one
   instruction function should never be a call. Fixing the rescan (keep a
   worklist instead of restarting at function 0) makes the cap affordable,
   and then this item largely disappears on its own. Compiler round, not a
   collector round.
2. **Let the single threaded case use a bump path.** Not by setting `S_MULTI`
   (that turns on locking and safepoints) but by giving `__gc_alloc_raw` a
   real bump path for classes 0..2 while phase 0 and below the limit. Target:
   ~20 instructions per allocation instead of 153. A round of its own,
   because it changes the frame layout of the hottest function, and the frame
   layout decides which dead objects survive the conservative scan — round
   37's `tests/520_gc_weak.fi` exit 6 is the standing warning.
3. **`noiv` in the register allocator** — 520 dead values in one function is a
   missing DCE pass after inlining. Would shrink every GC frame at once.

## Correctness

* **1264 of 1264 test programs pass, 0 fail** — every program in `tests/` and
  `examples/` with an `expect_exit:`/`expect_out:` header, built and run in
  **four build levels** (`release-fast`, `release-safe`, `dev-fast`,
  `--no-opt`), exit code and stdout checked. That is the core of `test.sh`
  step 3, which is the part that can see a collector bug.
  (`.bench-work/cases-NACHHER.txt`)
* **36 of 36** GC and threading tests at `release-fast` after the last step,
  and re-run after *every* step separately: `520_gc_weak` (conservative stack
  scan + weak references), `822_gc_weak_zeroed`, `820..823` (finalizers,
  limits, reentrancy), `771_gc_build_without_stw`, `860_thread_basic`,
  `861_thread_gc`, `862_thread_local`, `834_arc_thread`, `1003_js_gc_cycle`,
  `1135_js_gen_gc`.
* **The 937 step soak** — a live tree, 200 objects of garbage per step, a
  **full `gc_collect()` between every step**, checksum verified each time:
  identical result `13974870` on base and on `sammler`, at `release-fast`,
  `release-safe`, `dev-fast` and `--no-opt`. Nothing reachable was freed.
* **`tools/gc_soak/soak.fi` for 25 s** — the run that really hands chunks
  back to the kernel, which is the path a stale chunk cache would crash on:
  heap bounded at 44 MB, 239 collections, no fault.
* **The JS engine produces byte identical output** on base and on `sammler`.
* `gc_bottom_swap` and `gc_root_register` are untouched — no step goes near
  root registration or the sleeping stack region.

### `./test.sh`

The full suite takes hours on this machine under the load of six parallel
rounds; the BEFORE run was started on the unmodified base at the beginning of
the round and was still inside `tools/self_compare.sh` (step 16 of ~30, a
complete self compilation of the compiler in Firn) when the round ended — it
had **0 failures up to that point**. Log: `.bench-work/test.vorher.log`.

Because that could not be finished on both sides in the time available, the
1264-program four-level run above was used as the correctness gate instead:
it is the same programs and the same four build levels as `test.sh` step 3,
run to completion on the final `sammler` build.
