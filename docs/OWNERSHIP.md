# Automatic ownership — design (roadmap block B, r1179/r1180)

Goal: the compiler infers ownership and lifetimes of heap objects by itself — no annotations, no borrow checker — and frees deterministically at the last use or reuses the memory in place. Whatever it cannot prove (cycles, unclear aliases) stays with the tracing GC (`docs/GC.md`). The prototype lives behind a flag and changes nothing while the flag is off.

Status: design + baseline only. No compiler change yet.

## 1. Prior art

| System | Core idea | Language requirements | Evidence (from the sources) | Take for Firn | Risk | Effort |
|---|---|---|---|---|---|---|
| Koka / Perceus | Precise RC inserted by the compiler (dup/drop at the last use), drop specialization, reuse analysis (a dead cell of the same size is reused by the next constructor), borrow inference for parameters, "functional but in-place" (FBIP) via a runtime is-unique test | Immutable data, no cycles in the RC heap, explicit control flow, whole-program knowledge of constructors | rbtree within 10 % of C++; Java needs ~10x the memory (1.7 GiB vs 170 MiB); without the RC optimizations rbtree is >2x slower; reuse can raise peak memory (no-opt version uses 11 % less on cfold); atomic RC costs 5 % (rbtree) to 59 % (nqueens) [1][2] | Reuse analysis (same size class, dead input -> output in place) and the "last use" placement; drop specialization for known-unique trees | RC everywhere is a different runtime; Firn data is mutable and may be cyclic, so Perceus' garbage-free proof does not carry over | high (as a whole), medium for reuse alone |
| Lean 4 "Counting Immutable Beans" | reset/reuse of unshared values, inferred borrowed parameters | pure functional core | "competitive with state-of-the-art compilers and often outperform them" (no numbers in abstract) [3] | borrowed-parameter inference = Firn already has it implicitly (GC does not count); reset/reuse idea same as Perceus | low | low (idea only) |
| Lobster | Compile-time RC: each allocation gets one owner (first variable/field it is stored into), all other uses borrow; RC ops only where a second owner appears; functions specialized per ownership of the arguments | Analysis interleaved with type checking in call-graph order; "cannot modify x while y borrows it" errors exist (rare: 2 places in dozens of programs) | "around 95 % of runtime reference count operations" removed; cycles not collected, only reported at exit [4][5] | The single-owner model ("first store owns, everything else borrows") and per-call-site specialization of summaries | Lobster still rejects some programs; Firn must never reject (escape.rs rule "where it cannot be decided, ALLOW") | medium |
| Nim ARC / ORC | Destructors + move semantics; `lastReadOf(x)` from a control-flow analysis turns copies into moves (sink inference), `lent`/cursor inference avoids RC for traversals; ORC adds a trial-deletion cycle collector only for types that can be cyclic (`acyclic` pragma helps) | Value semantics for containers, hooks `=destroy/=sink/=copy/=trace` per type | Web server bench: ORC max latency 6.24 ms vs mark&sweep 204.79 ms, 137 MiB vs 153 MiB; with memForSpeed M&S has higher throughput (39,562 vs 34,949 req/s) [6][7] | Last-read analysis on FIR (exactly what a "free at last use" needs), the split "acyclic types -> deterministic, possibly cyclic -> collector" | Moves need a fall-back copy when the last read is not provable — Firn would fall back to the GC instead | medium |
| Swift ARC + OSSA | Every reference is RC'd; optimizer removes retain/release pairs (RC identity, loop hoisting, code motion); OSSA puts ownership on SSA edges so the verifier and passes see owned/guaranteed values | Classes with RC header everywhere, is_unique for copy-on-write | Forum reports: retain/release in a loop disappear with `-enable-ossa-modules`; RC-identity analysis is flow insensitive; Perceus authors found Swift's RC "less effective" than Koka's [8][9][2] | Ownership kinds on FIR values (owned / borrowed / unknown) as a verifier-checked annotation of the IR, not of the source | Swift needed years; RC-everywhere is not wanted | high |
| Mojo ASAP | A value dies at its last use (not at scope end), also temporaries inside expressions; composes with "copy+del -> move" | Value types with known lifecycle methods, ownership conventions on arguments | Documented policy, no numbers [10] | Free intermediates of expression chains (str `a + b + c`) at their last use; ASAP placement instead of scope end | Low on its own; Mojo has explicit ownership conventions Firn does not | low |

Short verdict: Perceus = best reuse story, but needs an RC world; Lobster = best fit for "one owner, rest borrows" without annotations; Nim = the right analysis (last read) and the right split (acyclic deterministic / cyclic collected); Swift = proof that RC everywhere is costly to optimize; Mojo = cheapest win (ASAP for temporaries).

## 2. What Firn has today

- `compiler/src/escape.rs` + twin `lib/firnc1/escape.fi` (round 79): taint analysis on the AST for raw pointers into LOCALS (sources: locals and parameters; sinks: return, stores through foreign pointers, thread start). Interprocedural summaries per parameter, fixed point over the program. Purpose: reject `return &x`. It does not follow pointers through loads, says nothing about heap objects and does not remove any allocation.
- `compiler/src/moves.rs` (OWN-1/OWN-2): explicit move checker for structs with `fn drop(inout self)`; drop calls at scope exits in reverse order. Conservative (no conditional moves, no moves out of loops). This is scope-based RAII for opt-in types, not inference.
- `compiler/src/refparam.rs`: second-class references `&T`/`inout T` for parameters only.
- GC (`lib/gc/gc.fi`, `compiler/src/gc.rs`, `gc_lower.rs`, `nogc.rs`): non-moving, incremental mark-sweep, precise heap / conservative stack, 14 size classes with free lists, block header with type id (type id 0 = free block, ignored by `__gc_mark`), finalizers, weak refs, Dijkstra insertion barrier. `str` concatenation allocates GC blocks (`__str_concat` -> `__gc_alloc_raw` + `__gc_blit`).
- Manual memory: `rt.heap_alloc/heap_free` (page granular, block cache), `Vec` buffers (registered as GC roots), arenas (`lib/db/arena.fi`, `lib/mem/core_alloc.fi`), `RefCount[T]` (`lib/std/rc.fi`), `lib/rc/arc.fi`.
- Liveness exists only inside the register allocator (`regalloc.rs`), per function on machine values, after all FIR passes.
- firnc1 has no optimizer pipeline (no inline/opt in `lib/firnc1`): FIR passes added to firnc0's `opt` pipeline do not touch `fir_compare` / fixpoint as long as they are off by default.

Missing for ownership inference:

1. Heap escape/alias analysis on FIR: which allocation sites (`__gc_alloc_raw` calls, `__str_concat`) flow where — into a heap field, a global, a root-registered Vec, a weak ref, another thread, an unknown callee.
2. Last-use / liveness of SSA values and of their copies on FIR (including copies through stack slots and phi nodes).
3. Interprocedural summaries for heap pointers: per parameter "borrowed (not stored, not returned)", "stored", "returned"; per function "returns a fresh object" (allocation site summary).
4. Uniqueness: "at this point there is exactly one reference" — for reuse and for freeing a whole structure (tree) at once.
5. Runtime entry points: `__gc_free_block(p)` (put a block back to its size-class free list with the collector's invariants) and region allocation.

## 3. Baseline (r1180)

Measured 30.10.2026 on main `e314dc53f`, firnc0 release build of that commit, host load 19-25 (shared machine). GC share = instructions in collector + allocator functions (`__gc_*`, `gc_*`) over all instructions (callgrind; the inline fast path in callers is not counted). Pauses and counters read from the GC state block at `exit` via gdb (no code change; script below). Pause clocks are wall clock, so under load they are upper bounds.

| Load | Build | GC share (alloc + collector) | max pause | sum of pauses | GC allocations / bytes | cycles | time (user) | max RSS |
|---|---|---|---|---|---|---|---|---|
| binary_trees (`bench/firn_vs_c/firn`) | release-fast | 66.7 % (1.9 + 64.8) | 0.61-0.80 ms | 252-260 ms | 8,711,918 / ~279 MB | 32 | 0.36 s (1.79 G instr.) | 22.9 MB |
| OpenPlan `op derive` 300 pages (56,400 functions, generated by `tools/bench/format_bench.fi` with PAGES=300) | dev-fast (default) | 13.9 % (9.1 + 4.8) | 4.28 ms | 94 ms | 2,766,111 / ~247 MB | 5 | 1.39 s (7.39 G instr.) | 218 MB |
| firnc1 self-build `bin/firnc1.fi --no-cache` | firnc1 release-fast | 0 % (no GC allocation at all) | - | - | 0 GC; heap runtime (`heap_alloc`, `buf_grow`) 1.3 % | 0 | 3.70 s user / 1.13 s wall (threads; 1.36 G instr.) | 107 MB |
| Certus bbc (bin r1977, firnc-gc fork) | release | 14.2 % of busy main-thread samples (209/1475) | not measured | - | - | - | 17.7 s user | PSS 1108 MB |
| Certus heise | release | 10.5 % (120/1140) | not measured | - | - | - | 14.8 s user | PSS 1013 MB |

Certus numbers come from `tools/tempo/phases.py` (main-thread wall samples); its "gc" phase includes the GcVec/GcMap container functions, so it is an upper bound for the collector itself.

What the numbers say:

- binary_trees is collector bound (sweep 23.6 %, trace 15 %, block lookup 10.8 %, mark 9.2 %). Every tree is built, checked and dropped as a whole. A region per `check(build(d))` call or a recursive drop of a provably unique tree removes almost all of it.
- OpenPlan is allocator bound: `__gc_blit` + `__gc_alloc_fast` + `__str_concat` are string building (JSON dump, ids). Temporaries of concat chains die immediately — ASAP free / chain fusion targets.
- firnc1 does not use the GC on its hot path, so ownership brings nothing there; it stays a regression guard (compile time must not grow by more than 3 %).
- Certus is the real target but mixes DOM (cyclic, must stay GC) with many short-lived GcVec/strings.

Reproduce: `/root/jarvis/data/staff_scratch/b1own/{meas.sh,meas2.sh,gcstate.sh,cgsym.py,cert.sh}` (scratch; `gcstate.sh` finds the state block via the first rip-relative `lea` in `gc_init` and dumps it at `exit`; offsets as in `lib/gc/gc.fi`: 32 runs, 72 pause max, 80 pause total, 104 allocation count, 296 total bytes + 88 since).

## 4. Recommended approach: hybrid, escape + last use, GC as the fall-back

No RC. Ownership is a compile-time fact or nothing; where it is nothing, the object is an ordinary GC object, exactly as today. The runtime only gains a way to give a block back early.

### 4.1 Ownership states on FIR values

Every value that holds a GC pointer gets one of:

- `fresh(site)` — produced by an allocation site in this function and not yet stored anywhere.
- `owned` — the only reference is this SSA value (or one stack slot that is only read through this function).
- `borrowed` — passed to a callee whose summary says the parameter does not escape.
- `unknown` — anything else (loaded from the heap, stored into a heap field/global/Vec root, passed to an unknown callee, turned into a weak ref, sent to a thread, converted to an integer, address taken). Unknown is absorbing.

Like `escape.rs`: decidable cases are decided, everything else is `unknown` = GC. A missed optimization is acceptable; a wrong `owned` is not.

### 4.2 Analyses (all on FIR in firnc0, after `gc_lower`, before inlining finishes the big functions)

1. **Allocation-site escape** (extends the round-79 model from locals to heap sites): forward data flow over SSA + stack slots; sinks = store into memory that is not a private stack slot, return (-> summary "returns fresh"), call argument (-> callee summary), integer cast, `weak()`, thread primitives, inline asm.
2. **Interprocedural summaries** per function, fixed point over the call graph (same driver shape as `escape.rs`): per GC parameter `{borrows, stores, returns}`; per function `returns_fresh`. Recursion converges because the lattice is finite. Indirect calls and `extern` = `stores` for every argument.
3. **Last use**: backward liveness per function over FIR blocks for `owned` values. The free goes after the last use on every path; on paths where the value is dead at a branch, the free goes to the start of the successor that no longer uses it (classic "kill at edge"). Values live around a loop back edge are not freed inside the loop.
4. **Uniqueness of structures (phase C)**: a tree whose every child pointer was stored only from `fresh` values of the same function family and never loaded out to an `unknown` place is unique; a drop of the root may then free recursively (drop specialization).

### 4.3 Transformations

- **A — ASAP free of temporaries**: `owned` value dead after its last use -> `__gc_free_block(p)`. First target: `str` temporaries of concat chains and of formatting, GcVec temporaries.
- **B — concat-chain fusion**: `a + b + c (+ ...)` with dead intermediates -> one `__str_concat_n` (one allocation, one copy). Pure rewrite, no runtime invariant involved; may land before A.
- **C — reuse**: a block freed by A whose size class equals the size class of an allocation that follows on the same path is handed over directly (no free list round trip): Perceus reuse without RC.
- **D — regions**: a call whose result is `owned` and whose callees allocate only into that result (summary `returns_fresh` + every store target fresh) runs with a thread-local region pointer; the whole region is released when the owner dies (binary_trees `check(build(d))`). Region chunks are ordinary GC chunks flagged "region", so a conservative pointer into a released region finds type id 0 and is ignored.

### 4.4 Runtime interaction

- `__gc_free_block(p)`: only when the collector is idle (`S_PHASE == 0`): set type id to 0, clear finalizer/slot bits, push to the size-class free list (thread-local list in multi-threaded programs). While a cycle is marking or sweeping the block goes to a small deferred list that the cycle end releases (avoids freeing a grey block that is on the mark stack and a double free by the sweeper).
- Write barrier: unchanged. A freed block is never the target of a barrier because only `owned` values (never stored to the heap) are freed.
- Conservative stack scan: stale copies of a freed pointer hit type id 0 and are ignored (already true for free blocks); if the block was reused they keep the new object alive one cycle longer — false retention, not unsafety.
- Finalizers: classes with a finalizer are never freed statically (the finalizer must run after collection semantics).
- Weak refs: any object whose value reached `weak()` is `unknown`; nothing that can have a weak ref is freed early.
- `#[no_gc]`: `__gc_free_block` is allowed in `#[no_gc]` code (it never collects).
- Accounting: freed bytes are subtracted from `S_SINCE`, so pacing sees the lower allocation rate and cycles become rarer.

### 4.5 Safety argument

Use-after-free is impossible if and only if: (1) the freed value is `owned` — no copy of it exists in the heap, a global, a root or another thread; (2) it is dead on every path after the free (liveness over all FIR copies, phis and stack slots); (3) no callee kept it (summary `borrows`). Every rule that cannot be shown falls to `unknown`. Stale register/stack copies are harmless because the runtime treats freed blocks as free and the program never reads them again (2).

Debug mode `--ownership=check`: `__gc_free_block` does not free but poisons the payload (0xDE pattern) and marks the header "poisoned"; `__gc_mark` and every GC read helper abort with the allocation site when they meet a poisoned block; a full `gc_collect()` at exit verifies no live object points at a poisoned block. All gates and the four loads run once with `check` before any `on` result counts.

### 4.6 Flag and compiler parity

- `--ownership=off|on|check` (default `off`). Env override `FIRNC_OWNERSHIP` for measurement only.
- Implemented in firnc0 only (FIR pass in the `opt` pipeline, registered in `opt.rs` like the other passes). firnc1 accepts the flag and ignores it; `fir_compare` compares pre-optimization FIR, so parity is unaffected; fixpoint builds with the default `off`.
- When the prototype is accepted, the FIR pass moves to firnc1 as a twin like `escape.fi`, with a compare tool (`tools/ownership/compare.sh`: same free sites per function in both compilers).

## 5. Phases and stop criteria

| Phase | Content | Files | Done when | Stop if |
|---|---|---|---|---|
| B3.0 | Runtime: `__gc_free_block` + deferred list + poison mode; GC test suite + tests for free during mark/sweep, conservative stale pointer, multi-thread | `lib/gc/gc.fi`, `lib/firnc1/gctext.fi` (gen_gctext.sh), `tests/` | all GC tests green in 4 stages, `gc_meas` counters unchanged | free during a running cycle cannot be made safe -> only idle-phase free |
| B3.1 | Concat-chain fusion (`__str_concat_n`) | `compiler/src/strtype.rs` / `lower.rs`, `lib/gc/gc.fi` | OpenPlan derive Ir down, output md5 equal | gain < 2 % |
| B3.2 | FIR heap escape + summaries + last use; ASAP free of `owned` str/GcVec temporaries behind `--ownership=on` | new `compiler/src/own/{mod,escape_heap,summary,lastuse,insert}.rs` | `check` mode clean on all tests + 4 loads; OpenPlan GC share 13.9 % -> <= 8 % | any poison hit that is not a pass bug fixable in the pass; or < 3 % gain on OpenPlan and Certus |
| B3.3 | Reuse (same size class hand-over) | `own/reuse.rs`, runtime hook | allocation count down further, peak RSS not up > 10 % | peak RSS up > 10 % |
| B3.4 | Regions / unique-tree drop | `own/region.rs`, runtime region chunks | binary_trees GC share 66.7 % -> <= 20 % | needs language annotations -> drop the phase |
| B4 | Measure the four loads against this baseline, gates green, merge | `tools/ownership/` | table in this file updated | - |

Expected gain (estimates, to be checked by B4): OpenPlan derive -5 to -10 % instructions (allocator share 9.1 % halved + fewer cycles); binary_trees up to 2-3x faster if B3.4 works (collector share 64.8 %); Certus 3-6 % CPU on string/GcVec-heavy pages; firnc1 0 % (guard only).

## 6. Measuring plan

- Same four loads, same scripts, A/B in the same session (`--ownership=off` vs `on`), callgrind instructions as the main number, user time as the second; Certus via `phases.py` with 3 alternating pairs (load noise).
- Per load: GC share, allocation count/bytes (state block), cycles, max pause, sum of pauses, max RSS, output checksum (binary_trees `expect_out`, OpenPlan dump md5, firnc2 = firnc1 bytes, Certus pixel score unchanged).
- Correctness: `--ownership=check` over `tests/` (all 4 stages), GC suite, `tools/gc_soak` short run, the four loads.
- Gates for the merge: Stage 2, fixpoint, self_compare shards, a64 run (pass is target independent, but free calls change codegen), `cargo test --release`.

## Sources

1. Reinking, Xie, de Moura, Leijen: Perceus: Garbage Free Reference Counting with Reuse, PLDI 2021 / MSR-TR-2020-42 — https://www.microsoft.com/en-us/research/publication/perceus-garbage-free-reference-counting-with-reuse/
2. Perceus TR v4 (benchmarks, Fig. 9, section 4) — https://microsoft.com/en-us/research/uploads/prod/2020/11/perceus-tr-v4.pdf
3. Ullrich, de Moura: Counting Immutable Beans, IFL 2019 — https://arxiv.org/abs/1908.05647
4. Lobster memory management — https://aardappel.github.io/lobster/memory_management.html
5. Lobster home page — https://strlen.com/lobster/
6. Nim destructors and move semantics — https://nim-lang.org/docs/destructors.html
7. Introducing ORC (Nim blog, 2020-12-08) — https://nim-lang.org/blog/2020/12/08/introducing-orc.html
8. Swift ARC optimization notes — https://fuchsia.googlesource.com/third_party/swift/+/refs/tags/swift-2.2.1-RELEASE/docs/ARCOptimization.rst
9. Swift SIL Ownership (OSSA) — https://sources.debian.org/src/swiftlang/6.2.3-1/swift/docs/SIL/Ownership.md
10. Mojo manual, death of a value — https://docs.modular.com/mojo/manual/lifecycle/death
