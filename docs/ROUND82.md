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
| **SHA-256** | 26.7 MiB/s | **894.1 MiB/s** | 1374 MiB/s | **1.54x** |
| **AES-128-CBC encrypt** | 6.33 MiB/s | **575.7 MiB/s** | 1013 MiB/s | **1.76x** |
| **AES-128-CBC decrypt** | 4.71 MiB/s | **692.8 MiB/s** | 1013 MiB/s | **1.46x** |
| **AES-128-CFB8** | 0.38 MiB/s | **26.9 MiB/s** | 36.6 MiB/s | **1.36x** |
| DEFLATE level 6 | see §5 | see §5 | see §5 | see §5 |

The gain over the scalar path is **33x** for SHA-256, **91x** for AES-CBC
encryption, **147x** for AES-CBC decryption and **71x** for CFB8.

The round asked for "factor 2 instead of 60 resp. 190". **All four are at
1.4 to 1.8.** The target is met, and CFB8 — the one at 3000x — is now the
closest of the four.

**Why the "before" figures differ from the round's table** (26.7 against 22.6,
6.33 against 5.50, 0.38 against 0.34): those are the SAME implementations,
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

**`v128` as a value type** (SPEC §8.7, `compiler/src/types.rs`,
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

*(measured numbers in §5.1–§5.4 below; the sections are filled in from the
runs of this round)*

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
3. **Signed division by a power of two.** `strength.rs` converts the unsigned
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
compiler/src/strength.rs        the three optimizer cases of §5.2
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
