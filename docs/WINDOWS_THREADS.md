# Threads and process trees on Windows (round WIN-THREADS)

Before this round the Windows target had no threads (`clone(2)` answered
ENOSYS and `std.pool` ran every job inside `submit`), and `std.process` could
kill a child but not what the child had started. Both are done now.

**Tested under Wine 8 only.** The machine the launcher is for (FLEI-ONE) is
not reachable from the build server. `tools/windows/threadkit.sh` writes a zip
(`build/windows-threads-kit.zip`) that needs only Python 3 on the Windows side
and runs the same programs there; its output tells what is still unproven.

## 1. Threads

The thread runtime is Firn source (`lib/gc/gc.fi`, round 49): a thread table,
a futex-style mutex, a channel, the stop-the-world collector. It stands on
five things the Linux kernel gave it. This is what each became on Windows:

| Linux | Windows |
|---|---|
| `clone(2)` (`Op::ThreadSpawn`) | `CreateThread` with `STACK_SIZE_PARAM_IS_A_RESERVATION`; the stack size is read out of the thread block, so `thread_stack_set` keeps working. The handle is closed at once; the thread is joined through the TID word (below). `thread::spawn_sequence_windows` |
| the entry in the child (`__thread_entry`) | a Win64 thunk `_Fwin.thread_main` (`win::thread_main_asm`): saves what Win64 wants saved, stores the TEB's `StackBase` (`gs:[8]`) as the stack bottom the collector scans up to, calls `__thread_entry`, then zeroes the TID word and wakes it |
| `fs:0` = the thread block (`Op::ThreadSelf`) | a TLS slot: `arch_prctl(ARCH_SET_FS, tcb)` in the seam does `TlsAlloc` once and `TlsSetValue` per thread; `Op::ThreadSelf` reads `gs:[0x1480 + slot*8]` (`TEB.TlsSlots`), two instructions and no call (`thread::self_sequence_windows`) |
| `futex(WAIT/WAKE)` | `WaitOnAddress` / `WakeByAddressSingle` / `WakeByAddressAll` (seam, call 202). Same contract: wait returns at once if the 32-bit word no longer holds the value; wakes may be spurious (every caller loops) |
| `CLONE_CHILD_CLEARTID` (the kernel zeroes the TID word and wakes it when the thread ends; `thread_wait` sleeps on it) | the TID word is set to 1 before `CreateThread`; the thunk zeroes it and calls `WakeByAddressAll` after `__thread_entry` returned |
| `mmap` / `mprotect` / `nanosleep` / `sched_yield` / `gettid` | already in the seam (`VirtualAlloc`, `VirtualProtect`, `Sleep`, `SwitchToThread`, `GetCurrentThreadId`) |

`lock cmpxchg` and `lock xadd` are plain instructions: `thread_lock`, `Arc`,
`__atomic_add` and `__atomic_swap` work unchanged (tests 834, 860, 2101 D).

**The seam is thread safe now** (`win_seam.rs`). It kept its scratch pages
(`__win_path`, `__win_tmp`) in globals, which two threads would have
overwritten. Calls that use them (open, openat, getdents64, statx, unlinkat,
symlinkat, access, unlink, getcwd, mkdir, rename, dup, dup2) now run under one
spin lock. Calls that may BLOCK (read, write, recv, accept, connect, poll,
sleep, futex) take no lock and work in locals only, so a thread waiting in
`recv` never keeps another from opening a file. Descriptor slots are claimed
with a compare-and-swap.

`lib/std/pool.windows.fi` is gone: `std.pool` is one file and runs on real
threads on both systems.

### HONEST (threads)

* **W-T1 Windows 8 or later.** `WaitOnAddress` lives in
  `API-MS-WIN-CORE-SYNCH-L1-2-0.dll`; the baseline already needed Windows 8
  (`GetCurrentThreadStackLimits`). The import is part of every Windows
  program's table now (the seam always references it), which costs one more DLL
  to load.
* **W-T2 The mapped stack is wasted.** `thread_start` still maps the stack
  block Linux needs (1 MiB of committed address space by default plus a guard
  page); on Windows the thread runs on the stack the system reserved, and the
  block stays unused until `thread_wait`. 64 threads = 64 MiB of commit
  charge, not of touched memory.
* **W-T3 TLS slot 0..63 only.** The slot the thread block lives in has to be
  one of the 64 inline slots of the TEB, because that is what makes the read
  two instructions. In a process that already used 64 slots the first
  `thread_start` fails (returns 0, nothing crashes).
* **W-T4 Not tested on real Windows**, see above. Under Wine: tests 834, 860,
  861, 862, 1600, 2065, 2066, 2091, 2101 give the same output as on Linux.
* **W-T5 Libraries with their own state.** `lib/window/backend.windows.fi` and
  `lib/appkit/platform.windows.fi` keep lazily filled globals (function
  pointers, a child table). They are meant for the UI thread; calling them from
  several threads at once is not promised.
* **W-T6 A thread that blocks must say so** (`thread_blocking_an/out`, as on
  Linux) or a collection waits for it. The seam does not do it for you.
* **W-T7 The self-hosted compiler** (`lib/firnc1`) has its own seam and does
  not have this yet.

## 2. Process trees

| | Linux | Windows |
|---|---|---|
| `process.set_group(&c, true)` | `setpgid(0, 0)` in the child before `execve`: a group of its own | the child is created suspended, put into a new job object (`CreateJobObjectW`, `AssignProcessToJobObject`), then resumed |
| `process.kill_tree(&ch)` | `kill(-pgid, SIGKILL)` | `TerminateJobObject(job, 137)` |
| `process.terminate_tree(&ch)` | `kill(-pgid, SIGTERM)` | `TerminateJobObject(job, 143)` (there is no gentle way for a tree) |
| `process.set_kill_with_launcher(&c, true)` | no effect; `kill_with_launcher_supported()` is false | the job carries `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`: when the launcher ends - normally, crashed or killed - the system closes the handle and kills every process in the job |
| `process.is_grouped(&ch)` | true if the group id is kept | true if there is a job |

Without a group `kill_tree`/`terminate_tree` kill/terminate the child alone and
answer false. A detached child (`set_detach`) on Linux is in its own group
anyway (`setsid`), so the two calls work on it as they are.

The group id lives in `Child.h` on Linux (it was always 0 there), the two
handles on Windows (process in the low 32 bits, job in the high 32; handles
are 32-bit values by design). `Child`'s layout did not change, so code that
builds a `Child` by hand (tests 2062-2064) still compiles.

The group id is verified (`getpgid`) before it is kept: `kill(-1, ...)` would
signal every process the user owns.

### HONEST (process trees)

* **W-P1 Linux cannot see a descendant that left the group** (`setsid`,
  `setpgid`, a daemon that double-forks). Only a cgroup would; that needs
  privileges. Windows jobs catch everything except a process created with
  `CREATE_BREAKAWAY_FROM_JOB` where the job allows it (ours does not).
* **W-P2 Process group ids are reused** once the whole group has ended. Do not
  call `kill_tree` on a child that was reaped long ago whose descendants are
  gone.
* **W-P3 A grouped child leaves the terminal's foreground group**: Ctrl+C in
  the launcher's terminal does not reach it.
* **W-P4 `set_kill_with_launcher` has no Linux counterpart that covers a
  tree.** `PR_SET_PDEATHSIG` fires per thread and reaches only the child, so it
  was not used. A launcher that needs the guarantee on Linux has to kill its
  tree itself on its way out (`kill_tree` from an exit path) - it cannot
  survive a `kill -9` of the launcher.
* **W-P5 On Windows `close(&ch)` does not close the job handle** (that would
  kill the tree of a launcher that only wanted to release the pipes); one
  handle per grouped child stays until the launcher ends.
* **W-P6 If the job cannot be made or assigned, `spawn` fails with Resource
  and the child is killed**: a launcher that asked for the guarantee does not
  get a child without it. Nested jobs (a launcher that itself runs in a job)
  work from Windows 8; on Windows 7 they would not.
* **W-P7 Wine**: jobs are implemented in Wine's server and the test passes
  (`tests/2100`: a grandchild that inherited the child's stdout pipe; the pipe
  reports end of file only when the grandchild is dead too). Real Windows is
  untested.
* **W-P8 AArch64**: `setpgid`/`getpgid` are in the call table
  (`compiler/src/syscalls.rs`); macOS, OrientOS and Android have no
  implementation in this round.

## Tests

* `tests/2100_std_process_tree.fi` - both systems: group kill, group
  terminate, the control without a group, a child that has ended, kill with
  the launcher.
* `tests/2101_threads_portable.fi` - both systems: eight threads doing file
  work at once through the seam, a deep recursion on a thread stack while two
  threads force collections, `thread_stack_set`, lock + atomics, a futex
  channel with two producers and two consumers, join in waves.
* Existing thread tests 834, 860, 861, 862, 1600, 2065, 2091 moved from
  "differs on Windows" to "same" in `tools/windows/run.sh`
  (`tools/windows/causes.txt` lost their THREADS lines).
