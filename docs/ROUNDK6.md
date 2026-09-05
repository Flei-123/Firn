# Round K6 -- a userland: a shell, and the tools

Round K1 made the shell a **file**: the kernel reads `/bin/sh` off a disk,
lays its segments into a fresh address space and starts it. That round
proved the loader. It did not prove a system, and it could not: the shell
it started could do exactly one thing, start a program, and every program
it started wrote to the same serial port and read the same console,
because the kernel had one of each.

Round K6 is the round that turns those programs into a userland.

    osum$ cat /d/nums.txt | grep 1 | wc -l
    4
    osum$ sort /d/three.txt | head -n 1 > /first.txt
    osum$ cd /d ; ls ; wc -l < nums.txt
    ./ ../ three.txt dup.txt nums.txt empty.txt
    12

Everything in this file is measured in `tools/userland/run.sh`
(section 58 of `./test.sh`): 91 checks, of which the transcripts of seven
whole scripts are compared **octet for octet** against a file written by
hand -- not "it started without crashing".

---

## 1. What was missing in the kernel, and what was added

The rule of this round was: round K4 is rewriting `demos/kernel/sys.fi`
into a POSIX layer at the same time, so touch that file in as few lines as
possible. Everything the kernel had to grow therefore lies in **one new
file**, `demos/kernel/uio.fi`, and `sys.fi` gained seven dispatch lines,
one helper (`fetch_path`) and a rewritten `do_write`/`do_read` head.

| call | number | what it is for |
| --- | --- | --- |
| `dup2(old, new)` | ~~40~~ **33** | `>` `<` `>>` `2>` and every pipe stage |
| `seek(fd, off, whence)` | ~~41~~ **8** (`lseek`) | `>>` |
| `chdir(path)` | ~~42~~ **80** | `cd`, and a child that sees it |
| `getcwd(buf, len)` | ~~43~~ **79** | `pwd` |
| `sysinfo(what)` | ~~44~~ **1002** | `df`, `date`, `uname` |
| `pstat(index, field)` | ~~45~~ **1003** | `ps` |
| `kill(pid, code)` | ~~46~~ **1004** | `kill` |

**THE STRUCK-OUT NUMBERS ARE WHAT THIS ROUND PICKED, AND THEY WERE
WRONG.** The plan was that round K4 would fill "26 upwards" and 40..46
would stay free. Round K4 did something else and something better: it
took **Linux's** numbers (`read` is 0, `write` is 1, `open` is 2, `exit`
is 60), so 40..46 are `semget`, `semop`, `shmdt` and their neighbours in
the table a Linux binary will one day arrive with. Nothing collided
numerically, but three of the seven calls were a **second door onto the
same thing**: K4 has `dup2` at 33, `lseek` at 8 and `getcwd` at 79.

So when the two rounds were brought together, four of these seven were
deleted and three moved:

* `dup2` and `seek` are **gone**. Round K6 carried three standard
  descriptors and a position each in a page of its own; round K4 built a
  real **open file table** with reference counts, sixteen descriptors per
  process and real pipes (`demos/kernel/file.fi`). Two descriptor layers
  in one kernel is not a merge, it is a bug waiting for the first program
  that opens a file in one of them and writes to it in the other. Half of
  `demos/kernel/uio.fi` was deleted and the callers go to `file.fi`.
* `chdir` and `getcwd` are **80 and 79** -- Linux's numbers, and round
  K4's `getcwd` stub (which answered `/` and said so in its own comment)
  was replaced with the one that prints the real path.
* `sysinfo`, `pstat` and `kill` are **1002, 1003 and 1004**, by the rule
  round K4 wrote down: what Osum has and Linux does not gets a number
  above 1000. `kill` in particular is **not** Linux's 62, because there
  are no signals here -- the second argument is the exit code the process
  is made to leave with. The day signals exist, 62 becomes the real
  `kill`.

What did **not** have to change is every program: the names
`ulib.SYS_OPEN`, `ulib.SYS_WRITE` and the rest are still the names, and
`demos/kernel/user/ulib.fi` is where they mean round K4's numbers. Where
the **shape** of a call changed and not only its number there is a
function in `ulib` instead of a constant -- `size_of` and `is_dir` for
`stat`, `wait_code` for `wait4`, `sleep_ms` for `nanosleep`, `O_MAKE` for
the flags of a `>`.

Three things in `uio.fi` are worth naming, because they are the whole
difference between "a program" and "a system":

1. **The three standard descriptors are state, not a constant.** Every
   task carries three handles of its own (0 = the console, otherwise an
   inode) plus a position each. `do_write` asks `res_inode(fd)` first;
   only a handle of 0 goes to the serial port. `close` on one of the three
   does not take it away -- it gives it back to the console, which is how
   a shell gets its output back after a redirection.
2. **`exec` hands them to the child.** `elf.spawn` calls
   `uio.adopt(current, child)` in the line before `sched.start`, inside
   the interrupt-off region. That is what makes `ls > /out` work: the
   shell redirects **itself**, and the child is born into it. One line
   later and the child could have run before it was given its output --
   the first version of this round had exactly that race.
3. **A working directory that the file system sees.** A path out of ring 3
   goes through `fetch_path` instead of `fetch_name`, and that is one line
   more: a name that does not start with a slash is completed with the
   task's working directory before `fs.path` ever sees it. `cd /bin ; ls`
   therefore lists `/bin` and not the root, and `.` and `..` are folded
   away textually so that `pwd` has something to print.

Two more changes were needed and are **not** in `uio.fi`:

* `demos/kernel/kbd.fi`: scan code 0x48 (the up arrow, which arrives as
  `E0 48`) now translates to 14. It is the one key a line editor needs and
  the table had no room for.
* `demos/kernel/kstate.fi`: one page for the per-task state, at
  `0x29000`. The first version put it at `0x12000`, which **looked** free
  according to the region map in that file -- and is where round K2 keeps
  its PCI counters, written in `pci.fi` and never entered in the map. The
  shell then wrote its first line into a file whose inode number was a PCI
  counter, silently, because a write to a file says nothing. It then moved
  to `0x1C000`, and **that page belongs to round K5**: the scalars of
  `smp.fi`, picked in exactly the same way, by reading a map that stopped
  at `0x0F000`. Three rounds made the same mistake in the same file. The
  map in `kstate.fi` now names every page of every round -- K2's, K4's,
  K5's and this one -- and the page went to `0x29000`, behind the last
  thing round K4 placed.

## 2. The file system had to grow (OFS) -- and round K4 grew it too

`demos/kernel/user/sh.fi` built by **firnc1** is over 50 KiB. A file in
OFS held 12 direct + 64 indirect blocks = **38912 octets**. The shell did
not fit on the disk.

So the twelfth direct block became a **double indirect** block.

**ROUND K4 DID THE SAME THING AT THE SAME TIME, AND THE TWO DISAGREED BY
EIGHT OCTETS.** Both rounds turned the twelfth direct slot into a double
indirect block for the same reason (a program that carries a libc does not
fit under 38912 octets), and both were right about the shape -- but round
K6 put the double indirect pointer at **112** and the single one at 120,
and round K4 put them the other way round. Two file systems that disagree
about which of two words is which read every big file as rubbish. Round
K4's layout won, because it was already on `main`, already written into
`tools/osum/mkfs.py` and already measured by `tools/posix/run.sh`. This
round's implementation in `fs.fi` and `mkfs.py` was dropped whole; the
description below is what round K4 built, and the numbers are the same: A file may now hold 11 + 64 + 64*64 = 4171 blocks =
**2135552 octets**. The change is in `demos/kernel/fs.fi` (`file_block`,
`file_truncate`, one more scratch buffer `buf_i2`) and in
`tools/osum/mkfs.py`, which has to write exactly the same shape -- the
host builds the image and the kernel reads it, and a format they disagree
on is a file that is silently short. The size check of round 62
(`tools/kernel/run.sh`, section 15) moved with it: 49152 octets now go
into one file, and the ceiling is measured where it is now -- a write past
2135552 octets places nothing and does not grow the file.

The ceiling above that one is the **block bitmap**: it is one block, 4096
bits, so an OFS volume stops at 4096 blocks = 2 MiB. The whole userland
built by firnc1 is 1115152 octets, so it fits with room to spare;
`kmain.OSUM_BLOCKS` is that 4096 and the number is checked in the test.

## 3. The shell

`demos/kernel/user/sh.fi`, and it does:

* **words, quoting and variables**: `'a b'` and `"a b"` are one argument,
  `$NAME`, `$?`, `$!`, `NAME=value`, and `"$X y"` expands inside the
  quotes while `'$X'` does not.
* **redirection**: `> file`, `>> file`, `< file`, `2> file`. `>` truncates
  by unlinking first (this file system has no truncate), `>>` seeks to the
  end. Because the shell redirects **itself** and `exec` inherits, this
  works for the built-in commands too, without a line of code for it.
* **pipes**: `a | b | c`, up to four stages.
* **background**: `cmd &` prints `[1] <pid>`, `$!` is that pid, `wait`
  picks the corpses up and `exit` waits for whatever is left. A shell that
  walked away from a background job would leak a page table and a kernel
  stack per job -- the frame count in the test would see it.
* **built-ins**: `cd`, `pwd`, `export`, `exit`, `wait`. Exactly the ones
  that **cannot** be programs: a child cannot change its parent's working
  directory.
* **a line editor**: 8 takes the last octet back, 14 (the up arrow)
  fetches the line before this one. The kernel already eats the backspace
  when the input is a real keyboard; everything that comes out of a
  **file** arrives at the shell untouched, which is how the editor is
  measured without a person in front of the machine (case `t5`).
* **scripts**: `sh /t/case.sh` reads its commands out of a file. A shell
  with a script is **quiet** -- no banner, no prompt, no exit code per
  command. The chatter of round K1 is what an *interactive* shell does,
  and `tools/osum/run.sh` still measures exactly that, unchanged.

### Pipes are staged through a file -- say it out loud

There is no `pipe()` in this kernel. `a | b` therefore runs as

    a > /tmp/sh0 ; b < /tmp/sh0

The output is the same and the exit code is the last stage's, but the two
ends **do not run at the same time**, a stage that never ends fills the
disk instead of blocking its writer, and an infinite producer
(`yes | head`) would never finish. `/tmp` is made when the first pipe of a
run needs it and not before, so an image without a pipe in it looks
exactly the way round K1 left it.

Round K4 **has** built `pipe()` -- `SYS_PIPE` is 22, the kernel has eight
pipes of 512 octets with reference counts on both ends, and
`tools/posix/run.sh` measures a program pushing octets through one. The
shell does **not** use it yet, and that is the largest thing still open
after the merge: `run_pipeline` in `sh.fi` is the one function that has to
change, and it needs two `exec`s without a `wait` between them, which is a
different control flow and not a different call. Until then a pipe is two
temporary files, with everything that follows from it (see section 7).

## 4. The tools

Twenty-three programs, each one a real ELF file in `/bin`, each one with
no undefined symbol at all -- no libc, no runtime, no kernel to fall back
on. What they share is `demos/kernel/user/ulib.fi`, which is linked into
every one of them.

| program | what it does, and what it does not |
| --- | --- |
| `ls` | short form as in round K1, `-l` adds kind and size. No argument means **this** directory. |
| `cat` | files, or the input when there is no file -- which is what makes it a pipe stage |
| `echo` | `-n` leaves the newline out |
| `cp` | `cp a dir` writes `dir/a` |
| `mv` | **a copy and an unlink**: this kernel has no `rename` |
| `rm` | `-f`; a directory is refused |
| `mkdir`, `rmdir` | `rmdir` only removes an empty one, and that refusal is in the file system |
| `touch` | there is no clock on a file here, so it only creates |
| `head`, `tail` | `-n N`; `tail` keeps at most 16 lines in a ring |
| `wc` | `-l`, `-w`, `-c`; with one flag it prints one number, so `wc -l` in a pipe is a number |
| `grep` | `-v`, `-c`; **substring, not a regular expression** |
| `sort` | `-r`; 16 KiB and 256 lines, and more is refused with an exit code |
| `uniq` | `-c`; adjacent lines only |
| `true`, `false` | 0 and 1, out of a file on the disk |
| `sleep` | `sleep 1` is a second, `sleep -m 50` fifty milliseconds |
| `ps` | the task table over `SYS_PSTAT`, eight calls per task and no buffer |
| `kill` | there are no signals: the task becomes a corpse with the given code (137 by default) |
| `uname` | `-a`; the round comes out of the **kernel**, not out of the program |
| `date` | the CMOS clock over ports 0x70/0x71; `-u` is the uptime |
| `df` | blocks and inodes, every number out of the kernel |

**The exit codes changed.** Round K1 let `ls` return the number of
entries, `cat` the number of octets and `echo` the number of words -- so
that the shell had something to show. A userland cannot work that way:
`grep` says "found" with 0, `false` says 1, and `$?` has to mean the same
thing everywhere. `ls`, `cat`, `echo` and `rm` now return 0 or 1, and the
four lines in `tools/osum/run.sh` that measured the old numbers were
changed with them.

## 5. What is measured (tools/userland/run.sh)

Six scripts run in the shell that came off the disk, and their whole
transcript is compared with `diff`:

* **t1** the tools: sort, sort -r, wc, head, tail, grep, grep -v, grep -c,
  uniq, uniq -c, cat, echo -- 27 lines, exact.
* **t2** redirection and pipes: `>`, `>>`, `<`, `2>`, three-stage pipes,
  `echo -n` and the octet count that follows from it.
* **t3** the shell: `cd` into a directory and a **child** program that
  sees it, `pwd`, `..`, a `cd` that fails and its `$?`, variables,
  quoting, `true`/`false` and their codes, `export`.
* **t4** the failures: a file that is not there, a directory where a file
  was asked for, an empty input through four programs, `rmdir` on a
  directory that is not empty, `mkdir` on one that exists, a command that
  does not exist (`-ENOENT` and `$? = 127`).
* **t5** the line editor: `echo fooX\bbar` -> `foobar`, and a line that is
  only the recall octet repeats the line before it.
* **t6** the disk: `mkdir`, `touch`, `cp`, `mv`, `rm`, `rmdir` -- and
  afterwards the **host** reads the image back and finds exactly what the
  userland left there.
* **t8** everything a line can be that is not a command: a comment, a
  lone `;`, `>` without a name, `cd` without an argument, ten arguments
  where eight fit, an argument of two hundred octets (`-ENAMETOOLONG` out
  of the kernel, and the shell says so and lives), five pipe stages where
  four fit, and `sh /nope`.

Plus: `ps`/`df`/`date`/`uname`/`kill` against patterns (their output
cannot be written down in advance), the interactive path over the console
the way round K1 drives it, the counter-check that `>` really moves
descriptor 1 (the same line, once on the serial port and once not), the
frame count before and after every run, the **same four transcripts out of
the userland built by firnc1** -- and the line editor a second time on a
REAL keyboard: five keys through the QEMU monitor (`l`, `s`, return, **up
arrow**, return) produce two listings, which is the whole path from scan
code 0x48 through `kbd.translate` and `sys.keyboard_read` into the
shell.

## 6. What is missing, honestly

* **`pipe()`** -- see above. The pipes are staged through a file.
* **`rename`** -- `mv` copies. Two lines in `fs.fi` and a call number.
* **an environment** -- `exec` has no `envp`, so `export` marks a variable
  and the mark reaches no child. The word is there so that the round which
  adds `execve` has somewhere to hang it.
* **`PATH`** -- one directory, `/bin`, and it is named in the shell.
* **no `2>>`, no `&&`, no `||`, no `if`/`for`, no globbing, no
  substitution `$(...)`** -- a shell language is a round of its own.
* **at most 8 arguments** per command, because `proc.MAX_ARGS` is 8, and
  at most 3 open files per process (`sched.FD_SLOTS`), which is why the
  shell needs one for the script and one for the redirection and has none
  left over.
* **`ls -l` shows kind and size and nothing else**, because there is
  nothing else: OFS has no owner, no rights and no time on a file.
* **the shell's own output cannot be redirected while it runs a script**
  with redirections in it: `close` gives a standard descriptor back to the
  console, not to whatever it was before.

---

## 7. What the merge with rounds K4 and K5 left doubled

Round K6 was written against the kernel of round K1 while rounds K4 (the
POSIX layer and a libc) and K5 (the second processor) were being written
against the same files. The list above says what was decided. This one
says what is **still** doubled or half-done, so that nobody has to find it
by tripping over it.

1. **`ulib.fi` is a second, smaller libc.** `lib/osum/libc/` has `text`,
   `io`, `mem`, `stdio`, `proc`; `demos/kernel/user/ulib.fi` has
   `strcpy`, `strncpy`, `strcat`, `strneq`, `find`, `cmp`, `to_num`,
   `pad`, `base_name` and a line reader, and those are **not** in the
   libc. The output and the descriptors do go through the libc
   (`io.write_text`, `io.read`, `io.stat_mode`), so there is one door to
   the kernel and not two -- but the string half belongs in
   `lib/osum/libc/text.fi` and is not there. Moving it is a round of its
   own, and it is the round that also gives the libc a `strtoul` and a
   `qsort`.
2. **`ulib` deliberately does not import `osum.libc.proc`.** That module
   flushes the buffered output before it forks, so it imports `stdio`,
   which imports the **allocator** -- and every one of the twenty-six
   programs then carries a heap it never uses. Measured: 41392 octets per
   program and 2235368 for the whole userland out of firnc1, against a
   drive that holds 2097152. `ulib.wait_code` and `ulib.sleep_ms`
   therefore build their own `status` word and their own `timespec` on
   the caller's stack and go straight to `kcall`. The real fix is
   dead-code elimination across modules in the linker, or a libc that is
   split finer; a program that wants buffered output imports
   `osum.libc.stdio` itself, and `/bin/posix` does.
3. **The pipes of the shell are still two temporary files.** `pipe()`
   exists in the kernel (section 3). Consequences that are real today:
   the two stages do not run at the same time, `yes | head` would never
   finish, and a stage that produces without end fills the disk instead
   of blocking its writer.
4. **`close` on descriptor 0, 1 or 2 is an ordinary `close` again.** In
   the kernel of this round it gave the descriptor back to the console,
   and the shell used that to take its output back after a redirection.
   Round K4's open file table frees the slot instead, and the next `open`
   gets it. The shell now does what every Unix shell has done since 1979:
   it keeps a **copy** of its own three (`dup`) and points them back with
   `dup2`. The copies are not handed to a child -- `elf.spawn` inherits
   the three standard descriptors and nothing else
   (`file.inherit_std`), which is also why the first `open` in a program
   is 3 and not 6.
5. **`exec` has no `envp`.** Unchanged: `export` marks a variable and the
   mark reaches no child. Round K4's `execve` takes `argv` and a zero.
6. **`rename` is still missing**, so `mv` copies and unlinks.
7. **`ps` reads the task table of the FIRST processor's scheduler.** Round
   K5 added a second core and a per-core record; `uio.pstat` reads
   `sched.T_*`, which is the one table both cores share, so the numbers
   are right -- but there is no column that says which core a task ran
   on, and `kstate.TCPU_OFF` has one.
