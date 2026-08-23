# Round 80 — the second machine: aarch64

Until this round Firn produced x86-64 and nothing else. That was not a
limitation somebody had written down; it was a limitation nobody had had to
notice. The intermediate representation was *called* machine independent,
the code generator was *called* a back end — but with one back end, the
sentence "FIR knows no x86 quirks" (`compiler/src/fir.rs`, line 10) could
not be checked by anybody, including its author.

This round checks it. `firnc --target=aarch64-linux` compiles the same
source, through the same lexer, parser, checker, lowering and optimiser,
into A64 for Linux, and the result is compared against the x86-64 build of
the SAME program by running both and looking at what they print.

---

## 1. The numbers

`tools/aarch64/run.sh` compiles every case of `tests/*.fi` twice, runs the
x86-64 build natively and the aarch64 build under `qemu-aarch64`, and
compares the standard output character for character and the exit code.

| | optimised (`release-fast`) | unoptimised (`--no-opt`) |
|---|---|---|
| **SAME** | **290 of 294 (98 %)** | **290 of 294 (98 %)** |
| DIFFERENT | 0 | 0 |
| NOT SUPPORTED | 4 | 4 |
| ENVIRONMENT (proven) | 1 | 1 |
| x86 already failing | 0 | 0 |

295 cases go in; 294 are comparable, 290 of them behave identically on both
machines, and the four that do not are all the same thing (inline assembler,
§5). The two build stages agree case for case.

`tools/aarch64/machine.sh` adds 16 checks that do not look at behaviour at
all: the ELF header, the relocation types, the disassembly, and the calling
convention against `aarch64-linux-gnu-gcc`. All 16 pass.

Both scripts are hung into `test.sh` as section 41, and the suite as a whole
stands at **PASS 1171, FAIL 2 of 1173** — the two failures being the ones
that are red on `main` too (§9).

The five cases that are not SAME are named, one by one, in §5. None of them
is filtered out of the corpus; the tool refuses to exit 0 if a case ever
lands in DIFFERENT.

---

## 2. Where the seam runs now

```
source ─▶ lexer ─▶ parser ─▶ checker ─▶ lowering ─▶ FIR ─▶ optimiser ─▶ FIR
                                                                        │
                                       target::active()  ───────────────┤
                                                                        │
                          ┌─────────────────────────────────────────────┴────┐
                          ▼                                                  ▼
                  codegen_x86.rs                                     codegen_a64.rs
                  (+ regalloc.rs, codegen_switch.rs)                 (base path only)
                          │                                                  │
                   as --64 / ld                             aarch64-linux-gnu-as / -ld
```

Everything left of `target::active()` was touched by this round in exactly
one way: not at all. The frontend has no idea which machine it is working
for, and that is the whole claim of the round.

Three new files carry the machine knowledge:

**`compiler/src/target.rs`** — which machine this compilation is for, the
assembler and the linker that belong to it, and the assembler directives
whose *meaning* differs between the two ports of GNU as. That last one is
the kind of trap that only shows up when a second machine exists: `.align 8`
means "align to 8 bytes" in the x86 port and "align to 2^8 = 256 bytes" in
the AArch64 port. The three data blocks (`gc.rs`, `iface.rs`, `fnval.rs`)
now ask `target::align(8)` instead of writing the directive themselves. On
x86 the answer is the string that stood there before, character for
character.

**`compiler/src/syscalls.rs`** — the number in a FIR `syscall` is read as
the canonical *name* of the call ("the one that is 1 on x86-64") and
translated. This was the first thing that had to be decided, because every
test and every module of the standard library writes the x86-64 number
literally (`lib/std/rt.fi`: `const SYS_WRITE: i64 = 1`). On AArch64, 1 is
`io_destroy`. Compiling that unchanged would not have failed — it would have
run the wrong call.

Four calls need more than a number:

| x86-64 | aarch64 | why |
|---|---|---|
| `open(path, …)` = 2 | `openat(AT_FDCWD, path, …)` = 56 | the generic table has no `open` |
| `fork()` = 57 | `clone(SIGCHLD, 0, …)` = 220 | the generic table has no `fork` |
| `dup2(a, b)` = 33 | `dup3(a, b, 0)` = 24 | only the flag-carrying form survived |
| `arch_prctl(ARCH_SET_FS, p)` = 158 | `msr tpidr_el0, p` | not a system call here at all |

The last one is the interesting one: AArch64 lets user code write its own
thread pointer, so what costs a system call on x86 costs one instruction —
and that single line is what makes the tracing collector (which asks
`__thread_self()` on every allocation path) run on this machine at all.
`clone` itself is *refused* by number: the generic table swaps two of its
arguments, and an argument order is not something a number table can fix.
That is `Op::ThreadSpawn`'s business, and it is handled there
(`compiler/src/thread.rs`, `spawn_sequence_a64`).

**`compiler/src/codegen_a64.rs`** — the code generator, ~1,100 lines.

---

## 3. The machine model

The same one round 1 chose for x86 and that `codegen_x86.rs` still keeps as
its base path: **every FIR value gets its own 8-byte slot in the frame**,
computing happens in a handful of scratch registers, and across a call no
value lives in a register. That is not fast, and it is not meant to be — it
is *checkable*. The linear scan of `regalloc.rs` is a second, separate step
that x86 got in round 43; aarch64 can get the same treatment later, and
until then nothing here can be wrong for a reason that has to do with
register lifetimes.

```
x9   first operand / result      (the `rax` of this file)
x10  second operand              (the `rcx`)
x11  third operand               (the `rdx`)
x12  ADDRESSES ONLY -- built here when a frame offset does not fit the
     immediate field of a ldr/str. Nothing else ever lives in x12.
x13, x14  helpers (large immediates, loop counters, store status)
x16  the frame size when it is too large for `sub sp, sp, #imm`
d0/d1, s0/s1  the floating point pair
x0-x7, d0-d7  arguments (AAPCS64), x8 the system call number
x19-x28  never handed out, so the prologue saves none of them
```

The frame:

```
        +---------------------+ <- x29 + 16 + 8k : incoming stack arguments
        | saved x29, x30      | <- x29
        | value slots         |
        | alloca storage      |
        | outgoing stack args | <- sp + 0
        +---------------------+ <- sp
```

**`sp` is set once in the prologue and never moves again.** That is the one
deliberate difference to the x86 base path, which does `sub rsp`/`add rsp`
around every call that has more than six arguments. On A64 the offsets of
`ldr`/`str` are unsigned and scaled, so an `sp` that stays put is what keeps
every slot access a single instruction; the area for the arguments of the
*widest* call in the function is therefore reserved once, at the bottom of
the frame. Slot addresses are `sp + (frame_size − off)`, where `off` is the
same downward offset from `x29` that `codegen_x86::layout` computes — both
files partition the frame by the same rule and only address it differently.

A few places where the two machines really are different, rather than the
same thing spelled another way:

* **Constants.** A64 has no "move a 64-bit immediate" instruction.
  `imm_into` builds every value out of `movz`/`movk`, or out of a single
  `movn` when at most one 16-bit chunk differs from `0xffff` (−1 is one
  instruction, not four — a case that was wrong until the module test
  caught it).
* **Remainder.** A64 has no remainder instruction. `sdiv`/`udiv` gives the
  quotient and `msub` subtracts `quotient × divisor` from the dividend.
* **Floating point comparison.** `fcmp` sets the flags so that the
  unordered case (NaN) is `C=1, V=1, N=0, Z=0`. With that, `mi`, `gt`,
  `ls`, `ge` and `eq` are all false and `ne` is true — exactly what
  IEEE-754 demands, with no extra instruction. On x86 the same result needs
  the parity flag and a swap of the operands (`codegen_x86.rs`, `Op::Cmp`).
* **Addresses.** `.rodata` labels are reached with `adrp` + `add :lo12:`,
  which is where `R_AARCH64_ADR_PREL_PG_HI21` and `ADD_ABS_LO12_NC` come
  from. Jump tables hold absolute label addresses (`R_AARCH64_ABS64`) and
  are entered with `ldr` + `br`. Calls are `bl` (`R_AARCH64_CALL26`). All
  four appear in `tools/aarch64/machine.sh`'s object file, and the script
  checks for them by name.
* **Atomics.** Without the large system extensions there is no
  `ldadd`/`cas`; `Op::AtomicAdd` and `Op::AtomicCas` become `ldaxr`/`stlxr`
  loops, with a `clrex` on the path that skips the store.
* **Threads really run.** This was not planned for the round and is the
  part that surprised its author. `clone(2)` exists here, with two of its
  arguments swapped (`thread.rs`, `spawn_sequence_a64`), and the thread
  pointer that x86 sets with a system call is a register EL0 may write. Put
  together, `tests/860_thread_basic.fi` passes on aarch64 with everything it
  asks for: four threads counting 80,000 times under a futex mutex, the same
  count atomically, the channel — and the counter-check that an *unlocked*
  counter has to LOSE increments, which only holds if the threads really run
  at the same time. `tests/861_thread_gc.fi`, `tests/862_thread_local.fi`
  and `tests/83x_arc_*` behave the same on both machines.

---

## 4. What was checked, and how

**`tools/aarch64/run.sh`** — the cross check. Every case of `tests/*.fi`
lands in exactly one bucket:

* `SAME` — both compiled, both ran, same output and same exit code.
* `DIFFERENT` — both compiled and they do not agree (or the aarch64 side
  crashed, hung, or its assembler refused the text). **The script exits
  non-zero if this is not 0.**
* `NOT SUPPORTED` — the aarch64 code generator *refused* the program with a
  clear message. Nothing was emitted, nothing was guessed.
* `ENVIRONMENT` — the difference belongs to the runner and not to the code
  generator, **and that was proven in this very run** (see §5).
* `X86 ALREADY` — the x86-64 side does not meet its own expectation from
  line 1, so there is nothing to compare.

**`tools/aarch64/machine.sh`** — 16 checks that do not look at behaviour:

1. The object file says `EM_AARCH64`, and the x86 build of the same source
   still says `EM_X86_64` (the counter-check: a `--target` that changed
   nothing would pass every behaviour test).
2. All four promised relocation types appear (`readelf -r`).
3. The disassembly is what we think it is (`aarch64-linux-gnu-objdump -d`):
   the frame is `stp x29, x30, [sp, #-16]!`, the call is a `bl`, the dense
   `match` became an indirect `br`, the system call is `svc #0` — and
   `write(2)` travels as **64**, not as the x86 **1**.
4. **AAPCS64 against `aarch64-linux-gnu-gcc`**, in both directions and past
   the end of the register file: C calls Firn with ten integer words (two of
   them on the stack) and with nine floating point words (one on the stack),
   Firn calls C with the same shapes, and every argument is weighted by its
   own position so that a pair which changed places cannot cancel out.
   4 of 4 agree.
5. `extern fn` of round 75 on this machine: Firn calls a hand-written A64
   `strlen` (`tools/aarch64/impl.s`, not libc), and a C program calls back
   into a `#[export_c]` function.

---

## 5. The five cases that are not SAME, one by one

**Four × inline assembler** — `tests/850_asm_basic.fi`,
`tests/851_asm_volatile.fi`, `tests/852_asm_no_cse.fi`,
`tests/1242_asm_multi_out.fi`. The assembler template in these programs is
x86 *text* in the Firn source (`asm("mov rax, 60")`). There is nothing to
port: the source of the test is machine specific by definition. The code
generator says so and stops.

**One × `tests/1284_std_io_text_owner.fi`** — the case measures a leak by
the cheapest indicator there is: two thousand rounds of allocate/free have
to land on the same address. That holds on Linux, because `munmap` really
gives the range back. Under `qemu-aarch64` it does not.

This is exactly the kind of exception that is usually a lie, so it is not
taken on trust. `tools/aarch64/environment.txt` names the case *and* the
probe that has to back it, and `tools/aarch64/qemu_mmap_probe.c` is a plain
C program — no Firn, no `firnc`, no FIR — that does 2,000 rounds of
`mmap`/`munmap` and prints the drift. Under `qemu-aarch64` it drifts by
32,751,616 bytes; natively it drifts by 0. **If the probe ever reports that
the runner reuses addresses, the entry stops counting and the case is
DIFFERENT again.** Nothing else can ever enter that bucket.

**Nothing else.** Two cases needed a second look before that sentence was
true, and how they were handled is part of the result:
`tests/860_thread_basic.fi` and `tests/834_arc_thread.fi` measure TIMING —
they check that an *unlocked* counter loses increments, which is a statement
about four threads really running at the same time. With eight cases
compiling and running at once, that race stops happening; on the x86 side
just as much as under qemu. So `run.sh` ends with a **quiet pass**: every
case that did not come out SAME is run once more, alone, after all the
parallel work is done, and the second verdict is the one that counts. That
is not an exception list — it applies to every case by the same rule, and a
case that still disagrees on a quiet machine stays DIFFERENT. Both of these
come out SAME.

---

## 6. What aarch64 cannot do yet

Each of these reports itself with a message and stops the compilation. None
is silently compiled into something else.

| | why |
|---|---|
| `#[interrupt]` | its calling convention *is* x86 (`iretq`, the full register file by hand) |
| inline assembler | the template is x86 text; the language has no per-machine form for it |
| `profile kernel` | the freestanding path needs both of the above |
| debug information | no `.loc`, no `.debug_info` — `tools/dwarf/run.sh` stays x86 |
| register allocation | `regalloc.rs` is x86; the A64 path is the base path only, so the code is correct and slow |

Two **semantic differences** that no test in the corpus happens to hit, and
that are therefore stated here rather than discovered later:

* **Division by zero.** `7 / 0` raises SIGFPE on x86-64 (exit 136) and
  yields 0 on aarch64 — `sdiv` does not trap. Measured, not assumed.
  The right answer is not a machine-specific patch; it is the checked
  arithmetic of round 72 (see §7).
* **Floating point to integer, out of range.** `1e400 as i64` gives
  `0x8000000000000000` on x86 (`cvttsd2si`'s "integer indefinite") and
  `0x7fffffffffffffff` on aarch64 (`fcvtzs` saturates). Both are outside
  what the language defines; neither is wrong, and they are not the same.

**Structs across `extern fn`.** Firn's lowering classifies aggregates by
the System V rules (`abi.rs`) and hands the resulting words to the code
generator. Firn↔Firn calls are therefore consistent on both machines — both
sides read the same FIR. C↔Firn calls with *aggregate* parameters are not:
AAPCS64 classifies composites differently (homogeneous float aggregates,
and anything above 16 bytes by reference). The proof in
`tools/aarch64/machine.sh` covers scalars, ten integer words and nine
floating point words; aggregates across the C boundary are untested on this
machine and should be assumed wrong until `abi.rs` grows a second
classifier.

---

## 7. Checked arithmetic (round 72) — BUILT IN ROUND 83

The round description asked for the checked arithmetic of round 72 to come
along, with the overflow branch on the condition flags (`b.vs` / `b.cs`).
When this round ran it was **not on `main`**: the work sat on the unmerged
branch `r72-arith`, and there was no `Op` in the FIR of `main` that carried
an overflow check, so there was nothing to port.

**Round 83 merged that branch and built this half.** It turned out not to
be optional: the command line default is `dev-fast`, `dev-fast` CHECKS, and
`tools/aarch64/run.sh` compiles every case without a flag — an aarch64
backend that refused `Op::CheckedBin` would have refused nearly every test
program in the suite and the comparison above would have collapsed to a
handful of cases.

The estimate above held. `compiler/src/panic_rt_a64.rs` (about 550 lines)
does what it said: `adds`/`subs` with `b.vs` for a signed type, and for an
unsigned one `b.cs` after an addition and `b.cc` after a subtraction — A64
spells the borrow the other way round from x86, which is the one place the
symmetry breaks. A 64-bit multiplication asks `smulh`/`umulh` for the upper
half and compares it against the sign extension of the lower one. Two
things the estimate did not mention:

* **8 and 16 bits have no arithmetic on this machine at all.** There is no
  `adds w9, w9, w10` that means `i8`. The operation happens at 32 bits,
  where two 8-bit values cannot overflow, and what is checked is the RANGE
  of the exact result (`sxtb`/`uxtb`/`sxth`/`uxth`, compare, `b.ne`). The
  32-bit multiplication is the same thought at 64 bits.
* **The trampoline had to be written a second time.** The message TABLE is
  shared with x86 (`panic_rt::intern`/`rodata_asm` produce `.ascii`, which
  is not machine specific), so the panic text is the same octet sequence on
  both machines — but everything that prints it is instructions: the
  decimal formatter, `write` (system call 64 here, 1 there) and
  `exit_group` (94 here, 231 there) with the same exit code 101.

Measured, `tools/aarch64/run.sh`, build stage `dev-fast` (the checking one):
**288 of 293 comparable cases identical on both machines, DIFFERENT 0**,
NOT SUPPORTED 5 — four inline assembler and one `v128` of round 82.

The twelve programs of `tools/checked/` compiled for both targets in all
four build levels: 49 of 52 runs identical. The three that differ are
`release-fast` (the level that does NOT check) division by zero and
`MIN / -1`: x86 raises `SIGFPE` and dies with 136, while A64's `sdiv`
quietly yields 0 resp. `MIN`. That is the machine, and it belongs in the
list of §6 rather than in this one.

`profile kernel` is still refused on aarch64 (§2), so the `osum_panic`
hand-off has no A64 form yet — this file does not invent an ending for a
kernel that cannot be built for this target in the first place.

---

## 8. What `firnc1` on aarch64 would need

Round 80 required `firnc0` (the Rust compiler) only, and deliberately so.
The self-hosted `firnc1` (`lib/firnc1/*.fi`) is a *Firn program that emits
x86 assembler text*, and that is the whole problem in one sentence: its code
generator is not a module behind an interface, it is
`lib/firnc1/codegen.fi`, which writes Intel-syntax strings.

Concretely, for `firnc1` to target aarch64:

1. **A second code generator in Firn.** `lib/firnc1/codegen.fi` would need
   the A64 twin of everything in `codegen_a64.rs` — around a thousand lines
   of Firn, written against the same FIR its x86 generator already reads.
   That is a round of its own, and it is exactly the round that would prove
   the *self-hosted* compiler has the same seam this one now has.
2. **The target flag through `firnc1`'s command line** (`bin/firnc1.fi`),
   plus the two tool names, plus the per-target system call table — the Firn
   copy of `syscalls.rs`.
3. **`as`/`ld` are called over `fork`/`execve`** in `lib/std/rt.fi`; the
   names would have to come from the target rather than being literals.
   Note that `fork` itself is one of the four calls that needed a shape
   change here (§2), so a `firnc1` *running on* aarch64 depends on that
   translation being right — which the corpus now exercises 32 times over,
   since every program that imports `std.rt` compiles the call whether it
   reaches it or not.
4. **The fixpoint on aarch64** (`tools/fixpoint.sh`) needs stage 2 and
   stage 3 to be byte-identical *on that machine*. Nothing about that is
   architecture specific except that it has to be run there.

None of this is blocked by anything in round 80. The seam is drawn at the
right place now; what is missing is a second implementation of the same
seam, in Firn.

---

## 9. The x86-64 path

Unchanged, and checked rather than claimed:

* `tools/fixpoint.sh` — stage 2 == stage 3, **character-identical**
  (3,621,184 octets, 617,667 lines of assembly), and `.firnc2` behaves like
  `firnc0` over the whole corpus.
* `tools/self_compare.sh` — 315 same behaviour, **0 differing, 0 faulty**.
* `./test.sh` — **PASS 1171, FAIL 2 of 1173**, and both failures are red on
  `main` as well, measured there rather than assumed:
  * section 23 (layout): 1082 of 1087 boxes equal to Chromium, 0.46 % off.
    `bash tools/layout/run.sh` was run in the **`main` worktree** in this
    same session and gives the same number and the same four worst cases
    (`a4_abs_icb` 2 of 7, `a2_fixed_bottom_right` 1 of 5, `a3_fixed_percent`
    1 of 5, `a7_sticky_bottom` 1 of 7) — which is also exactly what round 76
    recorded (`docs/ROUND76.md`). Round 80 touches nothing in `lib/layout`.
  * section 24 (formatter): `lib/firnc1/parser.fi is not formatted`.
    `firnfmt -c lib/firnc1/parser.fi` says the same thing in the `main`
    worktree. Round 80 adds two `.fi` files of its own
    (`tools/aarch64/abi_probe.fi`, `tools/aarch64/table.fi`); both pass
    `firnfmt -c`.

  A note on running the suite on this machine: `earlyoom` is active here and
  sends SIGTERM to the biggest process when memory gets tight. Two runs died
  that way mid-suite (`EXIT=143`, and once a `Terminated` inside
  `tools/fixpoint.sh`'s corpus pass). Both parts pass when they are run
  again; that is a property of the machine, not of the round, and it is
  written down so the next reader does not chase it.

The only files on the x86 path that this round touched at all are
`gc.rs`, `iface.rs` and `fnval.rs`, and only to replace a literal
`".align 8"` with `target::align(8)` — which returns that same string on
x86. `codegen_x86.rs`, `regalloc.rs` and `codegen_switch.rs` were not
touched.

---

## 10. Running it

```bash
# a single program for the other machine
firnc --target=aarch64-linux -o hello tests/001_return_const.fi
qemu-aarch64 ./hello

# the cross check over the whole corpus
bash tools/aarch64/run.sh            # optimised
bash tools/aarch64/run.sh --no-opt   # and without the optimiser

# the machine, the relocations, and the ABI against gcc
bash tools/aarch64/machine.sh

# both of them, inside the suite
./test.sh          # section 41
```

Needs `qemu-user`, `binutils-aarch64-linux-gnu` and
`gcc-aarch64-linux-gnu`. Without them both scripts say `SKIP` and the suite
stays green — the checks are honest about their own absence too.
