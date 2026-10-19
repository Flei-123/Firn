# Loop idioms

Optimizer passes that recognise a hand written counting loop and replace it by
one machine-level block operation. Each pass has its own name and can be
switched off with `--no-pass=<name>` (`--list-passes`).

| Pass     | Source              | Loop                                  | Becomes                          |
|----------|---------------------|---------------------------------------|----------------------------------|
| `memset` | `compiler/src/memset.rs`       | `while i < n { st8(p, i, 0) }`   | `secure_zero` (`rep stosb`)      |
| `memcpy` | `compiler/src/memcpy_idiom.rs` | `while i < n { dst[i] = src[i] }` | `memcopy` (`rep movsb/d/q`), x86-64 only |

## memcpy

### Pattern (FIR after inlining, release-fast)

```text
H:  %i = phi [P %zero, B %i2]
    %c = cmp.lt.u64 %i, %n
    brcond %c, B, X
B:  %o = mul.u64 %i, 8            ; absent for bytes, `shl` also accepted
    %s = add.u64 %src, %o
    %v = load.u64 %s
    %d = add.u64 %dst, %o
    store.u64 %v, %d
    %i2 = add.u64 %i, 1
    br H
```

Element width 8, 32 or 64 bit (no floats). Result in the pre-header:
`memcopy %dst, %src, n=%n, esz=8` and `H` jumps straight to `X`.

### Why overlap needs no alias analysis

The loop copies forward, element by element: load element `i`, store element
`i`, next. `rep movsb/movsd/movsq` (DF = 0) is defined as exactly this
sequence, so the result is identical for **every** overlap, including a
distance smaller than the element size (`dst = src + 3` with 8 byte
elements) and the classic `a[i+1] = a[i]` fill. A 16 byte `movdqu` block loop
would not have this property (it reads 16 bytes before writing any), so the
pass deliberately uses the string instructions. Overlapping operands may be
slow (microcode path), never different.

Short copies (under 64 bytes) are emitted as a plain forward loop inside the
instruction (`rep` has a start-up cost of a few dozen cycles).

### Conditions

* body = address arithmetic + exactly one load + one store of the loaded value
  (same width) + the increment; anything else (a counter, a call, a second
  access) leaves the loop alone
* both addresses `base + i * esz`, bases defined outside the loop, start 0,
  step 1, unsigned 64 bit compare
* nothing defined in the loop is used outside of it
* constant trip counts below 16 stay loops
* only the check-free form (`add`/`mul`/`shl`/`ptradd`, i.e. release-fast);
  release-safe keeps the `checked_add` panic exits and is NOT transformed (open)
* x86-64 only: `opt.rs` gates the pass on the target; `codegen_a64` and
  `codegen_wasm` reject `MemCopy` (never reached)

### Test

`tests/4460_memcpy_loop_idiom.fi`: lengths 0/1/2/15/16/17/31/33/40, u8/u32/u64,
unaligned, overlap in both directions (distance 1, 2, 3, 4, 8), same address,
guard bytes, side-effect loop; every case is compared against a reference loop
with a counter in the body (which the pass cannot touch).

### Measurement (this server, load 15-30, best of 5-7, release-fast)

| Program | before | after | note |
|---|---|---|---|
| `bench/firn_vs_c/memcpy_loops` | 0.31-0.33 s | 0.23-0.26 s | byte copy 16 MiB and u64 copy 16 MiB, 12 rounds; gcc -O2 0.221, clang 0.136 |
| `binary_trees` | 0.63-0.66 s | 0.61-0.76 s (noise) | 2 loops matched, result unchanged |
| `strbuild`, `jsonscan` | unchanged | unchanged | no copy loop in the hot path (see below) |

Instruction counts (callgrind) were not taken: `rep movs` counts per
repetition under valgrind, which does not model its speed.

### Why `strbuild` / `jsonscan` are slower than gcc (analysis, not fixed here)

`strbuild` (`--emit=asm`): the hot function `push_dec` has
* a 24 byte `[0; 24]` initialiser as 24 single `mov byte ptr [r9], 0` each
  preceded by its own `lea r9, [rbp-N]` (store merging into three `qword`
  stores would remove ~48 instructions per call),
* `x / 10` and `x % 10` as two separate multiply-high sequences (gcc computes
  the quotient once: `r = x - q * 10`),
* the three static addresses (`G_LEN`, `G_CAP`, `G_BUF`) re-materialised with
  `lea` per call.

None of this is a copy loop; the copy in `grow()` runs only at the doubling
points. Next steps: store merging of constant byte stores, CSE of
`div`+`rem` by a constant.
