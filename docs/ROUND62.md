# Round 62 — the kernel becomes an operating system: tasks, address spaces, files

Branch `r62-os`, base `a2a2ed4` (main after rounds 58, 59, 60).

Round 59 built a kernel core: its own IDT, exceptions with a register
dump, a timer, a frame allocator, a heap, a keyboard, and one excursion
into ring 3 and back. Its own files named the limit openly —
`demos/kernel/user.fi` said it in one sentence:

> What is deliberately NOT here: an address space of its own per process,
> a scheduler, memory protection between several user programs. This is
> the switch, not an operating system.

This round is that sentence, removed. Five things were built, in this
order, and every one of them has a counter-check that has to collapse
when the thing is switched off.

**Result up front, measured (20.08.2026):**

| | base `a2a2ed4` | round 62 |
|---|---|---|
| `bash test.sh` | 905/905 | **911/911** |
| `bash tools/kernel/run.sh` | 46 passed, 0 failed | **173 passed, 0 failed** |
| `bash tools/freestanding/run.sh` | 41 passed, 0 failed | **41 passed, 0 failed** |
| `bash tools/self_compare.sh` | 234 same / 0 differing / 0 faulty | **234 / 0 / 0** |
| `bash tools/fixpoint.sh` | character-identical | **character-identical** |
| `bash tools/english/check.sh` | 0 0 0 0 0 | **0 0 0 0 0** |

The six test cases more are the two new tests (970, 971), each in three
build stages. **Not one line of the compiler was changed** — the two
reserved FIR opcode numbers and the reserved state block slots stayed
untouched, and the fixpoint is character-identical for exactly that
reason. What the operating system needed, the language already had.

---

## 1. What is there now

```
demos/kernel/
    boot.s      long mode, GDT, TSS, hand-over of the addresses   (round 59)
    isr.s       48 interrupt entry points, the syscall entry      (round 59/62)
    switch.s    THE CONTEXT SWITCH, and the way into ring 3       (round 62)
    kstate.fi   the mutable state, now with task table and disk
    serial.fi   COM1                                              (round 59)
    idt.fi      IDT, TSS, PIC, PIT                                (round 59)
    mem.fi      frames and heap                                   (round 59)
    trap.fi     what happens after an interrupt — now with the scheduler
                and with "a fault in ring 3 kills the process"
    kbd.fi      the keyboard                                      (round 59)
    user.fi     the excursion of round 59, unchanged
    sched.fi    tasks, weighted round robin, preemption           NEW
    tasks.fi    what the kernel tasks do                          NEW
    proc.fi     processes with their own page tables              NEW
    sys.fi      the system calls, with error codes                NEW
    blk.fi      the block device: RAM disk and ATA PIO            NEW
    fs.fi       superblock, inodes, directories, block bitmap     NEW
    uprog.fi    the programs of ring 3, in Firn, own object file  NEW
    kernel.ld   the layout, now with a user region of its own
tools/kernel/run.sh   46 cases -> 168
tests/970_fs_layout.fi      the file system arithmetic as a normal program
tests/971_round_robin.fi    the scheduler decision as a normal program
```

The kernel of `kmain.fi` is 5 346 lines of Firn, 2 932 of them new in
this round, against 643 lines of assembly, 116 of them new (`switch.s`).
The ratio is the point — everything that could be Firn is Firn, and the
three assembly files each say in their head why they are not.

---

## 2. Point 1: tasks and a scheduler

A task is 256 octets in the data area plus a **kernel stack of its own**
out of the frame allocator. What a task needs in order to be interrupted
in the middle and continued later lies on that stack; the record only
holds the stack pointer.

`switch.s` does the six lines a language cannot do, and it does them with
one deliberate decision: **`context_switch` preserves every general
purpose register and the flags.** Only `rsp` changes. A switch that only
saved the callee-saved registers would be enough for a compiler that
knows about the switch — neither firnc0 (which has a register allocator)
nor firnc1 knows anything about it. Fifteen pushes cost nothing here and
remove a whole class of bugs that would only ever show up under load.

The initial stack of a new task is built (`sched.frame_build`) so that it
looks exactly like a task that was switched away from a moment ago: the
return address is the first Firn function, and `rdi`/`rsi`/`rdx` sit in
the slots the switch pops them out of. A first start is therefore not a
special case.

**The plan is a weighted round robin:** everybody gets a turn in the order
of the table, and the length of the turn is the priority in timer ticks.
Priority 3 gets three times the processor of priority 1, and nothing
starves — which a strict priority order would do.

Measured in QEMU (`tools/kernel/run.sh`, section 10), three workers with
the priorities 1, 2 and 3 doing the same amount of work:

```
sched: preempt=1
sched: workers 3 4 5
sched: switches=134  alternations=71  entries=134
trace: 3 4 5 1 3 4 5 1 3 4 5 1 3 4 5 1 3 4 5 1 3 4 5 1 3 4 5 1 3 4 5 1 …
task: pid=3  kind=2  prio=1  state=5  runs=44  ticks=43  work=40  end=401  exit=40
task: pid=4  kind=2  prio=2  state=5  runs=26  ticks=50  work=40  end=384  exit=40
task: pid=5  kind=2  prio=3  state=5  runs=19  ticks=55  work=40  end=365  exit=40
```

`ticks/runs` is 0.98, 1.92 and 2.89 — the length of a turn **is** the
priority, and the test checks exactly that quotient. The one with the
highest priority is done first (365 < 384 < 401), and all three did their
full work (`work=40`, `exit=40`).

**The counter-check** is the same kernel with `nopreempt` on the command
line: the timer no longer switches. Then each of the three is scheduled
**exactly once** (`runs=1`), they run one after the other, the
alternations drop to at most 2 — and all three still do their full work.
Interleaving that came from somewhere else would survive that run.

---

## 3. Point 2: every process has its own memory

Per process: PML4, PDPT, PD, PT — four frames — plus one data page and
eight stack pages. Entry 0 of the private PDPT takes over the kernel's
page directory, so the identity mapping of the first gigabyte is shared
by everybody (the kernel has to be there when an interrupt arrives) but
is not reachable from ring 3: the 2 MiB leaves have no user bit. The one
page that is open is the code of `uprog.fi`.

```
0x00000000..0x3FFFFFFF  kernel, identity mapped, shared, no user bit
0x40000000              USER_DATA   one page, private
0x40001000..0x40008FFF  USER_STACK  eight pages, private
everything else         not mapped
```

**The proof** is that two processes write to the *same* address and read
back *their own* value. In between there is a `sleep`, so the second one
really has the chance to overwrite the first one's value:

```
proc: hello pid=3
user: hello #7
proc: hello exit=3
user: wrote here 111
user: wrote here 222
user: read here 111
user: read here 222
proc: data 0x1ca000 vs 0x1d2000  separated=1
```

**The counter-checks** are two processes that reach for what is not
theirs. One reads kernel memory at 1 MiB, the other an address in its own
region that was never mapped:

```
user fault: pid=9   vector=14  err=0x5  cr2=0x100000    rip=0x138a4b  -- process killed
user fault: pid=10  vector=14  err=0x4  cr2=0x40010000  rip=0x13c6d6  -- process killed
proc: killed=2  exit=142  exit=142
```

142 is 128 + 14, the way a system with signals counts it. This is the
point where an operating system differs from the kernel of round 59:
there, every exception stopped the machine, and rightly so — there was
nobody the fault could belong to. Here it belongs to a process, the
process dies, and the kernel goes on to the end (`kernel: done`). A fault
in ring 0 still stops everything.

Afterwards every frame is back: `proc: frames_free=32284 of 32284`.

**The fork-like half of `spawn`.** A child does not start with an empty
page: it gets the CONTENTS of its parent's data page, copied onto a frame
of its own. Inheritance and separation are proved in the same run — the
child sees the parent's value, overwrites it, and the parent still reads
its own afterwards:

```
user: parent wrote 12345
user: child inherited 12345
user: fork child exit=5          <- the child confirms what it inherited
user: parent still has 12345     <- the child wrote 999 into ITS page
proc: fork parent kept 12345
```

---

## 4. Point 3: system calls that say what went wrong

Fifteen numbers: `exit`, `write`, `read`, `getpid`, `getppid`, `sleep`,
`spawn`, `wait`, `yield`, `open`, `close`, `unlink`, `mkdir`, `list`,
`stat`. (1 and 2 stay the two marks of round 59, so that the cases of that
round keep measuring what they measured.)

The rule of `sys.fi` is that nothing is answered silently:

* every unknown number gives `-ENOSYS`,
* **every pointer out of ring 3 is checked by the kernel before it is
  followed** (`proc.user_ok` walks the page tables of the process), and a
  bad one gives `-EFAULT`,
* every descriptor is checked, a bad one gives `-EBADF`,
* errors are negative, the way Unix has counted since 1970.

The counter-check is a program of its own that hands the kernel bad
arguments on purpose and prints what comes back:

```
user: write(kernel)=-14      an address out of the kernel
user: write(nil)=-14         a null pointer
user: read(badfd)=-9         descriptor 77
user: nosys=-38              call number 999
user: sleep(1000s)=-22       an absurd duration
user: wait(bad)=-10          a pid that is not my child
```

A kernel that followed the first pointer would not print the second line
— it would be dead.

`spawn` and `wait` seen from ring 3, with two children:

```
user: spawned pid13
user: spawned pid14
user: child arg=1
user: child arg=2
user: child exit=41
user: child exit=42
```

The kernel copies through the data area in both directions: the file
system never sees an address of a process, and ring 3 never sees a
kernel address.

---

## 5. Point 4: a file system, from the block upwards

`blk.fi` is the whole interface between the file system and the device:
read block n, write block n, 512 octets. That the blocks lie in RAM is the
business of that file alone.

```
block 0        superblock: magic "FIRNFS62", sizes, where everything lies
block 1        block bitmap, one bit per block
blocks 2..33   128 inodes of 128 octets: type, size, links,
               twelve direct blocks, one indirect one
from block 34  data
```

A directory is a file whose contents are entries of 32 octets: eight for
the inode number, twenty-four for the name. The root directory is inode 1
and has `.` and `..` like every other one. The name is not in the inode —
which is why two names could mean the same file.

```
fs: disk=0x1c4000  blocks=2048
fs: mount unformatted=0                       <- the counter-check
fs: format 1=1  free=2013  inodes=1
fs: small wrote=14  read=14  same=1
fs: big wrote=1500  read=1500  same=1  free=2008
fs: list .:2 ..:2 hello.txt:1 docs:2
fs: subdir .:2 ..:2 big.bin:1
fs: unlink 1  gone=1  after=2011 of 2008
```

`mount` on an unformatted disk **has to fail** — that is the difference
between a file system and a guess about foreign octets. Deleting gives
the blocks back (2008 → 2011).

**And the same file system on a real disk.** `blk.use_ata` switches the
device to ATA PIO — primary bus, master drive, the ports every PC has had
since 1986. Not a line of `fs.fi` changes, which is what the interface is
for. QEMU gets an image, and afterwards the test looks **into that image
on the host**:

```
ata: drive present
ata: format 1  mount=1
ata: wrote=29  read=29  same=1
ata: list .:2 ..:2 ata.txt:1
```

```
$ strings disk.img | head -3
26SFNRIF                       <- the magic number, little endian
ata.txt                        <- the name, in a directory block
ata disk holds this line r62   <- the contents, in a data block
```

A file system that only ever wrote to memory does not pass that check.

---

## 6. Point 5: a command line in ring 3

`ls`, `cat`, `echo`, `write`, `rm`, `mkdir`, `exit`. The shell can do
none of it itself — every command is one or more system calls, and that
is the point: what was built underneath is enough to write a program on
top of it that is useful for something.

The input comes from descriptor 0. In the test that is what the boot
loader was given (`script=ls;cat /hello.txt;…`, a semicolon is a line
break); on a machine with a keyboard it would be the keyboard. The shell
does not notice the difference.

```
sh: ready, type away
sh$ ls
./ ../ hello.txt docs/
sh$ cat /hello.txt
firn round 62
sh$ echo hallo welt
hallo welt
sh$ write /neu.txt zeile aus der shell
sh: written 19
sh$ cat /neu.txt
zeile aus der shell
sh$ ls
./ ../ hello.txt docs/ neu.txt
sh$ rm /neu.txt
sh: rm 0
sh$ ls
./ ../ hello.txt docs/
sh$ quatsch
sh: what is quatsch
sh$ exit
sh: bye
sh: commands=10
```

---

## 7. The user programs are Firn, not assembly

Round 59 had its user program in `isr.s` and gave a reason:

> Deliberately not in Firn: everything that runs here has to lie in a page
> mapped for ring 3 […] A compiled Firn function would sit in the middle
> of the kernel's text.

That reason is gone, and it cost three lines of linker script.
`uprog.fi` is compiled into an **object file of its own**; the script
puts its code between `__user_begin` and `__user_end` on pages of its
own, and `proc.map_programs` opens exactly those and no others:

```ld
.text : ALIGN(4K) {
    *(.multiboot) *(.text.boot)
    *(EXCLUDE_FILE(*uprog*.o) .text)
    *(EXCLUDE_FILE(*uprog*.o) .text.*)
}
.utext : ALIGN(4K) {
    __user_begin = .;
    *(.utext)
    *uprog*.o(.text .text.*)
    . = ALIGN(4K);
    __user_end = .;
}
```

Two properties of that arrangement are checked in the image, not claimed:

* every `syscall` instruction of the whole image lies in `.utext`, none in
  the kernel's text — and their number is exactly the number in
  `uprog$s.o` plus the two of round 59 in `isr.s`,
* the user region is page aligned, and the number of pages the kernel
  reports having opened is exactly `(__user_end - __user_begin) / 4096`.

Both object files are freestanding in the sense of round 52: ELF `REL`, no
undefined name, no libc, no collector.

The compiled user program needs nothing from the kernel — it does not
even have `.rodata`. String literals in the kernel profile become
immediate stores into the frame, which is why a Firn function in ring 3
gets by with its code page and its stack.

---

## 8. What went wrong, and what it cost

**A real race, found by the test, not by reading.** `sched.create` marked
a new task as ready and returned; `proc.create` then filled in the address
space and the entry point. A timer interrupt in between took a process
whose `rip` was still zero:

```
user fault: pid=15  vector=14  err=0x5  cr2=0x0  rip=0x0  -- process killed
```

It hit in **one run out of eight** — the kind of thing that does not show
up in a demo and does show up in a night of running. The fix is a state
of its own: a fresh task is `S_NEW`, the scheduler never takes it, and its
creator says `sched.start` when the task is complete. On top of that
`proc.create` keeps interrupts down until the process stands. Ten runs in
a row afterwards: exactly the two intended faults, nothing else. The full
run in `tools/kernel/run.sh` section 18 now counts the faults, so a
regression of this cannot pass unnoticed.

**The user stack was too small.** Eight pages now, not one. The code
firnc0 and firnc1 generate gives every expression a slot in the frame; the
shell with its three buffers and its dozen string literals uses several
kilobytes per call level. The first symptom was a `#PF` at `0x3ffffee8` —
exactly one word below the user region, i.e. the stack had run out of the
bottom of it. That the kernel survived it and named it is what made it a
five minute fix.

**The two compilers produce a different number of `syscall`
instructions** (58 against 1): firnc0 inlines the wrapper at every call
site, firnc1 leaves the call standing. The first version of the test
demanded "at least 20" and failed on firnc1. The number says nothing about
the language, so the test now compares the number in the image against
the number in the object file — a relation that holds for both.

---

## 9. What is NOT there — honestly

* **`spawn` is not `fork`.** It creates a process with an address space of
  its own out of a program that is compiled into the image, and the child
  inherits the CONTENTS of the parent's data page (§3). What it does not
  do is what `fork` really means: the child does not continue in the
  parent's code at the point of the call, the stack is not copied, and
  there is no copy-on-write and no `exec`. A real `fork` needs shared page
  tables with a write bit taken away, and `exec` needs a program loaded
  out of the file system — both are a round of their own.
* **No demand paging, no swapping.** Every page a process has is there
  from the start.
* **A task inside a system call cannot be preempted.** `SFMASK` takes the
  interrupt flag down for the whole call. That is honest for a kernel on
  one processor, and it is the reason the scratch buffers of the file
  system need no lock — but a long call delays every other task.
* **One processor.** No SMP, no spinlocks, no per-CPU data. The locking is
  the interrupt flag and says so.
* **The file system has no cache.** Every access to an inode reads its
  block again. On the RAM disk that is a copy; on ATA it is a real read,
  and that is why the ATA case does little more than write a file and read
  it back.
* **Limits of the file system:** 128 inodes, at most 12 + 64 blocks per
  file (38 KiB), names up to 23 octets, no timestamps, no permissions, no
  hard link call, no rename. The working directory of a process exists as
  a field and is not used yet — every path starts at the root.
* **The console is not the keyboard.** `read` on descriptor 0 delivers
  what the boot loader handed in. Wiring `kbd.fi` into the same buffer is
  a few lines and was not done, because the test for it would be the
  slowest one in the suite.
* **ATA is PIO, one sector at a time, primary master, polled.** No DMA, no
  IRQ 14, no second drive, no partition table.
* **The shell has no pipes, no redirection, no quoting.** A line is a
  command and its arguments, separated by blanks.

---

## 10. The numbers of the round

```
bash test.sh                    911/911
bash tools/kernel/run.sh        173 passed, 0 failed
bash tools/freestanding/run.sh  41 passed, 0 failed
bash tools/self_compare.sh      234 same, 0 differing, 0 faulty
bash tools/fixpoint.sh          character-identical
bash tools/english/check.sh     0 0 0 0 0
```

The 173 cases of `tools/kernel/run.sh` in eighteen sections; eleven of them
are counter-checks whose measurement has to collapse when the thing under
test is switched off:

| counter-check | what it measures |
|---|---|
| `nopreempt` | without the timer switch each worker is scheduled exactly once |
| `notimer` | with IRQ0 masked the spin loop counts zero ticks (round 59) |
| kernel memory out of ring 3 | `#PF` at `cr2=0x100000`, process dead, kernel alive |
| unmapped page of its own | `#PF` at `cr2=0x40010000`, same |
| kernel pointer to `write` | `-EFAULT`, and the kernel does not follow it |
| `wait` for a foreign pid | `-ECHILD` instead of a hang |
| `mount` unformatted | refused, no foreign octets read as inodes |
| no drive | the kernel says so and gets to the end |
| no `script=` | no shell starts, the kernel does not hang on an empty console |
| `hlt` in ring 3 | `#GP` with `cs=0x2b` (round 59) |
| no keys | the keyboard reports `(none)` (round 59) |
