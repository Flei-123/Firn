# ROUND 82 — SPEED

Everything in this file was **measured on this machine**, not estimated.
Where a number is missing, it says so.

```
AMD EPYC 7571 32-Core Processor, 8 vCPU, 12 GiB RAM, Linux 6.x x86_64
cpuid: SSE2 SSE4.1 SSE4.2 AES-NI PCLMULQDQ SHA-NI AVX2 BMI2 SSSE3  (bit set 0x1ff)
openssl 3.x, gzip 1.x, rustc 1.99.0-nightly, GNU as/ld
Branch: r82-speed (merge base: main d87113b3 + r81-std 2ff70db7)
```

Repeatable with `bash tools/bench82/run.sh` (fast) or
`BENCH82_FULL=1 bash tools/bench82/run.sh` (bigger buffers). It is section 45
of `test.sh`.

**On the measuring method:** all throughput figures are the **best of five**
passes, not the average. This is a shared virtual machine; the same binary
measured between 429 and 894 MiB/s for SHA-256 depending on what the
neighbours were doing. The best pass is the one in which the machine was
actually free, and it is the only one that says anything about the code.
That is also why the regression limits in `tools/bench82/minquota_*.txt` sit
at roughly **half** of what was measured — they are there to catch a factor,
not a percent.

---

## 1. What this round was about

The round arrived with four numbers and one thesis. The numbers:

| | Firn before | reference | behind by |
|---|---|---|---|
| DEFLATE level 6 | 16.9 MiB/s | `gzip -6` 33.7 MiB/s | 2x |
| SHA-256 | 22.6 MiB/s | OpenSSL 1424 MB/s | ~60x |
| AES-128-CBC | 5.50 MiB/s | OpenSSL 1025 MB/s | ~190x |
| AES-128-CFB8 | 0.34 MiB/s | | ~3000x |

The thesis: the factor of two at DEFLATE is honest work lost to thirty years
of optimised C, and the crypto factors are **not an optimizer problem at
all**. OpenSSL does not compute AES, it *executes* it — `aesenc` is one
machine instruction that does a whole round, `sha256rnds2` does two rounds of
SHA-256. Firn could not emit those instructions. That was the lack.

The thesis was right, and closing it was the bulk of this round.

---

## 2. The result, in one table

Measured with `tools/bench82/run.sh`, best of five, buffers of 8 MiB
(2 MiB for CFB8, 1–2 MiB for the scalar paths):

| workload | before this round | after | OpenSSL / gzip | behind by |
|---|---|---|---|---|
| **SHA-256** | 27.6 MiB/s | **968.3 MiB/s** | 1372.6 MiB/s | **1.42x** |
| **AES-128-CBC encrypt** | 7.4 MiB/s | **582.0 MiB/s** | 1056.4 MiB/s | **1.82x** |
| **AES-128-CBC decrypt** | 4.7 MiB/s | **691.8 MiB/s** | 1056.4 MiB/s | **1.53x** |
| **AES-128-CFB8** | 0.5 MiB/s | **26.9 MiB/s** | 37.1 MiB/s | **1.38x** |
| DEFLATE level 6 | — | 11.2 MiB/s | `gzip -6` 21.0 MiB/s | **1.88x** |

The gain over the scalar path is **35.1x** for SHA-256, **78.6x** for AES-CBC
encryption, **147.2x** for AES-CBC decryption and **53.8x** for CFB8. (Those
are the ratios of one and the same run of `tools/bench82/run.sh`; the scalar
figures there are measured over smaller buffers, which is why they differ a
little from the isolated measurements quoted elsewhere in this file. Every
number in this table comes from the same run.)

The round asked for "factor 2 instead of 60 resp. 190". **All four are at
1.4 to 1.8.** The target is met, and CFB8 — the one at 3000x — is now the
closest of the four.

**Why the "before" figures differ from the round's table** (27.6 against 22.6,
7.4 against 5.50, 0.5 against 0.34): those are the SAME implementations,
measured with a different harness. `tools/stdlib81/run.sh` measures 8 MiB in
one call and includes the key schedule; `tools/bench82/run.sh` takes the best
of five and starts the clock after the schedule is built. The difference is
15–18 % and it is the measurement, not the code. Both numbers are in this
repository and both can be reproduced.

**And the scalar path is still there.** Nothing was replaced. `sha256_soft`,
`aes128_new_soft` and the `*_soft` mode functions are the old code under their
own names; the fast path is chosen once, in `sha256_new` / `aes128_new`, by
asking `cpuid`. A binary built here runs on a processor from 2008 — slowly,
and correctly.

---

## 3. How the instructions got into the language

### 3.1 The decision: a type AND intrinsics

Two roads were open. The round said "decide and write down why". Both halves
were taken, deliberately:

**`v128` as a value type** (SPEC §8.6, `compiler/src/types.rs`,
`compiler/src/fir.rs`). Sixteen octets, sixteen byte aligned, at home in an
`xmm` register. Without it every instruction would have to take and give its
operands through memory, and the compiler could never keep an intermediate in
a register. For AES that is not a detail: the round chain is strictly serial,
`aesenc` has four cycles of latency, and a store/load round trip costs five to
six on top. The measurement in §3.3 puts the price of NOT having a value type
at a factor of **seven**.

**Intrinsics, not operators.** `v128` has no `+`, no `^`, no `<<`. Sixteen
octets have no element type; `a + b` would have to mean `paddb`, `paddw`,
`paddd` or `paddq`, and whichever one the language chose would be wrong three
times out of four. The processor's own instructions carry the reading in their
NAME, so the intrinsics do the same: `__v128_add32`, `__v128_add64`. Reading
the Firn source tells you exactly which machine instruction comes out.

**The spelling is `__name(...)`, not `@name(...)`.** That is the house style of
every other primitive in this compiler — `__atomic_add` (round 47),
`__mmio_read32` (round 52), `__gc_state` (SPEC §3.5). A leading `@` would have
been a second syntax for the same thing and would have needed the lexer, the
parser, `firnfmt` and `lib/firnc1` to learn it, for no gain whatsoever.

**42 intrinsics**, all in `compiler/src/simd.rs`, which is the only file in
the compiler where a vector instruction is written down:

* memory and construction — `__v128_load`, `__v128_store`, `__v128_zero`,
  `__v128_from_u64`, `__v128_get_u64`, `__v128_get_u32`, `__v128_set_u32`
* bitwise — `__v128_xor`, `__v128_and`, `__v128_or`, `__v128_andnot`
* integer — `__v128_add8`, `__v128_add32`, `__v128_add64`, `__v128_sub32`
* shuffling and shifting — `__v128_shuffle8` (`pshufb`), `__v128_shuffle32`
  (`pshufd`), `__v128_alignr` (`palignr`), `__v128_unpacklo32/hi32/lo64/hi64`,
  `__v128_shl_bytes`/`__v128_shr_bytes` (`pslldq`/`psrldq`),
  `__v128_shl32/shr32/shl64/shr64`, `__v128_blend16` (`pblendw`)
* crypto — `__aesenc`, `__aesenclast`, `__aesdec`, `__aesdeclast`, `__aesimc`,
  `__aeskeygenassist`, `__sha256rnds2`, `__sha256msg1`, `__sha256msg2`,
  `__pclmulqdq`
* scalar — `__crc32_u8`, `__crc32_u64`, `__cpu_features`

The immediate operand of `pshufd`, `palignr`, `pblendw`, `aeskeygenassist` and
`pclmulqdq` **has to be a literal** and is checked for its range at compile
time. It is encoded into the instruction; there is no register form of it, so
a variable there is not a limitation but an impossibility, and the error
message says so.

### 3.2 Runtime detection, and what the compiler does NOT do

`__cpu_features() -> u64` emits the `cpuid` sequence inline: leaf 0 for the
highest leaf available, leaf 1, and leaf 7/0 **only if leaf 0 reported that it
exists**. On a processor that stops at leaf 1 the upper bits simply stay zero.
Nothing here can fault on an old machine — `cpuid` itself is on every x86_64
there is. `rbx` is callee saved and `cpuid` overwrites it, so it is pushed.

The bit set, also available as `FEAT_*` constants and `has_*()` functions in
the new `lib/std/cpu.fi`:

| bit | feature | `cpuid` |
|---|---|---|
| 0 | SSE2 | leaf 1, `edx` 26 |
| 1 | SSE4.1 | leaf 1, `ecx` 19 |
| 2 | SSE4.2 | leaf 1, `ecx` 20 |
| 3 | AES-NI | leaf 1, `ecx` 25 |
| 4 | PCLMULQDQ | leaf 1, `ecx` 1 |
| 5 | SHA-NI | leaf 7/0, `ebx` 29 |
| 6 | AVX2 | leaf 7/0, `ebx` 5 |
| 7 | BMI2 | leaf 7/0, `ebx` 8 |
| 8 | SSSE3 | leaf 1, `ecx` 9 |

**The compiler inserts no check by itself, and that is on purpose.** It cannot
know which two implementations you consider equivalent. What it guarantees is
that asking is cheap and possible everywhere. `lib/std/crypto/accel.fi` uses
exactly this shape, and both halves are held against the same vectors (§4).

### 3.3 The calling convention, the frame, and the register cache

* A `v128` **parameter** travels in the SSE class of System V AMD64, exactly
  like `f32`/`f64`: `xmm0`–`xmm7`, then the stack. A `v128` **result** comes
  back in `xmm0`. `place_args` in `codegen_x86.rs` already had that queue for
  the floating point types; `v128` joins it.
* All sixteen `xmm` registers are **caller saved** on System V. There is
  nothing to rescue in a prologue — and that is exactly why the cache below
  has to be emptied before every `call`, `syscall` and `asm`. That is the only
  invalidation rule there is, and it is the whole of it.
* The frame gains a wider slot: a `v128` value gets sixteen octets instead of
  eight, at an offset that is a multiple of sixteen (`rbp` is 16-aligned, so
  that is enough for `movdqa`).
* `regalloc.rs` (the linear scan of round 43) hands out **integer** registers
  only. A function with a `v128` in it therefore goes over the **base path** of
  `codegen_x86.rs` — exactly as a function with an `f64` in it has since round
  71. Only the three vector instructions with a *scalar* result (`__crc32_u8`,
  `__crc32_u64`, `__cpu_features`) get through the register path; they touch no
  `xmm` register.

The base path gives every value a frame slot and reloads it at every use.
For integers that costs an L1 access; for the AES round chain it costs the
whole gain. So `v128` values got a small write-back cache over twelve `xmm`
registers (`xmm4`–`xmm15`; `xmm0`–`xmm3` stay scratch, because `xmm0` is the
implicit third operand of `sha256rnds2` and the floating point paths compute
in `xmm0`/`xmm1`).

Three things were built on top of it, and **each one was measured separately**
— `FIRN_NO_XMM_CACHE=1` and `FIRN_NO_XMM_RETIRE=1` switch the first two off,
which is how these numbers exist:

| stage | SHA-256 | AES-128-CBC |
|---|---|---|
| slot to slot, no cache at all | 126.6 MiB/s | 218.2 MiB/s |
| **+ the register cache** | 317.5 MiB/s | 491.7 MiB/s |
| **+ retiring dead values** | 425.2 MiB/s | 515.2 MiB/s |
| **+ promoting `v128` cells** | **894.1 MiB/s** | **575.7 MiB/s** |

**The cache** (`+151 %` / `+125 %`): every value keeps its frame slot as its
home; a register copy is marked dirty until written back. Flushed at the end
of every basic block and in front of every call. Frame value slots have no
address in the program, so no `store`, `copymem` or `secure_zero` can reach
one — memory writes therefore do not invalidate.

**Retiring** (`+34 %` / `+5 %`): `xplan` works out, per function, for every
value whether **every** use of it lies in ONE basic block, and where the last
of them is. After that instruction the register may be taken away *without
being written back* — nobody will read it again. If the value is defined in
that same block, the next pass through the loop defines it afresh; if it comes
from another block, its home slot holds it, because every block flushes before
its terminator. A value used in two blocks is never retired.

**Cell promotion** (`+110 %` for SHA-256): this was the big one, and it was
found by reading the emitted assembler. `var st0: v128` is a MUTABLE local, so
it lives in an `alloca` and every read is a `load` through a pointer. Counted
in `sha256_ni_blocks`: **277 `movdqu` and 304 `mov` per 64 octet block**, for
32 `sha256rnds2`. `mem2reg` cannot help — it promotes cells written *once*, and
a loop variable is written in every pass. So the code generator now promotes an
`alloca` into the register cache when it is sixteen octets big, sixteen byte
aligned, and **every** use of its pointer is the direct address of a `v128`
`load`/`store`. One `ptradd`, one call argument, one pointer stored away, and
the cell stays in memory.

That last step alone took SHA-256 from 425 to 894 MiB/s.

### 3.4 Two source level findings

Both came out of the same reading of the assembler, and both are in
`lib/std/crypto/accel.fi`:

* **The round constants of SHA-256 as values in front of the loop**, not as a
  pointer computation per group. `__v128_load((kp + 16 * g) as *u8)` inside the
  loop costs an address addition, a slot write, a slot read and only then the
  vector load. Sixteen groups per block. Small but free.
* **The CFB8 shift register in the register**, not in memory. CFB8 encrypts one
  full block PER OCTET and then shifts a sixteen octet register by one. The
  first version did that with a fifteen step loop of byte loads and stores —
  thirty memory accesses against the cipher's eleven instructions. `psrldq` by
  one octet, the new octet built as a vector, `por`: **three** instructions.
  **9.2 MiB/s → 26.9 MiB/s.**

---

## 4. Correctness — and it comes first

**The 1,919 NIST CAVP vectors of `testdata/crypto/` pass through the hardware
path: 1,919 ok, 0 wrong** (`tools/stdlib81/run.sh`, unchanged from round 81,
now exercising the new code because the dispatch picks it).

```
CBCGFSbox128   14 ok   CFB8GFSbox128   14 ok   SHA256LongMsg   64 ok
CBCKeySbox128  42 ok   CFB8KeySbox128  42 ok   SHA256ShortMsg  65 ok
CBCVarKey128  256 ok   CFB8VarKey128  256 ok   HMAC           525 ok
CBCVarTxt128  256 ok   CFB8VarTxt128  256 ok   SHA1 (both)    129 ok
NIST TOTAL: 1919 ok, 0 wrong
python/openssl cross-check: 106 of 106 agree
```

On top of that, `tools/bench82/cross.fi` holds the two implementations against
**each other**, which the NIST files cannot do:

* SHA-256 over **every** length from 0 to 300 octets, plus 1000…4000 in steps
  of 997 — because the padding has its boundary at 55/56/64 and a second block
  appears there;
* AES-CBC in **both** directions over 0…288 octets in steps of 16, including
  the check that the shift register ends up in the same state, and that a
  length of 17 is refused by both;
* AES-CFB8 in **both** directions over every length from 0 to 200 — the shift
  register is what 15, 16 and 17 octets put under strain;
* the FIPS 197 known answer and the FIPS 180-4 `"abc"` digest, both of which
  come from outside this repository and hold both paths at once.

That check runs **before** the stopwatch in `tools/bench82/run.sh`, and its
failure is fatal. A fast implementation that is wrong is worth less than a
slow one that is right.

One consequence worth naming: **the hardware path is constant time by
construction.** `lib/std/crypto/aes.fi` note A3 says the scalar implementation
uses S-box lookups and therefore leaks through the cache. `aesenc` has no data
dependent timing and touches no table, so on a processor with AES-NI that
concern is simply not there. A3 still applies — on a processor without it, and
exactly as written.

---

## 5. DEFLATE, the optimizer, the register allocation, the self compile

### 5.1 DEFLATE — the honest factor of two, unchanged

| | Firn | `gzip -6` | behind by |
|---|---|---|---|
| DEFLATE level 6 | **11.2 MiB/s** | 21.0 MiB/s | **1.88x** |
| inflate | 20.2 MiB/s | | — |
| CRC-32 (table driven, scalar) | 188.4 MiB/s | | — |

**On literally the same octets**: `tools/bench82/speed.fi dump` writes the test
data out and `gzip -6` gets that file. A comparison against different input is
not a comparison, and DEFLATE is more sensitive to its input than anything
else in this table — the same implementation measures 11 MiB/s on structured
data and several hundred on a stream of one repeated octet.

**Nothing was done to DEFLATE in this round, deliberately.** A factor of 1.88
against thirty years of tuned C, in a language whose compiler has no vector
unit for it and hands out ten integer registers, is the honest number the
round called it. The two things that would move it are a better match finder
(lazy matching over more chain steps) and `pclmulqdq` for the CRC — the second
one is now *possible* (`__pclmulqdq` exists), and it is not this round's work.

`crc32` is listed because the intrinsic `__crc32_u64` (SSE4.2) now exists and
the table driven implementation of `lib/std/deflate.fi` does **not** use it:
`crc32` the instruction computes CRC-32**C** (Castagnoli polynomial), gzip
needs CRC-32 (IEEE). They are different functions and one cannot stand in for
the other. Written down here so that nobody "optimises" it later and breaks
every gzip file this library ever writes.

### 5.2 The optimizer — three cases it was leaving

Found the way the round asked: small Firn programs, `objdump -d`, and
`gcc -O2` on the same C next to it. Three differences were real. Everything
else the existing passes already had — `lea` for address arithmetic, `imul` by
a power of two as `shl`, common subexpressions, and the fusion of `cmp` with
`jcc` when a `brcond` reads a comparison directly.

The new pass is `compiler/src/peephole.rs`, registered as `strength` and
debug preserving, so it runs at `dev-fast` too.

**1. A negated comparison.** `if !(a < b)`, block `bb0` of the function:

```
before (8 instructions)                after (2)
    cmp r8, r9                             cmp r8, r9
    setl al                                jl .Lnegcmp__bb2
    movzx r10d, al
    mov rax, r10
    xor eax, 1
    mov r9, rax
    test r9b, r9b
    jnz .Lnegcmp__bb1
```

`gcc -O2` writes `cmp rdi, rsi ; setge al` for the same thing. The `not` of a
comparison IS a comparison, with the opposite operator.

**Floating point does not join in**, and this is the one trap in the
transformation: `!(a < b)` and `a >= b` are the same for integers and
DIFFERENT for IEEE-754. With a NaN on either side `a < b` is false, so
`!(a < b)` is true, while `a >= b` is false as well — every ordering
comparison with NaN is false. The guard is `!ty.is_float()`, and there is a
module test for it (`a_float_comparison_stays_as_it_is`).

**2. `brcond` over a negation.** `if !flag` for a `flag` that is not a
comparison stayed `xor 1 ; test ; jnz`. A branch that swaps its two targets
does the same thing without the negation, and the negation then falls to dead
code elimination. This also catches the case where the negated thing is a call
result or a loaded octet.

**3. Unsigned `/` and `%` by a power of two.**

```
a / 8, before (5)              after (2)      gcc -O2
    mov r9, 8                      mov r9, r8     mov rax, rdi
    mov rax, r8                    shr r9, 3      shr rax, 0x3
    mov rcx, r9
    xor edx, edx
    div rcx
```

`div r64` costs some 20 to 40 cycles on this processor against one for `shr`.
`a % 8` was the same instruction reading `rdx`, and becomes `and r9, 7`.

**Only unsigned.** For a signed type the two are not the same: Firn rounds
towards zero, an arithmetic right shift towards minus infinity, so `-1 / 2` is
`0` and `-1 >> 1` is `-1`. There is a module test for that too
(`signed_division_stays_a_division`). The correct signed sequence needs a bias
and three more instructions; it is §7 point 3.

**Measured, on a program and not on a listing.** A 200,000,000 pass loop with
`i % 1024`, `i / 256` and one negated comparison in it
(`--no-pass=strength` switches the pass off, everything else identical):

```
without the pass   3041 ms      21 instructions in the loop function
with the pass       341 ms      13 instructions
                   8.9x         same result (138)
```

On the compiler's own source the pass costs 33.9 ms of 3120 ms (1.1 % of the
optimizer) and is worth its place.

### 5.3 The register allocation — measured, and one clear finding

`FIRN_RA_STATS=1` makes `regalloc.rs` write one line per function;
`tools/bench82/ra_report.py` adds them up. Three real workloads:

| | functions | on the base path | values | in registers | SPILLED |
|---|---|---|---|---|---|
| DEFLATE (`tools/stdlib81/deflate_cli.fi`) | 667 | 37 (**5.3 %** of the code) | 35,640 | 38.1 % | **45.2 %** |
| the JS engine (`lib/js/run_main.fi`) | 1,561 | 222 (**23.6 %** of the code) | 148,127 | 29.3 % | **50.4 %** |
| the compiler itself (`bin/firnc1.fi`) | 1,308 | 6 (**0.1 %**) | 174,459 | 19.0 % | **57.2 %** |

("Spilled" is what is left after the values that need no storage at all are
taken out: constants that stand as an immediate at every use site, and
`alloca` addresses folded into the operand. Those are counted separately and
are not spills.)

**The finding, and it is a number the round did not have before: 23.6 % of the
JavaScript engine's code gets NO register allocation at all.** 222 functions,
32,819 instructions, every value in a frame slot, every use a memory access.
The reason is a single line in `regalloc.rs::unsupported_basic`: *"f64 in the
value set"*. That is restriction F1 of round 71 — the linear scan knows only
the integer registers, so one `f64` anywhere in a function puts the WHOLE
function on the base path, integer code and all. In a JavaScript engine, where
every number is an `f64`, that is a quarter of the code.

For comparison: the compiler itself has almost no floating point and reaches
0.1 %. DEFLATE sits between the two at 5.3 %.

That is the single biggest thing an "optimizer round" could still buy, and it
is not this round's work — it is a second register class in the linear scan
(§7 point 5). Round 82 made the same restriction apply to `v128` and then
built the xmm cache of §3.3 so that vector code does not pay for it; the
floating point side has no such cache and pays in full.

The hottest single functions, for scale:

```
gctext__gctext_write      68657 instructions, 56359 values, 463 in registers, 65.9 % spilled
unicode_id__id_continue_fill   2906 instructions, 3018 values,   1 in registers, 53.6 % spilled
deflate__emit_block             689 instructions, 1000 values, 396 in registers, 49.4 % spilled
```

`unicode_id__id_continue_fill` getting exactly ONE register out of 3,018
values is not a typo and not explained by this round. It is a generated table
filler; the intervals in it apparently cross something the allocator will not
hand a register across. Named here, not fixed.

### 5.4 The compiler on itself — where the time goes

`firnc --timings` (new in this round) prints the wall clock per phase.
`bin/firnc1.fi`, 30,643 lines of Firn, `--opt-level=release-fast`:

```
  optimizer             3120.3 ms   61.1 %
  codegen                975.4 ms   19.1 %
  as + ld                668.5 ms   13.1 %
  sema                   157.4 ms    3.1 %
  lex+parse              106.7 ms    2.1 %
  lower                   73.1 ms    1.4 %
  mono                     5.0 ms    0.1 %
  write .s                 2.8 ms    0.1 %
  comptime                 0.0 ms    0.0 %
total 5110.3 ms
```

**The three most expensive phases are the optimizer (61 %), the code generator
(19 %) and the assembler/linker (13 %).** Everything the front end does
together is 7 %.

`FIRN_PASS_TIMINGS=1` goes one level deeper, into the optimizer:

```
  mem2reg             834.0 ms   26.5 %      5,840 fixpoint rounds in all
  licm                802.4 ms   25.5 %      (over ~1,300 functions, so
  merge-blocks        604.3 ms   19.2 %       about 4.5 rounds per function)
  inline              291.9 ms    9.3 %
  cse                 182.1 ms    5.8 %
  dce                 175.0 ms    5.6 %
  fold                110.4 ms    3.5 %
  copyprop             37.9 ms    1.2 %
  thread-bool          35.6 ms    1.1 %
  strength             33.9 ms    1.1 %   <- the new pass of §5.2
  simplify-term        27.6 ms    0.9 %
  bce                  15.8 ms    0.5 %
```

**The cheap improvement that came out of it**, and it is cheap in both senses:
`licm` and `mem2reg::promote_single_store` both built a **dominator matrix**
before they had established that there was anything to do. `licm` did it for
every function, including the ones without a loop; `promote_single_store` did
it for every function, including the ones without an `alloca`. Two guards:

* `licm`: if EVERY control flow edge goes strictly forward in the block
  numbering, the graph is acyclic and there is no natural loop. Sufficient,
  not necessary — a function numbered differently still takes the long way.
* `promote_single_store`: no cells, no dominators.

Measured, best of three, same machine, and **the emitted assembler is
character-identical** (that is the point — this is a pure cost saving, not a
change of behaviour):

```
without the two guards   4919 / 5033 ms
with them                4725 / 4720 ms      -5.1 %
```

Five percent is not a revolution. It is what an honest measurement of a
lightly instrumented compiler gives, and it is reported as such. The 61 % that
the optimizer costs is not waste — it is 5,840 fixpoint rounds doing real
work; making it substantially cheaper means changing the fixpoint itself
(running a pass only when something it depends on changed), and that is a
round of its own.

For Justin's six hour acceptance the relevant number is a different one:
`tools/fixpoint.sh` measured **stage 2 in 12,685 ms and stage 3 in 36,750 ms**
in the acceptance run of this round, stage 2 and stage 3 character-identical
over 649,720 lines of assembly. Stage 3 is `firnc1` compiling itself, and `firnc1` has no
register allocation at all (`lib/firnc1/codegen.fi`, every value in a frame
slot). That factor of three is where the acceptance time sits, and closing it
means giving the self-hosted code generator registers — not making `firnc0`
faster.

---

## 6. aarch64

The equivalents exist — `aese`/`aesmc`/`aesd`/`aesimc` for AES,
`sha256h`/`sha256h2`/`sha256su0`/`sha256su1` for SHA-256, and NEON has
`v128` in hardware in a way SSE2 does not (32 registers instead of 16).

**They are not in this round, and the reason is a merge, not a difficulty.**
The aarch64 code generator is round 80, branch `r80-arm`, and it was **not on
`main`** when this round started (`main` was at `d87113b3`, the merge of R79).
Building against a branch that is still being reviewed would have produced a
conflict for somebody else to resolve.

What is here instead: `Op::Simd` reaches exactly one code generator
(`codegen_x86.rs`). When `codegen_a64.rs` arrives it will hit its `match` on
`Op` and the compiler **will not build** until somebody writes the arm — which
is the right kind of failure. It cannot silently produce wrong code.

The work for a later round is small and well shaped:
`compiler/src/simd.rs` already separates the *set* (the `TABLE`, the sema hook,
the lowering) from the *emission* (`emit`, `xget`/`xdef`, `emit_cpuid`). Only
the second half is x86. The feature question changes from `cpuid` to
`getauxval(AT_HWCAP)` / `HWCAP_AES` / `HWCAP_SHA2`, and `lib/std/cpu.fi` is
the one place that would have to learn it.

---

## 7. What was deliberately left undone

1. **AES-192 and AES-256.** Round 81 stopped at 128 bit keys for a stated
   reason (the key schedule differs by more than a loop bound) and this round
   did not widen that. The hardware path expands the same eleven round keys the
   scalar path has.
2. **GCM.** It needs `pclmulqdq` — which this round DOES expose
   (`__pclmulqdq`) — plus an authentication design, a tag comparison that must
   be constant time, and nonce discipline. Half a GCM is worse than none.
3. **Signed division by a power of two.** `peephole.rs` converts the unsigned
   case to `shr`/`and`. Signed is not the same thing: Firn rounds towards zero,
   an arithmetic shift towards minus infinity, so `-1 / 2` is `0` and
   `-1 >> 1` is `-1`. The correct sequence needs a bias (`sar`/`add`/`sar`) and
   three more instructions. Not done.
4. **Division by a general constant** through a multiply-high. `gcc -O2` does
   it, Firn does not. It is a bigger change (128 bit multiplication, a
   magic-number search) and it belongs to its own round.
5. **A second register class in the linear scan.** `f64`/`f32` (since round 71)
   and `v128` (since this round) push a whole function onto the base path. The
   xmm cache of §3.3 makes that bearable for vector code — the numbers in §2
   prove it — but the *integer* code of such a function still runs without
   register allocation. §5.3 measures what that costs.
6. **CBC decryption in four parallel streams.** CBC decryption is
   parallelisable (every block needs only the *ciphertext* of its predecessor)
   and OpenSSL runs eight blocks at once to fill the latency of `aesdec`. This
   implementation is serial and still reaches 692.8 MiB/s, 1.46x behind. Four-way
   would probably close most of the rest; it needs sixteen more live vector
   values and would want the register class of point 5 first.
7. **AVX2 and the wider paths.** `__cpu_features` reports AVX2, nothing uses
   it. 256 bit vectors would need a `v256` type and `vzeroupper` discipline.

---

## 8. Files

New:

```
compiler/src/simd.rs            the 42 intrinsics, cpuid, the xmm register cache
compiler/src/peephole.rs        the three optimizer cases of §5.2
lib/std/cpu.fi                  the feature bits with names
lib/std/crypto/accel.fi         AES-NI and SHA-NI, and nothing else
tools/bench82/speed.fi          the stopwatch
tools/bench82/cross.fi          hardware against scalar, every length
tools/bench82/run.sh            the measurement, the yardsticks, the limits
tools/bench82/minquota_*.txt    the regression limits
docs/ROUND82.md                 this file
```

Changed:

```
compiler/src/types.rs           Type::V128
compiler/src/fir.rs             FTy::V128, Op::Simd, new_val_pub
compiler/src/sema.rs            'v128' as a type name, the call hook
compiler/src/lower.rs           the lowering hook, scalar_fty
compiler/src/codegen_x86.rs     16 octet slots, SSE class, Op::Simd, the flushes
compiler/src/regalloc.rs        v128 -> base path, crc32/cpuid on the register path
compiler/src/opt.rs             the pass 'strength'
compiler/src/main.rs            --timings
compiler/src/{inline,mem2reg,layout_canon}.rs   the new Op/Type in their matches
lib/std/crypto/sha256.fi        the dispatch, sha256_soft, block bulk
lib/std/crypto/aes.fi           the dispatch, the *_soft names
test.sh                         section 45
```

---
## 9. The acceptance

Run at the end of this round, on the machine named at the top, `bash test.sh`
from an empty `.test-work` and with `.firnc1`/`.firnc2`/`.firnc3` deleted
first, so that every stage was really rebuilt.

```
PASS 1197 / 1198          one failure, and it is not this round's (below)
```

| proof | result |
|---|---|
| positive tests, three build stages each | all pass, no `FAIL` in section 3 |
| negative tests (error messages) | all pass |
| the optimizer proof (`test_opt.sh`) | **PASS 45/45** |
| `tools/self_compare.sh` (section 16) | **321 the same, 0 differing, 0 faulty** |
| `tools/fixpoint.sh` (section 17) | **stage 2 == stage 3, character-identical**, 649,720 lines of assembly |
| `tools/stdlib81/run.sh` (section 41) | NIST **1919 ok, 0 wrong**; python/openssl 106 of 106 |
| `tools/bench82/run.sh` (section 45, new) | `RESULT ok`, every limit met |
| `tools/english/check.sh` | 0 German identifiers, 0 paths, 0 comment lines |
| `firnfmt -c` over the whole tree | everything in canonical shape |
| `cargo test` | 216 passed, 0 failed |

### The one failure, and why it is not this round's

```
== 9d. JavaScript: lexer, parser, interpreter (tools/js/run.sh) ==
  FAIL  tools/js/run.sh failed
      jobs   rc=-11    6.7s  RSS first 11332 KiB  max 12356 KiB  growth +1024 KiB
             output: jobs 120 0 | OK
```

`rc=-11` is SIGSEGV in the promise endurance run. The program produces its
CORRECT output and then dies. `docs/ROUND76.md` §4.6 recorded it on `main`,
`docs/ROUND78.md` established it again there, and it is **intermittent**.

Established a third time here, because "inherited" is a claim and not an
excuse. The same job blob (200 realms of the promise program in one process),
eight runs on each tree, same machine, same minute:

| tree | SIGSEGV |
|---|---|
| `main` (`d87113b3`) | **6 of 8** |
| `r82-speed` | **6 of 8** |

Identical. The two engines were built from the same `lib/js/` with the
respective compiler of each tree.

The crash deserves a round of its own — it is an abandoned promise queue
under the collector, which is exactly where a dangling reference would show.
Section 45 of this round cannot see it and does not claim to.

### Reproducing the numbers

```sh
bash tools/bench82/run.sh                  # the table of §2 and §5.1
BENCH82_FULL=1 bash tools/bench82/run.sh   # the same with bigger buffers

firnc --timings --opt-level=release-fast -o /tmp/a bin/firnc1.fi   # §5.4
FIRN_PASS_TIMINGS=1 firnc --opt-level=release-fast --emit=fir \
    -o /dev/null bin/firnc1.fi                                     # per pass

FIRN_RA_STATS=1 firnc --opt-level=release-fast -o /tmp/b lib/js/run_main.fi \
    2>&1 >/dev/null | python3 tools/bench82/ra_report.py --hot 8   # §5.3

FIRN_NO_XMM_CACHE=1  firnc ...   # the xmm cache off  (§3.3)
FIRN_NO_XMM_RETIRE=1 firnc ...   # the retirement off (§3.3)
firnc --no-pass=strength ...     # the new optimizer pass off (§5.2)
```
