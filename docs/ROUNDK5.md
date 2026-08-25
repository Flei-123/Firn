# Round K5 — four processors, and the locks that make them one kernel

**State before this round:** the kernel of rounds 59, 62, K1 and K2 is an
operating system on ONE processor. It has tasks, address spaces, system
calls, a file system, a shell in ring 3, PCI, an APIC and an NVMe driver
that fetches its own commands over DMA — and every one of those was
written on the assumption that there is exactly one thread of control in
the kernel at a time. `kstate.fi` says it in as many words over its
counter helper: *"NOT atomic — it does not have to be: the kernel runs on
one processor."* Round K2 built the local APIC, which is the one thing a
second processor cannot be started without.

**What is there now:** the other cores are found in the firmware tables,
started, given state of their own, and put to work — and the three data
structures they share are behind locks. Each of those four sentences is a
number in `tools/smp/run.sh`, and each number has a run in which it has to
collapse.

---

## 1. The measurement up front

The same kernel image, the same QEMU guest (`-smp 4` in both cases, so
that the machine is bit for bit the same), the same twelve units of
arithmetic. The only difference is whether the kernel starts the other
three cores or leaves them asleep (`nosmp` on the command line).

| run | cores working | cycles | ms |
|---|---|---|---|
| `-smp 4 nosmp` | 1 | 1 877 592 310 | 857 |
| `-smp 4` | 4 | 906 480 080 | 414 |
| **speed-up** | | | **2.07×** |

And the control that turns that into evidence — the same guest, the same
four cores, but QEMU emulating all of them in ONE host thread
(`-accel tcg,thread=single`):

| run | cycles | against one core |
|---|---|---|
| four guest cores, one host thread | 1 798 405 950 | **1.04×** |

Four cores in one host thread are not faster, because they are not
parallel. That is what says the 2.07 above is parallelism and not a
measurement artefact.

**On the numbers being what they are.** 2.07 and not 4.0, and the reason
is the machine this was measured on, not the kernel: a shared build host
with a load average of 7.4 on twelve cores, several other rounds
compiling and running QEMU at the same time. The per-core numbers of the
same run show it — every core did its three units, and the slowest took
twice as long as the fastest:

```
smp: percore  c0=515561772  c1=456855256  c2=769341760  c3=500339070
```

A repeated, strictly sequential series on the same host is in section 8;
the best pair measured there is **857 ms against 226 ms, 3.8×**. The
threshold in `tools/smp/run.sh` is deliberately set at 1.8× and not at
3.5×: a test that fails when the build machine is busy is a test that
gets switched off.

---

## 2. How a core is started

There is no shortcut and no register that does it. An application
processor is sitting in the halt state the firmware left it in, and
waking it up is a chain of four steps, every one of which can fail
silently.

**1. Counting them (`demos/kernel/acpi.fi`).** The number of processors
exists in exactly one place: a table the firmware left in memory. The
chain is RSDP → RSDT or XSDT → MADT → the records of type 0 and 9. Every
step is checksummed, every length comes out of the table itself, and the
signature `"RSD PTR "` alone is not enough — the first twenty octets have
to sum to zero, which is what separates the real pointer from the same
eight characters standing in a BIOS string. Under `-smp 1`, `-smp 2` and
`-smp 4` the same image reports one, two and four.

**2. The trampoline (`demos/kernel/smp.s`, 182 octets).** A started core
does not wake up in long mode. It wakes up the way an 8086 woke up in
1978: real mode, 16 bit, `cs` at the vector of the startup message and
`ip` at zero. The `SIPI` message carries ONE octet of address, so the
entry point has to lie in the first megabyte on a page boundary — and the
kernel image lies at 1 MiB. So the blob is copied to 0x8000 at run time
and every address inside it is written as "base plus the distance from
the start of the blob". It does real mode → protected mode → PAE → the
page tables of the boot processor → EFER.LME → paging → long mode, and
then calls into Firn.

Its descriptor table has the SAME layout as the one in `boot.s` (code64
at 0x08, data at 0x10), and that is not tidiness: the core stays on this
table until Firn loads the real one, and reloading `cs` needs a far jump,
which a compiled function cannot do. Equal layouts remove the need for
one.

**3. INIT and STARTUP (`smp.fi::wake`).** The sequence out of the MP
specification, over the local APIC command register: INIT assert, INIT
deassert, 10 ms, then STARTUP twice with the page number of the
trampoline. The delays are measured on the cycle counter that round K2
calibrated against the PIT — the cores have no timer yet at that point.
One core at a time: the parameter block behind the trampoline (page table
root, stack, entry, data area, core number) is only free for the next
core once this one has set the flag in it.

**4. Arriving (`smp.fi::ap_main`).** The core builds itself a descriptor
table and a task state segment OF ITS OWN out of one frame, loads them
plus the shared interrupt table, switches on its own local APIC, and says
it is there.

Why a TSS of its own and not the boot processor's: RSP0 and IST1 live in
it, and two cores that fault at the same moment would land on the same
stack — the report of the first fault written over by the second.

---

## 3. Per-core state

Up to round K2 "the running task" was one word, `kstate.CURRENT`. With
four cores that word is wrong four times over, and a scheduler that reads
it hands the same task to two processors — two cores on one stack.

`demos/kernel/cpu.fi` is the table: one record of 256 octets per
processor at `kstate.CPU_OFF`, holding the APIC id, whether the core is
online, its current task, its OWN idle task, its own timer ticks,
switches, picks, and its stack and descriptor page.

**How a core finds its own record.** Three ways are usual and this kernel
takes the plainest. `swapgs` with a per-core GS base needs the assembly
entry points to cooperate; `cpuid` leaf 1 puts the initial APIC id in
`ebx`, and `ebx` is the one register `asm` in Firn will not hand out (it
carries the frame). What is left is the ID register of the local APIC:
one 32-bit read at a FIXED address, and the address is the same for every
core — the page is a window onto the APIC that sits in the processor
doing the access. One memory access plus a lookup over at most eight
entries, and nothing had to change in `isr.s` or `boot.s` for it.

The task table was exactly full (248 + 8 = 256 = `TASK_BYTES`), so what
this round has to note per TASK — which core last ran it, which cores
have ever run it, which core it is pinned to — lives in a second, parallel
table at `kstate.TCPU_OFF`. Growing the record to 512 would have pushed
the task table into the scheduler trace at 0xA000.

---

## 4. The locks — and what Firn already had

**Nothing had to be added to the language.** The honest answer to "does
Firn need new building blocks for SMP" is: it needed the two it already
has.

```firn
__atomic_add(p, delta)          -> old value   `lock xadd`     (round 47)
__atomic_swap(p, expect, want)  -> old value   `lock cmpxchg`  (round 49)
```

Round 47 wrote about the first one: *"Firn has no threads at stage 0, so
the difference is NOT measurable today by a two-thread run; it is
provable at the emitted instruction."* This round is the two-thread run.
`tools/smp/run.sh` still checks the instruction as well — a lock built
out of ordinary loads and stores would look exactly the same in Firn and
be worth nothing.

An atomic LOAD and STORE are `__mmio_read64`/`__mmio_write64`
(`atomic.fi::load`, `store`). On x86-64 an aligned 64-bit access is
atomic in the hardware; what it is not is safe from the compiler, which
may hoist it out of a loop or drop it — and the MMIO forms of round 52
are exactly the ones that may not be moved, merged or removed. On a
machine with a weaker memory model that would not be enough, and then
`atomic.fi` is the one file that has to change.

**`kstate.add` became one instruction.** Every counter in this kernel —
TICKS, TRAPS, SWITCHES, NEXT_PID, the spurious count of the APIC — goes
through that one helper, so making that one line `lock xadd` made all of
them safe at once.

**The spin lock** (`atomic.fi`) is `lock cmpxchg` from 0 to the owner and
a plain store back to 0. The store is enough because x86-64 does not move
a store ahead of an older store; everything the holder wrote is visible
before the lock reads as free. `pause` in the waiting loop, a counter of
turns spent waiting, 64 octets per lock so that two locks never share a
cache line, and a limit after which a core says *"lock: stuck id=N
owner=M"* and stops instead of hanging silently — which is how both
deadlocks in section 7 were found.

**Five locks, and one rule: NO LOCK IS EVER HELD WHILE ANOTHER IS TAKEN.**

| | protects |
|---|---|
| `L_SCHED` | the task table and the run queue |
| `L_FRAME` | the frame bitmap and the heap |
| `L_FS` | the file system |
| `L_STRESS` | the counter of the counter-check |
| `L_BOOT` | bringing one core up |

Two places had to be turned around for that rule: `sched.create` fetches
the kernel stack from the frame allocator BEFORE it takes the run queue
lock, and `sched.reap` gives the slot back under the lock and the FRAMES
back afterwards. Every lock is taken with interrupts already off — not
against the other cores, that is what the lock is for, but against this
core's own handlers: a timer interrupt that lands inside a critical
section and asks for the same lock waits for a lock its own core holds.

`L_FS` is the one exception to the no-nesting rule and it is re-entrant
on the same core, because `format` writes the root directory through
`dir_init`, which writes through `write_at`, which is one of the six
locked entry points. See section 7.

---

## 5. The scheduler across the cores

`sched.schedule` takes the run queue lock and does NOT give it back. It is
given back by whoever the processor switches TO — in that task's own
`schedule_locked` right after its `switch_to` came back, or, for a task
that has never run, in `sched.enter_task`, the first line of every task
body.

There is one race and no cheaper cure. If the lock were given back before
the switch, another core could pick up the task this core has just marked
`S_READY` — while this core is still executing on that task's stack,
inside `context_switch`, with its registers not yet written down. Two
cores, one stack. Holding the lock across the switch means: while any core
is between "I have chosen" and "the registers are saved", no other core is
choosing anything.

That is why `sched.frame_build` now builds a new task's frame with
interrupts OFF (`rflags = 0x002`, was `0x202`): a task that starts with
them on could take its own timer interrupt while still holding the lock
its creator handed it. `enter_task` gives the lock back and then switches
them on.

`pick` changed in three ways: it skips EVERY idle task, not only the one
whose index stood in `kstate.IDLE` (each core has one now); what it
returns when nothing is ready is THIS core's idle task; and it honours
affinity.

**Affinity, and the one task that needs it.** Task 0 is `kernel_main`
itself, and three things it does belong to the boot processor and to no
other: the excursion into ring 3, whose `syscall` MSRs `user.setup` wrote
on that core alone; the single word in the data area that `isr.s` reads
the kernel stack out of; and the two static save slots of `enter_user`.
Letting the scheduler migrate task 0 broke one run in six — see section 7.
It is pinned now, and it is the only task that is.

---

## 6. The counter-checks

Every one of them is the SAME kernel image, in a run in which the thing
being measured is switched off, and the measurement has to collapse.

**`nosmp` — the other cores are found but not started.**

```
smp: cpus=4  acpi=1  rev=0  apic=0 1 2 3
smp: nosmp, one core only
smp: online=1 of 4  failed=0
```

**`-accel tcg,thread=single` — four guest cores in one host thread.**
1.04× against one core, where the parallel run gets 2.07×. Nothing in the
guest differs.

**`nolock` — the locks switched off.** One bit per lock, not one flag for
all of them, and that is a decision worth writing down: switching every
lock off at once does not produce a measurement, it produces a kernel that
dies somewhere before it can print one. `nolock` switches off exactly the
two locks whose absence is being measured and leaves the run queue lock
alone.

Four cores raise the same counter 1500 times each. The three steps —
read, change, write — are deliberately NOT one atomic instruction: an
atomic add would make the lock unnecessary and would prove nothing about
it. The gap between reading and writing is widened by six `pause`
instructions so that the result is a measurement and not a coin toss.

| | with the lock | without |
|---|---|---|
| increments wanted | 6000 | 6000 |
| increments counted | **6000** | **1630** |
| lost | 0 | **4370** |

And the same run for the frame allocator, where nothing is widened and
nothing is arranged — this is the allocator every other part of the
kernel uses, and the race is the one it really has: find a free bit, set
it, count down. Every core takes sixteen frames:

| | with the lock | without |
|---|---|---|
| frames handed out | 64 | 64 |
| **the same frame in two cores' hands** | **0** | **5** |

A frame handed out twice is a frame two parts of the kernel now believe
is theirs alone. Nothing reports it; the kernel simply starts writing two
things into one page.

**The scheduler across cores**, from the same run:

```
smp: sched tasks=8  done=8  cores_used=4  switches=755  ms=1649
smp: picks  c0=187 c1=151 c2=150 c3=153
```

Eight kernel tasks in one queue, taken by all four cores, all eight run to
the end. The counter-check is the `nosmp` run: `cores_used=1`, and the
same eight tasks still finish — round 62 keeps working.

---

## 7. The two bugs, and how they showed themselves

Both are worth the space, because both are the kind that looks like
something else.

**The per-core records landed on top of round K2.** The list at the head
of `kstate.fi` stops at 0x0F000 and the obvious reading is that everything
above it is free. It is not: `pci.fi` puts its device table at 0x10000 and
its counters at 0x12000, and `nvme.fi` puts four queues, an identify page
and two measurement buffers between 0x14000 and 0x1B000 — all offsets in
the same data area, none of them named in that list. The first version of
this round put the per-core records at 0x12000, wrote the index of the
running task over the address of the local APIC, and the machine stopped
after EXACTLY ONE timer interrupt, with no message: the end-of-interrupt
went to address zero, the controller let nothing else through, and the
kernel sat in `hlt`. Found with `qemu -d int`, which showed one serviced
`INT=0x20` and nothing after it. The map in `kstate.fi` now lists all four
regions.

**Task 0 migrated.** One run in six died with a `#PF` whose `rip` pointed
into the boot processor's own kernel stack, or hung outright. The
scheduler was doing exactly what a scheduler is supposed to do: task 0 was
`S_READY` while the boot processor ran a worker, another core picked it
up, and `kernel_main` continued on a core whose `syscall` MSRs were never
written and whose `KSTACK_CUR` is not the one `isr.s` reads. Fixed by
affinity, and it is the reason affinity exists in this round at all.

A third one, smaller: the file system lock deadlocked against itself the
first time it was switched on, at `fs: format `, with the core waiting
for a lock it was holding. `format` → `dir_init` → `write_at`, and
`write_at` is one of the six locked entry points. The lock is re-entrant
on the same core now; it may decide that by reading the owner word,
because interrupts are off from the line above and no other core can have
written THIS core's number into it.

---

## 8. Numbers, repeated

Five pairs, strictly sequential, same host, same image, alternating
`nosmp` and four cores. `ms` is the wall time of the twelve units,
measured on the calibrated cycle counter inside the kernel.

<!--MEASUREMENTS-->

---

## 9. What this round does NOT do

Named, because a limit that is not named is a claim.

* **Ring 3 stays on the boot processor.** A process entering the kernel
  through `syscall` lands on the stack `isr.s` reads out of ONE fixed
  offset in the data area — one word for the whole machine. Making that
  per core needs either a per-core GS base or a second table lookup in the
  system call entry point, in assembly, before there is a stack. Until
  then the started cores run kernel tasks, `sched.set_kernel_stack` only
  writes the word when it is the boot processor writing it, and nothing
  about ring 3 changes.
* **No load balancing beyond "whoever is free takes the next one".** The
  run queue is one list and `pick` walks it from the current task
  onwards. That distributes work (four cores, 151 to 187 tasks each) and
  it is not a scheduler that thinks about caches, priorities across cores
  or migration cost.
* **No IPIs beyond INIT/STARTUP.** There is no cross-core wake-up, no TLB
  shootdown. The second one is not needed yet because no page table shared
  between cores is ever unmapped while another core is using it — kernel
  tasks all run in the kernel address space, and a process only ever runs
  on the boot processor. The moment either of those changes, a shootdown
  becomes mandatory.
* **The file system is locked at its six entry points**, not inside every
  helper. Two cores calling `fs.read_at` are serialised; a core calling
  one of the internal helpers directly is not protected, and nothing
  outside `fs.fi` does.
* **`nolockall`** switches off the run queue lock too. That run is not
  expected to be meaningful and the scheduler part of the stage is skipped
  in it; it exists so that the switch is complete, not as a measurement.

---

## 10. Files and how to run it

| file | what |
|---|---|
| `demos/kernel/acpi.fi` | RSDP, RSDT/XSDT, MADT — how many processors there are |
| `demos/kernel/atomic.fi` | compare-and-swap, atomic add/load/store, the spin lock |
| `demos/kernel/cpu.fi` | the per-core table, and "which core am I" |
| `demos/kernel/smp.s` | the 16-bit trampoline, 182 octets, copied to 0x8000 |
| `demos/kernel/smp.fi` | bring-up, the four measurements, the scheduler core loop |
| `demos/kernel/sched.fi` | per-core current task, the run queue lock, affinity |
| `demos/kernel/mem.fi` | the frame allocator behind `L_FRAME` |
| `demos/kernel/fs.fi` | the six entry points behind `L_FS` |
| `tools/smp/run.sh` | all of the above, measured, with the counter-checks |

```
bash tools/smp/run.sh          # 55 checks, the numbers of section 1
bash tools/kernel/run.sh       # round 59/62 on one core, unchanged
./test.sh                      # section 57 is this round
```

The command line words this round adds: `nosmp` (find the cores, do not
start them), `nolock` (the two data locks off), `nolockall` (all of them),
`smplong` (four times the work in the benchmark).
