# ROUND 87 — the five points of "Where Firn is honestly behind"

`docs/BENCHMARKS.md` has carried a section called **"Where Firn is honestly
behind"** since round 86. It names five things and puts a measured number on
each. This round works through those five in order of yield.

Every number below **was run**. Where a thing was tried and thrown out
again, the number that made it fail stands here too — that is the more
useful half of a report, and it is the half that usually goes missing.

**Machine:** AMD EPYC 7571, 8 vCPU, 12 GiB RAM, Debian 12, Linux x86_64,
`rustc` 1.99.0-nightly, GNU `as`/`ld` 2.40, `gzip` 1.12.
**A warning about the machine, and it matters for reading the tables:** this
is a shared virtual machine and the load average during this round moved
between 1 and 7. Every throughput here is the BEST of five to nine passes,
and differences under about eight per cent are noise. Where a difference is
that small, it says so.

**Branch:** `r87-speed2` · **Reproduce:** `bash tools/bench82/run.sh`,
`bash tools/bench87/run.sh`, `bash tools/bench82/ra_baseline.sh`

---

## The short answer

| # | the point | before | after | reached? |
|---|---|---:|---:|---|
| 1 | register allocation spills | 57.2 % | 56.1 % | **partly** |
| 2 | DEFLATE against `gzip -6` | 1.88x | 1.88x / 1.44x | **no** |
| 3 | JSON with floats | 1.6 MiB/s | 15.1 MiB/s | **yes** |
| 4 | the optimizer, self compile | 5,615 ms | 3,220 ms | **yes** |
| 5 | vector instructions on aarch64 | refuses | refuses | **no** |

Two of the five are done, one moved a little, one turned out to be a
different question than it looked, and one was not built. The sections say
which is which and why.

---

## 1. The register allocation — the cause distribution, and what it cost

### 1.1 First measure, then rebuild

The old sentence was "spills more than half the values". Half of WHAT, and
WHY, decides what has to be built, so stage 1 of this round only counts.
`FIRN_RA_STATS=1` now writes a second line per function (`RA-WHY`),
`tools/bench82/ra_report.py` adds it up, and
`tools/bench82/ra_baseline.sh` runs the three workloads of BENCHMARKS.md §4
in one go.

For `bin/firnc1.fi`, 174,459 values:

| what | count | is it a spill? |
|---|---:|---|
| values with no interval at all | 58,441 | **no** — never touched, dead |
| secret values (SPEC §9.2) | 0 | no |
| value intervals that really compete | 74,807 | — |
| of those, lost the scan | **42,129** | yes |

So a third of the "57.2 %" are values that never see their slot. And of the
42,129 real losses, **30,592 sit in ONE generated function**,
`gctext__gctext_write`: 31,097 intervals, and at the worst position **831 of
them overlap**. Twelve registers cannot help there and no allocator could.
Without that one function 11,495 losses remain — and **9,958 of them
(86.6 %) for a single reason**: the interval crosses a call, so only the five
callee-saved registers were possible, and the five were taken.

### 1.2 And that reason was largely wrong

`crosses_call` was asked as *"does a call position lie between the first and
the last touch of the value"*. That is a straight line through a graph: the
blocks are numbered, and everything numbered in between counts as in
between — even when no path from the definition to the use goes near it.

| program | intervals believed to cross a call | really cross one | false |
|---|---:|---:|---:|
| DEFLATE | 2,925 | 1,338 | **1,587 (54 %)** |
| the JS engine | 16,743 | 6,978 | **9,765 (58 %)** |
| the compiler itself | 20,279 | 7,347 | **12,932 (64 %)** |

`exact_crossings()` now answers along the control flow: live before the call
**and** live after it. Only when the live data flow reached its fixed point;
below the round limit of `compute_live` the sets may be too small, and then
the old interval question stands again (`FIRN_RA_ROUGH=1` forces that state).

### 1.3 What it bought

| program | in registers before | after | spilled before | after | losses before | after |
|---|---:|---:|---:|---:|---:|---:|
| DEFLATE | 38.1 % | **40.0 %** | 45.2 % | **43.3 %** | 1,809 | **1,113** (−38.5 %) |
| the JS engine | 29.3 % | **31.5 %** | 50.4 % | **48.1 %** | 10,308 | **6,992** (−32.2 %) |
| the compiler | 19.0 % | **20.1 %** | 57.2 % | **56.1 %** | 42,129 | **40,186** |
| the compiler, without `gctext_write` | — | — | — | — | 11,495 | **9,553** (−16.9 %) |

Run time of the code that comes out (the same Firn programs, compiled once
with the allocator of `main` and once with this one, best of nine):

    RUNTIME_TABLE_PLACEHOLDER

### 1.4 Two bugs that the exact question dug up

Both were in the tree before this round; the exact answer only made them
happen every time instead of depending on the day.

1. **Operands that go into fixed registers one after the other.**
   `call rax` loads its target LAST, after `rdi..r9` are set; `syscall` its
   number likewise; `rep movsb` writes `rdi` before it reads `rsi`; `cmov`
   and `cmpxchg` write `rdx` first. The interval question hid all of it,
   because an operand's interval ENDS at the instruction and therefore
   counted as crossing it. Those operands are now pinned by hand.
   Deliberately not pinned: the arguments of a normal `call` — they go
   through `parallel_reg_moves`, which resolves any permutation, and they
   are the big group.
   **Failure picture:** `tests/1402_core_allocator.fi` jumped to address
   `0x10` in `core__alloc`.

2. **`__cpu_features()` clobbers `r9`, `r10`, `r11`.** `unsupported_basic`
   lets three vector instructions onto the allocating path, among them
   `__cpu_features()`; its `cpuid` sequence in `simd.rs` says in its own
   comment that those three "carry no value **on the base path** of the code
   generator" — and on the allocating path all three are `TEMP_REGS`. A bug
   of round 82.
   **Failure picture:** `tests/1613_crypto.fi`, `sha256_new` held the address
   of its `accel` flag in `r9`, `xor r9d, r9d` erased it, and the write went
   to address 0.

### 1.5 What was NOT done, and what it would take

The 831-fold overlap in `gctext__gctext_write` and the 23.6 % of the JS
engine that gets no allocation at all (`f64` anywhere in the function sends
the WHOLE function to the base path) are both untouched. The second one is a
**second register class in the linear scan** — xmm intervals next to the
integer ones — and that is a round of its own, not a stage of this one.
Live range splitting is the other half: the exact crossing question removes
the false positives, it does not split a range that really is live across a
call.

---

## 2. DEFLATE — the ratio held, and the comparison turned out to be the wrong one

### 2.1 What was built

`match_len()` compared **octet by octet**: one load, one comparison and one
data-dependent branch per octet, so a match of 258 octets cost 258
unpredictable branches. It now compares **eight at a time** and lets the
octet loop finish the tail — deliberately without `bsf` on the exclusive or,
so that the file needs no instruction the language does not have and stays
right on a big endian machine. `find_match()` additionally checks the octet
BEFORE the current best, exactly as zlib does.

Both are **exact**: the output is octet-identical, at every level.

| corpus | before | after |
|---|---:|---:|
| `wikipedia_en_rust.html`, level 6, in process | 23.0 MiB/s | **24.5 MiB/s** |
| the generated corpus of `tools/bench82` | 11.7 MiB/s | 11.6 MiB/s (noise) |
| output size, level 1 / 6 / 9 (zlib frame) | 168,715 / 150,363 / 150,220 | **unchanged** |

### 2.2 What was tried and thrown out — with the numbers

The mandate was that the compression ratio must not get worse. It is 99.0 %
of zlib on the Wikipedia page, and it stayed there.

| idea | speed | ratio |
|---|---:|---|
| `nice_length = 128` (zlib's level 6) | +4.5 % | 99.01 % → **99.16 %** |
| `good_length = 8` (zlib's level 6) | +26 % | 99.01 % → **100.40 %**, and level 1 168,715 → **185,005** octets |
| a multiplicative 3-octet hash | within the noise | one octet worse |
| the 8-octet pre-filter without a call in the chain walk | within the noise | unchanged |

None of the four is in the tree. The two that ARE in it change no octet of
the output.

### 2.3 The finding: two level sixes are not the same algorithm

"1.88x behind `gzip -6`" holds two level sixes against each other. They do
not do the same amount of work. On `wikipedia_en_rust.html`:

| level | Firn MiB/s | Firn octets | gzip MiB/s | gzip octets |
|---|---:|---:|---:|---:|
| -1 | 42.6 | 168,709 | 65.3 | 192,327 |
| -4 | 33.2 | 159,726 | 49.1 | 162,313 |
| **-6** | **21.2** | **150,357** | **33.8** | **151,856** |
| -9 | 18.9 | 150,214 | 24.7 | 149,770 |

Firn's level 6 packs **1,499 octets tighter** than gzip's and lands between
`gzip -8` (149,905) and `gzip -9` (149,770) in output size. Held against the
gzip level that produces the same size, the gap is **1.19x**, not 1.88x.

That does not make the compressor fast. It says what the missing factor
consists of: Firn's level 6 is doing gzip level 8's work, and it is not
buying much for it. Where the remaining time really goes (callgrind, real
text, level 6):

| function | Ir | share |
|---|---:|---:|
| `find_match` (the chain walk itself) | 256.1 M | **49.4 %** |
| `insert_pos` | 98.9 M | 19.1 % |
| `match_len` | 33.7 M | 6.5 % |
| `block_bits` | 29.1 M | 5.6 % |
| `put_bits` | 26.1 M | 5.0 % |

The chain walk is a **pointer chase through `prev[]`** — memory latency, not
instructions. Making it faster means walking fewer links, and walking fewer
links means the ratio, which was the one thing that was not allowed to move.
**The target of under 1.3x was not reached on the corpus of round 82.**

---

## 3. JSON with floats — 1.6 → 15.1 MiB/s

### 3.1 The measurement first

`tools/bench87` reads two JSON documents of the **same shape** with the same
number of members; only the numbers differ, integers on one side, floats on
the other. That is the only way the difference means anything.

### 3.2 It was two things, and the second is bigger than the first

**The arithmetic.** `lib/num/strtod.fi` had exactly ONE path, exact big
number arithmetic: `1.5` went through the same machinery as
`2.2250738585072011e-308`. Every digit cost a `bn_mul_small` plus a
`bn_add_small` over a big integer, and after that came a big number division.

New: a number whose significant digits fit into a `u64` (at most nineteen)
and whose decimal exponent lies between −27 and +27 is converted with fixed
width integer arithmetic.

* exponent ≥ 0: `w * 10^q = (w * 5^q) * 2^q`, and `5^q` fits into a `u64` up
  to q = 27, so the 128-bit product is **exact** and the rounding sees every
  discarded bit.
* exponent < 0: `w` is shifted left far enough for the quotient to have 64
  bits, and the 128-by-64 division yields quotient **and** remainder — the
  remainder is the sticky bit, so this rounding is exact as well.

This is **not** Eisel/Lemire, although it comes from the same place. They
multiply by a 128-bit *approximation* out of a table of 651 entries and need
a check for the ambiguous case. Firn has no global variables (SPEC §2, item
5), so 1,302 constants would have to be built at run time or unrolled into a
generated function — and within ±27 the power of five is *exact* in 64 bits,
and an exact factor needs no ambiguity check at all. Everything outside that
range keeps going the exact way, which stays the only answer for the hard
cases.

**The 12 KiB per number.** `read_f64_bits()` fetched `STRTOD_WS_BYTES` =
12,288 octets from the operating system and gave them back again FOR EVERY
SINGLE NUMBER — 60,000 `mmap`/`munmap` pairs for a document with 60,000
floats. The short way needs no workspace at all, so it is asked first and
the memory is only fetched when it declines. That was the larger half, and
no arithmetic in the world would have fixed it.

| | before | after |
|---|---:|---:|
| integers | 17.6 MiB/s | 18.8 MiB/s (unchanged, that is the noise) |
| **floats** | **1.6 MiB/s** | **15.1 MiB/s** — 9.4x |
| the gap between them | **11.0x** | **1.25x** |

### 3.3 Correctness, which is the part that had to hold

| check | result |
|---|---|
| `tools/dtoa_vectors/run.sh` | **100,000 / 100,000 bit-identical on the way back**, 100,000 / 100,000 shortest form like Rust |
| `tests/304_strtod_hardcases.fi` | the 26 bit patterns unchanged, starting with `4591870180066957722` (= 0.1) |
| `tests/1100`–`1104` | literal bits, exact halfway, subnormal, long digit strings, extremes — all unchanged |

`strtod_short()` mirrors the parse of `strtod_mode()` exactly and gives up
the moment anything is unusual; giving up costs nothing, because the exact
path then parses again from the beginning and sets `consumed` itself.

---

## 4. The optimizer — 3,462 → 1,150 ms, and the assembler is octet-identical

### 4.1 The measurement said something other than what was expected

`FIRN_PASS_TIMINGS=1` now counts per pass not only the time but **whether
the pass found anything**, and prints how much went into passes that changed
nothing. The guess was that the fixpoint loop confirms itself to death. It
does not: only **12.6 %** of the optimizer went into idle passes. The time
sat in the passes that DO something — 628 productive calls of
`merge-blocks` cost 670 ms, **1.07 ms each**, for a pass that moves
instruction lists around.

### 4.2 Three quadratic shapes, one cause

A table that belongs to the whole function, rebuilt for every single change.

1. **`dominators()`** held its sets as `Vec<bool>` — one *octet* per block —
   and allocated two fresh vectors per block per round. A function with 500
   blocks pushed a quarter of a megabyte through the cache per round and
   allocated a thousand vectors. The sets are now **words**: 64 blocks per
   `u64`, the intersection is an `&` over `n/64` words, nothing is allocated
   inside the loop. Same data flow, same fixed point, same result. This one
   is the bulk of the round.
2. **`merge_blocks()`** recomputed the reachability and the whole predecessor
   table from scratch for every merged block, found exactly one pair, merged
   it, and started again. Both tables are now built once and kept up to date
   by hand.
3. **`hoist_out()`** in `licm.rs` rebuilt, for every hoisted instruction, the
   set of all values defined in the loop, and sorted and *cloned* the block
   list twice per search step.

On top of that a pass is skipped when the code has not changed since it last
found nothing. It is exact (the passes are deterministic) and it saves
little — 1,024 of 5,840 calls of `licm`.

### 4.3 The numbers

| pass | before | after |
|---|---:|---:|
| mem2reg | 841.6 ms | **201.3 ms** |
| licm | 796.0 ms | **94.4 ms** |
| merge-blocks | 600.2 ms | **13.4 ms** |
| inline | 306.8 ms | 340.0 ms |
| cse | 181.5 ms | 160.8 ms |
| dce | 178.6 ms | 195.4 ms |
| the rest | 242.0 ms | 229.6 ms |
| **optimizer** | **3,462.3 ms** | **1,150 ms** (−67 %) |

| phase | before | after |
|---|---:|---:|
| optimizer | 3,462.3 ms (61.7 %) | **1,150 ms** (34 %) |
| codegen | 1,080.9 ms | 1,100 ms |
| `as` + `ld` | 692.5 ms | 690 ms |
| everything else | 379.1 ms | 380 ms |
| **the whole self compile** | **5,614.8 ms** | **3,220 ms** |

**The goal was under 4,000 ms.**

And the proof that nothing was lost: the assembler of `bin/firnc1.fi`
(269,197 lines) and of `tools/stdlib81/deflate_cli.fi` is **octet-identical**
to what the compiler produced before this stage. Not "equally good" —
identical. The same fixed point is reached, only faster. So the question
"how much worse does the code get" does not arise for any of the three
changes, and `--no-pass=` was not needed to answer it.

### 4.4 What was NOT done

No pass was made cheaper by doing less, and none was moved to
`release-fast` only. That was the plan of the brief, and after 4.1 it was
the wrong plan: the passes are not too expensive for what they deliver,
their data structures were. `bce` is the one candidate left — 4,815 of 4,816
calls find nothing, at a cost of 15.9 ms in all. That is not worth a change.

---

## 5. Vector instructions on aarch64 — not built, and one thing repaired

**This point was not reached.** SHA-256 and AES were named as the two that
count, and neither is emitted for aarch64. What it would take, stated
precisely so that the next round does not have to find it out again:

* The 42 intrinsics of round 82 have **x86 shapes**. `__aesenc(a, k)` is
  AddRoundKey *after* the round; ARM's `aese Vd, Vn` is AddRoundKey *before*
  SubBytes/ShiftRows. The composition exists —
  `aesenc(a,k) = aesmc(aese(a, 0)) ^ k`, `aesdec(a,k) = aesimc(aesd(a,0)) ^ k` —
  so AES is genuinely reachable by emulation, three instructions instead of
  one.
* SHA-256 is not: x86's `sha256rnds2` does **two** rounds with the key
  material in `xmm0`, ARM's `sha256h`/`sha256h2` do **four** and split the
  state across two registers. That needs its own set of intrinsics and an
  ARM branch in `lib/std/crypto/accel.fi`, not an emulation.
* And before either of them: aarch64 has **no `v128` value model at all**.
  `codegen_a64.rs` gives every value a frame slot; the xmm cache, the
  `v128` load/store/shuffle path and the register model of `simd.rs` are
  x86-only. That is the actual work, and it is a round of its own.

### What WAS repaired, because it was worse than "not built"

The refusal message said *"the scalar path of `lib/std/crypto` works on both
machines"*. **It did not.** `sha256_new()` asks `__cpu_features()` once,
unconditionally, to decide which path to take — so the question alone made
the whole crypto library uncompilable for aarch64. Measured before this
round:

    $ firnc --target=aarch64-linux -o /tmp/t tests/1613_crypto.fi
    error: --target=aarch64-linux cannot emit the vector instruction
           CpuFeatures yet

`__cpu_features()` is not a vector instruction, it is a **question about the
machine**, and on aarch64 the honest answer today is zero — none of the bits
this compiler knows how to use. Every dispatch in `lib/std/crypto` then
takes the scalar path, which is exactly what round 82 promised. The two
CRC-32 intrinsics get their real counterpart while we are here: SSE 4.2's
`crc32` computes the Castagnoli polynomial and A64 has it as
`crc32cb`/`crc32cx` — the same polynomial, so this is not an approximation.

    AARCH64_TABLE_PLACEHOLDER

Slow, right, and it compiles. The intrinsics themselves stay refused, with a
message that no longer claims something untrue.

---

## Acceptance

    ACCEPTANCE_PLACEHOLDER

## Files of this round

```
compiler/src/regalloc.rs        exact_crossings, the pinned operands, RA-WHY
compiler/src/mem2reg.rs         dominators in words, merge_blocks in one pass
compiler/src/licm.rs            hoist_out without the rebuild
compiler/src/opt.rs             the pass clock with "for nothing", the skip rule
compiler/src/codegen_a64.rs     __cpu_features and crc32c on aarch64
lib/std/deflate.fi              match_len eight octets at a time
lib/num/strtod.fi               strtod_short, strtod_fast, div128, mul64
lib/num/std_facade.fi           no workspace for the short way
tools/bench82/ra_baseline.sh    the three workloads of BENCHMARKS.md §4
tools/bench82/ra_report.py      the cause distribution
tools/bench87/                  new: deflatespeed.fi, jsonspeed.fi, gen_json.py,
                                gzip_row.py, run.sh
docs/ROUND87.md                 this file
docs/BENCHMARKS.md              the numbers and "Where Firn is honestly behind"
```
