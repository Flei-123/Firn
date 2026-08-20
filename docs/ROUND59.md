# Round 59 — the kernel: from a booting prologue to a small operating system core

Branch `r59-kernel`, base `04bb780`.

Round 52 made `profile kernel` real: no libc, no runtime, no `_start`,
inline assembly, MMIO, `#[interrupt]`, an ELF object file. The example
`demos/kernel/core.fi` booted in QEMU, said its name over the serial port
and stopped. Its own header named the limit openly:

> What is NOT in it, and why: an IDT needs global, mutable data. Stage 0
> does not have that (SPEC 14, item 5: `const` only). As long as that is
> missing, this kernel cannot install its own interrupt vector.

This round removes that limit and builds the four things a core needs:
interrupts, memory, a device, and the boundary between kernel and user.

**Result up front, measured (20.08.2026):**

| | base `04bb780` (round 57) | round 59 |
|---|---|---|
| `bash test.sh` | 847/847 | **854/854** |
| `bash tools/freestanding/run.sh` | 41 passed, 0 failed | **41 passed, 0 failed** |
| `bash tools/kernel/run.sh` | — | **46 passed, 0 failed** |
| `bash tools/self_compare.sh` | 232 same / 0 differing / 0 faulty | **234 / 0 / 0** |
| `bash tools/fixpoint.sh` | character-identical, 554 923 lines | **character-identical, 554 923 lines** |
| `bash tools/english/check.sh` | 0 0 0 0 0 | **0 0 0 0 0** |

The seven test cases more are the two new tests (890, 891, each in three
build stages) plus the new section 22. That the fixpoint has exactly the
same number of lines as before is not a coincidence and is the strongest
single number of this round: **not one line of the compiler was
changed.**

No line of the compiler was changed. What the kernel needed, the language
already had — that is the actual finding of this round, and it is
reported below with the two places where it was close.

---

## 1. What is there now

`demos/kernel/kmain.fi` plus six modules, all in Firn:

| file | lines | what is in it |
|---|---|---|
| `kmain.fi` | 403 | the boot sequence, the self tests, the command line |
| `mem.fi` | 470 | memory map, frame allocator, heap, both proofs |
| `user.fi` | 201 | `syscall`/`sysret`, page splitting, ring 3 |
| `idt.fi` | 175 | IDT, TSS, the two PICs, the PIT |
| `serial.fi` | 151 | COM1, hexadecimal and decimal output |
| `trap.fi` | 141 | the exception report, the interrupt dispatch |
| `kstate.fi` | 100 | the mutable state, see 2 |
| `kbd.fi` | 62 | scan code set 1 over IRQ1 |
| `power.fi` | 33 | halting, and the shutdown for the test harness |

1736 lines of Firn, comments included. Beside it the test harness
`tools/kernel/run.sh` with 321 lines and the two tests 890/891.

Assembly is in exactly two files, and each line in them is there because
a compiled function could not be there:

* `boot.s` (223 lines) — the multiboot header (a data structure), the way
  from 32-bit protected mode into long mode (page tables, CR0/CR3/CR4,
  EFER, the far jump that reloads CS), the GDT, and the hand-over of the
  addresses. It also zeroes the data area, because a multiboot loader is
  not obliged to clear `.bss`.
* `isr.s` (277 lines) — 48 interrupt entry points, the common stub, the
  entry point of the system call, the way into ring 3 and back, and the
  four-instruction user program.

Both files are **tracked with `git add -f`**: `.gitignore` contains `*.s`,
and the trap of round 52 (hand-written assembly that works locally and is
missing in the repository) was avoided with two exception lines
(`!demos/kernel/boot.s`, `!demos/kernel/isr.s`). Checked with
`git status --porcelain --ignored | grep '\.s$'` — nothing.

## 2. The state problem, and how it is solved honestly

Stage 0 of Firn has no global mutable variables. A kernel needs some: an
IDT, a frame bitmap, a tick counter that an interrupt increments while
the main loop reads it.

The way out is the one an operating system takes anyway: **the state does
not lie in the language, it lies in a memory region whose address is
handed over.** `boot.s` reserves 128 KiB, zeroes them and puts the
address into a register; `kernel_main` and `trap_entry` both get it. What
is a global variable elsewhere is here an offset into that region
(`kstate.fi`).

That is not a workaround for a missing feature — it is what a boot loader
does with a kernel, only one level further down. What it costs honestly:
every access is a hand-written offset, and the type checker does not
check what lies at that offset. A `static` in the language would be
better. It is missing, it is named here, and it is not disguised.

Every access to that region goes through `__mmio_read64` /
`__mmio_write64`, not through an ordinary pointer. That is not decoration:

```firn
while kstate.get(state, kstate.TICKS) - start < TICKS_WANTED {
    idt.halt()
}
```

The tick counter is changed by the interrupt handler, in the middle of
the loop that reads it. An ordinary load may be hoisted out of the loop
by `licm.rs` — the kernel would then wait forever. The volatile promise
of round 52 is what makes this loop terminate, and `tools/kernel/run.sh`
measures that it does.

## 3. Interrupts

**48 stubs instead of `#[interrupt]`.** Round 52 produces with
`#[interrupt]` a handler that saves all registers and ends with `iretq`.
That is right and it is not enough: such a handler does not know which
vector it was entered through, cannot read the error code that the
processor pushes for ten of the exceptions (and only for those), and
cannot pass a pointer to the saved registers to a function in Firn.

Hence 48 stubs of three instructions in `isr.s`. They equalise the two
shapes of the exception frame, push the vector number and jump into one
common stub; that one saves all 15 general purpose registers and hands
`rsp` — the address of the frame — to `trap.entry`. Everything after
that is Firn.

**The IDT is built in Firn**, gate by gate (`idt.build`): 48 present
gates, 208 not present, and vector 8 (`#DF`) with IST index 1. Then
`lidt` over a descriptor that the kernel writes itself.

**The TSS as well.** `boot.s` leaves two GDT slots empty; `idt.tss_install`
writes the 16-octet system descriptor into them, fills RSP0 (the stack an
interrupt out of ring 3 lands on) and IST1 (the stack of the double
fault), and executes `ltr`.

**A report that is worth something.** An exception prints vector, name,
error code, for `#PF` also CR2, then the frame of the processor and all
sixteen general purpose registers:

```
*** EXCEPTION 14 #PF  err=0x0  cr2=0x40000000
  rip=0x10b471  cs=0x8  rflags=0x2  rsp=0x135360  ss=0x0
  rax=0x0000000040000000  rbx=0x0000000000009500  rcx=0x0000000040000000  rdx=0x00000000000003f8
  rsi=0x0000000000000000  rdi=0x0000000000000032  rbp=0x00000000001358d0  rsp=0x0000000000135360
  r8 =0x0000000000000031  r9 =0x00000000001350f0  r10=0x0000000000000000  r11=0x0000000000000000
  r12=0x0000000000000000  r13=0x0000000000000000  r14=0x0000000000000000  r15=0x0000000000000000
*** kernel halted
```

And then the machine stops. A kernel that keeps going after a `#GP` is
lying; `tools/kernel/run.sh` checks in every one of the four trap runs
that `kernel: done` does **not** appear.

**The four faults are provoked by the kernel itself**, controlled by the
command line of the boot loader (`-append "trap=de|pf|gp|df"`):

| kind | how it is provoked | what arrives |
|---|---|---|
| `#DE` | division by a divisor that comes out of the data area and is zero | vector 0, err=0x0 |
| `#PF` | a read at `0x40000000` — the first octet that `boot.s` did not map | vector 14, err=0x0, cr2=0x40000000 |
| `#GP` | a write to `0x0000800000000000`, an address that is not canonical | vector 13 |
| `#DF` | `mov rsp, 0` and then a push: the fault report itself faults | vector 8, over IST1 |

The last one is the interesting one: without IST1 the machine would
triple fault and reset. That it reports instead is the proof that the TSS
is right — and it works with both compilers (section 6 of the test).

**The timer.** The two PICs are remapped to 0x20/0x28 (without that the
timer arrives as vector 8, as a double fault), IRQ0 and IRQ1 are opened,
the PIT runs at 100 Hz (divisor 11931). The counter runs up measurably:
21 ticks in the wait loop, and 121 more ticks in a spin loop of four
million `pause` instructions.

## 4. Memory

Three layers:

1. **The memory map.** Multiboot puts a list of regions somewhere in
   memory; `mem.scan` reads it (7 entries with `-m 128`, 130 559 KiB
   usable, top `0x7fe0000`).
2. **The frame allocator.** One bit per 4 KiB page, 16 KiB of bitmap for
   512 MiB of RAM. Everything the boot loader did not report as usable
   stays taken, as does everything below `kernel_end` — the kernel is
   standing in it. A full octet is skipped in one go in the search; that
   is the reason for a bitmap in the first place.
3. **The heap.** 64 frames in a row, and on them an implicit free list:
   every block carries size and flag, first fit, `kfree` merges free
   neighbours again.

The proofs run in the kernel and print what they measured:

```
frames: 131072 covered, 32376 free
frame test: a=0x1a8000  b=0x1a9000  c=0x1aa000  again b=0x1a9000  free=32309
frame test: ok
heap test: p1=0x168010  p2=0x168040  p3=0x1680c0  reuse=0x168040  blocks=4  1M refused=1
heap test: ok
```

Allocate, free, allocate again — and the freed one comes back, at the
same address. No two blocks overlap (checked with the requested lengths),
each block carries a pattern that the neighbours must not touch, after
everything has been given back exactly ONE block is left, and a request
of 1 MiB against a 256 KiB heap is **refused** — an allocator that hands
out something there would hand out memory it does not own.

The same two algorithms run outside QEMU as well: `tests/890_frame_bitmap.fi`
and `tests/891_kernel_heap.fi` go through the normal test suite, in three
build stages and with both compilers.

## 5. The keyboard

The first real device: IRQ1, port 0x60, scan code set 1. Below 0x80 a key
was pressed, from 0x80 on it was released; the kernel is interested in the
press. No shift, no modifiers, no repeat — a keyboard driver is a round
of its own.

Tested without a hand on a key: QEMU gets a monitor on a unix socket, the
test script waits until the kernel says `kbd: ready`, then sends four
`sendkey` commands and reads back what came out of the serial port:

```
key: f
key: i
key: r
key: n
kbd: firn
```

With the counter-check that matters: a run **without** keys has to print
`kbd: (none)`. Without that probe the four letters above would prove
nothing — they could be a string in the kernel.

## 6. Ring 3

The last point of the round, and the one with the most detail:

* **The MSRs.** `STAR` says which segments the processor loads, and its
  layout is prescribed: `sysret` takes SS from `STAR[63:48]+8` and CS from
  `STAR[63:48]+16`. That is why the GDT of `boot.s` has an unused 32-bit
  user code segment at 0x18 — not out of nostalgia, but because the
  processor counts from there. `LSTAR` is the entry point, `SFMASK`
  clears IF during the system call (an interrupt while `rsp` still points
  at the user stack would be fatal), `EFER.SCE` switches the pair on.
* **The page.** Ring 3 may only touch pages whose user bit is set at
  every level. `boot.s` maps the first gigabyte with 2 MiB pages without
  that bit; giving the bit to a whole 2 MiB page would hand kernel code to
  the user program. Therefore `user.map_user` **splits** the page in
  question: a page table of 512 entries of 4 KiB is built in Firn, the
  user bit goes to exactly one of them, and CR3 is reloaded. The user
  program lies in its own section `.utext`, which the linker script puts
  on a page of its own.
* **The way back** is not a `sysret` but the abandonment of the user
  program: `enter_user` saves the kernel stack pointer, `leave_user` puts
  it back and returns to whoever called `enter_user`.

What that looks like in the run:

```
ring3: entering at rip=0x11e000  stack=0x1a8000
ring3: syscall 1 arg 59
ring3: syscall 2, back to ring 0
ring3: back in ring 0
```

**The counter-check.** "We were in ring 3" would otherwise be a claim.
With `ring3fault` on the command line the kernel answers system call 1
with a one, and the user program then executes `hlt` — allowed in ring 0,
forbidden in ring 3. What arrives is:

```
*** EXCEPTION 13 #GP  err=0x0
  rip=0x11e017  cs=0x2b  rflags=0x202  rsp=0x1a8ff0  ss=0x23
```

`cs=0x2b` is the user code segment 0x28 with RPL 3, `ss=0x23` the user
data segment. The processor really was down there.

## 7. What the test harness does

`tools/kernel/run.sh`, 46 cases, ten QEMU runs, 16 seconds in total.
Every run has a time limit; nothing can hang.

The kernel ends the machine itself: QEMU's `isa-debug-exit` at port 0xF4
turns a write into an exit code — 21 means "got to the end on its own",
63 means "stopped at an exception". Without it a test could only be ended
from outside, with a timeout, and a kernel that is killed after ten
seconds tells you nothing about whether it reached its end.

Sections 1 and 2 are the freestanding evidence per object file (ELF REL,
no undefined symbol, no libc name, no collector, no `syscall` in Firn
code — the two `syscall` instructions of the image are both in the user
program of `isr.s`). Section 3 is the full run with both compilers,
including the numbers: at least 20 ticks, at least one tick in the spin
loop, three different frames, the reuse of the freed one. Section 4 holds
the serial output of the two compilers against each other — equal apart
from addresses. Sections 5 and 6 are the four exceptions, 7 the keyboard,
8 and 9 the two counter-checks.

Hooked into `test.sh` as section 22.

## 8. What the compiler needed — and what it did not

**Nothing was changed in the compiler.** That was not the expectation at
the start of the round; the assignment explicitly allowed a missing
inline assembly form or a missing volatile guarantee. Two places came
close:

1. **`asm` has exactly one output operand.** `rdmsr` delivers its result
   in two registers (edx:eax). The way out was not a change to the
   language but a template that puts them together itself:
   `asm("rdmsr\nshl rdx, 32\nor rax, rdx", in("rcx") index, out("rax"), clobber("rdx"))`.
   That is honest, because the shift really happens there — but a second
   `out` would be the better form, and the kernel is the first user who
   would notice.
2. **Calling an address.** Firn has no function pointers in stage 0.
   `enter_user` and `leave_user` are addresses out of a table; they are
   called with `asm("call rax", in("rax") addr, ...)`. That works because
   the code generator treats `Op::Asm` like a call (the register
   allocation of round 51 saves what it has to). It is at the limit of
   what inline assembly should do.

What was **not** missing, although it was expected to be: variable shift
amounts, 64-bit arithmetic, arrays with a computed index, `bool` in
conditions, modules in the kernel profile (the search path works there
too), and the volatile guarantee — the tick loop terminates in all three
build stages.

Two things the language does not have and the kernel felt: **`~`** (the
bitwise complement; written here as `x ^ 0xFFFF…`) and **line
continuation** — an expression has to stand on one line, which is why the
descriptor fields are assembled with `var … =` and `|=`-like steps.

## 9. What is honestly missing

* **No `static`.** See 2. The state lives in a handed-over region, and
  the type checker does not check what lies at which offset.
* **No APIC.** The PIT and the two 8259 PICs are what a PC from 1990 has.
  An APIC timer, and with it several processors, is a round of its own.
* **The keyboard driver is minimal**: no shift, no modifiers, no repeat,
  no released keys, no scan code set 2.
* **Ring 3 has no address space of its own.** The user program runs in
  the kernel's page tables, with one page opened. Without an address
  space of its own there is no process — this is the switch, not an
  operating system.
* **No scheduler, no processes, no file system.** The syscall interface
  knows two numbers.
* **The heap does not merge backwards**, only forwards over the whole
  chain in `kfree`. That is O(n) per free; it is enough for a kernel of
  this size and it is not enough for one that allocates a lot.
* **`demos/kernel/core.fi` of round 52 stays as it is.** It is the
  minimal example that `tools/freestanding/run.sh` measures; the new
  kernel is a second, bigger one beside it.
