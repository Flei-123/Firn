# Round OPT-GENERAL -- faster in general, WebAssembly first (2026-09-24)

Occasion: the LogicLab simulator (`/root/bench/simcmp/sim.fi`) ran natively at
C++ level, but as WebAssembly only 1.0-1.6x faster than tuned JavaScript and
on idle circuits SLOWER (88 M vs 147 M ticks/s).

## 1. Struct fields in registers across loops (`compiler/src/promote.rs`)

Cause, measured: the tick loop of `run()` loaded eight fields of `*s` and
stored six on every tick (`(*s).tick`, `read_q`, `write_n`, ...). Kept in
locals by hand the loop ran 1.6x faster; keeping only the loads out and the
stores in gained nothing -- the stores must leave the loop too.

FIR has no alias analysis, and the loop stores octets through integer
addresses that may hit `*s`. What can be proven is WHERE those stores are:
in the inner loops. The new pass (`promote`, release levels only):

* rotates `while` loops into guarded do-while form (the guard runs the test
  once, the preheader runs only if the body does; header values that are used
  elsewhere go through a slot and `mem2reg` rebuilds SSA);
* keeps a cell `(root, constant offset, type)` in a register across the loop,
  writes it back in front of each inner loop that may touch it (a REGION) and
  reads it again at every exit of that region. An inner loop that does not run
  costs nothing -- that is the idle case;
* conditions (module text of `promote.rs`): nothing in the loop that may
  unmap memory (munmap/mremap/brk/mprotect/madvise/clone/fork/execve, inline
  assembler, indirect calls, callees that may -- a call graph summary); every
  other write of possibly the cell inside a region; an access (and, for a
  written cell, a store) in a block dominating every latch and exit.

Bug found and fixed on the way: rotation did not route phi entries IN the
header that name header values (a flag unchanged on `continue`) through the
slot -- the JS lexer crashed (tests 1501/1503). Regression test
`tests/1720_promote_regions.fi` (aliasing store into the promoted field,
idle inner loop, the `continue` flag).

Compile time: a cheap pre-check and a call graph worklist keep the pass at
67 ms for `bin/firnc1.fi` (about 3 %).

Switches: `FIRN_NO_PROMOTE=1`, `FIRN_PROMOTE_TRACE=1`, `FIRN_PROMOTE_ONLY=fn`,
`FIRN_PROMOTE_LIMIT=n` (bisecting), `FIRN_PROMOTE_ROTATE_ALL=1` +
`FIRN_PROMOTE_CLEANUP_FIRST=1` (stress mode: rotate every loop).

**Native needs the register allocator of TEMPO 1-13 (branch `unroll`).** With
the old allocator of main the promoted loop phis spill (sim.fi chain/rand 3x
slower); with the new one it is a clear gain (below).

## 2. The WebAssembly backend

* **Expression trees** (`plan_trees`): a value used once, in the same block,
  by an instruction that reads each operand once, is computed at its use and
  stays on the operand stack. Constants and data addresses are computed again
  at every use. Anything that writes a value a tree reads blocks it -- after
  phi elimination that includes ordinary instructions (`phi.rs` coalesces;
  found by the Mandelbrot kernel, `tests/1721_wasm_tree_phi.fi`).
* **Offsets** of `add/ptradd base, constant` go into the memory instruction.
* **32-bit addresses** (`plan_low32`): a 64-bit value of which every use needs
  only the low 32 bits (addresses, their sums, products, constant shifts) is
  kept and computed as `i32`.
* **Locals packed by liveness** (`wasm_locals.rs`): basic blocks of the
  structured code, liveness, interference, greedy colouring per type; a local
  read before written keeps its own slot. `run()` of sim.fi: 325 -> 42 locals.
* Peepholes: `if br a else br b end` -> `br_if; br`, self copies vanish,
  `set x; get x` -> `tee x`.

Switches: `FIRN_WASM_NO_TREES`, `_NO_OFFSET`, `_NO_LOW32`, `_NO_PACK`,
`_NO_PEEP`, `_NO_REMAT`.

## 3. WebAssembly SIMD

`v128` is a value type now. The SSE-style intrinsics map to WebAssembly
SIMD with their exact x86 meaning (`pshufb` -> `swizzle` of `b & 0x8F`,
`pshufd`/`palignr`/`punpck*`/`pslldq`/`psrldq`/`pblendw` -> `i8x16.shuffle`,
`pandn` -> `andnot` with swapped operands). The crypto intrinsics (AES,
SHA-256, PCLMUL, crc32) have no WebAssembly form: they trap, and
`__cpu_features()` answers SSE2|SSE4.1|SSSE3, so code that asks first takes
its scalar path. `tests/1614_simd_ops.fi` moves from REFUSED to SAME; the
encoding agrees with `wat2wasm` octet for octet. `FIRN_WASM_NO_SIMD=1` restores
the refusal. (The f32x4 kinds of TEMPO 4/5 are mapped as well once `unroll`
is on main; checked there against tests 1615-1617.)

## 4. WebAssembly threads

Feasible, planned in `ROADMAP.md` ("WebAssembly threads -- the plan"): the
collector stops the world cooperatively (safepoints + futex), so shared
memory, atomics, a worker-based thread start and a per-instance thread
pointer are enough -- a round of its own.

## Measurements (best of 2-3, loaded 20-thread machine)

sim.fi, ticks/s (`/root/bench/simcmp/opt/q.sh`, Chromium via `opt/web/cmp.sh`):

| circuit | nat before* | nat after* | wasm node18 before | after | Chromium 151 before | after |
|---|---|---|---|---|---|---|
| idle-1000 | 149 M | 341 M | 89 M | 283 M | 94-98 M | 250-420 M |
| chain-1000 | 74.8 k | 72.1 k | 50 k | 45-53 k | 52-54 k | 47-50 k |
| rand-10000 | 2.47 M | 2.43 M | 1.86 M | 2.00 M | 1.62-1.94 M | 1.72-1.83 M |
| fanout-1000 | 123 k | 141 k | | | 88 k | 88 k |

\* native with the allocator of `unroll`, promote off/on.

Seven C kernels of Certus (`tools/wasm/tempo/*.c`) ported 1:1 to Firn
(`/root/bench/kern`), ms, Firn wasm before -> after (and C = clang -O2 wasm):

| kernel | V8 node18 | Liftoff | Certus AOT |
|---|---|---|---|
| sieb | 258 -> 240 (C 229) | 848 -> 636 (525) | 633 -> 280 (242) |
| matrix | 32 -> 25 (16) | 182 -> 141 (84) | 203 -> 44 (25) |
| mandel | 162 -> 153 (154) | 319 -> 288 (213) | 315 -> 167 (143) |
| fib | 96 -> 38 (25) | 86 -> 21 (20) | 46 -> 25 (17) |
| sortieren | 376 -> 364 (334) | 829 -> 617 (643) | 947 -> 413 (415) |
| hash | 65 -> 64 (65) | 223 -> 178 (100) | 244 -> 67 (64) |
| nbody | 261 -> 272 (242) | 1026 -> 463 (351) | 1540 -> 391 (277) |

Every checksum equals the C one on every engine.
