# Security audit: "Schrödinger's TOCTOU" (compiler-invented loads)

Date: 2026-10-13. Subject: <https://github.com/xoreaxeaxeax/schrodingers-toctou>
(README read; nothing from the repository was executed).

**Question:** can the Firn compiler duplicate a read of attacker-writable memory
(shared memory, user pointers in a kernel) so that a value that passed a check is
not the value that is used?

**Answer: no case found on any backend or build level.** The evidence is
measured (gate 140), not only read from the source. Details and the honest
limits are below.

## 1. The README pattern in Firn

`tools/toctou/toctou.fi` rebuilds `receive()` (`let local: Message = SH` →
check `local.len <= 20` → publish `*out = local`) plus seven variants:
scalar snapshot under heavy register pressure and calls, by-value struct
parameter (inlined and not inlined), nested struct copy with a narrow
(`u8`) re-use, loop bound from the snapshot, `n + 4` check followed by an
arithmetic use, and a struct copy followed by a field check.

`tools/toctou/check.sh` (gate 140) compiles it with `--emit=asm` (x86-64,
aarch64) and to wasm (`wasm2wat`) at `dev`, `dev-fast`, `release-safe`,
`release-fast` and requires **exactly one read of the shared symbol per
function** (one scalar load or one bulk copy). A mutation run proves the
checker fails when a second read is expected.

| Backend | dev | dev-fast | release-safe | release-fast |
|---|---|---|---|---|
| x86-64 (firnc0) | no | no | no | no |
| aarch64 (firnc0), asm text only | no | no | no | no |
| wasm32 (firnc0) | no | no | no | no |
| firnc1 (x86-64, no optimizer) | no (checked by hand, 1 read per function) | - | - | - |

"no" = not affected for the tested programs. The first-order cause: the
snapshot is a `copymem` into a frame slot (`rep movsb` on x86-64, load/store
words or a byte loop on aarch64, `memory.copy` on wasm); every later read hits
that frame copy.

## 2. Pass review (why it holds)

| Cat-state class | Firn |
|---|---|
| Rematerialization | Only constants/addresses are rematerialized (`regalloc_a64.rs` `cheap_const`, `codegen_wasm.rs` `plan_trees`: `Const`, `GlobalAddr`, `FnRef`, `VtabAddr`). x86-64 `regalloc.rs` has no remat at all. Loads are never rematerialized. |
| Reload after spill from the original | Linear scan does **not split** intervals; a spilled value lives in its own frame slot for its whole life (`regalloc.rs` header). Reloads read that slot. `split.rs` makes `copy` values, never loads. Checked ops (round 90) reload operands from their homes, not from memory. |
| Bulk copy vs scalar overlap | `Op::CopyMem` is a clobber for `mem2reg`/`forward_local_loads`; `sroa.rs` refuses any alloca used by a `copymem`; no pass turns a scalar read of the frame copy into a read of the source. |
| Width-mismatch reload | No pass re-reads memory at a different width; `forward_local_loads` only merges loads (fewer reads). |
| CISC mem-op fold | `foldable_addresses`/multi-fold fold *addresses* into one access only; a load is never folded into several ALU instructions. A twice-read address is not folded (test `twice_read_address_becomes_not_folded`). |
| Duplicating loads in the CFG | `licm.rs` hoists only pure ops and read-only data; `promote.rs` rotation duplicates the loop test (dynamic sequence unchanged); `threading.rs` duplicates blocks, each path still executes one read. |
| firnc1 | Has no optimizer; every FIR value gets its own frame slot (`lib/firnc1/codegen.fi`). |

## 3. What Firn does NOT promise (residual risks)

1. **Ordinary loads are not volatile.** The compiler may *remove or merge*
   reads (CSE of read-only data, `forward_local_loads`, wasm single-use load
   sinking within a block, `promote.rs`). `promote.rs` keeps a memory cell in a
   register across a loop, writes it back before an inner region and **loads it
   again after the region** (seen in release-fast: a next-iteration read is
   issued at the end of the previous iteration). In the measured case the
   reloaded value is the one that is checked afterwards, so check and use stay
   consistent, but: polling loops on shared memory can be hoisted, and stale
   register values can be written back over concurrent writes. Shared memory
   that is polled or written concurrently needs `read_once`/atomics (below).

   **Wait loops (r489, fixed):** a loop whose exit test is a pure function of
   loop-invariant values and exact loads of a cell that nothing in the loop
   writes (`while *flag == 0 {}`, also with a bound `&& n < LIMIT`) is a wait
   for memory that another thread / a signal / hardware changes: without a
   concurrent writer it never loops or never ends. `promote.rs` (`polls_cell`)
   now leaves such a cell in memory, so the load stays in the loop (tests/2944).
   Not covered: loops that also write the cell, and any real synchronisation
   need (ordering, torn values): use atomics / `read_once` for those.
2. **References and raw pointers are read at every source access.** A `&T`/
   `inout T` parameter or `*T` is not a snapshot: `p.len` twice in the source
   is two reads (by design, same as C without the compiler-invented ones).
   Snapshot first: `let snap: T = *p`.
3. **A struct snapshot is not atomic.** `rep movsb` / word loops may see a torn
   mix of old and new bytes; the copy is *consistent with itself* (check and
   use see the same bytes), but not a point-in-time image.
4. **Future risk:** `sroa.rs` documents splitting of `copymem`-ed allocas as the
   "next step". It would turn the copy into per-field loads of the source; it
   must keep exactly one load per field. Gate 140 fails if a second read ever
   appears.
5. **Not verifiable here:** real aarch64 hardware (only the emitted text was
   inspected, not run), JIT behaviour of wasm engines (data races on non-atomic
   shared-memory accesses are unspecified in the wasm threads model), and kernel
   `copy_from_user` fault handling. No LLVM backend exists.

## 4. "Read exactly once" primitive

`std.mem.read_once_u8/u16/u32/u64(p)` (new, `lib/std/mem.fi`) lower to
`fir::Op::MmioLoad`: one machine load that **no pass may merge, move, remove,
duplicate or rematerialize** (`fir.rs`, `mem2reg.rs`, `promote.rs` treat it as
a wild effect; the code generators emit a single instruction). It survives
inlining (`inline.rs` copies the op once). Verified: `tests/2899_read_once.fi`
at all four levels on x86-64; the wrapper compiles for aarch64 and wasm32
(not executed there).

Recommended pattern for untrusted memory:

```firn
let n: u32 = mem.read_once_u32(&(*shared).len)   // or: let snap: Msg = *shared
if n > 20 { return }
copy(out, data, n)                               // only the local n, never (*shared).len
```

## 5. Reproduce

`bash tools/toctou/check.sh` (needs `wasm2wat` for the wasm rows); wired as
stage 140 of `test.sh`.
