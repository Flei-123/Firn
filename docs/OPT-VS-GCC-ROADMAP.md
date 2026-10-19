# OPT-VS-GCC-ROADMAP -- LLVM-style passes for the Firn compiler (own code, no LLVM dependency)

Source: `bench/firn_vs_c/` (20 identical Firn/C pairs; x86_64, EPYC 7571, median of 5 CPU-time runs, main `72fdf0d2f`).
Geomean Firn `release-fast` / gcc -O2 = **1.30x**, / gcc -O3 = 1.32x, `release-safe` / gcc -O2 = 1.40x.
Even or ahead (<= 1.05x): crc32, floatloop, fnv_hash, hashmap, mandelbrot, matmul, primes, qsort.
Gaps: fib 5.0x, binary_trees 2.2x, memcpy_loops 1.6-2.7x, sha256 1.6x, strbuild 1.6x, jsonscan 1.6x, spectral_norm 1.5x, soa_access 1.3x, fannkuch 1.25x, nbody 1.2x, wordcount 1.17x, bitops 1.13x.

Coordination: r468 (a64 register allocation, branch `staff/fui-mvedlwxk`, files `regalloc_a64.rs`/`codegen_a64.rs`) belongs to the fUi manager. This roadmap touches the mid-end (FIR passes in `opt.rs`) and the x86 back end only.

## State of the pass pipeline (what exists, `compiler/src/opt.rs`)

| LLVM-style pass | Firn today |
|---|---|
| SSA form / pass pipeline | exists: `phi.rs`, `mem2reg.rs`, fixed-point driver `optimize_func`, per-pass switches (`--no-pass`), `phi_check` |
| Constant folding / propagation | exists: `fold`, `copyprop`, `simplify-term`, `thread-bool` |
| DCE | exists: `dce`, `merge-blocks` |
| CSE / GVN | PARTIAL: `cse` is dominator-tree CSE of PURE expressions (+ checked ops, r797). No load CSE, no store-to-load forwarding, no PRE |
| Inlining | exists: size heuristic, 2 rounds, leaf inliner for dev levels, requested inline (`inline.rs`) |
| LICM | exists: `licm` (pure values; loads only through `promote`) |
| Strength reduction / induction variables | exists: `strength`, `ivsr` (TEMPO 15) |
| Loop unrolling | PARTIAL: full unrolling of short constant trip counts (`unroll`); no partial unrolling by factor |
| Loop rotation / scalar promotion | exists: `promote` (cells in registers across loops) |
| Bounds-check elimination | exists: `bce` / `rangecheck.rs` (range analysis, `fuse-checks`) |
| Register allocation | exists: linear scan with intervals (x86 `regalloc.rs`); a64 density spill (`regalloc_a64.rs`, r468 owner fUi) |
| Instruction selection / peepholes | exists: `peephole.rs`, `rotate.rs`, a64 `spill_peephole`; x86 load-op folding incomplete |
| Alias assumptions | MISSING: FIR has no alias analysis (see `promote.rs`: only "where are the stores" reasoning) |
| Escape analysis / stack allocation | PARTIAL: `escape.rs` only diagnoses dangling local pointers; no heap-to-stack promotion of `gc class` objects |
| SIMD autovectorization | MISSING: `simd.rs`/`vec2reg.rs` are explicit intrinsics only; `memset` idiom exists, no `memcpy` idiom |
| Tail calls | MISSING: no tail-recursion elimination, no sibling-call optimisation |
| Whole-program specialization | PARTIAL: monomorphization of generics (`mono.rs`); no constant-argument specialization / cloning |

## Priority list (gain from the measured gap, effort S <1 d, M 1-3 d, L >3 d)

| # | Pass | Target benchmarks | Expected gain | Effort | Gate |
|---|---|---|---|---|---|
| 1 | Tail-recursion elimination with accumulator (`return f(..) op e`) + one-level self-inline | fib (5.0x), binary_trees (check walk), qsort | fib 5x -> ~1.5x; other recursion small | M | cargo test, fixpoint, a64 848 SAME, new tests |
| 2 | `memcpy` loop idiom (next to `memset`) + simple SSE copy/fill vectorization | memcpy_loops (1.6-2.7x), strbuild (1.6x) | 1.3-2x on copy loops | M | same |
| 3 | x86 instruction selection: load-op folding for ALU/SSE, 32-bit rotate idiom, lea chains, cmp+jcc | sha256 (1.6x), spectral_norm (1.5x), nbody (1.2x), bitops | 5-25 % on those | M | same |
| 4 | Alias facts: per-default no-alias for distinct `alloca`/heap roots + `restrict`-like param attribute; feeds load CSE and store-to-load forwarding | jsonscan, soa_access, fannkuch, wordcount | 5-15 % | L | cargo test, self_compare (fir_compare), fixpoint |
| 5 | Load CSE / redundant load elimination (GVN with memory state) | jsonscan, fannkuch | 3-10 % | M (after 4) | same |
| 6 | Partial loop unrolling (x2/x4) of simple counted loops with scheduling-friendly accumulators | matmul safe, floatloop, bitops | 3-10 % | M | same |
| 7 | Escape analysis + stack allocation of non-escaping `gc class` objects, bump allocator for GC young objects | binary_trees (2.2x) | 1.5-2x on alloc-heavy code | L | cargo test, GC tests, fixpoint |
| 8 | Real SIMD autovectorization (reductions, maps over u8/u32/f32/f64) across x86/a64/wasm | memcpy_loops, floatloop, crc32 tail, bytecount | 2-4x on loops that qualify | L | same + a64 gate |
| 9 | x86 spill choice by use density (port of the a64 r847 idea, NOT editing `regalloc_a64.rs`) | sha256, jsonscan, fannkuch | 3-10 % | M | same |
| 10 | Constant-argument specialization (function cloning) + sibling tail calls | wordcount, jsonscan helpers | 2-5 % | M | same |

Rule for every pass: switchable by name, Firn-only gate set (`firnfmt -c`, `tools/english/no_german_ci.sh`, `tools/hygiene/check.sh`, `RUST_MIN_STACK=268435456 cargo test --release`, `tools/fixpoint.sh` 2==3, `A64_TIMEOUT=900 JOBS=16 tools/aarch64/run.sh` 848 SAME), measured before/after with `bench/firn_vs_c/run.sh` through `/root/jarvis/bin/heavy` and the instruction count (callgrind, `tools/bench90/icount.py`).

## Started in this round

Passes 1, 2 and 3 are running as separate workers on their own branches (`staff/firn-tailrec`, `staff/firn-loopidiom`, `staff/firn-x86sel`); results and commit hashes are in the manager report.
