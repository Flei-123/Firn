# Round 91 — the vector instructions on the second machine

Branch `r91-a64simd`. One sentence: **round 82 built 42 vector and crypto
intrinsics for x86-64, round 91 builds their aarch64 counterparts** — and
with that the last case in which the two machines disagreed is gone.

Everything below was **run**. Every number has the command next to it.

**Machine:** AMD EPYC 7571, 8 vCPU, Debian 12, Linux x86_64.
**Toolchain:** `rustc` 1.99.0-nightly, GNU `as`/`ld` 2.40,
`aarch64-linux-gnu-as`/`-ld` 2.40, `qemu-aarch64` 7.2.22.
**Base:** `main` at `458e48e2`.

---

## 1. What was measured before the round

```
$ bash tools/aarch64/run.sh                       # main at 458e48e2
  DIFF tests/1613_crypto.fi :: aarch64 compilation failed:
       --target=aarch64-linux cannot emit the vector instruction Load yet.
  SAME:           294
  DIFFERENT:      1
  NOT SUPPORTED:  4
  ENVIRONMENT:    1
  x86 already:    4
  RESULT: 294 of 299 comparable cases identical on both machines (98%)
FAIL 1 case(s) differ between the two machines
```

One case. It was the only one left: everything else in the corpus already did
the same thing on x86-64 and under `qemu-aarch64`, character for character.

The history behind that one line is worth two sentences, because it is a
lesson about refusing too much. Round 82 gave the language `v128` and 42
intrinsics; it wrote them for x86-64 only and made `Op::Simd` a named abort on
aarch64. Round 87 found that the abort had swallowed more than it should:
`sha256_new()` asks `__cpu_features()` **unconditionally**, so the question
alone — a question about the machine, not a vector instruction — made the
whole crypto library uncompilable for aarch64. Round 87 answered it (with
zero) and gave `__crc32_u8`/`__crc32_u64` their real instructions, and said
plainly that this was not the whole repair: `lib/std/crypto/accel.fi`
**mentions** `v128`, and a mention was enough. `tests/1613_crypto.fi` then
failed on `Load` instead of on `CpuFeatures`.

This round removes the mention problem by removing the reason for it.

---

## 2. The result

```
$ bash tools/aarch64/run.sh              # dev-fast
  SAME:           296
  DIFFERENT:      0
  NOT SUPPORTED:  4     (inline assembler — x86 text by its nature)
  ENVIRONMENT:    1     (proven by the C probe in the same run)
  x86 already:    4
  RESULT: 296 of 300 comparable cases identical on both machines (98%)
PASS no case differs between x86-64 and aarch64

$ bash tools/aarch64/run.sh --no-opt
  SAME:           296
  DIFFERENT:      0
  RESULT: 296 of 300 comparable cases identical on both machines (98%)
PASS no case differs between x86-64 and aarch64
```

`tests/1613_crypto.fi` is `SAME` in both stages, and one case is new:
`tests/1614_simd_ops.fi` (section 6).

The four `x86 already` are not this round's: they are programs that meant to
wrap and said `as`, and they were red on `main` before the branch existed.

---

## 3. The three kinds of counterpart

An intrinsic is named after an x86 instruction and **means that
instruction's semantics**, not "whatever the other machine's instruction of
the same name does". Everything in `compiler/src/simd_a64.rs` falls into one
of three groups.

### 3.1 Exact — one instruction for one instruction

| x86 | A64 | | x86 | A64 |
|---|---|---|---|---|
| `movdqu` load/store | `ldr q` / `str q` | | `pxor` | `eor .16b` |
| `pand` | `and .16b` | | `por` | `orr .16b` |
| `paddb/d/q` | `add .16b/.4s/.2d` | | `psubd` | `sub .4s` |
| `punpckldq` | `zip1 .4s` | | `punpckhdq` | `zip2 .4s` |
| `punpcklqdq` | `zip1 .2d` | | `punpckhqdq` | `zip2 .2d` |
| `pslld`/`psllq` | `shl .4s`/`.2d` | | `psrld`/`psrlq` | `ushr .4s`/`.2d` |
| `palignr` | `ext` | | `pblendw` | `bsl` |
| `pextrd`/`pextrq` | `umov` | | `pinsrd` | `ins` |
| `aesimc` | `aesimc` | | `sha256msg1` | `sha256su0` |
| `pclmulqdq` | `pmull` | | `crc32b`/`crc32q` | `crc32cb`/`crc32cx` |

Three of them are worth a line each because the *rule* differs, not the
instruction:

* **`pandn(a, b) = ~a & b`**, A64's `bic d, n, m` is `n & ~m` — so the two
  operands change places. Exactly here and nowhere else.
* **`pshufb`** writes a zero octet for an index whose top bit is set and
  otherwise uses only the low four bits; `tbl` writes a zero for every index
  ≥ 16. One `and` against `0x8f` turns the one rule into the other, exactly.
* **`psrld xmm, 0`** is legal on x86; `ushr` cannot encode a shift of zero at
  all (its range is 1..=width). That case is a move.

`pshufd` has no immediate-lane-shuffle counterpart on A64 at all: the
permutation becomes a 16-octet index vector and one `tbl`.

### 3.2 The counterpart exists, and is laid out differently — AES

This is the trap of the round, and it is the one the task named in advance.

```
x86   AESENC(s, k) = MixColumns(SubBytes(ShiftRows(s))) xor k
ARM   AESE(s, k)   = SubBytes(ShiftRows(s xor k))
      AESMC(x)     = MixColumns(x)
```

ARM's `AESE` **contains** the AddRoundKey and does it **first**; x86's
`AESENC` does it **last**. Writing `aese s, k` for `__aesenc(s, k)` compiles,
assembles, runs — and computes a different cipher. The identity that does
hold puts a zero where ARM expects the key and xors the real key on
afterwards:

```
AESENC(s, k)     == AESMC (AESE(s, 0)) xor k      # movi/mov/aese/aesmc/eor
AESENCLAST(s, k) ==        AESE(s, 0)  xor k
AESDEC(s, k)     == AESIMC(AESD(s, 0)) xor k
AESDECLAST(s, k) ==        AESD(s, 0)  xor k
```

It was checked before a line of the code generator was written, in C under
the same `qemu-aarch64` (`/tmp` scratch, not kept): the FIPS 197 C.1 vector
through those four identities gives `69c4e0d86a7b0430d8cdb78070b4c55a` and
decrypts back to `00112233445566778899aabbccddeeff`. It is checked again in
this repository, in Firn, on both machines — `tests/1614_simd_ops.fi`
section E.

**`aeskeygenassist` is the harder one of the family.** It needs plain
SubBytes, and A64 has no plain SubBytes: `aese` always shifts the rows first.
So the rows are shifted **back** before it — one `tbl` with the inverse
permutation `[0,13,10,7,4,1,14,11,8,5,2,15,12,9,6,3]`, which is
`i - 4*(i mod 4) (mod 16)` and is checked as such by a unit test
(`simd_a64::tests::the_inverse_shiftrows_table_is_the_inverse`) — and what
comes out of `aese` is then SubBytes and nothing else. A second `tbl` picks
the two `SubWord`/`RotWord` pairs and one `eor` puts the round constant in.
Six instructions and three constants.

That is not decoration. `tests/1614_simd_ops.fi` builds the **whole AES-128
key schedule out of `__aeskeygenassist`** and holds it against FIPS 197
A.1 — `d6aa74fdd2af72fadaa678f1d6ab76fe` for round 1 and
`13111d7fe3944a17f307a78b4d2b30c5` for round 10 — before it encrypts
anything with it.

### 3.3 The counterpart does not decompose the same way — SHA-256

| | state | rounds per instruction |
|---|---|---|
| x86 `sha256rnds2` | `ABEF` / `CDGH`, two registers | **2** |
| ARM `sha256h`/`sha256h2` | `ABCD` / `EFGH` | **4** |

There is no way to build one out of the other. Four rounds cannot be halved,
and the state is not split along the same line. The same for the message
schedule: ARM's `sha256su1` folds in a `W[i+9]` term that x86's `SHA256MSG2`
does not, and that term comes from registers `SHA256MSG2` never sees.

So two of the three are **written out**: `__sha256rnds2` is the two rounds it
defines, in general registers (fourteen of them at once, no memory traffic in
between), and `__sha256msg2` is the four σ₁ steps with their serial
dependency. The third, `__sha256msg1`, **is** `sha256su0` word for word and
gets it.

What that costs, measured on the emitted assembler
(`firnc --emit=asm --opt-level=release-fast`, instruction lines per
function):

| function | x86-64 | aarch64 | |
|---|---|---|---|
| `accel__aes128_ni_encrypt_block` | 117 | 191 | 1.63x |
| `accel__aes128_ni_cbc_encrypt` | 200 | 258 | 1.29x |
| `accel__aes128_ni_cfb8_encrypt` | 234 | 295 | 1.26x |
| `accel__sha256_ni_blocks` | 1559 | **5605** | **3.60x** |

The 3.6x is the SHA emulation, and it is visible in the mnemonic histogram of
that one function: 480 `ror`, 392 `umov`. Part of the rest is that this
backend has no register allocator — every FIR value goes through its frame
slot (1232 `ldr`, 769 `str`), which is round 80's deliberate base path and
not this round's doing.

**And it is still worth having**, which is a measurement and not an opinion —
see section 5.

---

## 4. `__cpu_features()` without `cpuid`

aarch64 has no `cpuid`. The kernel answers this question through **AT_HWCAP
in the auxiliary vector**, and the usual way to read it is `getauxval(3)` —
a libc function, and this compiler has no libc.

What libc does is read the vector the kernel wrote above the initial stack,
and that pointer is still there. So `_start` keeps it (eight octets in
`.bss`, four instructions, and only in a program that asks at all), and
`__cpu_features()` walks it: argc, argv, the NULL after argv, the
environment, the NULL after it, then pairs of (type, value) until type 0.
Type 16 is AT_HWCAP.

The mapping — the bits keep their x86 names because that is where they were
minted, and what they mean on either machine is "the intrinsics of this
family work here":

| bit | name | aarch64 answer | why |
|---|---|---|---|
| 0 | SSE2 | HWCAP_ASIMD | the 128-bit integer instructions |
| 1 | SSE4.1 | HWCAP_ASIMD | `pblendw` → `bsl`, `pextrd` → `umov` |
| 2 | SSE4.2 | HWCAP_CRC32 | this bit is only ever asked about `crc32` |
| 3 | AES-NI | HWCAP_AES | `aese`/`aesmc`/`aesd`/`aesimc` |
| 4 | PCLMULQDQ | HWCAP_PMULL | `pmull` |
| 5 | SHA-NI | HWCAP_SHA2 | `sha256su0`; the other two are built |
| 6 | AVX2 | **0** | there is no 256-bit register here |
| 7 | BMI2 | **0** | no intrinsic of round 82 needs it |
| 8 | SSSE3 | HWCAP_ASIMD | `pshufb` → `tbl`, `palignr` → `ext` |

Measured, same program, both machines:

```
$ firnc --target=x86_64-linux  ...   ->  1ff   accel_aes true  accel_sha true
$ firnc --target=aarch64-linux ...   ->  13f   accel_aes true  accel_sha true
```

`0x13f` is `0x1ff` without AVX2 and without BMI2 — exactly the two rows that
say 0 above. Round 87's answer was `0`, and with `0` every dispatch in
`lib/std/crypto` took its scalar path; now the same binary takes the fast one
where the machine really has the instructions.

`qemu-aarch64` 7.2.22 with its default CPU reports
`AT_HWCAP = 0xecfffffb` — FP, ASIMD, AES, PMULL, SHA1, SHA2, CRC32 and
atomics all present — which is why the accelerated path is the one that is
actually exercised by `tools/aarch64/run.sh` here.

### 4.1 A bug that fell out of this: `.arch`

`aese`, `sha256su0`, `crc32cb` and `pmull` are **optional** extensions of
armv8-a, and GNU as refuses them unless it is told:

```
$ aarch64-linux-gnu-as -o /dev/null <<< 'aese v1.16b, v0.16b'
Error: selected processor does not support `aese v1.16b,v0.16b'
```

Round 87 emitted `crc32cb`/`crc32cx` **without** that line. No program in the
corpus reaches `__crc32_u8` on aarch64, so the assembler was never asked and
nobody found out. Six lines of Firn are enough to see it — with the compiler
from `main` (`22aca863`, before this branch):

```
$ firnc --target=aarch64-linux -o crc crc.fi        # acc = __crc32_u8(acc, 'A')
crc.s:29: Error: selected processor does not support `crc32cb w9,w9,w10'
crc.s:35: Error: selected processor does not support `crc32cx w9,w9,x10'
error: 'aarch64-linux-gnu-as' failed (exit status: 1)

$ # the same program, round 91:
$ firnc --target=aarch64-linux -o crc crc.fi && qemu-aarch64 ./crc; echo $?
0
```

One line at the top of the emitted text fixes it for the whole family:

```
.arch armv8-a+crypto+crc
```

It costs a program that uses none of them nothing: it selects what the
**assembler** accepts, not what the processor has. What the processor has is
a run time question, and section 4 is the answer to it.

---

## 5. Is the accelerated path worth anything on this machine?

Under `qemu-aarch64` a wall clock is not a statement about ARM silicon —
qemu's cost per guest instruction is roughly uniform, so what this measures
is **how many instructions the two paths execute**, not what a real chip
would do. As that, it is exactly the question worth asking about an emulated
`__sha256rnds2`.

4 MiB through `lib/std/crypto`, `--opt-level=release-fast`, best of three,
wall clock of the whole process:

| | scalar path | accelerated path | |
|---|---|---|---|
| **aarch64** (qemu) SHA-256 | 1749 ms | **339 ms** | **5.2x** |
| **aarch64** (qemu) AES-128-CBC | 11565 ms | **287 ms** | **40.3x** |
| x86-64 (native) SHA-256 | 173 ms | 29 ms | 6.0x |
| x86-64 (native) AES-128-CBC | 537 ms | 32 ms | 16.8x |

So even the **emulated** SHA-256 path — 3.6x more instructions than the x86
one — is 5.2x fewer instructions than the scalar Firn implementation, because
that one pays a frame slot for every intermediate value. Reporting bit 5 as
"available" is therefore not a courtesy: the dispatch it drives picks the
faster of the two, on this machine as much as on the other one.

The AES figure is what one expects when ten scalar rounds become ten `aese`.

---

## 6. `tests/1614_simd_ops.fi` — the new guard, and why it exists

Only **twenty** of the 42 intrinsics are reachable through `lib/std/crypto`
and therefore through `tests/1613_crypto.fi` (twenty-one with `__cpu_features`,
which `lib/std/cpu.fi` asks). The other twenty-two would have
been code that compiles, assembles, and nobody ever ran. An untested
instruction mapping is a claim, not a measurement.

So the new case holds every intrinsic against the same thing computed
**without** it, in plain Firn, in the same file: the byte permutations
against byte loops, the lane arithmetic against `rt.ld32`/`rt.st32`, CRC-32C
against eight shifts of the reflected polynomial per octet, the carry-less
product against shift-and-xor, AES against FIPS 197 A.1 and C.1, SHA-256
against FIPS 180-4 Appendix A.

That check is **not** about the two machines: it fails on x86-64 just as
loudly. What the two machines add is `tools/aarch64/run.sh`, which compiles
the same file for both and compares what it prints — so a mapping that is
wrong only on aarch64 shows up there.

It asks the processor first (`cpu.has_ssse3()`, `has_aes()`, `has_sha()`,
`has_sse42()`, `has_pclmul()`) and prints the same line and returns 0 on a
machine that has none of it. That is the house rule of round 82 and the
reason the scalar path still exists.

**No new `test.sh` section.** `tests/*.fi` is already the corpus of section
`4`, of `tools/aarch64/run.sh` (section 43) and of `tools/self_compare.sh`;
a new guard that is a test program needs no new gate. (`grep -n 'echo "== '
test.sh` — the next free number would be 49, and it stays free.)

Verified in all four build levels on both machines:

```
dev / dev-fast / release-safe / release-fast   x  x86_64 / aarch64
  tests/1614_simd_ops.fi -> 'simd ok'  rc=0     (8 of 8)
  tests/1613_crypto.fi   -> 'crypto ok' rc=0    (8 of 8)
```

---

## 7. What changed in the compiler

| file | |
|---|---|
| `compiler/src/simd_a64.rs` | **new** — the only place an A64 vector instruction is written down. The counterpart of `simd.rs`. |
| `compiler/src/codegen_a64.rs` | the `v128` value model: a sixteen-octet, sixteen-aligned frame slot; the SIMD&FP class of AAPCS64 for parameters and the result; `Op::Load`/`Op::Store` of `v128`; the `.arch` line; the auxiliary-vector pointer in `_start`. `Op::Simd` is one line now. |
| `compiler/src/main.rs` | `mod simd_a64;` |
| `lib/std/cpu.fi` | the aarch64 answer, written down where a user of the library looks for it. |
| `tests/1614_simd_ops.fi` | **new** — section 6. |

### The registers

`v16`-`v23` are caller-saved in AAPCS64, and the base path of round 80 keeps
**no** value in a register across two instructions — every FIR value has a
frame slot. So the vector scratch set is free at the start of every
`Op::Simd`, and so are `x0`-`x7`: they are argument registers, and arguments
are only ever loaded immediately before a `bl` or an `svc`. `__sha256rnds2`
needs fourteen 32-bit registers at once and uses them for exactly that
reason. `x12` stays what it is everywhere else in this backend: addresses
only.

### What is still refused, and says so

A **ninth** `v128` argument to a call. AAPCS64 gives a stack-passed 128-bit
vector sixteen octets and a sixteen-octet boundary; the outgoing area of this
backend counts in words of eight. Nothing in this repository passes more than
eight vector arguments, so the case is refused with a message that names
itself rather than laid out wrong. Everything else that round 82 can express,
this backend now emits — the `match` over `SimdKind` is exhaustive, so the
compiler itself enforces that there is no forty-third case hiding.

---

## 8. Acceptance

```
$ cargo test --release --manifest-path compiler/Cargo.toml
    238 passed; 0 failed

$ bash tools/aarch64/run.sh                 SAME 296  DIFFERENT 0   PASS
$ bash tools/aarch64/run.sh --no-opt        SAME 296  DIFFERENT 0   PASS
$ bash tools/optlevels/run.sh               optlevels: ok
$ bash tools/fixpoint.sh                    FIXPOINT: stage 2 == stage 3,
                                            character-identical
                                            CORPUS: .firnc2 behaves like firnc0
```

`lib/firnc1` does not need to learn anything: it recognises `v128` and the
`__v128_*` / `__aes*` / `__sha256*` / `__pclmulqdq` / `__crc32_*` /
`__cpu_features` spellings by prefix and counts such a file as "not core"
(round 82, `lib/firnc1/parser.fi`). `tests/1614_simd_ops.fi` joins
`tests/1613_crypto.fi` in that bucket, cleanly, and the fixpoint is
unaffected.

---

## 9. What is not done

* **AVX2 and BMI2 have no aarch64 answer and get none.** There is no
  256-bit register on this machine and no intrinsic of round 82 needs BMI2;
  both bits are 0 and a program that asks takes its other path.
* **`__sha256rnds2` and `__sha256msg2` are emulations**, and the 3.60x of
  section 3.3 is what they cost. Closing it needs the *pair* of `rnds2`
  calls to be recognised as the four rounds they are and turned into one
  `sha256h`/`sha256h2` — that is a peephole over two FIR instructions, not a
  translation of one, and it belongs to a round that has a reason to want
  SHA-256 fast on ARM.
* **No register allocation on aarch64.** 1232 `ldr` and 769 `str` in one
  function are the base path of round 80, not this round. `regalloc.rs`
  refuses `v128` outright on x86 too (`"v128 in the value set"`), so the two
  machines are in the same place there.
* **The numbers in section 5 are qemu's.** They are an instruction-count
  ratio, honestly labelled as one. What Firn's AES does on real ARM silicon
  is unmeasured, because there is no ARM machine here.
