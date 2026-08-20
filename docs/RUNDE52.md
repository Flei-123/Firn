# Round 52 — `profile kernel` becomes real: compiling freestanding

Branch `r52-freistehend`, base `cc1710f`.

Up to round 51, `SPEC.md` §14, item 6 contained this sentence:

> The **`profile` declaration** is parsed and checked but has no effect:
> what is produced is always a freestanding binary with `_start` without
> libc.

It was true. `sema.rs::check_profile` checked exactly one thing — whether
the name is `kernel` or `app` — and then did nothing. There was no
inline assembly, no MMIO accesses, no interrupt entry points, and
`main.rs` always linked with a fixed `as --64` + `ld -n` into an executable
file. This round makes the declaration true.

**Result up front, all measured by ourselves (19.08.2026):**

| | base `cc1710f` | round 52 |
|---|---|---|
| `bash ./test.sh` | 751/751 | **782/782** |
| `bash tools/self_compare.sh` | 213 / 0 differing / 0 failing | **218 / 0 / 0** |
| `bash tools/fixpoint.sh` | character-identical, 427 401 lines | **character-identical, 448 038 lines** |
| `bash tools/freestanding/run.sh` | — | **41 / 41** |
| kernel example booted in QEMU | — | **yes, with both compilers** |

---

## 1. What the kernel profile guarantees now

`--profile=kernel` (command line, wins) resp. `profile kernel` in the
first line of the root file. Without an entry, `app` still applies.

| SPEC §2 says | round 52 enforces | the message names |
|---|---|---|
| no global allocator, no runtime | `import std.*` rejected | the module and why |
| no `Gc[T]` (tracing collector) | `gc class` rejected | "'gc class' needs the tracing collector" |
| no unwinding / `throw` | `#[unwinds]` rejected | the function |
| no hidden allocation | follows from both | -- |
| floating point only with `#[allow_fp]` | `f64` **and** floating point literals | "floating point (the type f64) ... #[allow_fp]" |
| freestanding | `syscall` rejected, no `_start` | "under a freestanding kernel there is no operating system" |
| target binary format ELF object | `as --64 -o x.o`, **no `ld`** | — |

Every violation is a compiler error with line, column, marker and a
hint that **names** the forbidden thing. Proof: 15 negative tests
`tests/neg/free_*.fi`, each with an expected position and expected text.

**`syscall` does not appear in the table of SPEC §2** — it belongs there
anyway and is the sharpest of the six rules: below a freestanding kernel
there is no operating system that could accept a system call. Exactly
this one rule makes the entire standard library unusable in the kernel
profile, because there every allocation goes through `mmap` and every
output through `write`.

A kernel needs no entry point: `fn main` is no longer mandatory in the
kernel profile, and `_start`, setting up `rsp` and the `exit` system call
are dropped without replacement.

## 2. Freestanding output

```sh
firnc -c -o /tmp/x.o file.fi         # only `as --64 -o /tmp/x.o`, no `ld`
firnc --object -o /tmp/x.o file.fi   # the same, written out
firnc -o /tmp/x.o kernel.fi          # `profile kernel` switches -c on by itself
```

In the app profile everything stays as before (`as` + `ld` → executable
file). `firnc1` knows the same switches (`-c`, `--object`,
`--profile=kernel|app`).

## 3. Inline assembly

```firn
asm("cli")
asm("out dx, al", in("dx") port, in("al") wert)
let alt: u64 = asm("rdtsc", out("rax"), clobber("rdx"))
```

```ebnf
asm_ausdruck = "asm" "(" str_lit { "," asm_op } ")" ;
asm_op       = "in"      "(" str_lit ")" ausdruck
             | "out"     "(" str_lit ")"
             | "clobber" "(" str_lit ")" ;
```

Five decisions, each with a price:

1. **`asm` is not a keyword.** The parser recognizes the form only if the
   identifier `asm` is immediately followed by `(` and a string literal.
   That way the token stream does not change (`tools/lex_compare.sh` stays
   untouched) and `asm` remains usable as a name. *Price:* whoever writes a
   function `asm(s: [u8; N])` and calls it with a literal gets the
   assembler instead of their function.
2. **Register binding instead of placeholders.** There is no `{0}`.
   Operands name their register themselves; before the block the code
   generator places `mov <reg>, <wert>` and reads `out` afterwards.
   *Price:* the template is bound to concrete registers, and the allocator
   can optimize nothing.
3. **Caller-saved registers only.** Permitted are `rax rcx rdx rsi rdi
   r8..r11` including the narrow names (`eax`/`ax`/`al`, `r8d`/`r8w`/`r8b`,
   …), plus `memory` in the clobber list. `rbx`, `rbp`, `rsp` and
   `r12`–`r15` are rejected with a message of their own: they carry the
   frame resp. are callee-saved. *Gain:* exactly this set is destroyed by
   an ordinary `call` as well — the register allocation needs **no** special
   rule, and a special rule in the allocator is exactly the sort of code
   that produced the bug in round 40.
4. **At most one `out`.** Its value always has the type `u64`; whoever
   wants less writes `as u8`. Without `out` the block has the type `()`.
5. **`volatile` cannot be deselected.** There is no non-volatile variant.

**Intel syntax**, because the code generator emits `.intel_syntax
noprefix`. `\n` in the template separates assembly lines.

### 3.1 The R40 trap, and what stands against it

In round 40 the optimizer removed code that it was not allowed to remove.
The countermeasures are small and local, as demanded:

| Place | Change |
|---|---|
| `fir.rs::Op::is_pure` | `Asm`, `MmioLoad`, `MmioStore` are **never** pure → no DCE |
| `opt.rs::key_of` | no CSE key (falls into the existing `_ => None` branch) |
| `mem2reg.rs::is_untouchable` | like `select`/`barrier`/`secure_zero`: operands are never rewritten |
| `mem2reg.rs::clobbers_memory` | all three modify memory → no `load` forwarding across them |
| `licm.rs::hebbare_op` | not hoistable (via `is_untouchable` and the `_ => false` branch) |
| `regalloc.rs::unsupported_grund` | functions with inline assembly, MMIO or `#[interrupt]` go over the base path |

That is demonstrated at three places, and it is **measured**, not
claimed:

* `tests/850`–`854` really run, in all three build stages, in both
  compilers. `851` returns 0 instead of 7 if the block is dropped; `852`
  returns 2 instead of 3 if two literally identical blocks are merged;
  `853` returns 10 or 18 instead of 14 if MMIO accesses are merged.
* `tools/freestanding/volatile.fi` sits entirely in **one** function and
  calls nothing — so inlining cannot shift the numbers. The counting is
  done in the FIR **after** the optimizer (`--emit=fir`): `asm.void
  "pause"` = 3, `asm.u64 "rdtsc"` = 1, `mmio_load.u32` = 2,
  `mmio_store.u32` = 1, in every build stage.
* Six Rust module tests in `compiler/src/core.rs`.

### 3.2 What inlining is allowed to do

`--opt-level=release-fast` inlines `out8`/`in8` into their callers; after
that `out dx, al` stands ten times instead of once in the assembly. That is
**right**: inlining duplicates the block along with the call, it does not
remove or merge it. `tools/freestanding/run.sh` therefore checks `cli` and
`hlt` exactly (they stand in `kern_start`, which nobody calls) and
`out`/`in` only for „at least once"; the exact counting is done by
`volatile.fi`.

## 4. MMIO

```firn
__mmio_read8(p)      __mmio_write8(p, w)
__mmio_read16(p)     __mmio_write16(p, w)
__mmio_read32(p)     __mmio_write32(p, w)
__mmio_read64(p)     __mmio_write64(p, w)
```

Eight built-in names with the reserved `__` prefix (like
`__atomar_addieren`, round 47). Each becomes **one** machine instruction:

```
mmio_load.u32  %3      ->   mov rcx, qword ptr [rbp-8] ; mov eax, dword ptr [rcx]
mmio_store.u16 %4, %3  ->   mov rcx, … ; mov rax, … ; mov word ptr [rcx], ax
```

No pass may merge two accesses, remove one or move it across
another memory access. MMIO is available in **both**
profiles — an application, too, occasionally maps in a device.

## 5. Interrupt entry points

```firn
#[interrupt]
fn timer_ih() {
    …
}
```

A calling convention of its own: **14 general purpose registers** are saved
(`rax rcx rdx rbx rsi rdi r8`–`r15`; `rbp` is saved by the ordinary
prologue, `rsp` by the processor), and the conclusion is **`iretq`**
instead of `ret` — only this instruction restores the `rflags`, `cs` and
`rsp` of the interrupted thread.

Conditions, each with its own message: kernel profile only, no parameters,
no return type, **not callable** (a `call` would end in an `iretq`
and tear the stack apart; only the IDT may point at them).

## 6. The proof: a kernel that really boots

`demos/kernel/core.fi` — 130 lines of Firn: serial port COM1 via
`in`/`out`, VGA text buffer at `0xB8000` via MMIO, one interrupt entry.
`demos/kernel/start.s` (60 lines, the only non-Firn part) carries the
multiboot header and the way into long mode; `demos/kernel/linker.ld`
links at 1 MiB.

`bash tools/freestanding/run.sh` — **41 checks, 41 passed.** Excerpt:

```
== 2. it is an OBJECT file, and it is freestanding ==
  OK    firnc0: ELF type REL (relocatable object file)
  OK    firnc0: NO undefined symbol
  OK    firnc0: every defined symbol is its own
  OK    firnc0: no syscall in the machine code
  OK    firnc1: ELF type REL (relocatable object file)
  OK    firnc1: NO undefined symbol
  OK    firnc1: every defined symbol is its own
  OK    firnc1: no syscall in the machine code
== 3. link against the linker script (no libc, no crt files) ==
  OK    start.s assembles (multiboot header, long mode)
  OK    firnc0: linked, entry point 0x10000c
  OK    firnc0: the linked image has no open symbol
  OK    firnc1: linked, entry point 0x10000c
  OK    firnc1: the linked image has no open symbol
== 3b. boot in QEMU (the real proof) ==
  OK    firnc0: booted, serial output appeared
  OK    firnc1: booted, serial output appeared
```

Reproducible by hand:

```sh
$ compiler/target/release/firnc -o /tmp/kern0.o demos/kernel/core.fi
$ file /tmp/kern0.o
/tmp/kern0.o: ELF 64-bit LSB relocatable, x86-64, version 1 (SYSV), with debug_info, not stripped
$ nm /tmp/kern0.o
000000000000077d T _F0.core_start
000000000000002e T _F0.in8
0000000000000000 T _F0.out8
00000000000001ef T _F0.serial_char
000000000000005c T _F0.serial_init
000000000000018f T _F0.serial_ready
0000000000000296 T _F0.serial_text
0000000000000677 T _F0.timer_ih
0000000000000306 T _F0.vga_cell
00000000000003a0 T _F0.vga_clear
00000000000004b4 T _F0.vga_text
$ nm -u /tmp/kern0.o            # undefined symbols
$                               # -- none.
$ readelf -h /tmp/kern0.o | grep Type
  Type:                              REL (Relocatable file)
$ objdump -d /tmp/kern0.o | grep -c syscall
0
```

And the boot:

```sh
$ as --64 -o /tmp/start.o demos/kernel/start.s
$ ld -n -T demos/kernel/linker.ld --defsym=KERN_START=_F0.kern_start \
     -o /tmp/kern0.elf /tmp/start.o /tmp/kern0.o
$ objcopy -O elf32-i386 /tmp/kern0.elf /tmp/kern0.mb   # QEMU's multiboot takes only ELF32
$ qemu-system-x86_64 -kernel /tmp/kern0.mb -serial stdio -display none -no-reboot
FIRN: profile kernel ist
freistehend.
```

The same with `./.firnc1 demos/kernel/core.fi -o /tmp/kern1.o` and
`--defsym=KERN_START=_F1.kern_start`: the same output.

## 7. Both compilers

| | `firnc0` (Rust) | `firnc1` (Firn) |
|---|---|---|
| `asm(…)` with `in`/`out`/`clobber` | `compiler/src/core.rs` | `lib/firnc1/{kern,parser,sema,lower,codegen}.fi` |
| MMIO ×8 | ✓ | ✓ |
| `#[interrupt]` → `iretq` | ✓ | ✓ |
| `-c` / `--object` | ✓ | ✓ |
| `--profile=kernel|app` | ✓ | ✓ (output format) |
| profile prohibition `syscall` | ✓ with a message | ✓ as a rejection |
| profile prohibition `gc class` | ✓ with a message | ✓ as a rejection |
| profile prohibitions `f64`/`import std.*`/`#[unwinds]` | ✓ with a message | indirectly, see §9 |

**FIR opcodes 50–52** (round 52 reserved 50–59):

```
O_ASM     = 50   asm.<ty> "vorlage" [out=reg] [in=[reg %v, …]] [clobber=[a, b]]
O_MMIOLD  = 51   mmio_load.<ty> %adr
O_MMIOST  = 52   mmio_store.<ty> %wert, %adr
```

The textual form is a **contract**: `tools/fir_compare.sh` compares
`firnc0 --emit=fir-raw` octet by octet with `bin/firdump.fi`. For
`tests/850`–`854` it is identical.

In `firnc1` the `asm` registry lies **in the tree** (`ast.fi`, `asm_add`),
not in a side table -- the same construction as `__match#N` in the pattern
registry. What remains in the tree is a call `asm$<number>` with the input
expressions as arguments; because of that, monomorphization, the
`#[no_gc]` check and the canonical printing run over it unchanged, and
`--emit=ast-kanon` yields the same text on both sides.

## 8. Acceptance, measured

```
$ rm -f .firnc1 .firnc2 .firnc3
$ bash tools/fixpoint.sh
STAGE 2: 2760 ms   2581456 octets
STAGE 3: 8004 ms   2581456 octets
FIXPOINT:  stage 2 == stage 3, character-identical (448038 lines of assembly)
  SAME BEHAVIOUR:     218
  DIFFERING:          0
  FAULTY:             0
  NOT CORE:           0
  DEFER:              0
  COMPTIME:           1
  CODEGEN MISSING:    0
  SKIPPED:            18
CORPUS:    .firnc2 behaves like firnc0

$ bash tools/freestanding/run.sh
FREESTANDING: 41 passed, 0 failed
```

```
$ bash ./test.sh
...
== 16. the compiler in Firn compiles, the result runs ==
   SAME BEHAVIOUR:     218
   DIFFERING:          0
   FAULTY:             0
== 17. the fixpoint: Firn compiles itself ==
   FIXPOINT:  stage 2 == stage 3, character-identical (448038 lines of assembly)
   CORPUS:    .firnc2 behaves like firnc0
== 19. freestanding ==
   FREESTANDING: 41 passed, 0 failed

PASS 782/782
```

Base 751/751; on top of that 5 new programs × 3 build stages = 15, 15 new
negative tests, and section 19 `freistehend`.

The runtimes above are **not** a performance statement: the machine is not
quiet, several rounds are running at the same time.

## 9. What is open — what a real kernel still lacks

Honestly and completely:

1. **Global, mutable data.** SPEC §14, item 5: there is only `const`.
   Without it a kernel can hold neither an IDT nor a GDT nor a tick
   counter. `demos/kernel/core.fi` therefore evades the issue and counts in
   the frame buffer. **That is the biggest blocker**, bigger than
   everything else in this list, and it does not belong to this round
   (territory: profile, output format, inline asm, MMIO, interrupt ABI).
2. **The IDT does not enter itself.** The entry point stands ready
   and correctly ends with `iretq`; the table still has to come from
   outside — see (1). `lidt` could be written with `asm` today already, but
   there would be no place for the table.
3. **`#[interrupt]` knows no error code.** Vectors 8, 10–14, 17, 21, 29
   and 30 put an error code on the stack; `iretq` then no longer expects it
   there. For these vectors a variant is needed that clears eight bytes
   before the `iretq` (`add rsp, 8`).
4. **The interrupt frame cannot be read.** An `#[interrupt]` function
   has no parameters and therefore cannot get at `rip`/`cs`/`rflags`. For
   a page fault handler that is too little.
5. **No `#[naked]`.** Every function gets a prologue and an epilogue. For
   the very first entry after the boot loader that is not needed (the
   32-bit preamble in `start.s` takes care of it), but it is for a context
   switch.
6. **No callee-saved register in `asm`.** `rbx` and `r12`–`r15` are
   locked. That could be resolved with `push`/`pop` around the block; the
   round deliberately took the smaller, demonstrable variant.
7. **No `#[allow_fp]` effect across call boundaries.** The attribute works
   per function; whoever calls an `#[allow_fp]` function from one without
   it gets no error. For the FPU state that is not enough yet.
8. **Register allocation is off in kernel code.** Every function with
   inline assembly, MMIO or `#[interrupt]` goes over the base path
   (`codegen_x86.rs`) and keeps every value in the frame. That is correct
   and slow. The allocator would have to learn fixed register bindings and
   clobber sets for it — a round of its own, and one that can repeat the
   bug from round 40 if it is done sloppily.
9. **`firnc1` rejects, it does not explain.** The type checker in Firn
   counts errors, it does not phrase them — that has been so since round 30
   and applies to *all* checks, not only to these. It rejects `syscall` and
   `gc class` in the kernel profile (return value 1), and `import std.*`
   indirectly (every std module uses `syscall`). What is **not** checked
   there is floating point without `#[allow_fp]` — for that `#[allow_fp]`
   would have to be passed through into the tree — and `#[unwinds]`, which
   `firnc1` already rejects in the pre-scan as an unported extension. The
   fully phrased messages with line, column and hint exist only in
   `firnc0`; and that is exactly where the 15 negative tests check them.
10. **No `#[max_stack]`.** SPEC §2 lists it for both profiles; it still
    does not exist.
11. **A panic calls no `karst_panic`.** SPEC §2 names that for the
    kernel profile; stage 0 has no bounds checking at all (SPEC §14,
    item 3).

## 10. Changed files

```
compiler/src/core.rs            new   inline assembler, MMIO, #[interrupt]
compiler/src/prof.rs            new   profile resolution and enforcement
compiler/src/fir.rs                   Op::Asm/MmioLoad/MmioStore, Func.interrupt
compiler/src/{opt,mem2reg,licm,inline,regalloc}.rs   volatile protection
compiler/src/codegen_x86.rs           no _start in the kernel profile, iretq epilogue
compiler/src/main.rs                  -c/--object, --profile=
compiler/src/{sema,lower,parser,modules,attrs}.rs    hooks
lib/firnc1/core.fi              new   register table, MMIO names, asm numbers
lib/firnc1/{ast,parser,sema,lower,fir,codegen}.fi    the same language in Firn
bin/firnc1.fi                         -c/--object, --profile=
demos/kernel/{core.fi,start.s,linker.ld}   new   the proof
tools/freestanding/{run.sh,volatile.fi}         new   41 checks
tests/85{0,1,2,3,4}_*.fi                        new   running programs
tests/neg/free_*.fi (15)                        new   every prohibition message
test.sh                               Abschnitt 19
```
