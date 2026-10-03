# Runde XMM 3 -- die zweite Registerklasse

State 18.09.2026, branch `xmm-ra`. This is the step that `docs/TON2.md`
and `docs/XMM1.md` point to: **the register allocator can now do floating point.**

## The sentence that previously stood in `regalloc.rs`

```rust
// FLOATING POINT: this allocator knows only the integer registers.
if f.val_types.iter().any(|t| t.is_float()) {
    return Some("f64 in the value set".into());
}
```

Every function in which even a single `f32` occurred thereby went through the
basic path -- and that gives EVERY intermediate value a slot of its own in the frame.
Six memory accesses for one multiplication.

## What this round built

1. **A second pass of the linear scan** for the SSE class
   (`xmm4`-`xmm15`; `xmm0`-`xmm3` stay scratch registers). It is simpler
   than the integer pass: all twelve registers are equivalent.
2. **A hard rule instead of a save:** on System V ALL
   sixteen `xmm` are caller-saved. A value whose lifetime crosses a call
   therefore gets no register -- there is none that the call does not
   destroy.
3. **`fp_taugt`** -- a value gets an `xmm` only if EVERY place
   that produces or reads it stands in the floating-point path of the output. The
   reason is the construction of the generator: in many places there stands
   `ra.load_full(e, "rax", v)`, and that emits `mov rax, <slot>`. If an
   `xmm` stood there, that would be a silent error. Outside therefore stay
   parameters, call results, `Select`, checked conversions and everything
   that serves as a call argument.
4. **The output** for floating point in the allocator path: constants, `Copy`,
   loading and storing (also from promoted cells), the four
   basic arithmetic operations, comparisons (with the NaN rule via the parity bit),
   all conversions, calls, return and the prologue.
5. **The calling convention counted correctly:** System V keeps two
   register sequences independent of each other (integers in `rdi`…`r9`,
   floating point in `xmm0`-`xmm7`). The allocator path previously counted the
   position stubbornly -- right as long as no function with floating point arrived.

## Four bugs that measuring found

1. **Floating-point constants stood in the list of immediate
   operands.** SSE has no form with a constant; putting the bit pattern as a number into
   a `movss` operand yields nonsense. (`immediate_consts`
   now skips them.)
2. **`&t` on a local floating-point number** -- an `alloca` whose address stands fixed
   in the frame has no slot at all in which the address would stand. Wanting to fetch it
   with `load_full` reads a slot that was never written:
   pointer into nothing, crash.
3. **Address calculations that moved entirely into the memory access**
   (`foldable_addresses`) -- the same case, another cause. Both are now solved
   by `addr_mem`.
4. **NaN.** `ucomis*` sets ZF *and* PF with NaN. `sete` alone would see
   `NaN == NaN` as true, `setne` would see `NaN != NaN` as false. The
   parity bit corrects it -- and because the correction does not fit between comparison
   and jump, there is no more merging for floating-point equality.

## Die Messung

All `--opt-level=release-fast`, same machine, same program.

| Measurement case | Start | after XMM1+2 | **after XMM3** | C (`gcc -O2`) |
|---|---|---|---|---|
| `f32` kernel, 2 M iterations | 0.50 s | 0.21 s | **0.05 s** | 0.03 s |
| MP3 decoder, 60 s of audio | 2.07 s | 1.79 s | **1.28 s** | 0.34 s |

The distance to C for the pure calculation kernel thereby falls from **17x to 1.7x**.
The decoder is at 3.8x; what is still missing there is no longer
floating-point work, but Huffman bits and address calculation.

Correctness: the output of the decoder is still **bit-identical**
(`tools/ton_build.sh` -> PASS 4/4), and the floating-point tests of the repo
(1101-1104, 1182, 1453, 111, 1002) pass in `dev` as in
`release-fast`.

`FIRN_NO_FP_RA=1` leaves the floating-point numbers in their slots -- the switch
stays, because it did half the work in narrowing down the four bugs above.

## What would be possible next

* **Saving around calls:** values whose lifetime crosses a call
  could keep a register if the generator writes them before the
  call and reads them after. Pays off only once it is measured that
  the affected values are hot.
* **Parameters in registers:** today the prologue writes every
  floating-point parameter into its slot.
* **The remaining ops** (`Select`, checked conversion, `Un`) to be brought into the
  floating-point path, so that fewer functions go through the basic path.
