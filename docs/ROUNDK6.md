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
(section 58 of `./test.sh`): 83 checks, of which the transcripts of six
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
| `dup2(old, new)` | 40 | `>` `<` `>>` `2>` and every pipe stage |
| `seek(fd, off, whence)` | 41 | `>>` (seek to the end) |
| `chdir(path)` | 42 | `cd`, and a child that sees it |
| `getcwd(buf, len)` | 43 | `pwd` |
| `sysinfo(what)` | 44 | `df`, `date`, `uname` |
| `pstat(index, field)` | 45 | `ps` |
| `kill(pid, code)` | 46 | `kill` |

The numbers start at 40 and not at 26 **on purpose**: round K4 is filling
26.. with the POSIX numbers. If both rounds land, nothing has to be
renumbered.

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
* `demos/kernel/kstate.fi`: one page for the per-task state at `0x1C000`.
  The first version put it at `0x12000`, which **looked** free according
  to the region map in that file -- and is where round K2 keeps its PCI
  counters, written in `pci.fi` and never entered in the map. The shell
  then wrote its first line into a file whose inode number was a PCI
  counter, silently, because a write to a file says nothing. The region
  map in `kstate.fi` now names K2's pages as well.

## 2. The file system had to grow (OFS)

`demos/kernel/user/sh.fi` built by **firnc1** is over 50 KiB. A file in
OFS held 12 direct + 64 indirect blocks = **38912 octets**. The shell did
not fit on the disk.

So the twelfth direct block became a **double indirect** block: the octets
112..119 of an inode. A file may now hold 11 + 64 + 64*64 = 4171 blocks =
**2135552 octets**. The change is in `demos/kernel/fs.fi` (`file_block`,
`file_truncate`, one more scratch buffer `buf_i2`) and in
`tools/osum/mkfs.py`, which has to write exactly the same shape -- the
host builds the image and the kernel reads it, and a format they disagree
on is a file that is silently short.

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

Round K4 is building `pipe()`. When it lands, `run_pipeline` in `sh.fi` is
the one function that changes: two `exec`s without a `wait` between them
and a real pair of descriptors instead of the two temporary files.

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

Plus: `ps`/`df`/`date`/`uname`/`kill` against patterns (their output
cannot be written down in advance), the interactive path over the console
the way round K1 drives it, the counter-check that `>` really moves
descriptor 1 (the same line, once on the serial port and once not), the
frame count before and after every run, and the **same six transcripts out
of the userland built by firnc1**.

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
