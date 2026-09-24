# Round B4 -- the GC write barrier, inline

Branch `runde-b4-barrier` (worktree `/root/firn-b4`), based on
`runde-luecken` (460cbfcb). Not merged. Everything below is measured on
this machine (AMD EPYC 7571, Linux x86_64, host under foreign load).

Gap B4 of `docs/LUECKEN.md`: every store of a `Gc[T]` pointer into a heap
field was a CALL of `__gc_barrier(field, value)`. On `release-*` the
inliner removed it; on `dev-fast` -- the level Certus is built at (B2) --
and in `firnc1` it stayed a real call.

## 1. What the barrier does now

`compiler/src/gc_lower.rs::emit_barrier` writes the fast half at the store:

```
    lea  rax, [rip + .L__gc_state]
    mov  r10, [rax+144]          ; S_BARRIEREN
    lea  r11, [r10+1]
    mov  [rax+144], r11          ; gc_barriers() stays exact
    mov  r8,  [rax+320]          ; S_PHASE
    cmp  r8, 0
    jne  .Lslow                  ; out of line, at the end of the function
```

The slow half -- greying, the finalizer check (abort 73), the lock -- stays
in the runtime as `__gc_barrier_slow(value)` (`lib/gc/gc.fi`), which runs
only while a cycle is marking or sweeping. The contract between compiler
and runtime is two offsets (144, 320) and one name. A module test
(`gc.rs::barrier_offsets_match_the_runtime`) reads the embedded runtime and
fails if either offset moves.

Explicit calls `__gc_barrier(field, value)` in the runtime collections
(`lib/gc/gcvec.fi`, `gcmap.fi`) get the same inline path. The function
`__gc_barrier` itself stays -- Certus' JIT helpers take its address.

The same in `firnc1` (`lib/firnc1/lower.fi::gc_barrier_inline`), with the
same instruction order as firnc0.

Dropped on purpose: the runtime's `field == 0 && value == 0` early return.
The field address of a store that has just happened is never 0.

## 2. A bug found on the way: the target was evaluated twice

`hook_assign` computed the field address a SECOND time for the barrier
(`lo.lower_addr(target)` after the store). With a side effect in the
target that is wrong: `pick(h, &calls).head = c` called `pick` twice.
Measured on `runde-luecken`: `tests/1664_gc_barrier_inline.fi` exits 11
(2 calls) on all four levels. Now the caller hands over the address the
store went to. Both compilers.

`firnc1` could not compile that line at all: its `lvalue` lacked the gc
hook of `sema.rs` ("writing THROUGH a Gc[T]", any expression as base).
Ported; `get(p).x = 5` on a raw pointer is still refused, like in firnc0.

## 3. Tests

`tests/1664_gc_barrier_inline.fi`, four levels, both compilers:

1. the target once (was 2 calls);
2. `gc_barriers()` exact: 2000 heap stores count 2000, a local store 0;
3. the slow half during marking: 64 holders x 200 cells, cells MOVED
   between holders by plain field stores while phase 1 runs (slice 2).

COUNTER-CHECK, really carried out: with an empty `__gc_barrier_slow`,
1664 exits 95 (cells swept, chains short) and `841_gcvec_incremental.fi`
exits 96 -- so both the compiler-inserted and the explicit barrier go
through the new slow path, and the tests would see it missing.

## 4. Measurements

### Micro (`bench/firn/gc_barrier.fi`, ps per unit, best of 3)

| compiler / level | store before | store after | reverse before | reverse after |
|---|---|---|---|---|
| firnc0 dev-fast | 11 635 | 2 885 | 11 732 | 2 945 |
| firnc0 release-fast | 2 897 | 2 993 | 4 019 | 3 987 |
| firnc1 | 55 407 | 11 625 | 58 761 | 15 869 |

dev-fast: **x4.0 per store**. release-fast: unchanged (the inliner already
did it there -- which is the point: dev-fast now gets what release-fast had).
firnc1: x4.8.

### Certus JS engine (`tools/nanbox/ab4.py`)

Certus' own compiler (`/root/firnc-gc`, copy in `/tmp/b4/fgc0`, bit
identical to the production binary) against the same copy plus this
round's change (`/tmp/b4/fgc`), both building `lib/js/run_main.fi` of
Certus main 9e119268 at dev-fast. 208 barrier call sites in `jsrun`, all
inline afterwards; binary +12.6 KB.

Two runs, interleaved, best of 5, 1 M iterations each, host under foreign
load 5-7; the second run with the two binaries swapped, so a bias of the
order shows up as a factor that does not invert:

| bench | A -> B (factor) | B -> A (factor) |
|---|---|---|
| leere_schleife | 1.007 | 1.065 |
| arith | 0.999 | 0.985 |
| funktionsaufruf | 1.029 | 0.947 |
| eigenschaft_lesen | 1.000 | 0.963 |
| eigenschaft_schreiben | 1.006 | 1.006 |
| methodenaufruf | 1.051 | 0.976 |
| feld_index | 0.997 | 0.951 |
| prototypkette | 1.086 | 0.973 |
| zeichenketten | 1.017 | 0.979 |
| objekt_anlegen | 1.063 | 0.918 |
| closure | 1.065 | 0.904 |
| fibonacci | 1.060 | 0.930 |
| **geometric mean** | **1.031** | **0.966** (= 1.035 the other way) |

So **+3.1 to +3.5 %** on the JS engine as a whole; the benches that build
and link objects (objekt_anlegen, closure, fibonacci's frames) +6 to +10 %
in both directions, the pure arithmetic ones in the noise. That is the same
1.031 the uncommitted experiment of round GAPS measured -- this round is
that experiment made real, in both compilers, with tests.

Correctness of the B binary: `jsrun-B` gives the same output as `jsrun-A`
on a sample script; the full Firn suite runs the JS engine of `lib/js`
(test262 subset) with the new compiler.

## 5. The test suite

`./test.sh` on this branch: 1662 checks, **1656 green**. The six red ones:

| check | why | state |
|---|---|---|
| `tools/js/run.sh` | `testdata/test262/subset.sha256` not in the repo | red on `main` too |
| `tools/english/check.sh` | 512 German identifiers in `lib/fui`, `lib/svg`, demos | red on `main` too, same count |
| `tools/fmt/run.sh` step 3 | 56 files in `lib/fui`, `lib/svg`, demos not canonical | red on `main` too; none of this round's files |
| `tools/self_compare.sh` | a second run of the script, started by hand, shared `.self-work` with the suite and overwrote its binaries | run again alone: **352 same behaviour, 0 differing, 0 faulty** |
| `tools/fixpoint.sh` | stage 2 == stage 3 character-identical (807 080 lines); the corpus step found `bench/firn/gc_barrier.fi` printing its TIMINGS on stdout | fixed (timings on stderr, `expect_out: 1002`), fixpoint run again: **stage 2 == stage 3, corpus 352 same, 0 differing, 0 faulty -- green** |
| `tools/js/round66.sh` | the promise endurance run grew 8388 KiB, limit 8192 | **borderline on the base too**: three runs each, same machine: base 8116 / 8300 / 8544 KiB, this branch 8500 / 8316 / 8592 KiB. The limit sits inside the spread of the base; roadmap item |

Everything else green, among it: 350 programs x 4 levels (1664 included),
all negative tests, the FIR comparison firnc0/firnc1 (195 same, 1 known),
the DOM soak run (flat 1760 KiB), the str soak run in both compilers,
thread, freestanding, aarch64, the four build levels agree, html5lib
6810/6810, CSS, WPT layout/paint/B4.

## 6. What is left

* Certus is built with `/root/firnc-gc`, not with main (B1). The change is
  made for that tree as a patch-equivalent in `/tmp/b4/fgc` only for the
  measurement; it reaches Certus when B1 is done or when it is carried
  over as a vendor patch.
* The fast path could be one `inc qword ptr [rip + .L__gc_state+144]`
  instead of four instructions -- a peephole, not done.
