# The inliner and the frames, for the Certus browser (rounds FLOAT / INLGZ / TK2)

Origin: the Certus fork of the compiler (`/root/firnc-gc`, patches 0009, 0011,
0014, 0015 in `certus-ui/vendor/firn/firnc-gc/`), ported onto main in
`docs/CERTUS-COMPILER-MERGE.md` stage 2 and 3. This page says what the code
does and why; the numbers are the ones measured on the way.

## The problem

Certus is ONE module of 2.8 million FIR instructions. The old inliner embedded
at most `MAX_INLINES = 2000` call sites per module, first come first served
and restarting at function 0 after every embedding. In a program that is one
big module the budget is gone before the interesting code is reached:

* the 2000 went to start-up and UI code; `html.mem.cp_push` (11 instructions)
  stayed a call 64 million times per github.com load;
* the GC runtime's accessors (`__gc_ld64`, one load: 316 call sites in
  `lib/gc/gc.fi`) got 2 embeddings;
* the search for a site and for the call graph was linear, which made the
  build slow.

And once more embedding is allowed, the next problem shows up: a (self-)
recursive function pays every octet of frame growth once per recursion level.

## What the inliner does now (`compiler/src/inline.rs`)

1. **Trivial leaves, outside the budget** (`inline_trivial_leaves`). One block,
   no calls, no allocas, at most `MAX_TRIVIAL_INSTS = 6` instructions. A leaf
   cannot reach any caller (no recursion check) and its body is never bigger
   than the call sequence it replaces.
2. **The size-ordered phase** (`inline_by_size`). One pass per callee size, 7 to
   40; in each pass every caller embeds every eligible site of that size. The
   smallest bodies (where the call costs as much as the body) come first. Budget
   `MAX_PRIO_INLINES = 200 000` sites and a growth guard of
   `MAX_GROWTH_PCT = 40` per cent of the module (when it stops the phase it
   stops at a size boundary: everything smaller is in). The call graph is built
   once with an index and memoised reachability.
3. **The first-come loop** as before, for what is left (explicit `#[inline]`
   requests above the size bound, sites that only appeared through earlier
   embeddings), with its own budget.

Knobs (A/B and measuring): `FIRNC_NO_TRIVIAL=1`, `FIRNC_INLINE_OLD=1` (skip
phase 2), `FIRNC_INLINE_BUDGET=<n>`, `FIRNC_INLINE_GROWTH=<percent>` (a smaller
guard such as 10 or 20 builds faster and keeps most of the gain),
`FIRNC_INLINE_STATS=1` (numbers per tier and the frame budget decisions),
`FIRNC_INLINE_REC=0|1|2` (below).

## Recursive callers and the frame budget

A recursive caller used to get nothing from phase 2: with one slot per value
the regexp backtracker's frame grew from 7.8 to 10.5 KiB and `/(a|b)*c/` over
60 000 characters ran off the stack. Two changes make embedding safe:

* **Value slots by lifetime** (`regalloc.rs`, `compact_frame`, x86-64). After
  the allocation only the values still homed in a slot get one, and values
  whose live ranges do not overlap share it (a linear scan over the slots).
  Values that already share a slot (TEMPO 10, the copy on a back edge) keep
  sharing; a value that lives in a caller-saved register across a call keeps a
  slot of its own (`call_saves`); a function with a `v128` value keeps the old
  layout. `FIRN_RA_COMPACT=0` gives the old layout.
* **The frame budget** (`FIRNC_INLINE_REC=2`, the default on x86-64). A
  recursive caller takes the same sites as everybody else. Its body as it was
  before is kept (`REC_ORIGINALS`), and so is a middle version with the small
  bodies only (`REC_SMALL`); both go through the optimiser and phi
  elimination like the module. The code generator (`codegen_x86::emit`)
  compiles the candidates' frames (`regalloc::frame_bytes`) and keeps the
  embedded body only if its frame is at most `max(64, kept / 8)` octets larger
  than the kept one; otherwise the middle version under the same rule;
  otherwise the function goes out as it was. `FIRNC_INLINE_REC=0` is round INLGZ
  (nothing into recursive callers), `=1` embeds one-block bodies up to ten
  instructions.

On aarch64 and for the browser target the budget is not used
(`FIRNC_INLINE_REC` defaults to 0 there): the A64 allocator has its own,
simpler compaction (only values in memory get a slot, see
`docs/ROUND-A64-REGALLOC.md`) and no sharing by lifetime.

LIMIT: the call graph sees direct calls only; mutual recursion (a calls b calls
a) is not treated as recursion by the budget.

## Measured

See the results section of `docs/CERTUS-COMPILER-MERGE.md` for the gate
against the fork (callgrind instruction counts of the JavaScript benchmarks,
pages, recursion depth, build times).
