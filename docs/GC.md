# Firn GC — overview

Opt-in tracing collector (SPEC 3.5). Only `gc class` / `Gc[T]` / `GcWeak[T]` live on the GC heap. Everything else (stack, `struct`, arenas, `rt.heap_alloc`, `Rc`/`Arc`) is not touched by it.

Files: runtime `lib/gc/gc.fi`, type table + parser `compiler/src/gc.rs`, lowering + barrier `compiler/src/gc_lower.rs`, `#[no_gc]` check `compiler/src/nogc.rs`.
The runtime is pulled in automatically as soon as a `gc class` appears; no import needed.

## Kind

- Mark-sweep, **non-moving**, **not generational**.
- **Incremental** (tri-colour; always incremental since round 44, `INCR_FROM = 0`).
- Heap tracing **precise** (compiler-generated type table, field offsets per class).
- Stack and registers **conservative** (callee-saved registers saved by `__gc_save_regs`). Interior pointers are allowed. Consequence: false retention possible, no compaction.
- Collection runs only at GC allocation sites and in `gc_collect()`. No safepoint polls, no signals. A loop without GC allocation blocks collection.
- Finalizers (after collection, no resurrection) and weak refs (zeroed on collection) exist.

## What is managed

| Managed by GC | Not managed |
|---|---|
| `gc class` objects (`Gc[T]`) | stack values, `struct`, tuples |
| | arenas, `rt.heap_alloc` / `heap_free` (manual) |
| | `Rc` / `Arc` (cycles leak on purpose) |

`#[no_gc]` on a function: compiler proves (transitively) no GC alloc, no `Gc` field write, no call to a non-`#[no_gc]` function. No pause, no barrier inside that call tree.

## Write barrier

- Dijkstra insertion barrier, only when a `Gc[T]` field is written.
- Fast half inline at every store (`gc_lower.rs::emit_barrier`); slow half `__gc_barrier_slow` only while a cycle is marking or sweeping.
- Measured: 11.6 -> 2.9 ns per store (dev-fast, `docs/GAPS.md` B4).

## Allocator

- 16-byte block header (type id, mark/finalizer bits, serial number), payload behind it.
- 14 size classes (payload 32 ... 4096 as ids 0-12, plus a 16-byte payload class as id 13, added last so no id changed), 256 KiB chunks (64-byte chunk header), per-class free lists. Chunk header word 56 holds the reciprocal `ceil(2^32 / block)` so `__gc_block_of` replaces the division by a multiply and shift (exact for offsets below 2^18; single-object chunks keep the division).
- Large live sets: sweep releases fully empty chunks (with hysteresis).

## Threads

- Spec: one heap per thread, `Gc[T]` is not sendable.
- Runtime: GC lock + stop-the-world for multi-threaded programs (`S_STOPP`, `__gc_stw_an`), per-thread free lists (`SCHWUNG = 32` blocks per fetch).
- Single-threaded programs pay one memory read and one predicted branch (`S_MULTI`).

## Pacing

- Next cycle starts when bytes allocated since the last cycle reach `limit = live * num / den`, clamped to `[MIN_LIMIT = 1 MiB, MAX_LIMIT = 256 MiB]`.
- **Default growth 2/1** (branch `staff/firn-mvfq84re`; before: 1/1). Peak heap is about 3x the live set.
- `gc_set_growth(num, den)` changes it (`(0, 0)` restores the default). `gc_set_limit(n)` sets the threshold in bytes (`u64::MAX` = never start a cycle).
- Slice budget: `SLICE_TRACE = 512` objects or `TIME_BUDGET_NS = 100000` (100 µs) per allocation step, clock probed every `TIME_PROBE = 16` objects.
- `gc_cycle_finish()` ends a running cycle without starting a full mark.

## Pauses (measured, see ACCEPTANCE.md; old numbers, not re-measured)

| Case | Value |
|---|---|
| Longest incremental pause (round 44) | 0.45 ms |
| DOM soak: 700 M objects, 47,300 collections | longest pause 3.54 ms, RSS constant 1,364 KiB |
| Whole interruption incl. root rescan (round 91, ~11 MiB live) | 114 ms (74 ms without rescan) |
| Full `gc_collect()` on Android | 270 ms at 25 MB live, 741 ms at 40 MB |

Read them with `gc_pause_ns_last/max/total`, `gc_hist`, `gc_stop_max`, `gc_pause_reset`.

## Cost

- Memory: 16 B per object + size-class rounding; peak about 2x live (1/1) or 3x live (2/1).
- Time: in `binary_trees` the GC is about 65 % of run time (re-mark of long-lived data, sweep). Stack allocation does not help there (nodes escape).
- binary_trees (callgrind, release-fast): 2.958 G -> 1.997 G instructions with growth 2/1 and a leaner alloc fast path; with the 16-byte class and the reciprocal 1.918 G, peak RSS 35 -> 24 MB (a 16-byte payload used to take a 48-byte block, now 32).

## Versus Rust / C

- Rust: no GC, zero runtime cost, no pauses. Firn level 1/2 (values, references, `Rc`) behaves the same; the GC is only for cyclic graphs (DOM, JS engine).
- C: manual memory. Firn code that avoids `gc class` pays nothing for the GC.

## Limits / open

- No generations: long-lived data is re-marked every cycle.
- Non-moving: fragmentation is the main risk; conservative stack: old pointer copies keep objects alive.
- Root rescan costs about 13 ms per collection at ~11 MiB live.
- 24 h soak not run (ACCEPTANCE.md).
- Next: generations.
