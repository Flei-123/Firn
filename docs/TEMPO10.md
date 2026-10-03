# Round TEMPO 10 — three answers to the same question: who owns the register?

State 23.09.2026, branch `xmm-ra`. The starting point was the count after
TEMPO 9 — not per function, but **per pattern over the whole program**
(`/tmp` tool, template in TEMPO 9):

| Pattern | Instructions | Share |
|---|---|---|
| **`movaps xmm,xmm`** (SSE two-operand form) | **16.3 M** | **11.1 %** |
| **fetch from the frame** | **10.3 M** | **7.0 %** |
| `mov rA,rB` + `add $K` (instead of `lea`) | 2.1 M | 1.4 % |
| `mov $K,r` + `imul` (instead of `lea`/`shl`) | 0.8 M | 0.6 % |

The first two are the same question in two disguises: **who gets a
register, and how long does he keep it.** This round gives three answers.

---

## 1. The two-operand form is also a copy (16.3 → 8.7 M)

SSE has no three-operand form: `mulps d, s` means `d = d * s`. The
generator therefore first copies the first operand into the destination and then calculates:

```text
movaps %xmm13,%xmm7
mulps  %xmm12,%xmm7
```

If `%xmm13` dies at this multiplication, the copy is for nothing — `a`
and `d` may have the same register. That is **exactly the question that the
merging from TEMPO 8 already answers**, only for a different kind of
instruction. The candidate search now additionally takes

* `Op::Bin(+,-,*,/)` with a floating-point type,
* `Op::Un(Neg)` with a floating-point type,
* every `Op::Simd` that calculates in its first operand
  (`addps`, `subps`, `mulps`, the three comparisons, `pcmpgtd`, `pand`,
  `pandn`, `por`, `pxor`, `paddd`, `psubd`, `punpckldq/hi`).

**The condition had to be sharpened for this**, and that was the instructive
part. For a real copy it said "the source has exactly one reader".
For the two-operand form that is too strict: in the hot loop of the
synthesis filter bank the same vector is multiplied TWICE, and the
second time it dies — exactly there the destination may inherit its register. The
right question is not how often the value is read, but whether it **still lives
after this instruction**. With "exactly one reader": 146.3 → 141.6 M. With
"dies here": 146.3 → **138.8 M**.

The same relaxation has applied since to real copies too: the Chaitin question
("do they interfere?") is the complete condition, "exactly one reader" was
only caution. `FIRN_COAL_ENG=1` restores the cautious version
(measured 1.0 M worse).

In addition, **two frame slots** are now also merged, but
**only for integers**: for floating-point numbers `fp_handover` puts a value without a
register into `xmm2` instead of the frame, and the copy would then have believed that
source and destination were the same slot. TEMPO 8 already died of exactly this trap
once (`tests/1182_layout_float_probe.fi`).

---

## 2. Density instead of sum (137.7 → 136.7 M, and the door opener)

When no register is free, the linear scan displaces the active interval
with the **smallest weight** (uses times loop depth). That
favours long intervals: a value with fifty uses scattered over the whole function
beats one with three in the innermost loop
— although the first occupies its register the whole time and the second would need it
only briefly.

Now **weight per length** is compared. A one-line change,
`FIRN_RA_SUMME=1` restores the old answer.

---

## 3. Cutting lifetimes — and why the first attempt was wrong

The linear scan knows ONE interval and ONE slot per value. A pointer that is
set at the beginning and needed once more at the end occupies its
register over the whole function — or none, and then in the
hot loop in between it is fetched from the frame at EVERY use.

The textbook answer is *live range splitting*. In the allocator itself that would be
a rebuild of every output site (`loc(v)` would have to depend on the POSITION).
The same result is obtained without this rebuild by making the piece
a separate VALUE: a copy into the pre-header of the loop, and the
body reads the copy.

**First attempt: a pass in the optimiser.** It cut every value that is read several times in
a loop and not written there. Measured:
137.7 → **140.4 M, so two percent WORSE**. The reason is obvious in hindsight: where the new value gets
only a frame slot, you pay for the copy in the pre-header and gain nothing — the body
then simply reads the other slot. Raising the threshold did not help (min=8: still
138.0).

**Second attempt: cut after you know where it pinches.**
`emit_func_ra` now allocates once, asks `split::nach_zuteilung` which
values really **landed in the frame AND are read several times in a loop**,
cuts only these, and allocates once more. If in the second allocation not a single one of the new values
gets a register, the
result is discarded — then the cut costs only translation time and not a
single bit in the program.

**And then exactly this happened: zero registers, every time.** Two causes,
found one after the other:

1. **The merging closed the cut at once again.** `%v2 = copy %v`
   is a perfect candidate for TEMPO 8. For that there is now
   `Func::no_coalesce` — a list of values that must not be merged,
   filled solely by the allocator for its own second version.
2. **Even after that: zero.** And that was no bug, but the answer of the
   allocator. With the SUM as the yardstick a short interval with
   three uses loses against a long one with fifty — always. Only with the
   density from point 2 does the cut win its registers.

The two changes are thus connected: **density without the cut brings
1.0 M, the cut without density brings nothing, both together 2.3 M.**

Threshold: three readers in the loop (`FIRN_SPLIT_MIN`), switchable off with
`FIRN_NO_SPLIT=1`.

---

## Die Zahlen

MP3-Dekoder, 8 s Ton, `release-fast`, `valgrind --tool=callgrind`:

| | Instructions |
|---|---|
| after TEMPO 9 | 146.3 M |
| + two-operand form | 138.8 M |
| + aggressive merging, slots | 137.7 M |
| **+ density + cut (TEMPO 10)** | **135.4 M** |
| the same with `--cpu=avx` | **127.1 M** |
| `minimp3` in C, `gcc -O2` | 74.8 M |

And the wall clock, 60 s of sound, smallest of eleven runs, output to
`/dev/null`:

| | Time |
|---|---|
| Firn after TEMPO 8 | 0.15 s |
| **Firn now** | **0.13 s** |
| Firn with `--cpu=avx` | 0.12 s |
| C, `gcc -O2` | 0.06 s |
| the same C without auto-vectorisation | 0.09 s |
| the same C with `-O0` | 0.36 s |

So **2.2x behind `gcc -O2`** (with AVX 2.0x) and **1.4x** behind the same
C without auto-vectorisation. At the start of the tempo rounds it was 8.3x.

The bank (`bench/firn/`) shows no deterioration: ten of eleven
programs instruction for instruction the same, `jsonscan` −1.8 %. These programs
have hardly any register pressure — there is nothing to distribute there.

The translation of `bin/firnc1.fi` takes 4.9 instead of 4.5 seconds (+7 %);
that is the second allocation for functions with loops and overflow.

## Geprueft

All 318 test programs in four build stages, `self_compare` 339 of 339 with
the same behaviour (0 deviating, 0 faulty), and the **fixed point**: Firn
translates itself, stage 2 and stage 3 character-identical (793,453 lines of
assembler). The PCM output of the decoder is bit-identical after every single step,
in both CPU stages. The three red points of the run
(`tools/js/run.sh` on a missing `testdata/test262/subset.sha256`,
`tools/fmt/run.sh` on unformatted files in `lib/fui/`,
`tools/english/check.sh` on identifiers in `lib/fui/`) are older than this
round; the identifiers IN THE TRANSLATOR are all English since this round
(25 -> 18 reported, none any more under `compiler/src/`).

## What still stands there now

| Pattern | Instructions |
|---|---|
| fetch from the frame | 10.0 M |
| `movaps xmm,xmm` (the source really lives on) | 8.7 M |
| `mov rA,rB` + `add $K` | 2.0 M |
| `mov $K,r` + `imul` | 0.8 M |

The last two are the next, simple step: `lea` may also be used with a
32-bit destination (`lea %edx,0x1(%r10)` calculates the address in 64
bits and cuts to 32 — exactly the arithmetic modulo 2^32 that a 32-bit
`add` does), and for a commutative calculation the constant belongs
on the right, so that the existing folding sees it.
