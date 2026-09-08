# ROUND INLINE — the inliner of Firn

Branch `inline`, worktree `/root/firn-inline`, base `683d78d5` (round SAMMLER).
Not merged, not pushed.

## What triggered this round

Round SAMMLER ended with a lever it could not pull. `__gc_ld64` — a **one
instruction** pointer dereference — was **21.07 % of the running JavaScript
engine**, 33.0 million calls, because `compiler/src/inline.rs` stops after
`MAX_INLINES = 2000` embeddings and `lib/js/run_main.fi` exhausts that budget
long before the collector is reached. Raising the cap was tried and reverted:
compile time went from **8.7 s to over 4 minutes**, because `inline_module`
is O(n²).

So this is a compiler round. Three things came out of it, and the second was
not on the list.

## The result in three numbers

| | base `683d78d5` | branch `inline` | change |
|---|---:|---:|---:|
| **JS engine, wall clock** (120k round allocating loop, median of 15 interleaved pairs) | 1446.40 ms | **860.13 ms** | **−40.53 %**, won 15 of 15 |
| **JS engine, instructions** (callgrind, 20k rounds) | 1,288,743,573 | **731,860,724** | **−43.21 %** |
| **compile time of the JS engine** (release-fast, median of 3) | 8.33 s | **4.16 s** | **−50.1 %** |

`__gc_ld64` **20.71 % → gone from the profile.** `__gc_st64` 5.32 % → gone.
Collector share 59.80 % → 38.53 %.

And the compile time fell *while doing eleven times as much inlining* — which
is the whole point of step 1.

## Step 1 — the inliner walks the module once (`2632e528`)

`inline_module` was O(n²) in **four** independent ways, stacked:

1. **`'outer: loop { for ci in 0..funcs.len() }`** — after every single
   embedding it `continue 'outer`'d, restarting the scan at function 0.
   Reaching function 1600 of 1638 meant walking functions 0..1599 again,
   2000 times over.
2. **`find_site` restarted at block 0** of the caller each time, over a body
   that had just grown.
3. **`m.funcs.iter().position(|f| &f.name == name)`** — a linear scan over all
   1638 functions to resolve *every* callee name at *every* call site.
4. **`reaches(m, callee, caller)`** — a fresh DFS over the whole call graph per
   candidate site, and `reaches_itself_self` one DFS per call instruction of
   every function on top.

Replaced by a worklist that stays with one caller until it has no site left, a
scan that resumes at the last site, a name→index `HashMap` built once, and a
memoised `Reach`.

**Why memoising reachability is exact and not an approximation.** Inlining only
ever *removes* a call edge from the caller and copies the callee's edges in its
place. Whatever the embedded body can reach, the caller could already reach
through the call it replaced. Reachability is therefore an invariant of the
pass.

| `lib/js/run_main.fi`, release-fast, `MAX_INLINES` unchanged at 2000 | before | after |
|---|---:|---:|
| the `inline` pass alone | 5767.0 ms | **19.4 ms** (297×) |
| optimizer in all | 9452.5 ms | 1070.9 ms |
| **total compile** | 12059.8 ms | **3380.6 ms** (−72.0 %) |

**Proof that it changes no decision:** the FIR after optimization is identical
(1638 functions, 23977 blocks, 133041 instructions) and `--emit=asm` is **byte
identical**, md5 `a327e150505130d3461f99a4542a575a` over 9,936,071 bytes.
Across `tests/` and `examples/`, **319 of 319 programs** produce byte identical
assembler (`tools/inline_bench/asmcmp.sh`).

> A first run of that comparison reported 109 differences. They were a bug in
> the script — all parallel jobs wrote the same two scratch files and
> overwrote each other. Written down because a measuring tool that lies is
> worse than none.

## Step 2 — a WRONG ANSWER that the round uncovered (`a7e87dfe`)

Not a slowdown. `tests/1133_js_class_private.fi` assertion 4, the private brand
check `#p in o`: **`B.has({})` answered `true`** for an object that has no such
field.

`inline.rs` returns the value of a multi-`ret` callee through a slot: every
`ret v` becomes `store slot, v` + `br cont`, and the continuation block starts
with `load slot`. `threading::fork_at` sees a block whose single instruction is
a bool `load` and whose terminator branches on exactly that value — its pattern
— and `thread_bool_cells` makes every predecessor jump **past** the block into
its two arms. The block goes unreachable and the `load` with it, while the
caller still names the loaded value further on, here in the phi that merges
`have`. The register allocator then hands out whatever the register held.

`fork_at` now refuses when the loaded value is used **outside its own block**.
The two checks it already had (the arms must carry no phi) do not cover this:
the use is not in an arm, it is anywhere in the function.

**The bug is older than this round.** `tests/1136_inline_multiret_thread.fi`,
the 30 line reproduction, miscompiles on base `683d78d5` as well — exit 1
instead of 0, at `release-fast` **and** `release-safe`. There the count budget
simply never reaches that call site in the JS engine. Raising the budget only
made an existing bug *reachable*.

The narrowed condition costs the engine nothing: at the unchanged default,
`--emit=asm` of `run_main.fi` is still byte identical to base.

## Step 3 — a SIZE bound next to the count bound (`0816ca2c`)

`MAX_INLINES` bounds the **number** of embeddings, and a number cannot tell a
one instruction accessor from a forty instruction body. `MAX_ALWAYS_INSTS = 8`:
once the count budget is spent the pass keeps going, but only for bodies of at
most 8 instructions.

**Raising the pure count bound is not monotonic** — that is the finding that
decides the design. `lib/js/run_main.fi`, 120k round allocating loop, median,
interleaved against the old cap-2000 binary:

| | wall clock | vs base | pairs won |
|---|---:|---:|---:|
| cap 2,000 (the old value) | 1687 ms | — | — |
| cap 5,000 | 1670 ms | **+6.1 % slower** | 1 of 11 |
| cap 20,000 | 1687 ms | **+10.1 % slower** | 0 of 11 |
| cap 22,138 (saturated) | 1228 ms | −27.2 % | 15 of 15 |
| **size bound ≤1**, count still 2,000 | 956 ms | −36.7 % | 11 of 11 |
| **size bound ≤4** | 929 ms | −38.4 % | 11 of 11 |
| **size bound ≤8** ← taken | 912 ms | −39.4 % | 11 of 11 |
| **size bound ≤12** | 879 ms | −41.5 % | 11 of 11 |

A half spent count budget is the worst of both worlds: the code has grown but
the hot accessors are still calls. 4 / 8 / 12 tie with **each other** (6:5 and
5:6 pairs — noise), so the middle of the plateau is taken.

The size bound also beats embedding **everything** by **−17.4 %** (11 of 11
pairs) while the binary grows 1.83 → 1.93 MB instead of 1.83 → 3.16 MB. In the
JS engine 29.5 % of all 22,138 wanted embeddings are one instruction bodies,
and `__gc_ld64` (3,480) and `__gc_st64` (2,496) head the list.

## The counter check on round SCHLEUSE's WASM interpreter (`28e48001`)

`/root/osum-schleuse/kernel/app/wasm.fi` built with the new `firnc` (read only
out of the Osum tree, output into this one), `tools/inline_bench/prim.wat` =
the same trial division algorithm as `kernel/app/prim.fi`, limit 200000. Every
build answers exit 64 = 17984 mod 256, the right prime count.

| | wall clock (median of 7) |
|---|---:|
| base, `release-fast` | 5987 ms |
| base, `release-fast --no-pass=inline` — SCHLEUSE's number | **3904 ms** |
| NEW, `release-fast` (default) | 5913 ms (−1.2 %) |
| NEW, count=0 size≤1 | **3806 ms** (−0.8 % vs `--no-pass`) |
| NEW, count=0 size≤8 | 4142 ms (+5.2 %) |

**The round does not flip SCHLEUSE's finding**, and the reason is worth more
than a flip would have been. The damage is **not inlining as such — it is the
COUNT budget**, which embeds bodies of any size up to 40 instructions:

| | wall clock | binary |
|---|---:|---:|
| count 0 (only the size rule) | 3818 ms | 287 KB |
| count 500 | 5156 ms (**+35.0 %**, 7 of 7) | 398 KB |
| count 2000 (the default) | 5778 ms (**+46.3 %**, 7 of 7) | 549 KB |

**500 embeddings already cost 35 %.** The interpreter is one long if/else chain
over the opcode; embedding forty instruction bodies into it doubles the binary
and pushes the chain out of the I-cache. The size rule costs it nothing.

That is the same conclusion the JS engine reached from the other side: the
useful embeddings are the tiny accessors, and a bound on the *number* of
embeddings is the wrong instrument in both directions.

## Point 3 of the brief — already done, with the call counts that show it

The brief carried SAMMLER's finding forward: "the single threaded case never
gets the lock free allocation path, all 480,008 allocations went through the
slow path". Measured on this branch that is no longer the situation to fix.
`tools/inline_bench/allocbench.fi`, 480,000 allocations, calls made **by**
`__gc_alloc_raw` in the whole run:

```
   480,000  __gc_class_for      <- one per allocation
        45  __gc_get_block      <- 45 in 480,000, not 480,000
       168  __gc_now_ns
```

SAMMLER's step 5 already serves the single threaded case without a call and
without a lock, and `__gc_alloc_in` returns 0 after **one load** when `S_MULTI`
is 0 — there is no lock on this path to remove. Cost per allocation is
unchanged at **153 Ir**, and base and branch are within 0.005 % of each other.

What is left is not a lock and not a call: **143 instructions of straight line
code in a frame of 8,528 bytes** (`sub $0x2150,%rsp`, larger than SAMMLER's
7,760 because inlining put more values in it). Shrinking that changes which
values live in the frame of the hottest function, and the frame layout decides
which dead objects survive the conservative scan. That is a collector round
with its own gates; round 37's `tests/520_gc_weak.fi` exit 6 is the warning.

## Correctness

* **1328 of 1340 four level runs pass** — every program in `tests/`,
  `tests/opt/` and `examples/` with an `expect_exit:`/`expect_out:` header,
  built and run at `release-fast`, `release-safe`, `dev-fast` and `--no-opt`.
  The 12 failures are `028_cast_narrow`, `030_wrap_u8`, `054_i16_ops` and
  `1334b_type_truncation` — they fail **identically on base** (12 there as
  well): deliberate overflow-panic programs that only reach their
  `expect_exit` at `release-fast`. Not this round's doing.
* **164 of 164 GC and threading runs pass** (41 tests × 4 levels), including
  `520_gc_weak` (conservative stack scan + weak references), `822_gc_weak_zeroed`,
  `820..823`, `771_gc_build_without_stw`, `860..862`, `834_arc_thread`,
  `1003_js_gc_cycle`, `1135_js_gen_gc`.
* **The 937 step soak** (`tools/inline_bench/soak_tree.fi`) — a live tree, 200
  objects of garbage per step, a **full `gc_collect()` between every step**,
  checksum verified after each collection: **8296429**, identical on base and
  on the branch, at all four build levels. Nothing reachable was freed.
* **The JS engine produces byte identical output** on base and branch over all
  16 programs in `tools/js/cases` and `tools/js/progs`.
* **267 of 267 compiler unit tests** pass.
* `tests/1136_inline_multiret_thread.fi` is new and **fails on base**, which is
  the point of it.

## Ideas that were tried and thrown away — with the number that killed each

**Raising `MAX_INLINES` alone.** The brief's own first suggestion, and after
step 1 it was finally affordable (22,138 embeddings, 8.6 s compile instead of
4+ minutes). Killed as a *default* by its own measurement: caps 5,000 and
20,000 are **slower than the old 2,000** (+6.1 %, +10.1 %), and even the
saturated cap is **17.4 % behind the size rule** while producing a 3.16 MB
binary against 1.93 MB. Kept only as `FIRNC_MAX_INLINES` for measuring.

**Size bound 12 instead of 8.** The fastest single number on the JS engine
(−41.5 %). Not taken: 4, 8 and 12 tie with each other (6:5, 5:6 pairs), so the
difference is noise, and 12 costs the WASM interpreter more (the larger the
bodies, the closer to the count budget's I-cache damage).

**Raising the size bound to reach `__gc_class_for`** (the last call left on the
allocation path, 480,000 calls). It has more than `MAX_CALLEE_BLOCKS` blocks,
so ≤16, ≤24 and ≤40 do not reach it and change the microbench by less than
**0.005 %** (116,824,674 / 116,824,694 / 116,824,652 against 116,824,698).

## What the next lever is

1. **A per caller size bound instead of the count budget.** This is what both
   halves of the round point at. The count budget is what hurts the WASM
   interpreter (+46 % at 2,000) and what starved the JS engine of its
   accessors. A bound of the shape "a caller may grow by at most X %" would
   let the tiny accessors through everywhere and stop the opcode chain from
   doubling. The two benches in `tools/inline_bench/` are set up to decide it.
2. **`noiv` in the register allocator.** SAMMLER measured 520 values in
   `__gc_alloc_raw` that are never touched; the frame is now 8,528 bytes. That
   is a missing DCE pass after inlining, and it would shrink every GC frame at
   once.
3. **`tools/self_compare.sh`** — see below.

## `./test.sh` and `self_compare.sh`

The full suite takes hours on this machine, which carried a load average of
**8** from other rounds throughout. `tools/self_compare.sh` was started on the
finished branch and was **still running when the round ended** (~35 minutes;
it prints only its final tally, so the log is empty until then). Log
`.work/self.log`. That is the same position round SAMMLER was in, and it is
reported as unfinished rather than as a pass.

What *was* established about self compilation:

* **`.firnc1` — the compiler written in Firn, 1,912,952 bytes — builds
  successfully with the new `firnc`.** The compiler compiling the compiler is
  a substantial gate on its own, and it is the step that has to work before
  `self_compare.sh` can compare anything.
* **`tests/303_wtf8_roundtrip.fi` is right at all four build levels and
  through `firnc1` as well** (`65536 2048 2049 0 1114112 0`). That is the
  program round 92 found the last inliner/phi bug with — it printed
  `0 0 0 0 2048 1112064` then, and only at the two levels that inline. It is
  the most directly relevant single case this round could check.

The 1340-run four level gate above was used as the correctness gate instead,
as in round SAMMLER: it is the same programs and the same four build levels as
`test.sh` step 3.
