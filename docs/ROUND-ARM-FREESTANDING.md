# Round ARM-FREESTANDING — a machine with nothing underneath it

Round 80 gave Firn a second instruction set and measured it honestly: 290 of
294 comparable programs behaved identically on x86-64 and on aarch64, and the
four that did not were all the same thing — inline assembler. Round 80's own
write-up put that in a table of "what aarch64 cannot do yet" together with
`#[interrupt]` and `profile kernel`, and those three lines are exactly the
three lines a kernel needs at every corner.

This round removes them, and adds the thing that was missing on both
machines at once: a **target with no operating system underneath**.

Three claims, and each of them is a measurement rather than a sentence:

1. `--target=aarch64-none` and `--target=x86_64-none` exist, and a program
   built with either of them **boots in QEMU and prints over the serial
   line**. Not "assembles", not "links" — prints.
2. Inline assembler works on A64. The four cases round 80 left at NOT
   SUPPORTED are **SAME**, and the corpus stands at **304 of 304 comparable
   cases identical on both machines, NOT SUPPORTED 0**.
3. The x86-64 path did not change, and that is checked the sharpest way
   available: `--target=x86_64-none` and the plain build of a `profile
   kernel` source produce **the same 24,138 octets** of assembler text.

---

## 1. The numbers

### 1.1 The two machines against each other (`tools/aarch64/run.sh`)

Every case of `tests/*.fi` compiled twice, the x86-64 build run natively and
the aarch64 build under `qemu-aarch64`, standard output compared character
for character and the exit code compared.

| | before (this branch's base) | after |
|---|---|---|
| **SAME** | 300 | **304** |
| DIFFERENT | 0 | **0** |
| NOT SUPPORTED | 4 | **0** |
| ENVIRONMENT (proven) | 1 | 1 |
| x86 already failing | 4 | 4 |
| comparable cases | 304 | 304 |
| **result** | 300 of 304 (98 %) | **304 of 304 (100 %)** |

Identical in both build stages (`dev-fast` and `--no-opt`). The four cases
that moved are the four round 80 named: `tests/850_asm_basic.fi`,
`tests/851_asm_volatile.fi`, `tests/852_asm_no_cse.fi`,
`tests/1242_asm_multi_out.fi`.

Two entries in that table are worth reading twice.

**"x86 already failing: 4"** is not new and is not this round's. Four cases
(`028_cast_narrow`, `030_wrap_u8`, `054_i16_ops`, `1334b_type_truncation`)
exit 101 — a checked-arithmetic panic — on the x86-64 side, on the base
commit as well. There is nothing to compare when one side does not meet its
own expectation, and the tool says so instead of counting it as a success.

**"ENVIRONMENT: 1"** is `tests/1284_std_io_text_owner.fi`, unchanged from
round 80: the case measures a leak by demanding that a freed mapping come
back at the same address, and `qemu-user` does not do that. The C probe that
has to back that claim ran in this measurement too and reported a drift of
32,751,616 octets over 2,000 rounds.

### 1.2 The object file, not the behaviour (`tools/aarch64/machine.sh`)

**16 of 16 pass**, unchanged. ELF header, the four relocation types, the
disassembly, AAPCS64 against `aarch64-linux-gnu-gcc` in both directions past
the end of the register file, `extern fn` both ways.

### 1.3 The freestanding targets (`tools/freestanding/none.sh`, new)

**27 of 27 pass.** The list, because the individual checks are the content:

```
== 1. the x86 path does not change ==
  OK    the assembler text is IDENTICAL (24138 octets)
  OK    counter-check: the app build has a _start, the freestanding one has none
== 2. what a target without an operating system refuses ==
  OK    x86_64-none:  'syscall' does not exist on the target 'x86_64-none'
  OK    aarch64-none: 'syscall' does not exist on the target 'aarch64-none'
  OK    aarch64-none: profile 'app' cannot be built for the target 'aarch64-none'
  OK    aarch64-none: import std.io is not available in profile 'kernel'
== 3. the aarch64 object file is freestanding ==
  OK    demos/freestanding/a64/core.fi -> ELF object (the source never says 'profile kernel')
  OK    ELF type REL, ELF machine AArch64
  OK    no undefined name except osum_panic and VECTORS
  OK    every defined symbol is its own (_F0.)
  OK    no _start (the entry point belongs to the kernel)
  OK    exactly one svc, and the source wrote it itself
== 4. what round 80 could not produce is really in there ==
  OK    'eret' / 'mrs' / 'msr' / 'wfe' in the machine code
  OK    sync_handler saves 10 register pairs + x29/x30
  OK    the checked arithmetic hands off to osum_panic
== 5. both machines BOOT and say something ==
  OK    x86_64-none: booted in qemu-system-x86_64, serial output appeared
  OK    aarch64-none in qemu-system-aarch64 -M virt: 'FIRN: freestanding aarch64 is'
  OK    ... 'alive.'  'EL=1'  'trap: !8'  'back from eret.'
```

The last four lines are the round. This is what really comes out of
`qemu-system-aarch64 -M virt -cpu cortex-a57 -nographic`:

```
FIRN: freestanding aarch64 is
alive.
EL=1
trap: !8
back from eret.
```

and every line of it is a different claim:

* **`FIRN: freestanding aarch64 is` / `alive.`** — MMIO, the frame, the
  string literals in `.rodata` and the `adrp`/`add` that reaches them all
  work in an object file that has no runtime under it.
* **`EL=1`** — `mrs x0, currentel` ran. A system register was read through
  the ordinary inline assembler (§4).
* **`trap: !8`** — `msr vbar_el1` installed a vector table, `svc #0` took a
  real synchronous exception, and the `#[interrupt]` function written **in
  Firn** ran at the other end. The `8` is the result of a CHECKED addition
  inside that handler, so `panic_rt_a64.rs`'s kernel branch is compiled into
  the image and linked against the `osum_panic` that `start.s` defines.
* **`back from eret.`** — the handler's `eret` returned to the instruction
  after the `svc`. That is the whole exception path, in both directions.

### 1.4 The two system call tables (`tools/aarch64/syscall_table.sh`, new)

**6 of 6 pass.** 60 entries, and `compiler/src/syscalls.rs` agrees with the
new `lib/firnc1/syscalls.fi` entry for entry — the Firn table read out of a
running program (`bin/sysdump.fi`) built by **both** compilers.

### 1.5 The x86-64 suite — and what could not be measured

This is the one place where the round has to be careful, because the machine
it ran on was not its own. Four to eight other rounds of this project were
running their own `test.sh` at the same time (load average 8-18 on twelve
cores), and long-running sections did not survive it: two attempts at the
full suite were killed during section 16 (`self_compare.sh`, which compiles
the whole corpus twice) and section 17 (`fixpoint.sh`), and a standalone run
of `self_compare.sh` was killed after ten minutes as well — with no output
written and no OOM entry in the kernel log. **The full 66-section suite could
not be run to completion, before or after.** Saying so is the honest thing;
quoting a number from a run that was killed would not be.

What WAS measured, and every one of these ran to completion:

| | |
|---|---|
| `test.sh` section 3 — the positive corpus, **329 programs × 4 build levels** (release-fast / no-opt / dev-fast / release-safe) | **0 FAIL** |
| `test.sh` sections 1-15 (module tests, negative tests, optimiser proof 45/45, result location, symbol scheme, atomics, bounds, html5lib tokenizer 6,810 cases, lexer / parser / layout / type checker / lowering of firnc1 against firnc0) | **0 FAIL** |
| compiler module tests (`cargo test`) | **262 passed, 0 failed** |
| the fixpoint, run on its own: firnc0 → stage 1 → stage 2 → stage 3 | **stage 2 == stage 3, character-identical** (23,278,384 octets, 786,249 lines of assembly) |
| `tools/freestanding/run.sh` (round 52, the x86 kernel object) | **41 passed, 0 failed** |
| `tools/aarch64/machine.sh` | **16 passed, 0 failed** |
| `tools/aarch64/run.sh`, both build stages | **304 of 304, 0 differing** |
| `tools/freestanding/none.sh` (new) | **27 passed, 0 failed** |
| `tools/aarch64/syscall_table.sh` (new) | **6 passed, 0 failed** |

Two things about the load are worth recording because they are exactly the
kind of noise that gets mistaken for a regression:

* In the runs that were killed, a handful of cases reported *"compilation
  failed"* with the compiler's error file MISSING — 11 in one run, 1 in
  another. Every one of them was recompiled on its own afterwards and every
  one of them built: `tests/1404_core_unbroken.fi`,
  `tests/140_symbolschema.fi`, `tests/1450_f32_basics.fi`,
  `tests/1130_js_generators.fi`. A missing error file is the signature of a
  compiler process that was killed, not of a program that does not compile.
* The four cases in the `x86 already failing` column of §1.1 are a different
  thing and are NOT load: they fail reproducibly, on the base commit as
  well, and they are counted rather than hidden.

For the claim this round actually has to make — *"x86-64 did not get worse"*
— the suite is in any case the weaker instrument. §1.6 is the stronger one.

### 1.6 The x86-64 assembler text, program by program

The octet comparison of §1.3 covers one program. It is worth doing over the
whole corpus, because it is the cheapest possible way to answer "did this
round change x86-64":

> **305 of 305** programs in `tests/` produce **character-identical**
> `--emit=asm` output from the compiler before this round and from the
> compiler after it. **0 differ.** The four inline-assembler cases are
> excluded because their SOURCE changed (§5); every other file in the
> directory is compared as it stands.

### 1.7 Compilation time

Measured as CPU time (`RUSAGE_CHILDREN`, user + system), because this
machine was running four other rounds' test suites at the same time and wall
clock says more about them than about the compiler.

| | before | after | |
|---|---|---|---|
| `bin/firnc1.fi` (55,000 octets, the biggest program in the repository), best of 11 | 2.578 s | 2.542 s | −1.4 % |
| `tests/1500_js_builtins2.fi`, compiler phases only (total minus `as`+`ld`), best of 7 | 1,667.8 ms | 1,644.4 ms | −1.4 % |

Both differences are negative and both are inside this machine's noise; the
honest summary is **no measurable change**. The one number that is not noise
is where the difference in the naive measurement came from: the same
`js_builtins2` run spent 1,062.9 ms in `as`+`ld` before and 1,232.7 ms after
— the external assembler and linker, on a machine at load 13. `--timings`
shows the optimiser at 643.5 ms on both sides, to the tenth of a
millisecond.

The new pass costs what it looks like it costs: `archsel::select` is three
walks over the function list and one `retain`, and it runs once per
compilation unit before monomorphization.

A freestanding kernel object, for scale: `demos/freestanding/a64/core.fi`
with `--target=aarch64-none` takes **12 ms** of CPU.

---

## 2. Two axes, not one

Round 80's `Target` was an enum with two members, and that was right for the
question round 80 asked ("which instruction set?"). A kernel asks a second
one that does not fold into the first: **is there an operating system
underneath?** x86-64 with Linux under it and x86-64 with nothing under it
share every instruction and share no system call at all.

So `compiler/src/target.rs` now carries a pair:

```
              Os::Linux         Os::None
Arch::X86_64  x86_64-linux      x86_64-none
Arch::Aarch64 aarch64-linux     aarch64-none
```

**Why the word `none`.** It is not invented here. In the triple convention
that GNU binutils and LLVM both use (`<arch>-<vendor>-<os>`), the literal
string `none` in the operating system field is what a bare-metal target is
called: `aarch64-none-elf` is the name of the cross toolchain that builds
kernels for this machine, and `x86_64-unknown-none` is the same idea on the
other one. Somebody who has ever built firmware knows what the target is
before reading a line of the file. `firn-bare` or `-kernel` would have meant
the opposite, so the long triple forms are accepted as aliases too
(`aarch64-none-elf`, `aarch64-unknown-none`, `arm64-none`).

Every function in `target.rs` answers for `Target::X86_64` exactly what stood
there before, character for character. The assembler, the linker and the
alignment directive now depend on the **arch** alone — which is the right
axis: the binutils that assemble A64 do not care whether Linux will run the
result. `main.rs` picks its code generator by `target::arch()` for the same
reason.

### 2.1 A freestanding target IS the kernel profile

This is the decision the round turns on, and the alternative was worse.

Since round 52, `profile kernel` in line 1 of the source has meant: no
`syscall`, no standard library, no `_start`, an ELF object file, and a panic
that hands off to `osum_panic`. That is, item for item, what "no operating
system underneath" means. Building a second, parallel mechanism next to it
would have produced two switches that have to be kept in agreement forever,
and the first round to forget one of them would ship a kernel with a `_start`
in it.

So: **a `-none` target turns the kernel profile on** (`prof::define`), and it
is the weakest of the three sources — the command line `--profile=` wins, and
a `profile` declaration in the source wins, so nothing is silently
reinterpreted. `profile app` together with a `-none` target is a
contradiction and is reported as one, at the declaration:

```
error: profile 'app' cannot be built for the target 'x86_64-none'
  --> app.fi:1:1
 1 | profile app
   | ^^^^^^^ here
   = note: the target name ends in '-none': there is no operating system under it,
     and the app profile presupposes one (write, mmap, exit_group, a _start that a
     loader jumps to) -- write 'profile kernel' or choose a target with an
     operating system
```

Two consequences follow, and both are pleasant:

* **The x86 path is checkable to the octet.** `--target=x86_64-none` and the
  plain build of `demos/freestanding/core.fi` must produce the same text, and
  `tools/freestanding/none.sh` compares them: 24,138 octets, identical. A
  round that had built a second path could not have made that statement.
* **`demos/freestanding/a64/core.fi` never says `profile kernel`.** It is
  freestanding because of the command line, and that is what makes the target
  worth having: the same source can be built for a machine with an operating
  system by changing the command line rather than editing line 1.

`syscall` under such a target reports the reason the reader actually has,
rather than a word he never typed:

```
error: 'syscall' does not exist on the target 'aarch64-none'
   = note: the target name ends in '-none': there is no operating system under it
     that could accept a system call -- a freestanding program reaches its machine
     through 'asm', MMIO and its own drivers
```

---

## 3. Inline assembler on A64

Round 80 refused `Op::Asm` with "inline assembler is x86 text and has no
meaning on aarch64". Half of that sentence was true and the other half was a
gap in the LANGUAGE, not in the code generator — §5.

### 3.1 The register table belongs to the instruction set

The first thing that had to move was not in `codegen_a64.rs` at all. Register
names are validated in `core.rs::check_reg`, which runs in the **type
checker**, and its table was the System V caller-saved set. A compiler that
kept it while generating A64 would have accepted `out("rax")` and emitted
`mov rax, ...` into an A64 object file.

`core.rs::stem` therefore asks the target. For `Arch::X86_64` it is the same
table lookup that stood there before, character for character. For
`Arch::Aarch64` it is x0-x17 and their `w` names — the whole corruptible set
of AAPCS64 (x0-x7 arguments, x8 the indirect result register, x9-x15 scratch,
x16/x17 the linker's veneer registers).

**x18 is refused, and it is the one that would have been easy to get wrong.**
It is neither caller-saved nor callee-saved: it belongs to the *platform*.
Allowing it would have broken nothing under `qemu-aarch64` today and
something else on a real machine tomorrow, which is the worst kind of error
to allow. x19-x28 (callee-saved), x29/x30 (frame and link) and `sp` are
refused with their own reason, so the message says WHY and not merely
"unknown":

```
error: register 'x19' is not allowed in the asm block (out)
   = note: allowed are only the corruptible registers x0-x17 (and their w names);
     x18 belongs to the platform, x19-x28 are callee-saved, x29/x30 carry the
     frame and the return address, sp is the stack pointer
```

### 3.2 Where the operands wait — and why not on the stack

`codegen_x86.rs` handles round 68's memory outputs by pushing every result
register and popping it back after the addresses have been built. **That is
not available here**, and the reason is structural: this backend sets `sp`
once in the prologue and never moves it, because every frame slot is
addressed relative to `sp` (round 80, §3). A `push` would move all of them.

So the operands are parked in the **outgoing argument area** at the bottom of
the frame — the region `layout` reserves for the arguments of the widest
call. It is free at an `asm` block by construction, because an `asm` block is
not a call, and `layout` now widens it to the wider of the two needs.

That parking is not a detail; it is what makes the whole thing safe. x12
(address building) and x13 (helper) are ALLOWED operand names, so a template
may perfectly well say `out("x12") &a`. The sequence is therefore:

1. every input value into the parking area (one at a time, through x13),
2. then, in a second pass, out of the parking area into the registers the
   template names — no address is computed in between, so nothing can be
   clobbered;
3. the template;
4. every output register **into the parking area first**, before a single
   address is built;
5. the value form into its slot, and the memory outputs through their
   addresses.

The test that proves it is the one that names the backend's own scratch
registers as operands on both sides:

```firn
asm("add x12, x12, x13\nmov x13, #100",
    in("x12") a, in("x13") a, out("x12") & out1, out("x13") & out2)
```

### 3.3 The clobber list, and the register allocator

On this path the clobber list costs nothing and is written into the assembly
as a comment, exactly as on x86. That is not laziness, it is the frame model:
no FIR value survives in a register across an instruction, so there is
nothing to rescue.

The one pass for which the list would matter is `regalloc.rs` — and
`regalloc.rs` **refuses any function containing an `Op::Asm` outright**
(`regalloc.rs`, "Inline-Assembler"), on x86 as well. It is an x86 pass
besides; A64 has the base path only. So the honest answer to "how does the
inline assembler cooperate with the register allocator" is: on A64 there is
no register allocator yet, and on x86 the two never meet. When A64 gets a
linear scan, the clobber list is where it will have to start.

---

## 4. System registers: no form of their own

The round description asked whether `MRS`/`MSR` need a construct of their own.
They do not, and the answer was checked before it was written down:

```firn
fn current_el() -> u64 {
    return asm("mrs x0, currentel", out("x0")) >> 2
}
fn set_vbar(p: u64) {
    asm("msr vbar_el1, x0\nisb", in("x0") p)
}
```

Both run. `EL=1` in the QEMU output above is the first one's result.

The reasoning, since it is a decision and not a discovery: the system
register NAME is part of the assembler TEXT, and GNU as owns the table of
which names exist, at which exception level, and read-only or not. It refuses
`mrs x0, not_a_register` by itself, with a line number:

```
core.s:24: Error: unknown or missing system register name at operand 2
```

What the compiler has to supply is only the general purpose register the
value travels in — and `in("x0")` / `out("x0")` are exactly that. A second
form would have meant copying ARM's system register table into the compiler
and keeping it in step with every architecture revision, in exchange for
nothing. It is refused for the same reason a `printf` builtin would be.

The one place this reasoning would break is a system register that has to be
named by a value computed at run time. A64 has no such addressing — `mrs`
takes an encoded constant — so the case does not exist.

---

## 5. `#[arch(...)]`: which machine a definition belongs to

The four NOT SUPPORTED cases of round 80 were stuck for a reason that lay in
the language:

```firn
let s: u64 = asm("add rax, rcx", out("rax"), in("rax") a, in("rcx") b)
```

`add rax, rcx` is not a Firn expression that has not been ported. It is a
line for a particular assembler, sitting inside a Firn program. A language
that offers an inline assembler owes its user a way to say **which machine a
piece of source belongs to**, and Firn had none.

It has one now, and it is one attribute and one `retain`:

```firn
#[arch(x86_64)]
fn add2(a: u64, b: u64) -> u64 {
    return asm("add rax, rcx", out("rax"), in("rax") a, in("rcx") b)
}

#[arch(aarch64)]
fn add2(a: u64, b: u64) -> u64 {
    return asm("add x9, x9, x10", out("x9"), in("x9") a, in("x10") b)
}
```

Both are written; exactly one is compiled. `compiler/src/archsel.rs` throws
the other away **before the type checker runs** — which is the whole point,
because the type checker is where register names are validated against the
machine (§3.1). A pass that ran later would have had to make the checker
tolerate `rax` while generating A64.

**Why on the function and not on the statement.** A statement-level
`arch { ... }` was the other candidate and was not taken:

1. *Overload by name falls out for free.* Two functions of the same name are
   a duplicate-definition error in `sema.rs` — and by the time `sema.rs`
   looks, one of them is gone. Nothing has to be taught to name resolution,
   and a call site reads `add2(a, b)` on both machines with no marker at all.
2. *A block has no value.* Three of the four cases are
   `let s: u64 = asm(...)`. A statement-level selector would have forced every
   one of them through a mutable variable assigned in two branches — a
   different shape from the one being ported.
3. *It is the smaller change.* A new `Stmt` variant has to be handled in every
   pass that walks statements (nine files), and every one of those is a place
   a later round can forget it. An attribute is handled in one file.

The price, named rather than hidden: what varies must be a whole function.
Two machines that share ten lines and differ in one write the ten lines twice
or factor the one line out. For inline assembler — the only thing that has
ever needed this — factoring it out is what one wants anyway.

It is **not** conditional compilation in general: no `#[arch]` on a struct or
a constant, no `not(...)`, no `any(...)`, no nesting. A typo is an error and
not a silent removal:

```
error: unknown machine 'arm64' in #[arch(...)]
   = note: known are x86_64, aarch64 -- the same words the --target names are built from
```

and a name whose every definition belongs to another machine says so at the
definition, not at the call site a hundred lines away.

`lib/firnc1/parser.fi` learned the same attribute, so the self-hosted
compiler compiles the rewritten tests too — measured, not assumed: all four
of them build with `firnc1` and yield 42, 7, 3 and 0.

**One cost, and it is a real one.** `tools/parser_compare.sh` compares the
canonical AST of both compilers over the whole corpus, and it puts a file in
the NOT CORE bucket when it uses something the comparison harness does not
carry — attributes are on that list and always have been. The four rewritten
cases now carry an attribute, so they moved out of the compared set and into
NOT CORE: **SAME 445 → 443, NOT CORE 182 → 186**. Nothing broke; four files'
worth of parser comparison was traded for four files' worth of two-machine
comparison. It is written down here because a number that moves for a reason
nobody recorded is how a suite starts lying.

There is also a genuine asymmetry underneath it, and it is worth knowing
before somebody trips on it: **firnc0 drops the other machine's definition
in a PASS after parsing, firnc1 drops it IN the parser.** So the two
compilers' `--emit=ast-canon` for a file with `#[arch]` would not agree —
firnc0's tree still contains both definitions at that point. That is not
laziness on either side: firnc0 keeps the tree complete because `firnfmt`
round-trips through it and a formatter that deleted the other machine's code
would be a catastrophe, while firnc1 has no formatter and no separate pass
list to hang one on.

---

## 6. `#[interrupt]` on A64

Round 80 refused it because "its calling convention *is* x86". Half true: the
convention is different, not absent.

```
compiler/src/codegen_a64.rs   INT_SAVE_A64
```

**A64 saves nothing by itself.** Where the x86 processor has already pushed
`ss:rsp`, `rflags` and `cs:rip` before the handler's first instruction, an
A64 exception writes the return address into `ELR_EL1` and the flags into
`SPSR_EL1` — two SYSTEM registers, not the stack — and jumps into the vector
table. Not one general purpose register is touched, which means not one of
them may be touched in the handler until it is safe.

So the prologue saves x0-x18 and x30 (ten `stp` pairs, 160 octets, already a
multiple of sixteen so `sp` keeps its alignment), and the epilogue restores
them and ends with **`eret`** instead of `ret`. `eret` restores the program
counter out of `ELR_EL1` and the flags out of `SPSR_EL1` in one instruction;
a `ret` would jump to whatever x30 happened to hold and leave the exception
level where it was, which is not a return but a second fault waiting.

x19-x28 are not saved, and that is correct rather than an omission: this
backend never hands them out, so the interrupted thread finds them as it left
them. The floating point and vector registers are not saved either — the same
decision the x86 side made (SPEC §2: in the kernel the FPU state belongs to
the interrupted thread, and `#[allow_fp]` is what says somebody thought about
it).

The vector table itself is **not** the compiler's business, on either
machine. x86's IDT comes from the kernel; A64's `VBAR_EL1` table comes from
`demos/freestanding/a64/start.s`, and `core.fi` installs it with two
instructions of inline assembler. That is the point of §4 in one sentence:
the language only has to be able to reach the system register.

---

## 7. Traps

Round 80 documented two. Both got new instances this round, and three more
turned up.

### 7.1 `.align 8` — and its silent form

Round 80's trap: `.align N` counts **bytes** in the x86 port of GNU as and
**powers of two** in the AArch64 port. Measured again, with the same three
lines assembled by both ports:

```
x86:      marker at offset 8
aarch64:  marker at offset 0x100 = 256
```

What round 80 did not know is that the same trap has a **silent** form, and
this round walked straight into where it lives. A `VBAR_EL1` vector table
needs 2048-octet alignment. Written the obvious way:

```
    .align 2048
```

the AArch64 assembler does not fail. It says

```
Warning: alignment too large: 63 assumed
```

— a *warning* — and produces a table aligned to 2^63, which is to say
aligned to whatever the section happens to give it. `msr vbar_el1` would then
silently write a value the processor ignores the low bits of, and the first
exception would go somewhere else entirely. `demos/freestanding/a64/start.s`
uses `.balign 2048`, which counts octets in both ports.

### 7.2 System call 1 — and the `svc` that is not one

Round 80's trap: system call 1 is `write` on x86-64 and `io_destroy` on
AArch64.

Its freestanding sibling is a check rather than a number.
`tools/freestanding/run.sh` proves an x86 kernel object is freestanding by
asserting it contains **no `syscall` instruction at all**. The literal
translation to A64 would be "no `svc`" — and that is wrong, because `svc` is
also how a kernel's *own* system call interface is entered from user space,
so a real kernel will contain them on purpose. `tools/freestanding/none.sh`
therefore asserts **exactly one**, and names why: the one the source wrote in
an `asm` template. A compiler-generated system call would be a second one.

### 7.3 `sp` may not move, so `push`/`pop` is not available

Covered in §3.2. It is listed here because it is the shape of trap that only
appears when a second machine exists: the x86 code is correct, the A64 code
would be correct in isolation, and the combination is wrong because the two
backends made different (both defensible) decisions about the frame.

### 7.4 A64 has no move-64-bit-immediate, and no store-immediate

Both bite inside `asm` templates, where the compiler cannot help.

```
mov x0, #0x89ABCDEF
```
```
Error: immediate cannot be moved by a single instruction
```

`movz x0, #0xCDEF` + `movk x0, #0x89AB, lsl #16` is the answer, and
`tests/1242_asm_multi_out.fi`'s point 5 is written that way.

The second one is quieter: A64 cannot store an immediate to memory. A
template that does what `mov qword ptr [rcx], 7` does on x86 needs a scratch
register of its own — and a scratch register a template uses **must stand in
the clobber list**, which is exactly what that list is for.
`tests/851_asm_volatile.fi` and `tests/852_asm_no_cse.fi` are written that
way.

### 7.5 The register table is checked in the type checker, not the backend

§3.1. Worth repeating as a trap because the failure mode is silence: a
compiler whose `stem()` had stayed x86-only would have accepted `out("rax")`
under `--target=aarch64-none` and emitted x86 register names into an A64
object file. The assembler would then have refused it — but only because
`rax` happens not to be an A64 register name. `x0`, which IS one on both
sides of nothing, would have gone through.

---

## 8. `firnc1` — what was done and what was not

The round asked for the self-hosted compiler to be brought along, and said
plainly: if it does not fit, write down exactly where it stands and do not
fake progress. It did not fit. Here is the exact state.

**What is done, and measured:**

| | |
|---|---|
| `#[arch(...)]` in `lib/firnc1/parser.fi` | the prescan accepts the six-token form, `attr()` reads the machine name, `fn_decl` parses the other machine's definition and **throws it away**. Measured: all four rewritten asm tests build with `firnc1` and yield 42, 7, 3, 0. |
| `lib/firnc1/syscalls.fi` (new, 100 lines) | the Firn twin of `compiler/src/syscalls.rs`: 60 entries, the six shapes (`K_DIRECT`, `K_ATFDCWD`, `K_FORKCLONE`, `K_DUP3`, `K_SETTP`, `K_MISSING`), `AT_FDCWD`, `SIGCHLD`, `ARCH_SET_FS`. |
| `bin/sysdump.fi` + `tools/aarch64/syscall_table.sh` (new) | the two tables are compared **entry for entry, every run**, and the Firn one is read out of a RUNNING program built by both compilers. 60 = 60, and `firnc0` and `firnc1` read the same table. |
| `--target=` in `bin/firnc1.fi` | `x86_64-linux` and `x86_64` are accepted; `x86_64-none` turns on the kernel profile, so **the same flag builds a freestanding object with both compilers** (measured: ELF `REL`); `aarch64-linux` and `aarch64-none` are **refused**, with a message that says what is missing. |

That last row is the part that matters most for the round's honesty. A
compiler that ignores an unknown flag would have answered
`--target=aarch64-linux` with an x86 binary. This one answers:

```
error: firnc1 cannot generate aarch64 yet
  note: the Rust bootstrap can (firnc --target=aarch64-linux, docs/ROUND80.md);
  what is missing here is the A64 code generator in Firn -- see
  docs/ROUND-ARM-FREESTANDING.md
```

**What is NOT done:** the A64 code generator in Firn. `lib/firnc1/codegen.fi`
is 2,997 lines that write Intel-syntax strings; the Rust twin of what it
would need is `codegen_a64.rs` (1,700 lines) plus `simd_a64.rs` (758) plus
`panic_rt_a64.rs` (690). That is a round of its own, and it is exactly the
round round 80 §8 described. Nothing in this round blocks it — the seam is
drawn, the target flag reaches the right place, the system call table exists
and is checked. What is missing is the second implementation.

**What is therefore still true:** there is no self-hosting on ARM. A Firn
compiler running on an ARM machine has to be `firnc0`. `tools/fixpoint.sh`
remains an x86-64 measurement.

---

## 9. What still does not work

Named rather than discovered later. Each of these reports itself with a
message and stops; none is silently compiled into something else.

| | why |
|---|---|
| `firnc1` for aarch64 | §8 — the code generator in Firn does not exist |
| debug information on aarch64 | no `.loc`, no `.debug_info`; `tools/dwarf/run.sh` stays x86 |
| register allocation on aarch64 | `regalloc.rs` is x86; A64 has the base path only, so the code is correct and slow |
| aggregates across `extern fn` on aarch64 | round 80's finding, unchanged: `abi.rs` classifies by System V, AAPCS64 classifies composites differently. Scalars and up to ten/nine words are proven; aggregates should be assumed wrong |
| `#[arch]` on anything but a function | §5 — deliberately not built |
| threads on a freestanding target | there is no `clone(2)` without an operating system; `Op::ThreadSpawn` under `-none` is a `syscall` and is refused with it |

Two **semantic differences** between the machines, unchanged from round 80
and repeated because they still hold: `7 / 0` raises SIGFPE on x86-64 (exit
136) and yields 0 on aarch64 (`sdiv` does not trap), and `1e400 as i64` gives
`0x8000000000000000` on x86 and `0x7fffffffffffffff` on aarch64 (`fcvtzs`
saturates). The right answer to the first is the checked arithmetic of round
72, which is on by default.

One more that is new and belongs to the freestanding side:
**`qemu-system-aarch64 -M virt` starts the image at EL1.** That decides which
system registers exist — `vbar_el1` does, `vbar_el3` does not — and a kernel
that expects to be entered at EL2 or EL3 (as a real board's firmware may do)
has to bring the drop-down code itself. `demos/freestanding/a64/start.s`
does not; it parks the secondary cores, sets a stack, zeroes `.bss` and
jumps, which is seventy instructions less than the x86 twin needs to reach
long mode.

---

## 10. What OSUM-ARM can use, and how

The parallel round makes the kernel architecture independent. It can start.

**The target name is `aarch64-none`.** Called like this:

```
firnc --target=aarch64-none -o kernel.o kernel/kmain.fi
aarch64-linux-gnu-as -o boot.o kernel/boot_a64.s
aarch64-linux-gnu-ld -T kernel/kernel_a64.ld \
    --defsym=KERNEL_MAIN=_F0.kernel_main ... \
    -o osum.elf boot.o kernel.o
qemu-system-aarch64 -M virt -cpu cortex-a57 -nographic -kernel osum.elf
```

Aliases `aarch64-none-elf`, `aarch64-unknown-none` and `arm64-none` mean the
same thing.

What is guaranteed to work, because it is measured in
`tools/freestanding/none.sh` on every run:

* an ELF `ET_REL` object with **no** `_start`, no libc, no dynamic loader and
  no undefined name the kernel author did not ask for;
* `syscall` refused with a clear error, so nothing reaches Linux by accident;
* inline assembler with `in`/`out`/`inout` bindings, memory operands and
  clobber lists — including `mrs`/`msr` on system registers, which is how
  `VBAR_EL1`, `TTBR0_EL1`, `SCTLR_EL1`, `MAIR_EL1`, `CNTP_*` and the GIC
  system registers are reached;
* `#[interrupt]` functions written in Firn, entered from the kernel's own
  vector table and leaving with `eret`;
* MMIO (`__mmio_read8/16/32/64`, `__mmio_write*`) with the volatile
  guarantee, which is what replaces x86's `in`/`out` port instructions;
* checked arithmetic with the `osum_panic(msg, len, a, b, code, unsigned)`
  hand-off in plain AAPCS64 order (x0..x5) — one place fewer to shuffle than
  on x86, where the trampoline has to move r9 into r8;
* `#[arch(x86_64)]` / `#[arch(aarch64)]` to keep one source tree with two
  machine-specific bodies per function.

What OSUM-ARM has to bring itself, because it is a kernel's business and not
a compiler's: the vector table, the drop from a higher exception level if its
board hands over at one, page tables, the GIC, the timer, and the `osum_panic`
definition. `demos/freestanding/a64/start.s` is a working 160-line example of
the first and the last of those.

One warning worth passing on: **build the kernel with `firnc0`.** `firnc1`
refuses `--target=aarch64-*` and says so (§8), so `tools/build-kernel.sh
--stufe 1` will not have an ARM equivalent until the A64 code generator
exists in Firn.
