# Round K1 — a program comes off the disk

Round 62 left the kernel with tasks, address spaces, system calls, a file
system and a shell, and with one hole in the middle of it:

> Every program it could run was compiled into it.

`demos/kernel/uprog.fi` is a translation unit of its own and lies on pages
of its own, but it travels in the kernel image. The list of programs was a
list of `if`s in `u_enter`, the shell was a chain of branches in
`u_shell`, and adding a command meant building a new kernel. That is not
an operating system. That is a demonstration with a fixed programme.

This round closes it. `/bin/sh` is an ELF64 file on a drive that the host
wrote and the kernel has never seen. The kernel reads it, lays its
segments into a fresh address space with the rights the file asks for,
zeroes what the file does not fill, builds an argument block and starts
it. Everything the shell then runs — `ls`, `cat`, `echo`, `rm` — it runs
the same way, through one system call, and the kernel has no idea what any
of them is.

The sentence the round had to reach was: *Osum boots, reads `/bin/sh` off
the drive, and one can type `ls` into it and get the contents of the file
system.* It does, and section 9 of `tools/osum/run.sh` types those two
letters through the QEMU monitor.

    OSUM: 129 passed, 0 failed
    KERNEL: 174 passed, 0 failed        (round 62, unchanged)
    FREESTANDING: 41 passed, 0 failed
    cargo test: 238 passed, 0 failed

---

## 1. What is new

| file | lines | what it is |
|---|---|---|
| `demos/kernel/elf.fi` | 762 | the ELF64 loader and `spawn` |
| `demos/kernel/user/ulib.fi` | 223 | what a program on the disk has instead of a library |
| `demos/kernel/user/sh.fi` | 138 | the shell — read a line, find a file, start it, wait |
| `demos/kernel/user/{ls,cat,echo,rm}.fi` | 165 | four tools, system calls only |
| `demos/kernel/user/{hello,hurt}.fi` | 146 | the two programs that exist to be measured |
| `demos/kernel/user/crt.s` | 52 | `_start` and `osum_panic` — the four instructions Firn has no words for |
| `demos/kernel/user/user.ld` | 79 | three segments, page granular |
| `tools/osum/mkfs.py` | 325 | an OFS image, built on the host |
| `tools/osum/break.py` | 168 | ELF files with exactly one thing wrong |
| `tools/osum/run.sh` | 599 | the guard |

and 600 changed lines in `kstate.fi`, `proc.fi`, `sys.fi`, `sched.fi`,
`kbd.fi`, `user.fi`, `kmain.fi`.

---

## 2. The decision: `spawn`, not a Unix `exec`

The brief asked for `exec` "and a clean `spawn`/`fork` model that fits
`proc.fi` — and say why". The answer is `SYS_EXEC = 25` with the semantics
of `posix_spawn`:

```
exec(path, argv, argc) -> pid          the caller keeps running
wait(pid)              -> exit code
```

**Why not `fork` + `exec`.** Unix `exec` replaces the image of the CALLING
process, and it needs `fork` in front of it to have a process to throw
away. `fork` without copy-on-write means copying a whole image that the
very next instruction discards; `fork` with copy-on-write means a page
fault handler that shares frames and counts references, which is a round
of its own in `mem.fi` and `proc.fi`. And the four things `fork` is really
used for — a redirection set up between `fork` and `exec`, a process that
goes on running as itself, a job in a shell, a daemon — none of them
exists in this system. `posix_spawn` and `vfork` exist in Unix for exactly
this situation.

**Why it fits `proc.fi`.** `proc.create` already did two things in one:
build a task with an address space, and point it at `uprog.u_enter`. Round
K1 split the first half out as `proc.create_bare` — a task plus an empty
address space, which is precisely the thing an image has to be poured
into. `proc.create` is now `create_bare` plus the compiled-in entry point,
and `elf.spawn` is `create_bare` plus a file. Round 62's `spawn` keeps its
meaning, including the inheritance of the parent's data page; an image off
the disk gets a ZEROED data page, because there is nothing sensible for it
to inherit.

**What it costs.** No redirection (`ls > file` is not possible), and a
program cannot replace itself. Both are honest gaps, and both are cheap to
add later on top of `create_bare`.

---

## 3. The address space of a loaded program

`proc.fi` built a private page table over 512 pages from `0x40000000`.
Nine of them were spoken for; the other 503 lay unused. The image goes
into them:

```
0x40000000   the data page                        rw, no execute
0x40001000   eight stack pages                    rw, no execute
0x40008000   the top one of those: argc/argv      rw, no execute
   ...       0xF7000 of nothing                   a growing stack faults here
0x40100000   IMAGE_BASE — where user.ld links
0x401FFFFF   IMAGE_END - 1
```

The gap between the stack and `IMAGE_BASE` is deliberate: a stack that
grew into the image would be a bug that only shows up under load, and
unmapped pages turn it into an immediate `#PF`.

`proc.free_space` already walked the whole page table when a process died,
so the image frames are not a special case — they come back with
everything else. Measured, in `tools/osum/run.sh` section 4: after eight
programs have been loaded and have exited, `frames_free` is the same
number it was before the shell started.

### The argument block

`switch.s::enter_user_task` hands ring 3 exactly ONE register (`rdi`) and
zeroes the rest, and round K1 did not change that file. So `argc`/`argv`
travel in memory, on the top stack page, and `rdi` is its address:

```
0x40008000  +0   argc
            +8   argv[0..argc-1], then a zero
            +80  the strings, each ended by a zero
rsp starts at 0x40007FF0, just under it
```

It costs no frame of its own (the page was already there as stack), it is
private per process, and it is gone when the process is.

---

## 4. W^X, and why the loader is strict

A page table entry has one set of rights per page. Two segments that share
a page therefore cannot have two sets of rights, and a loader that quietly
gives the shared page the union of both has silently made the constants
writable and the data executable without ever saying so.

So the loader demands page-granular segments and refuses anything else
(`R_ALIGN`, `R_OVERLAP`), and `user.ld` produces three:

```
text    PT_LOAD FILEHDR PHDRS FLAGS(5)   R+X   .text
rodata  PT_LOAD FLAGS(4)                 R     .rodata
data    PT_LOAD FLAGS(6)                 R+W   .data .bss
```

The language already sorts the three: `compiler/src/statics.rs` puts a
`static` without `mut` in `.rodata`, a `static mut` with a value in
`.data`, and an all-zero `static mut` in `.bss`. The linker script only
has to keep them apart.

`EFER.NXE` is switched on in `user.setup`, but only after `CPUID` leaf
`0x80000001` EDX bit 20 says the processor knows it — without NXE, bit 63
of a page table entry is RESERVED, and setting it would turn every access
through that entry into a fault. `proc.nx` is the only place the bit comes
from, and it hands back 0 when the answer was no.

### The rights, measured in faults

`/bin/hurt` does the four things a process must not be able to do, one per
argument. The error code of a page fault says WHY (bit 0 = the page was
there, bit 1 = it was a write, bit 2 = it came out of ring 3, bit 4 = it
was an instruction fetch), so the four faults are four different numbers:

| command | fault | what it proves |
|---|---|---|
| `hurt text` | `err=0x7  cr2=0x40100000` | there, write, ring 3 → the code page is read only |
| `hurt stack` | `err=0x15 cr2=0x4000....` | there, **instruction fetch**, ring 3 → the no-execute bit really reached the stack |
| `hurt kernel` | `err=0x5  cr2=0x100000` | there, ring 3 → kernel memory stays closed |
| `hurt wild` | `err=0x4  cr2=0x40050000` | not there, ring 3 |

All four processes are killed with exit 142 (128 + vector 14), the shell
carries on, the kernel shuts the machine down itself, and every frame
comes back. That whole run is section 6 of the guard.

---

## 5. Checked arithmetic and hostile numbers

This is the part of the round that would have been a crash rather than a
refusal, and it is worth writing down.

Under `profile kernel`, arithmetic is CHECKED (SPEC section 13, item L9):
a `+` that goes out of range calls `osum_panic`, and `osum_panic` halts
the machine. An ELF header is a file, a file can hold anything, and the
obvious way to write a bounds check is

```
if off + filesz > size { return R_SEGFILE }        // WRONG
```

With `p_offset = 0xFFFFFFFFFFFFF000` that addition does not wrap silently
— it panics, and the kernel is gone. Every bound in `elf.fi` is therefore
written as a SUBTRACTION on the side that is known to be big enough:

```
if off > size          { return R_SEGFILE }
if size - off < filesz { return R_SEGFILE }        // right
```

Three of the twenty-one broken files in the guard exist only to measure
this: `phoff` (`e_phoff = 0xFFFFFFFFFFFFFF00`), `bigoff`
(`p_offset = 0xFFFFFFFFFFFFF000`) and `bigmem`
(`p_memsz = 0xFFFFFFFFFFFF0000`). All three are refused with a named
reason, and `hasnot "$B" "osum_panic"` is the check that says the kernel
never reached the panic handler.

---

## 6. The twenty-one refusals

`tools/osum/break.py` takes a program that WORKS and changes exactly ONE
field, so that what the kernel says can be traced back to that field and
to nothing else. One QEMU run drives all of them through the shell:

| file | reason | file | reason |
|---|---|---|---|
| 32 octets long | 3 shorter than a header | a segment past the end | 13 |
| magic changed | 4 no ELF magic | `p_offset` huge | 13 |
| ELFCLASS32 | 5 not ELFCLASS64 | `p_filesz > p_memsz` | 14 |
| big endian | 6 not little endian | vaddr `0x50000000` | 15 outside the region |
| version 0 | 7 wrong version | `p_memsz` huge | 15 |
| ET_DYN | 8 not ET_EXEC | vaddr 8 octets off | 16 not page aligned |
| EM_386 | 9 not x86-64 | two segments one page | 17 |
| `e_phentsize` 32 | 10 wrong sizes | entry segment without PF_X | 19 entry on no exec page |
| `e_phnum` 99 | 11 too many | every PT_LOAD → PT_NOTE | 21 no PT_LOAD |
| `e_phoff` huge | 12 past the end | a DIRECTORY | 2 not a plain file |
| | | a text file | 3 |

Each reason is checked ONE BY ONE, against the block of output that
belongs to that file — not merely "the number turns up somewhere". Round
K1 needed that: two different defects both came out as reason 14 at first,
because the tool that produced them raised `p_filesz` as well and an
earlier check fired. The measurement said so; a looser one would not have.

After all twenty-one the shell is still running, the kernel still ends the
machine itself (exit 21), and there is not one exception in the whole run.

---

## 7. The counter-check that matters most

Everything above could still be true of a kernel that carried a copy of
`ls` inside it and only pretended to read the disk. So:

> Build a second image in which the octets of `/bin/echo` sit at the path
> `/bin/ls`. Boot the SAME kernel binary. Type `ls one two`.

    osum$ ls one two
    one two
    osum$ ls -> 2

It echoes. It does not list. Even the exit code is echo's. Nothing changed
but the file — and against the honest image the same kernel, the same
shell and the same command list the directory. That is what "the program
comes off the disk" means, and it is section 8.

Two smaller ones in the same spirit: `rm /readme.txt` inside the shell, and
afterwards the HOST reads its own image back and the file is gone
(`mkfs.py list`) — the writes reach the real drive. And `tools/osum/
mkfs.py` is a SECOND implementation of the on-disk format of `fs.fi`,
written on the other side of the disk, in another language, from the
constants in that file and nothing else. Two implementations agreeing on
where an inode lies is a much stronger statement than one implementation
agreeing with itself.

---

## 8. The bug that cost the round its afternoon

With the loader finished, `ls` worked — and then the machine died, in
about six runs out of eight, always after the shell said `sh: bye`:

```
*** EXCEPTION 13 #GP  err=0x0
  rip=0x1bf138  cs=0x8  rflags=0x93  rsp=0x1beff8  ss=0x10
  rax=0x0000000000ac10fa  rsi=0x00000000000001f7  rdi=0x0000000000000020
```

`rsi = 0x1F7` is the ATA command port and `rdi = 0x20` is the ATA READ
command, so the register dump pointed straight at `blk.fi`. The driver was
innocent. `rip` was an address inside a page the frame allocator had handed
out, and once the loader printed the kernel stack of each task
(`elf: start 3 ... kstack=0x1bf000`) the arithmetic was plain: `rsp` was
eight octets BELOW the base of the shell's own kernel stack.

**The kernel stack overflowed.** Round 62 gave a task two frames, and two
were enough for what a system call did then. Round K1 made the deepest one
much deeper:

```
syscall_entry -> sys.entry -> do_exec -> elf.spawn -> elf.load
              -> elf.segment -> fs.read_at -> blk.read -> ata_read
```

So the number is not a guess any more. `sched.create` now PAINTS every
kernel stack with `0x5A5A...`, `sched.stack_high` reads the high-water
mark back off it, `kmain.osum` reports it, and the guard checks both
bounds:

    osum: kstack deepest=9216 of 16384

— under the 16 KiB a task now has, and OVER the 8192 that round 62 gave
it. The second bound is the counter-check: it is the number that says the
two frames were not enough and `KSTACK_FRAMES` had to change.

The same measurement then caught a second thing. `elf.say_reason` first
had all twenty-two texts as locals in one function — 500 octets in a frame
that sits at the deepest point the kernel ever reaches — and the run
measured **16240 of 16384**. Split across five functions that are called
one after the other and never one inside the other, the same run measures
9216. Without the painting nobody would have known how close that was.

---

## 9. A gap in firnc1, measured

The obvious shape for those texts is `static` arrays in `.rodata` (round
89). It does not work, and the reason is worth recording because it is a
concrete, reproducible gap in the self-hosted compiler:

| program | firnc0 | firnc1 |
|---|---|---|
| `static MSG: [u8; 13] = "…"` and `MSG[0]` | ok | ok |
| `static mut X: u64 = 7` and `(&X) as u64` | ok | ok |
| `static MSG: [u8; 13] = "…"` and `(&MSG[0]) as u64` | ok | **exit 1, no message** |
| the same with `(&MSG) as u64` | ok | **exit 1, no message** |

firnc1 compiles a `static` array and reads out of it, but cannot take its
ADDRESS. Both compilers have to build this kernel, so the texts stayed
locals, split across five small functions. When the gap closes, twenty-two
`static`s and one function are the better shape; the note is in `elf.fi`
above `say_reason`. (Measured 2026-08-24 against `.firnc1`. The silent
exit is itself worth a look: a compiler that refuses something should say
what.)

---

## 10. The keyboard, and how a run ends

`kbd.fi` collected keys into a 64-octet buffer and stopped at 64, which is
right for round 59 (nothing read it) and wrong for a shell (a keyboard
that dies after 64 characters). It is a RING now: `KEYLEN` counts every
key that ever arrived, `KEYPOS` how many the console has eaten, and what
overruns an unread ring is dropped — the writer is an interrupt handler
and has nowhere to wait. `0x0E` became 8 instead of silence, so a line can
be corrected.

`sys.console_read` has two sources and tries them in this order: what the
boot loader put there (`script=ls;cat /x`, which is how every scripted
measurement works and why round 62's cases still measure what they
measured), and then the keyboard, if the run has one. The waiting is
`sched.sleep_ticks(1)` — which hands the processor to the idle task, where
interrupts are on and IRQ1 arrives; a system call itself runs with `IF`
clear (`SFMASK`), so a busy wait inside one would wait forever.

A quiet console for four seconds is the END OF THE INPUT, and the shell
then leaves. A shell that waited forever would be right in front of a
person and wrong in a test run: the kernel could never shut itself down,
and a measurement that ends in a time limit measures nothing.

Section 9 of the guard sends `l`, `s`, `ret` through the QEMU monitor into
a kernel booted with a drive and no script, and reads back:

    key: l
    key: s
    key: [enter]
    osum$ ls
    elf: start 4 entry=0x40100000 ustack=0x40007ff0 kstack=0x… bytes=7588 pages=3
    ./ ../ bin/ readme.txt
    osum$ ls -> 4

Typed in, loaded off the disk, printed back.

---

## 11. Numbers

| | |
|---|---|
| programs in `/bin` | 7 (`sh ls cat echo rm hurt hello`) |
| biggest program, firnc0 | 19 576 octets |
| biggest program, firnc1 | 35 656 octets |
| a file in OFS may hold | 38 912 octets (12 direct + 64 indirect blocks) |
| image | 2048 blocks = 1 MiB, 1775 free after the userland |
| segments of `/bin/hello` | 3 — R+X 5635, R 2384, R+W 8 of 16 (a `.bss`) |
| deepest kernel stack in a run | 9216 of 16 384 |
| programs loaded in the main run | 8 |
| refusals measured | 21 |
| frames leaked | 0 |
| checks in `tools/osum/run.sh` | 129 |

`FILEHDR PHDRS` in `user.ld` is in there for the fifth row: without it
every program carries a page of nothing at the front, and firnc1's shell
was 39 752 octets — over the limit of a file in this file system. It also
means a program can read its own ELF header, the way a program on any
other system can.

---

## 12. The acceptance run, honestly

    cargo test --release            238 passed, 0 failed
    tools/kernel/run.sh             174 passed, 0 failed   (round 62, untouched)
    tools/freestanding/run.sh        41 passed, 0 failed
    tools/osum/run.sh               129 passed, 0 failed
    test.sh                        1526 passed, 8 failed

The eight are named here rather than explained away, because a report that
hides them is worth nothing:

    tests/834_arc_thread.fi  [opt] [devfast] [safe]   exit 9
    tests/860_thread_basic.fi [opt] [devfast]         exit 14
    tools/self_compare.sh    (the same 834 file)
    tools/fixpoint.sh        (the same 834 file, in its corpus phase)
    tools/bench82/run.sh     self compile 7082 ms, limit 5000 ms

Exit 9 and exit 14 are the SAME kind of check, and the source says what it
is:

```
// The counter-check MUST lose. If it does not, the threads did not
// really run at the same time, and B proves nothing.
if ld(z, Z_ROHZ) >= 4 * ROUNDS {
    return 14
}
```

A non-atomic counter incremented by four threads has to lose increments;
if it loses none, the threads were serialised and the atomic counter next
to it proves nothing. This machine was running six rounds of this project
at once, at a load average of 15 to 25 on eight cores, so the threads WERE
serialised. `bench82` is a wall-clock limit on the self compile and fails
for the same reason.

That this has nothing to do with round K1 is not an assertion. The same
test, built and run in a DIFFERENT worktree at the same base commit and
without a line of this round in it, gives:

    9 9 9 0 0 9 9 9 9 9 9 9

— nondeterministic, mostly failing, and failing without K1. Everything
round K1 touched is green: the kernel guard, the freestanding guard, the
new guard, and sections 46/47/48 (statics, checked index, build levels)
which are the ones its changes could plausibly have disturbed.

---

## 13. What is still open

* **No redirection and no pipe.** `ls > file` needs a descriptor table
  that survives `exec`, which needs the fork/exec split this round argued
  its way out of. `create_bare` is the place it would go.
* **A program cannot replace itself.** `exec` always makes a new process.
* **One directory of programs.** `/bin`, named in `sh.fi`. There is no
  `PATH` and no working directory: `T_CWD` exists in the task record and
  nothing uses it.
* **No `argv` beyond eight, no environment.** `proc.MAX_ARGS` is 8 and the
  block is one page.
* **The loader refuses segments that share a page.** That is deliberate
  (section 4) and it means a file from a foreign linker will usually be
  refused with reason 16 or 17. A loader that wanted to accept them would
  have to compute the union of the rights and say so out loud.
* **`create_bare` leaks its page tables when the frame allocator runs dry
  in the middle of `space_build`.** `free_space` bails out when `T_CR3` is
  still the kernel's, which it is at that point. Round 62 had the same
  hole; this round did not widen it and did not close it.
* **The disk is read through ATA PIO with interrupts off**, one sector at
  a time, inside the `exec` system call. Loading a 20 KiB program is some
  40 sectors. That is fine for a demonstration and wrong for a system.
