# ROUND PHI — the miscompilation, hunted systematically

Branch `phi`, worktree `/root/firn-phi`, base `fe88abf9` (round INLINE).
Not merged, not pushed.

## The short version

**Part 1 succeeded.** The miscompilation is understood, the fix from round
INLINE is shown to be load bearing and sufficient for its class, a second
shape of it is now a permanent test, and a systematic search over 935 programs
found **no unknown sibling**. The "12 of 1340 that fail identically on the
base", carried unexplained through rounds SAMMLER and INLINE, are **not
compiler failures at all** — they are the gate script asking four programs a
question `test.sh` stopped asking in round 72.

**Part 2 failed, and the failure is the result.** The per caller bound was
built and measured: it is **worse** than the count budget it would replace, on
both benchmarks. Removing the count budget instead *looked* like a win and was
committed — then turned out to rest on **a broken instrument**, and was
reverted. The compiler at the end of this round emits a **byte identical** JS
engine to round INLINE's (`md5 67a8886c…`): **no code generation decision was
changed.** What the round leaves behind is the fixed instrument, the tests, and
the numbers that say what the next round should do instead.

## Part 1 — the miscompilation

### The cause, in two sentences

`inline.rs` returns the value of a callee with several `ret`s through a slot,
so the continuation block of an embedded call is exactly one
`%d = load.bool %slot` ending in `brcond %d, T, E` — character for character
the pattern `threading::fork_at` exists to recognise. `fork_at` then makes
every predecessor jump **past** that block into `T` and `E`; the block goes
unreachable and the `load` defining `%d` with it, while the caller still names
`%d` further on (in `tests/1136`: the phi that merges `have`) — so the
register allocator hands out whatever the register happened to hold.

### The fix, and the evidence that it is the right one

Round INLINE's `a7e87dfe` already narrowed `fork_at` to refuse when the loaded
value is used **anywhere outside its own block**. Round PHI **verified** that
rather than changing it further — the brief's hypotheses, checked in order:

* *"Are phi operands not counted as a use?"* — They are. `Op::Phi::uses()`
  (`fir.rs:552`) lists the incoming values, so the guard sees them. This was
  the first thing checked and it is **not** what happens.
* *"Load elimination vs. phi construction ordering?"* — Not that either. The
  fork set is collected once from a snapshot and the predecessors rewritten
  afterwards; there is no window in which one pass sees the other half done.
* *"Does the embedded return make a cell the pass thinks is dead?"* — This is
  the one. The cell is not dead; the pass simply never asked whether anything
  *else* names the loaded value, because in the shape `mem2reg` builds nothing
  ever does.

**The guard is load bearing.** With it removed, `tests/1136` goes to exit 1
and **236 of 600** generated bool/phi programs answer wrong. With it in place,
**600 of 600** are right at `release-fast`, `release-safe` and `dev-fast`, and
with `FIRNC_MAX_INLINES=100000` on top.

### The systematic hunt for siblings

Three new instruments in `tools/inline_bench/`:

**`genbool.py`** — a generator, because the shape is too specific to hand
write a hundred times: a callee with several `ret`s, a continuation block
matching `fork_at` exactly, the caller naming the loaded value again, and a
budget that reaches the site. Five caller shapes (guard-on-false,
guard-on-true, `||` merge, merge inside a loop, if/else chain), 2–4 `ret`s per
callee. Every program **computes its own expected answers in Python over the
same integers**, so it is its own oracle: exit 0 is the whole specification, no
golden file. 600 programs, seeds 1..600.

**`diffpass.sh`** — the same source translated two ways must answer the same.
`--no-opt` against `--opt-level=release-fast`. Any difference is a compiler bug
by construction. This needs no `expect_exit:` header and therefore sees wrong
answers the four level gate structurally cannot.

**`gate_par.sh`** — the four level gate in parallel, one scratch file per job.

**Result: no unknown sibling in this corpus.**

* `diffpass.sh` over all 335 programs: **331 same, 4 different, 0 build
  differences** — and the 4 are the `only_mode: opt` programs below, i.e.
  intended behaviour.
* The 600 generated programs: **600 of 600** right at three levels and with a
  saturated budget.

What the generator *did* find is a **second shape that no test covered**, now
`tests/1137_thread_bool_phi_use.fi`: the second call guarded by the first
being **true** rather than false, merging without a `||`. Of the first 30
failing generated programs, 19 are `tests/1136`'s shape and **11 are this
one**. Reduced to one three-`ret` callee, two call sites, six assertions;
**exit 1** without the guard, 0 with it at all four levels.

> A first attempt at `1137` hand wrote three other use shapes — value
> returned directly, used as a call argument, read in both arms. All three
> passed **without** the guard too, i.e. proved nothing, and were thrown away
> instead of committed. A regression test that cannot fail is not one.

### The 12 of 1340 — named, and none of them is a compiler bug

Rounds SAMMLER and INLINE both reported "1328 of 1340, the 12 fail identically
on base" and neither chased it down. They are **4 programs × 3 build levels**:

| program | at release-fast | at dev-fast / release-safe / --no-opt |
|---|---|---|
| `tests/028_cast_narrow.fi` | 57 | exit 101 |
| `tests/030_wrap_u8.fi` | 44 | exit 101 |
| `tests/054_i16_ops.fi` | 1 | exit 101 |
| `tests/1334b_type_truncation.fi` | 0 | exit 101 |

All four carry **`// only_mode: opt`** on line 2. They exist to *demonstrate*
that `release-fast` truncates and wraps instead of checking (SPEC §13 `L9`,
round 72): each deliberately casts or adds a value that does not fit. **Exit
101 is the correct answer** at the three checked levels — getting the expected
value there would be the bug. `test.sh` has skipped them outside `opt` mode
since round 72 (line 262); the round gate script never did. They are neither
miscompilations nor `firnc1` building sites. **1328 was always the true
denominator.**

## Part 2 — the per caller bound, and a lesson about instruments

### The per caller growth bound: built, measured, rejected

`GROW_PERCENT` / `GROW_MIN_INSTS` (`FIRNC_GROW_*`), a bound on how far one
caller may grow, computed from its size *before* the pass so it cannot
compound. Measured:

| | JS engine | WASM interpreter |
|---|---:|---:|
| growth bound 30 % | **+6.66 %** (won 2 of 11 pairs) | **+23.0 %** (0 of 7), 382 KB vs 287 KB |
| growth bound 100 % | −0.41 % (7 of 9) — a tie | — |
| growth bound 400 % | +2.61 % (1 of 9) | — |

**It does not work, and the reason is instructive:** what hurts the
interpreter is not *how much* one caller grows but *which bodies* get in at
all. A percentage bound still admits forty instruction bodies into the opcode
chain, it just admits fewer — and the I-cache does not care how many there
are. Kept in the source, **switched off by default**; both knobs at 0 is a
byte identical no-op, verified by md5, which is what makes it an honest
reference side. (Its first version was *not* a no-op — `allow = 0` is the
harshest bound, not the absent one. Caught by the md5 check before any number
was taken.)

### The removal of the count budget: committed, then reverted

Setting `MAX_INLINES` to 0 (the size rule alone deciding) read as a clean win
on callgrind — 718.1 M against 729.7 M instructions on the JS engine, a
smaller binary, a faster compile — and was committed on that basis. Measured
against the wall clock properly it is **slower**:

| | median | pairs |
|---|---:|---:|
| INLINE default vs count-0, run 1 | +2.67 % | A won **13 of 15** |
| INLINE default vs count-0, run 2 | +4.27 % | A won **12 of 15** |
| **noise floor** (two byte identical copies) | +0.87 % | 9:6 |

12 and 13 of 15 is not noise. Reverted.

**Why the first reading lied — the part worth keeping. Callgrind is not
deterministic on the JS engine.** Three runs of *one* binary: 725.6 M /
725.9 M / 721.6 M. The collector's incremental slice has a **time** budget
(`lib/gc/gc.fi:1216`, `__gc_now_ns() - t0 >= budget`), so under valgrind's
~50× slowdown it does a different amount of work each run — and a claimed
1.6 % improvement fits inside that spread. The round had switched to
instruction counts *because* the wall clock was noisy, and failed to check
that the new instrument was steady on this program.

Where no clock enters the program's own decisions the counts are exact: the
**WASM interpreter, which has no collector, gives `1279457762` three times
running, to the digit.** Everything concluded from the WASM numbers stands.
Cross checked on an allocation-poor JS job (arithmetic only, so the time
sliced collector barely runs): **1461.6 M for cap 2000 against 1468.9 M for
cap 0** — agreeing with the wall clock, opposite to the first reading.

### What the numbers do say, on the bench where they are trustworthy

WASM interpreter, `prim.wat` trial division, every build verified to answer
exit 64. Instruction counts exact and reproducible:

| configuration | instructions | binary |
|---|---:|---:|
| round INLINE default (cap 2000, size ≤8) | 1376.7 M | 549048 B |
| `--no-pass=inline` (round SCHLEUSE's flag) | 1253.1 M | 287392 B |
| count 0, size ≤8 | 1279.5 M | 301792 B |
| count 0, size ≤6 | 1220.4 M | 286688 B |
| count 0, size ≤5 | 1187.4 M | 287832 B |
| **count 0, size ≤4** | **1105.9 M** | **280728 B** |
| growth bound 30 % | 1298.1 M | 382176 B |

### Can round SCHLEUSE strike `--no-pass=inline`? — **Not at the default. Yes with one flag.**

At the shipped default the interpreter is still **+4.59 % slower** with
inlining than without it (wall clock, A won 6 of 7; +2.1 % in instructions).
So the honest answer for the default is **no**.

But **`FIRNC_ALWAYS_INSTS=4`** — the size knob that already exists — gives:

| | instructions | binary | wall clock |
|---|---:|---:|---:|
| `--no-pass=inline` | 1253.1 M | 287392 B | 3903.6 ms |
| `FIRNC_ALWAYS_INSTS=4` | **1105.9 M** (−11.8 %) | **280728 B** | **3399.6 ms (−12.9 %, won 7 of 7)** |

**Better than `--no-pass=inline` on every count**, and verified to answer
exit 64. If SCHLEUSE wants the flag gone, that is the way — and the general
fix is to make the size bound settable per module rather than one global
constant, because **the two programs want opposite values**: the JS engine
wants ≤8, the interpreter wants ≤4, and the difference is worth 11.8 % to the
interpreter and ~5 % to the engine. No single constant is best for both.

## The gates

| gate | result |
|---|---|
| four level runs | **1328 pass, 0 fail**, 12 skipped (`only_mode: opt`) |
| GC / threading | **316 of 316** (79 tests × 4 levels — a superset of round INLINE's 164) |
| soak checksum | **8296429** at all four levels — identical |
| JS output | **16 of 16 byte identical** to round INLINE's engine |
| compiler unit tests | **267 of 267** |
| generated bool/phi corpus | **600 of 600** at three levels and with a saturated budget |
| differential `--no-opt` vs `-Ofast` | **331 same, 4 different** (the 4 are `only_mode: opt`) |
| **`tools/self_compare.sh`** | **RAN TO COMPLETION: 338 same behaviour, 0 differing, 0 faulty, exit 0** — 12 min 45 s |

`self_compare.sh` is the gate rounds SAMMLER and INLINE both had to report
unfinished. It is finished: `.firnc1` (the compiler in Firn, 1,912,952 bytes)
builds with this `firnc`, and over 338 programs the binary it produces behaves
identically to the one `firnc0` produces.

### Two measuring tools that lied, both fixed

* **`jsout.sh` had `cd /root/firn-inline` hardcoded** in its second line, so on
  this branch it compared round INLINE's engine **against itself** and would
  have reported 16/16 whatever this round did. Now takes its root from the
  environment; the 16/16 above was taken after the fix.
* **The parallel harnesses shared stdin.** `tests/1283_std_io_ask.fi` reads
  stdin; under `xargs -P` that is the job list, so the test ate lines of it,
  answered `''`, and starved other jobs — a failure the compiler did not
  cause, in one run of four. `< /dev/null` in all three harnesses.

## What the next lever is

1. **A size bound that is not one global constant.** The finding of Part 2:
   the JS engine wants ≤8 and the WASM interpreter wants ≤4, and the gap is
   worth 11.8 % to the interpreter. A per module or per function bound lets
   round SCHLEUSE strike `--no-pass=inline` today — the measurement is above.
2. **Never measure the collector with callgrind again.** The GC's incremental
   slice is time budgeted, so instruction counts on any allocating Firn
   program are not reproducible. Either measure allocation-poor workloads, or
   give the collector a deterministic (allocation counted) budget mode for
   benchmarking — the latter would make every future GC round measurable.
3. **The generator is a fuzzer with five shapes**, and it found a real second
   shape inside its own narrow subject. Widening it — integers and pointers as
   well as bools, `switch`, nested calls — is cheap now that the self checking
   scaffolding exists, and it reaches passes no `expect_exit:` header does.
4. **`noiv` in the register allocator** — unchanged from round INLINE: 520
   values in `__gc_alloc_raw` never touched, a frame of 8,528 bytes, a missing
   DCE pass after inlining.
