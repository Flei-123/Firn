# TEMPO 15 -- the MP3 synthesis without the detour through memory, and strength reduction of induction variables

State before: `main` at `dcf4d285` (TEMPO 1-14 and round OPT-GENERAL
merged). MP3 decoder, 8 s of sound (callgrind, `release-fast`): **107.61
million instructions**, `minimp3` in C with `gcc -O2`: 74.8 million.
`bench/firn/matmul.fi`: 459.6 million.

The two tasks of this round were the two items TEMPO 14 left as "worth
doing next": `synth` without the detour through memory, and strength
reduction for `matmul`. Measuring the first one led to five more causes,
all general, none of them specific to the decoder.

## What is built

1. **`vec2reg.rs`** (in the `sroa` slot, release levels). A sixteen octet
   stack cell used only as ONE vector -- zeroed, written with
   `__v128_store`, read back lane by lane or as a vector, all in one block
   -- becomes a `v128` value. Lane reads become `pshufd` + `movd` (SSE2),
   and `(lane as i16)` becomes `pextrw` directly (new internal kind
   `SimdKind::GetU16`, emitted by all four back ends). `synth` had four such
   arrays per pass: 16 zero stores, 4 vector stores, 2 vector loads and 8
   lane loads, each behind an address reloaded from the frame. The register
   allocator learned `GetU32`/`GetU16` (before, one of them sent the whole
   function down the base path).
2. **`ivsr.rs`** (new pass, slot 14, release levels). Strength reduction of
   induction variables: for a loop with one back edge and a preheader,
   every value `base + a * iv + b` (`a`, `b` loop invariant; `add`, `sub`,
   `mul`, `shl`, `ptradd`; plain or wrapping arithmetic in the type of the
   induction variable, so the identity is exact modulo 2^w) whose chain
   costs at least two instructions per pass becomes a phi of its own,
   stepped by `a * c`. Scaling by 1/2/4/8 and a last addition feeding only
   addresses count nothing (x86 folds them into the operand). Dead phi
   cycles and twin phis that the pass leaves when it runs before and after
   inlining are cleaned up by the pass itself. `FIRN_NO_IVSR=1`,
   `FIRN_IVSR_TRACE=1`.
3. **Inliner.** (a) The scan no longer restarts at function 0 after every
   embedding -- an embedding only changes its caller; same result, linear
   instead of quadratic. (b) A **second round** after the clean-up of all
   functions, restricted to call sites inside loops and to callees that call
   nothing. Reason: the module bound of 2,000 embeddings was used up before
   the MP3 functions were reached, and the first round is not bottom-up --
   `scale_pcm4` got three copies of `bcast` first and was then, unoptimized,
   above the size bound when `synth` asked for it. `FIRN_NO_INLINE2=1`,
   `FIRN_MAX_INLINES=<n>`.
4. **`licm`/`cse`/`dce` and constants.** Loads from an IMMUTABLE `static`
   (`.rodata`, `statics::rodata_size`) are loop invariant and cannot fault
   inside the object: `licm` moves them, `cse` merges them. `licm` also
   moves `GlobalAddr`/`FnRef`/`VtabAddr` and the pure vector instructions
   (none of them traps); `cse` merges those too. A vector load without a
   reader may go (`Op::is_pure`), like a scalar one.
5. **Address folding with several readers** (`regalloc.rs`). `base + k`
   whose every reader is a memory access through it is written as
   `[base + k]` at each reader when the base is live there.
6. **`addrsink.rs`**: before allocation, such an address is copied in front
   of each of its accesses, so that each copy folds (`[base + k]`, no `lea`,
   no register for the address). That lengthens the base's life. A first
   version allocated and emitted BOTH versions and kept the one with fewer
   instructions (each weighted 8^(loop depth) of its block): over
   `bin/firnc1.fi` 174 of 260 functions were better sunk, the other 86 lost
   3,587 weighted instructions against 27,121 won, and on the MP3 decoder and
   the bench bank the choice changed not one executed instruction -- but it
   doubled the code generation time (+0.8 s on `bin/firnc1.fi`). Sinking is
   therefore unconditional; `FIRN_ADDRSINK_CHOOSE=1` (+ `FIRN_ADDRSINK_DBG=1`)
   keeps the comparison as a measuring aid, `FIRN_NO_ADDRSINK=1` switches it
   off. (An even earlier version compared the number of values in the frame
   -- the wrong measure: folded addresses count there although they are
   never materialized, so it rejected every function.) Without item 5 next
   to it the decoder is 1.9 M instructions slower (the negative offsets,
   which the single reader fold does not take, go through item 5).
7. **`cmp` + `jcc` behind phi copies** (`regalloc.rs::cmp_behind_copies`).
   After phi elimination a latch reads `cmp; copy; copy; brcond`; the fusion
   wanted the comparison LAST and materialized the bool (`setcc`, `movzx`,
   `test`, `jnz`). An integer comparison now moves behind the copies when
   no copy writes a place (register or slot) it reads. `FIRN_NO_CMPSINK=1`.

## A bug on `main` found on the way

`mem2reg::forward_local_loads` did not count `__v128_store` /
`__v128_store64` as memory writes: `load p+8; simd.Store p, v; load p+8`
forwarded the first load to the second. `tests/1723_vec2reg.fi`
(`read_before_write`: six passes, inlined and unrolled into `main`) gave 0
instead of 35 at `release-safe` and `release-fast` on `main`.
`threading.rs::disturbs_memory` had the same gap (plus `AtomicCas`, `Asm`,
`MmioStore`, `ThreadSpawn`). Both fixed.

## Measured

MP3 decoder, 8 s, callgrind, `release-fast`, output bit identical (8 s and
60 s against `ref60.pcm`), the decoder source (`lib/ton`, branch `ton`)
UNCHANGED:

| function | before | after | C (`gcc -O2`) |
|---|---|---|---|
| `synth` + `scale_pcm4` | 34.81 M | 25.97 M | 22.7 M |
| `l3_imdct36` + `l3_dct3_9` | 19.83 M | 16.25 M | 12.9 M |
| `dct_ii_4` + `dct_ii` | 12.69 M | 10.82 M | 11.1 M |
| `l3_huffman` | 16.37 M | 15.28 M | 12.1 M |
| **total** | **107.61 M** | **91.64 M (-14.8 %)** | 74.8 M |

`.text` of the decoder: 227,219 -> 215,931 octets.

With two lines changed in the decoder (branch `ton`): the rounding
constants `PCM_KONST`/`PCM_GRENZEN`/`DCT_KONST` as `static` instead of
`static mut` (they are never written) and `PCM_KONST` padded to eight
values (the three `bcast` loads read sixteen octets from `+4` and `+8`,
past the end of a four-value array -- `licm` rightly refuses to move a load
that leaves its object): **90.59 M**.

Benchmark bank (`bench/firn`, `release-fast`, instructions, output identical
old/new): `matmul` 459.6 -> 418.7 M (**-8.9 %**, inner loop 11 -> 10
instructions, two stepped pointers), `gc_barrier` 2051.8 -> 1901.8 M
(**-7.3 %**), all others unchanged.

The self-hosting compiler `bin/firnc1.fi` built by the new `firnc`:
executing `firnc1 tests/1704_unroll.fi` costs 3.624 -> 3.327 G instructions
(**-8.2 %**, all of it from the second inline round), same output.

**Compile time.** `firnc --opt-level=release-fast bin/firnc1.fi` on this
(shared, loaded) machine: 4.0-4.2 s before, 4.4-4.6 s after (about +10 %).
Of that, the second inline round is the larger half (it re-optimizes the
callers it changed -- 940 more fixpoint rounds), `ivsr` about 90 ms. The
first version of this round cost +30 % (5.5 s): the double emission of
`addrsink`, a whole-function scan in `ivsr` on every call (now: cheap exits,
clean-ups only in functions the pass changed), `rodata_addrs` per loop, and
reader lists for every value. `.text` of `firnc1`: 1,198,165 -> about
1,197,000 octets (the sunk addresses save more than the inlining adds).

## Tried and dropped

* **Unbounded inlining** (`FIRN_MAX_INLINES=100000`): `bin/firnc1.fi` did
  not finish in minutes (the quadratic restart, now fixed). With the fix and
  a bound of 20,000: 20 s instead of 5 s, `.text` 1.20 -> 1.77 MB.
* **Second inline round for everything**: `.text` of `firnc1` +17 %,
  compile time +70 %. Restricted to leaves in loops: +1.4 %.
* **Strength reduction of every chain with a multiplication**: MP3 +0.7 M
  (`dct_ii_4` +0.64 M, `l3_imdct36` +0.20 M): `j * 18` is one `imul`, the
  phi one `add` plus a register for the whole loop. Hence the cost model.

## Tests

`tests/1723_vec2reg.fi` (seven shapes: the `synth` shape in a loop, a
zeroed cell read as a vector, read before write -- the one that was wrong
on `main` --, a float lane of a vector, a non-zero lane then a vector
read, an escaping address, cell to cell) and `tests/1724_ivsr.fi` (matmul
against a reference, counting down times a run time value, a wrapping u8
product, a run time step, a step by subtraction, nested loops, a product
carried out of the loop, `break`, a wrapping i32 product), both in every
build level.

## What is worth doing next

1. **Unroll loops with a second exit** (`while s < 4 && !fertig` in the
   count1 part of `l3_huffman`: 211,084 passes of a loop the trip count of
   which is at most four). gcc peels those completely.
2. **Inline `l3_dct3_9` into `l3_imdct36`** (C does): then the two nine
   value arrays `co`/`si` are pure registers -- the remaining 3.4 M of the
   imdct gap.
3. **IVSR across a sign extension** when the range of the induction
   variable is known (constant start, step and bound): `synth` keeps eight
   i32 phis and `movsxd` + `lea` for every address.
