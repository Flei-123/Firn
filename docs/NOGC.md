# Ownership-only mode `--no-gc` (r1556)

Goal: Firn runs like Rust without a collector. `firnc --no-gc` links no GC runtime: no `gc_init`, no write barriers, no stack scans, no state block.

## Status

- firnc0: implemented (`compiler/src/gcfree.rs`, hook in `modules.rs`, runtime `lib/gc/nogc.fi`).
- firnc1: refuses the flag with a clear error (`bin/firnc1.fi`); parity is roadmap.
- Gate: `bash tools/nogc/run.sh` (compile with `--no-gc`, output, no `gc_init`/`__gc_*` symbol, same output as the GC build, negative cases with position, `--ownership` conflicts).

## Rules in this mode

- `gc class`, `gc fn`, `Gc[T]`, `GcWeak[T]`, `GcVec*`, `GcMap*`, `gcvec_*`, `gcmap_*` are compile errors: `'gc class' needs the garbage collector, which '--no-gc' leaves out` (token position, `help:` line).
- `--no-gc` implies `--ownership=strict`; any other explicit `--ownership` value is refused (`'--no-gc' requires --ownership=strict`), in either order.
- The runtime is replaced by `lib/gc/nogc.fi` (`__str_eq`, `__gc_blit`, `__str_copy` that aborts with exit 70). It is only added when a `str` token shows up; programs without `str` carry no runtime at all.
- No `str` allocation: `a + b` on `str` is the existing error `the operator '+' on str needs the collector runtime` (no `__str_concat` in this runtime). `__str_copy` (used by `io.fmt_string` and the `fmt_*` helpers that return an owned `str`) exists only so `std.io` still type-checks; a call aborts at run time.
- Memory comes from Vec/Map (`rt.heap_alloc`), `std.str.Bytes`, `std.rc`, `lib/mem/slab.fi` (new, fixed-size objects), arenas. The compiler does not insert frees in this mode (nothing it manages is on a GC heap); frees are explicit (`drop_tree` in `tools/nogc/cases/binary_trees.fi`).

## What hangs on the GC (analysis)

Runtime: `lib/gc/gc.fi` 218 KB, `gcvec.fi` 12.6 KB, `gcmap.fi` 10.4 KB, `own.fi` 20.7 KB; embedded by `gc.rs::runtime_source` (firnc0) and `lib/firnc1/gctext.fi` (firnc1).

Pulled in by token scan (`gc.rs::source_needs_gc`, `modules.rs::gc_runtime`, `gc_source_scan` in firnc1):
- `gc class` / `gc fn` (closure records are GC objects).
- the identifier `str` (not next to a dot) or a literal next to `+ == !=`: `str` lives in the runtime (`__str_eq`, `__str_concat*`, `__str_copy`, `__own_str_*`). This is why plain `io.print_line("x")` programs carry the collector (hello: 15.7 KB / 133 KB at dev-fast).
- identifiers `thread_*` / `__thread*`: the thread runtime shares the state block (`lib/std/process.fi`, `net.fi`, `pool.fi`, `dynlib.fi`, `time.fi` mention them).
- `GcVec*`, `GcMap*`, `gcvec_*`, `gcmap_*`.

std and runtime users:
- No GC: `std.vec`, `std.map`, `rt` buffers, `std.str` views and `str.Bytes`, `std.io` printing and `Fmt` (`f"..."`, `io.fmt_print_line`), `std.rc` (RcHeap), numbers, hashes.
- `gc_track`/`gc_untrack` in `std.vec`/`rt.vec`/`rt.map`/`std.deque`: compile-time no-ops when `T` holds no GC pointers (`has_gc[T]`).
- GC bound: `std.gcobj`, `std.scope`, `std.mem` (Gc references), 14 std files that call `__str_copy`/`__str_concat` (`log`, `dict`, `hashfile`, `secret`, `pystr`, `fs`, `extract`, `shell`, `env`, `args`, `fsx`, `dir`, `time`, `io.fmt_string`), 76 lib files in total.
- Outside std: `browser`, `js`, `layout`, `dom`, `css`, `paint` (DOM and JS values are cyclic; they stay GC programs).

## Measured (release-fast, same output hash)

| Program | GC build | `--no-gc` |
|---|---|---|
| binary_trees (depth 16) binary | 66968 B | 11136 B |
| binary_trees max RSS | 8192 KB | 4096 KB |
| binary_trees time | 0.19 s | 0.12 s |
| hello (`io.print_line`) binary | 15688 B | 9976 B |

The GC binary_trees is `bench/firn_vs_c/firn/binary_trees.fi` (`gc class`); the no-gc one uses `lib/mem/slab.fi` and recursive `drop_tree`.

## Open

- Owned `str` in no-gc mode: allocation for `+`, `fmt_string`, `pystr` results with compiler-inserted frees (producer registry in `own/fir/registry`, free function over `rt.heap_free`/slab).
- Threads without the GC state block (`thread_*` need their own runtime).
- firnc1 parity (flag, scan, nogc runtime text, same error text and position).
- Edition/manifest option (`gc = off` in `firn.package`).
- A lint/transform for std modules that return GC `str` (`#[gc_str]` marker, so the compiler can name the offending call instead of a run-time abort).
- Rc/Arc pair with cycle detection is out of scope (`std.rc` leaks cycles on purpose).
