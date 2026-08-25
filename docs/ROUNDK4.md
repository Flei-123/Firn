# Round K4 — the POSIX floor, and a libc in Firn

Round 62 gave Osum tasks, address spaces, a file system and seventeen
system calls of its own invention. Round K1 put `/bin/sh` on a drive and
added `exec`. That was the ceiling of the system, and the ceiling was low
enough to see: every program ever written for Unix asks for `open`,
`lseek`, `fstat`, `getdents64`, `brk`, `mmap`, `fork`, `execve`, `wait4`,
`pipe`, `dup2` — and not one of them was there.

This round lays the floor: **twenty-six system calls with the numbers of
Linux x86-64, and a libc in Firn on top of them.** Nothing in it is C.

    POSIX:   134 passed, 0 failed        (tools/posix/run.sh, new)
    KERNEL:  174 passed, 0 failed        (unchanged)
    OSUM:    129 passed, 0 failed        (unchanged)

---

## 1. What is new

| file | lines | what it is |
|---|---|---|
| `demos/kernel/errno.fi` | 78 | ONE list of error numbers for the whole system |
| `demos/kernel/file.fi` | 380 | the open file table, the descriptor tables, the pipes |
| `demos/kernel/sys.fi` | 1470 | the table itself — rewritten, not extended |
| `lib/osum/libc/kcall.fi` | 145 | the door out of a process, six argument registers |
| `lib/osum/libc/errno.fi` | 105 | the numbers, and `-1` with a reason |
| `lib/osum/libc/text.fi` | 175 | what `<string.h>` is |
| `lib/osum/libc/mem.fi` | 265 | `brk`, `mmap`, and a first-fit allocator on them |
| `lib/osum/libc/io.fi` | 235 | descriptors, whole reads and writes, directories |
| `lib/osum/libc/stdio.fi` | 210 | buffered output, a line of input |
| `lib/osum/libc/proc.fi` | 175 | fork, execve, wait4 — and the three of them together |
| `demos/kernel/user/posix.fi` | 430 | the program that measures every call |
| `tools/posix/run.sh` | 480 | the guard |

and changed: `isr.s` (the entry point), `proc.fi` (fork and execve),
`sched.fi` (two new fields), `kstate.fi` (the regions), `kmain.fi`,
`elf.fi`, `fs.fi`, `user.fi`, `uprog.fi`, `crt.s`, all seven programs
under `demos/kernel/user/`, `tools/osum/mkfs.py`.

---

## 2. The table

Everything Osum implements carries the number Linux x86-64 gives it
(`arch/x86/entry/syscalls/syscall_64.tbl`). That is not politeness: stage 2
of this plan is to run a statically linked Linux binary, and such a binary
puts its number in `rax` and its fourth argument in `r10` without asking
anybody. Every number picked differently is a translation table somebody
has to write and keep right.

| nr | call | what it does here |
|---|---|---|
| 0 | `read(fd, buf, n)` | file, pipe or console; 0 = end |
| 1 | `write(fd, buf, n)` | file, pipe or console |
| 2 | `open(path, flags, mode)` | O_CREAT/O_EXCL/O_TRUNC/O_APPEND/O_DIRECTORY |
| 3 | `close(fd)` | |
| 4 | `stat(path, buf)` | `struct stat`, 144 octets, Linux layout |
| 5 | `fstat(fd, buf)` | also for pipes (S_IFIFO) and the console (S_IFCHR) |
| 8 | `lseek(fd, off, whence)` | SET/CUR/END; a pipe gives -ESPIPE |
| 9 | `mmap(...)` | anonymous only, see the deviations |
| 11 | `munmap(addr, len)` | gives the pages back |
| 12 | `brk(addr)` | 0 asks, a refusal answers with the old break |
| 22 | `pipe(fds)` | two `int`s, 512 octets of buffer |
| 24 | `sched_yield()` | |
| 32 | `dup(fd)` | |
| 33 | `dup2(from, to)` | |
| 35 | `nanosleep(req, rem)` | `struct timespec`, 10 ms grain |
| 39 | `getpid()` | |
| 57 | `fork()` | eager copy of every mapped page |
| 59 | `execve(path, argv, envp)` | the caller becomes the program |
| 60 | `exit(code)` | |
| 61 | `wait4(pid, status, opts, ru)` | returns the pid, status POSIX-shaped |
| 79 | `getcwd(buf, size)` | the real path since round K6 landed on top of this layer |
| 83 | `mkdir(path, mode)` | |
| 87 | `unlink(path)` | a directory gives -EISDIR |
| 110 | `getppid()` | |
| 217 | `getdents64(fd, dirp, n)` | `struct linux_dirent64` |
| 231 | `exit_group(code)` | the same as `exit`: one task per process |

And two Osum has of its own, at **1000 and 1001** — far above anything
Linux will ever hand out, so that stage 2 cannot collide with them:

| nr | call | why it exists |
|---|---|---|
| 1000 | `spawn(program, arg)` | the programs compiled into the kernel image (`uprog.fi`). There is no file behind them, so no `execve` can start them. |
| 1001 | `exec(path, argv, argc) -> pid` | round K1's one-call start of a program off the disk. `fork` + `execve` do the same in two calls now and cost a copy of the whole image; this one is what `/bin/sh` uses when there is nothing to redirect. |

**The numbers 1 and 2 are `write` and `open`, and they used to be the two
marks of round 59.** Those marks are still there, and they are reachable
in exactly one situation: while `user.run` has the hand written program of
`isr.s` in ring 3 (`kstate.EXCURSION` is 1 for the length of that
excursion). That program is a mode of the kernel, not a process — no
process can be running while it runs — so section 3 of
`tools/kernel/run.sh` goes on measuring what it measured in round 59, and
a program still gets `write` when it asks for 1.

---

## 3. The entry point had to change first

`demos/kernel/isr.s` did two things that made the rest impossible:

1. **It wrote the user stack pointer over `r10`.** Linux passes the fourth
   argument of a system call in `r10` (the `syscall` instruction destroys
   `rcx`, so the C convention's fourth register is not available), the
   fifth in `r8`, the sixth in `r9`. `mmap` has six. The stack pointer
   goes into a slot of its own now (`sys_rsp`), and it is safe because
   `MSR_SFMASK` masks IF for the whole system call — nothing can get
   between the store and the push that reads it back.
2. **It saved three registers.** `fork` has to give the child the register
   set of its parent, `rbx`, `rbp` and `r12..r15` included — registers a
   `call` preserves, which therefore never appear in a Firn argument.

So what `syscall_entry` saves now is a COMPLETE user context, sixteen
words on the kernel stack of the calling task, and the kernel gets its
address:

    +0   r15  +8  r14  +16 r13  +24 r12  +32 rbp  +40 rbx
    +48  r9   +56 r8   +64 r10  +72 rdx  +80 rsi  +88 rdi
    +96  rax (the number on the way in, the result on the way out)
    +104 rip  +112 rflags  +120 user rsp

Three calls are written with it and cannot be written without it:

* **`fork`** copies the frame into the data area, puts a zero in the `rax`
  of the copy, and `user_resume` (isr.s) puts it back — from there the
  child is indistinguishable from a process that has just returned from a
  system call.
* **`execve`** returns to a DIFFERENT `rip` with a DIFFERENT `rsp` in the
  SAME task: it writes both into the frame and lets the ordinary way out
  run. No second path into ring 3, no special case.
* a signal handler, when there is one, needs exactly the same thing.

---

## 4. A descriptor is not an inode any more

Round 62 kept three descriptors in the task record, and a descriptor WAS
an inode. Three things are impossible in that shape, and all three are
things Unix programs do every day: `dup2` (two descriptors, ONE file
position), `fork` (parent and child SHARE the position of an inherited
descriptor), and a pipe (which has no inode at all).

`demos/kernel/file.fi` introduces the layer Unix has had since 1975:

    task -> 16 descriptors -> the OPEN FILE TABLE (64 entries) -> inode
                                    kind, position, flags, references

`dup2` is then one line, `fork` is one loop, and a pipe is an entry whose
"inode" is a ring buffer. Entries 0, 1 and 2 are the console and belong to
nobody — a process that closes its standard output must not take the
serial port away from every other process.

---

## 5. fork and execve, without copy-on-write

`fork` copies every mapped page of the parent: 4096 octets per page, no
copy-on-write, no sharing (`proc.copy_space`). That is honest for this
system — a page fault handler that shares frames and counts references is
a round of its own — and it is why `fork` + `execve` is the expensive way
to start a program here. The cheap way (`SYS_OSUM_EXEC`) stays.

`execve` does not tear its own address space down while standing in it.
The loader builds a WHOLE NEW process for the file, the two swap address
spaces (`proc.swap_space`), the caller loads the new tables into `cr3`,
and the empty shell of the loader's process is discarded — which frees
exactly the old image, with no second copy of `free_space` anywhere. Same
pid, same parent, same descriptors, and that is what `execve` promises.

---

## 6. Deliberate deviations from Linux

Every one of these is a place where Osum answers differently on purpose.
Nothing here is an oversight; the oversights are in section 7.

1. **`nanosleep` refuses more than sixty seconds** with -EINVAL. Linux
   would sleep. A kernel that can be stopped for an hour by one number
   cannot be measured, and `tools/kernel/run.sh` has been checking that
   refusal since round 62.
2. **`mmap` maps anonymous memory only.** Without MAP_ANONYMOUS the answer
   is -ENODEV: a mapping backed by a file needs the page cache this system
   does not have, and pretending otherwise would be the kind of half-truth
   that costs a day later. The address hint is ignored, MAP_FIXED is not
   understood, and the area grows DOWNWARDS from 0x400F0000 while `brk`
   grows upwards from 0x40020000 — whichever reaches the other first is
   refused.
3. **`munmap` gives the pages back but not the address space.** The bump
   pointer does not move back; a hole in the middle of a bump allocator is
   a free list, and that is the libc's business.
4. **`getcwd` always answers `/`.** ROUND K6 CLOSED THIS: there is a
   working directory now, `chdir` is 80, and `getcwd` prints the path
   (`demos/kernel/uio.fi`). The paragraph is kept because the rest of it
   is still what this round decided. -- There is no working directory yet (see
   the gaps) and the honest answer is the root, not a lie.
5. **`wait4` ignores `rusage`** and understands only `WNOHANG` of the
   options. `WUNTRACED` and friends need job control, which needs signals.
6. **`execve` reads the environment and drops it.** Osum has no
   environment; a program that hands one over is not refused for it.
7. **The exit code in the task record is not masked.** POSIX masks a
   status to eight bits and `wait4` does exactly that — but the kernel's
   own dumps (rounds 62 and K1) print what the process really said, and a
   process that leaves with 12345 did not leave with 57.
8. **`read` and `write` move at most 4096 octets per call** and give a
   SHORT answer for more. Short answers are what the interface has always
   allowed, and every program that ignores them has a bug that shows up on
   the day a pipe fills up (`io.write_all` in the libc is the loop).
9. **`st_uid`, `st_gid` and the three time stamps are zero**, `st_dev` is
   1 and `st_nlink` is 1. There are no users in this system and no clock
   that survives a boot.
10. **The libc does not use the C names.** `malloc` is `mem.alloc`,
    `memcpy` is `mem.copy`, `strlen` is `text.length`, `printf` has no
    counterpart at all. `tools/osum/run.sh` proves that no HOST libc is
    linked into a program by looking for exactly those names in the symbol
    table — a function of our own carrying one of them would make that
    proof worthless. The same reason renamed the kernel's `do_mmap` to
    `do_map`.

---

## 7. What is still missing — the honest list

* **No working directory.** CLOSED BY ROUND K6 -- `chdir` is 80,
  `getcwd` answers with the real path, and a relative name is completed
  before the file system sees it (`sys.fetch_path`). What follows is what
  this round left behind: every path is absolute; `chdir` does not
  exist and `getcwd` answers `/`. The task record has had a `T_CWD` field
  since round 62 and nothing writes it.
* **No signals.** No `kill`, no handlers, no job control, and therefore no
  way to stop a program that does not want to stop. The saved user context
  of section 3 is exactly what a handler would need, so the shape is
  there.
* **No `openat`/`*at` family, no `fcntl`, no `ioctl`, no `poll`.** A
  modern statically linked Linux binary uses `openat` where this system
  has `open`, and that is the first thing stage 2 will run into.
* **No permissions.** `mode` is stored in `stat` and honoured by nobody;
  there are no users.
* **No threads.** `clone` does not exist; one task is one process.
* **A pipe holds 512 octets** and there are eight of them in the system. A
  writer that fills one waits four seconds and then gives up with -EAGAIN
  instead of blocking for ever — a blocked writer in a test run is a run
  that ends in a time limit and measures nothing.
* **16 descriptors per process, 64 open files in the system.** Both are
  arrays in the kernel data area, both are checked, and both are small.
* **`fork` is eager.** A process with a 40 KiB image pays 40 KiB per fork.
* **No `alarm`, no timers, no `times`.** The only clock is the tick
  counter.

---

## 8. Two things this round had to fix on the way

**The file system could not hold a program with a libc in it.** A file was
12 direct + 64 indirect blocks = 38912 octets, and `/bin/sh` built by
firnc1 was 35656 of them before this round added a line. The twelfth
direct slot of an inode is a DOUBLE INDIRECT pointer now: 11 + 64 + 64×64
blocks = 2135552 octets, in `demos/kernel/fs.fi` and in
`tools/osum/mkfs.py`, which have to agree octet for octet. Section 3 of
`tools/posix/run.sh` writes a file of 100000 octets on the host and reads
every octet of it back.

**Two rounds had picked the same page of the kernel data area.** Round K1
put the argument block of `exec` at 0x10000 and one page of file data at
0x11000; round K2 put the PCI device table at 0x10000. Both were right
about the area being free when they were written. A run that reads its PCI
bus and then starts a program off the disk would have had `exec` write
over the device table. The data area is 192 KiB now, every region of every
round is listed in one place (`demos/kernel/kstate.fi`), and the two
buffers moved out of the way.

And one that was not a fix but a trap that went off: `crt.s` said `exit`
with the number **10**, which was the number of round 62. On the day the
table changed underneath it, every program on the disk printed everything
it had and then spun in the loop under `exit` — because `exit` had become
a call the kernel does not have, and -ENOSYS does not stop anybody.

---

## 9. How it is measured

`tools/posix/run.sh`, section 56 of `./test.sh`. Seven sections, 134
measurements:

1. **The numbers are Linux's numbers** — read out of `sys.fi` and compared
   against the table above; the kernel's error list and the libc's are
   compared name by name (two copies of a list are two lists).
2. **The build** — both compilers, seven programs, no undefined symbol, no
   host libc name.
3. **The image**, and a file past the old ceiling, written on the host and
   read back octet for octet.
4. **Every call and every error.** `/bin/posix` makes each of them and
   prints `posix: <name> = <number>`; fourteen of those lines are ways to
   be wrong (a file that is not there, a descriptor that is not open, a
   pointer into the kernel, a null pointer, a buffer too short for one
   directory entry, a directory read as a file, a file opened as a
   directory, a directory opened for writing, unlinking a directory, a
   seek before the start of a file, a descriptor closed twice, an `execve`
   of nothing, a sleep of a thousand seconds, an unknown call number) and
   the kernel is still alive at the end of them.
5. **`fork` + `dup2` + `execve`** — `/bin/sh` understands `ls > /out.txt`
   since this round, and the file has to contain what the console would
   have shown. Counter-check: without the `>` the listing goes to the
   console and no file is made.
6. **Nothing leaks** — frames free before = frames free after, over a
   dozen processes, pipes and a heap; and the six counters the kernel
   keeps (`osum: syscalls=... forks=... execves=... pipes=... maps=...
   opens=...`).
7. **Both compilers measure the same numbers**, line for line.
