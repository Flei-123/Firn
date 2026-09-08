# ROUND PHI — the miscompilation, hunted systematically

Branch `phi`, worktree `/root/firn-phi`, base `fe88abf9` (round INLINE).
Not merged, not pushed.

## What the round was asked and what it found

Two jobs. The first was the wrong answer round INLINE stumbled over —
understand it, fix it minimally, pin it with a test, and then go looking for
its **siblings** instead of waiting for the next one to turn up. The second
was to replace the inliner's count budget with a per caller bound.

The first job produced a finding the brief did not ask for: **the "12 of 1340
that fail identically on the base" are not compiler failures at all.** They
are the round gate script asking four programs a question `test.sh` stopped
asking in round 72.

The second job produced a negative result and a positive one. The per caller
growth bound was **built, measured and thrown away** — it is worse than what
it replaces on both benchmarks. What actually pays is the other half of the
same idea: **switching the count budget off entirely.**

## Part 1 — the miscompilation

### The cause, in two sentences

`inline.rs` returns the value of a callee with several `ret`s through a slot,
so the continuation block of an embedded call is exactly one
`%d = load.bool %slot` ending in `brcond %d, T, E` — which is character for
character the pattern `threading::fork_at` exists to recognise. `fork_at` then
makes every predecessor jump **past** that block into `T` and `E`, the block
goes unreachable and the `load` that defines `%d` goes with it, while the
caller still names `%d` further on (in 1136: the phi that merges `have`) — so
the register allocator hands out whatever the register happened to hold.

### The fix

Round INLINE's `a7e87dfe` already narrowed `fork_at`: it now refuses when the
loaded value is used **anywhere outside its own block**, walking every
instruction of every block and the `Ret`/`BrCond`/`Switch` operands. Round PHI
**verified that this fix is load bearing and complete for this class** rather
than changing it further:

* `Op::Phi::uses()` (fir.rs:552) does list the incoming values, so a phi
  operand really is caught by the guard. This was the first thing checked,
  because "phi operands are not counted as a use" was the brief's own first
  hypothesis for the cause. It is not what happens — the guard sees them.
* With the guard **removed**, `tests/1136` goes to exit 1 and **236 of 600**
  generated bool/phi programs answer wrong. With it in place, **600 of 600**
  are right, at `release-fast`, `release-safe`, `dev-fast`, and with
  `FIRNC_MAX_INLINES=100000` on top.

The pass is not switched off and its condition is not widened. Nothing in
`fork_at` needed to change this round; what was missing was the evidence that
it is enough, and the tests that keep it that way.

### The systematic hunt for siblings

Three instruments, all new this round, all in `tools/inline_bench/`:

**1. `genbool.py` — a generator, because the shape is too specific to hand
write a hundred times.** The bug needs four things at once: a callee with
several `ret`s, a continuation block matching `fork_at` exactly, the caller
naming the loaded value again, and the budget reaching the site. The generator
produces that shape in five variants (guard on false / guard on true / `||`
merge / merge inside a loop / if-else chain), with 2–4 `ret`s per callee, and
**computes the expected answers in Python over the same integers** — so every
program is its own oracle and exit 0 is the whole specification. 600 programs,
seeds 1..600.

**2. `diffpass.sh` — the same program translated two ways must answer the
same.** `--no-opt` against `--opt-level=release-fast`: no inliner, no
threading, no mem2reg on one side. Any difference is a compiler bug by
construction, because the source is byte identical. This needs no
`expect_exit:` header and therefore sees wrong answers the four level gate
structurally cannot see.

**3. `gate_par.sh` — the four level gate, in parallel**, one scratch file per
job. (Round INLINE's `asmcmp.sh` had all parallel jobs writing the same two
scratch names; it reported 109 differences that were not there. The same trap
is why the binary path here contains both `$BASHPID` and `$RANDOM`.)

**Result of the hunt: no unknown sibling exists in this corpus.**

* `diffpass.sh` over all 335 programs: **331 same, 4 different, 0 build
  differences** — and the 4 are the `only_mode: opt` programs below, i.e.
  intended behaviour, not miscompilation.
* The 600 generated programs: **600 of 600 right** at every level with the
  guard in, 364 of 600 with it out.

What the generator did find is a **second shape of the same bug that no test
covered**, now `tests/1137_thread_bool_phi_use.fi`: the second call guarded by
the first being **true** rather than false, merging without a `||`. Of the
first 30 failing generated programs, 19 are the shape `tests/1136` pins down
and **11 are this one**. Reduced to one three-`ret` callee, two call sites and
six assertions; it answers **exit 1** without the guard and 0 with it, at all
four levels and with a saturated budget.

> A first attempt at `1137` hand wrote three other use shapes — the value
> returned directly, used as a call argument, read in both arms. All three
> passed **without** the guard as well, i.e. proved nothing, and were thrown
> away instead of committed. A regression test that cannot fail is not a
> regression test.

### The 12 of 1340 — named, and they are not compiler bugs

Rounds SAMMLER and INLINE both reported "1328 of 1340, the 12 fail identically
on the base" and neither chased it down. They are **4 programs × 3 build
levels**:

| program | at release-fast | at dev-fast / release-safe / --no-opt |
|---|---|---|
| `tests/028_cast_narrow.fi` | 57 | exit 101 |
| `tests/030_wrap_u8.fi` | 44 | exit 101 |
| `tests/054_i16_ops.fi` | 1 | exit 101 |
| `tests/1334b_type_truncation.fi` | 0 | exit 101 |

All four carry **`// only_mode: opt`** on line 2. They exist to *demonstrate*
that `release-fast` truncates and wraps instead of checking (SPEC §13, `L9`,
round 72): every one of them deliberately casts or adds a value that does not
fit. **Exit 101 is the correct answer** at the three checked levels — getting
the expected value there would be the bug. `test.sh` has skipped them outside
`opt` mode since round 72 (line 262); the round gate script never did.

`gate_par.sh` now honours the header. **1328 was always the true denominator**;
neither this round nor round INLINE nor round SAMMLER ever had 12 real
failures, and they are not firnc1 building sites either.

## Part 2 — the per caller bound: built, measured, thrown away

### Why the wall clock could not decide this

The host carries the load of other rounds; the load average moved between
**5.5 and 12.4** during the round. Two **byte identical** copies of the JS
engine, interleaved 15 pairs, came out **+0.82 % apart at 9:6 pairs**. The
same `size<=6` binary measured **−1.19 % (7 of 11)** in one run and **+8.16 %
(A won 7 of 13)** twenty minutes later.

So the decision was taken on **callgrind instruction counts**, which do not
move, with the wall clock kept only where it agrees.

### The measurements

JS engine = `lib/js/run_main.fi`, 20k round allocating loop.
WASM interpreter = `/root/osum-schleuse/kernel/app/wasm.fi` (read only),
`prim.wat` trial division, every build verified to answer exit 64.

| configuration | JS engine Ir | JS binary | WASM Ir | WASM binary |
|---|---:|---:|---:|---:|
| round INLINE default (cap 2000, size ≤8) | 729.7 M | 1.931 MB | 1376.7 M | 549048 B |
| `--no-pass=inline` (round SCHLEUSE's flag) | — | — | **1253.1 M** | 287392 B |
| **count 0, size ≤8  ← taken** | **718.1 M** | **1.837 MB** | 1279.5 M | 301792 B |
| count 0, size ≤6 | 737.7 M | 1.788 MB | 1220.4 M | 286688 B |
| count 0, size ≤5 | 748.8 M | 1.786 MB | 1187.4 M | 287832 B |
| count 0, size ≤4 | 752.8 M | 1.778 MB | **1105.9 M** | 280728 B |
| count 0, size ≤3 | 768.7 M | 1.749 MB | 1105.9 M | 279936 B |
| growth bound 30 %, count on | — | 1.906 MB | 1298.1 M | 382176 B |

**The per caller growth bound (`GROW_PERCENT` / `GROW_MIN_INSTS`) is worse
than what it replaces.** At 30 % it is **+6.66 %** on the JS engine (2 of 11
pairs) and **+23.0 %** on the WASM interpreter (0 of 7). At 100 % it is a tie
with the INLINE default (−0.41 %); at 400 % it converges back onto it. The
reason is visible in the table: what hurts the interpreter is not *how much*
one caller grows, it is *which bodies* get embedded at all. A percentage bound
still lets forty instruction bodies into the opcode chain, it just lets fewer
of them.

The mechanism stays in the source, switched off by default. Both knobs at 0 is
a **byte identical no-op** against round INLINE's default — verified by md5 of
the built engine — which is what makes it an honest reference side for every
number above. (Its first version was *not* a no-op: `allow = 0` is the
harshest possible bound, not the absent one. That was caught by the md5 check
and fixed before any measurement was taken.)

### What was taken instead: `MAX_INLINES` 2000 → 0

The count budget is off. The size rule alone decides, and it is the best JS
number of the seven configurations — **−1.6 % against round INLINE**, which
was itself −43 % against its base.

| | round INLINE | round PHI | change |
|---|---:|---:|---:|
| JS engine, instructions (20k rounds) | 729.7 M | **718.1 M** | **−1.6 %** |
| JS engine binary | 1.931 MB | **1.837 MB** | **−4.9 %** |
| JS engine compile time (median of 5) | 3867 ms | **3667 ms** | **−5.2 %** |
| WASM interpreter, instructions | 1376.7 M | **1279.5 M** | **−7.1 %** |
| WASM interpreter binary | 549048 B | **301792 B** | **−45.0 %** |

Nothing in that table is worse.

### Can round SCHLEUSE strike `--no-pass=inline`? — **Not yet.**

`--no-pass=inline` is **1253.1 M** instructions; the new default is
**1279.5 M**, i.e. the interpreter is still **+2.1 %** worse with inlining on
than with it off. That is a great deal better than round INLINE's +9.9 % and
than the base's +53 %, and the binary is now 302 KB against `--no-pass`'s
287 KB rather than 549 KB — but it is not "at least as fast", so the honest
answer is **no**.

**And the reason it cannot simply be fixed is the finding of Part 2.** The two
programs want **opposite** size bounds:

* `size <= 4` is the best WASM number by a distance — **1105.9 M, −11.8 %
  against `--no-pass=inline`**, which *would* let SCHLEUSE strike the flag —
  and it is one of the worst JS numbers (+4.8 % against `size <= 8`).
* `size <= 8` is the best JS number and +2.1 % on the WASM interpreter.

There is no single constant that is best for both. The round took the one that
is good for both and never bad, and did not dress the tie up as a win. **If
SCHLEUSE wants the flag gone, the lever is a per module or per function size
bound** (`#[inline_size(4)]` on the interpreter, or a profile flag) — the
knob already exists as `FIRNC_ALWAYS_INSTS` and building the interpreter with
`FIRNC_ALWAYS_INSTS=4` today gives it **1105.9 M against 1253.1 M, 280 KB
against 287 KB**, i.e. better than `--no-pass=inline` on both counts.

## The gates

| gate | result |
|---|---|
| four level runs | **1328 pass, 0 fail**, 12 skipped (`only_mode: opt`, see above) |
| GC / threading | **316 of 316** (79 tests × 4 levels — a superset of round INLINE's 164) |
| soak checksum | **8296429** at `release-fast`, `release-safe`, `dev-fast`, `--no-opt` — identical |
| JS output | **16 of 16 byte identical** against round INLINE's engine |
| compiler unit tests | **267 of 267** |
| generated bool/phi corpus | **600 of 600** at three levels and with a saturated budget |
| differential `--no-opt` vs `-Ofast` | **331 same, 4 different** (the 4 are `only_mode: opt`) |
| `tools/self_compare.sh` | see below |

> `tools/inline_bench/jsout.sh` had `cd /root/firn-inline` hardcoded in its
> second line, so on this branch it compared round INLINE's engine **against
> itself** and would have reported 16/16 no matter what this round did. Fixed
> to take its root from the environment before the number above was taken.

## What the next lever is

1. **A size bound that is not one global constant.** Part 2's finding is that
   the JS engine wants ≤8 and the WASM interpreter wants ≤4, and that the
   difference is worth 11.8 % to the interpreter. A per module bound would let
   round SCHLEUSE strike `--no-pass=inline` today.
2. **The generator is a fuzzer with five shapes.** It found a second shape of
   a known bug within its own narrow subject. Widening it — integers and
   pointers as well as bools, `switch`, nested calls — is cheap now that the
   self checking scaffolding exists, and it tests the passes no
   `expect_exit:` header reaches.
3. **`noiv` in the register allocator** — unchanged from round INLINE: 520
   values in `__gc_alloc_raw` that are never touched, a frame of 8,528 bytes,
   a missing DCE pass after inlining.
