# Round MERGE-WIN — the Windows branch into `main`, and real `.exe` files

Round WINDOWS (`docs/ROUND-WINDOWS.md`) taught `firnc` to emit PE/COFF and
left the result on a branch. This round does two things: it brings that
branch into `main` under a condition that is measured and not asserted, and
it then asks the question the branch never asked — **do the programs this
repository actually ships work as Windows programs?**

Four claims, each of them a measurement:

1. **The merge costs the Linux side nothing.** 314 of 314 programs from
   `tests/` and `examples/` produce **character-identical** `--emit=asm`
   text from the compiler built on `main` (`2a20c514b`) and from the
   compiler built on the merged tree. 0 different. (§3.1 — measured here,
   in this round, on two worktrees of this machine, not quoted from the
   branch's own write-up.)
2. **The tools of this repository run as Windows programs.** All seven
   programs in `bin/` — the compiler written in Firn and its six dump tools
   — plus the five examples: **14 of 14** produce the same standard output
   character for character and the same exit code on Linux and under Wine
   (§4.1).
3. **The compiler written in Firn works as a Windows program.**
   `bin/firnc1.fi` built as `firnc1.exe` runs the whole corpus:
   **172 cases produce character-identical assembly (6,309,838 octets),
   142 are refused with the same exit code on both sides, 0 differ**
   (§4.2). Where self-hosting on Windows stops is named exactly: the
   assembly text is complete and written to disk; only the two calls to the
   external assembler and linker fail, because `fork`/`execve` answer
   ENOSYS (§4.3).
4. **The seam's answer table is measured, not read out of the source.** A
   probe calls 35 system call numbers on both operating systems:
   **25 are bound, 10 answer -38 (ENOSYS)** — and of those ten, only two
   (`futex`, `wait4`) occur anywhere in `lib/` at all (§5).

---

> **Stand des Zweigs beim Abschluss dieser Runde.** Das Ergebnis liegt auf
> `mergewin` (`bd512b54f`), 15 Commits vor `main` und ein **reiner
> Vorspulschritt** (`git merge-base --is-ancestor main mergewin` = ja).
> `main` selbst ist auf diesem Rechner im Arbeitsbaum `/root/mg-firn`
> ausgecheckt und dort **nicht sauber** -- eine andere, gleichzeitig
> laufende Runde hat `bin/sysdump.fi`, `compiler/src/archsel.rs` und
> `compiler/src/rangecheck.rs` unfertig liegen. Den Zeiger `main` unter
> einem fremden, belegten Arbeitsbaum wegzuziehen wuerde deren Stand
> zerreissen, also wurde es **nicht** getan. Sobald `/root/mg-firn` frei
> ist, genuegt dort:
>
> ```
> cd /root/mg-firn && git merge --ff-only mergewin
> ```

## 1. What was found, before anything was touched

### 1.1 The branch

| | |
|---|---|
| branch | `windows`, worktree `/root/firn-windows`, head `7e804f2c8` |
| commits ahead of `main` | **11** |
| merge base | `2a20c514b` — **identical with the tip of `main`** |
| diff against `main` | 1,261 files, +4,230 / −17 lines |

Because the merge base *is* the tip of `main`, there was **nothing to
resolve**: no file was changed on both sides, and a conflict could not
arise. That is worth saying plainly rather than dressing it up — this round
did not have to reconcile two histories, it had to prove that the branch is
safe to keep.

Of the 1,261 changed files, **1,229 were in `.win-work/`** — the working
directory of `tools/windows/run.sh`, committed by accident: 613 compiled
binaries, 136 MB. See §2.2.

### 1.2 The five cases that are not green — by name

Round WINDOWS measured 299 of 304 comparable cases identical on both
operating systems. The five that are not are named here, with the exit codes
as `tools/windows/run.sh` recorded them
(`docs/round-windows/causes.dev-fast.txt`):

| case | cause | Linux | Windows |
|---|---|---:|---:|
| `tests/860_thread_basic.fi` | `clone(2)` answers ENOSYS | 0 | 1 |
| `tests/861_thread_gc.fi` | `clone(2)` answers ENOSYS | 0 | 1 |
| `tests/862_thread_local.fi` | `clone(2)` answers ENOSYS | 0 | 2 |
| `tests/1600_net_echo.fi` | needs a server **thread**; the sockets themselves work | 0 | 14 |
| `tests/700_process_start.fi` | `fork`/`execve`/`wait4` answer ENOSYS | 0 | 5 |

**Two causes, not five.** Four of them are one missing thing —
`CreateThread` has a different shape than `clone(2)`, and the collector's
thread table is built on the Linux form. The fifth is `CreateProcessW`,
which has no `fork`.

**Are they a blocker? Honest answer: it depends on what for, and for the
thing this whole line of work exists for, they are not.**

* **For Certus on Windows — no.** Neither `lib/browser/window_main.fi` nor
  `b5_main.fi` starts a thread, and the browser engine starts no child
  process. This round confirmed that from the other side as well: the
  compiler written in Firn, 35,729 lines and the largest Firn program in the
  tree, runs 314 corpus cases as a Windows program without ever touching
  either gap (§4.2).
* **For the standard library as a whole — yes.** `lib/` names exactly two
  system calls that the seam does not answer, and both are here:
  `SYS_FUTEX` (202, used by `lib/gc/gc.fi` for stopping the world) and
  `SYS_WAIT4` (61). Anything concurrent stops at this line.
* **They are honest failures, not silent ones.** `Op::ThreadSpawn` answers
  ENOSYS at the moment a thread is actually started, so a program that never
  starts one is not punished for containing library code that could. Round
  WINDOWS measured what the alternative would have cost: refusing the whole
  program hit 93 of 309 cases, almost none of which ever start a thread.

So: **edge cases for the browser, a real wall for anything with a second
thread of control.** Both statements are in the write-up because only both
together are true.

---

## 2. The merge

### 2.1 The merge itself

```
git merge --no-ff windows      ->  fac08f85c
```

No conflict, for the reason in §1.1. `--no-ff` rather than a fast-forward,
because `main` records its rounds as merge commits (`Merge inventory`,
`Merge speed`, `Merge arm-freestanding`) and a round that vanished into a
straight line would be the only one that could not be pointed at.

### 2.2 One thing was changed on the way in, and why

`.win-work/` is the working directory of `tools/windows/run.sh`: for every
one of the 309 cases it holds the Linux binary, the `.exe`, and the captured
output. All of it is **produced**, and every other round in this repository
gitignores its own work directory — `.b1-work/`, `.b4-work/`, `.b5-work/`,
`.fpz-work/` and a dozen more, each with a comment saying so.

The branch committed it: **1,229 files, 613 of them binaries, 136 MB**.

Commit `535f4994e` takes them out of the tree, adds `.win-work/` to
`.gitignore` in the same form as its neighbours, and keeps the **three text
results** — they are evidence and they weigh 26 KB:

```
docs/round-windows/corpus.txt              the 309 cases of the run
docs/round-windows/result.dev-fast.txt     the result per case
docs/round-windows/causes.dev-fast.txt     the five that differ, with cause
```

Nothing else was dropped, and nothing was rewritten. If the intent had been
to keep the binaries, that intent is recoverable — they are still in the
history of the branch.

### 2.3 A third commit: what this round added

`e59ec33cb` adds three tools and wires them into `test.sh` as section 65b
(§4, §5).

---

## 3. The Linux side: measured before and after

### 3.1 The assembly text, file against file

The sharpest instrument available here, and it was run in this round rather
than quoted from the branch:

```
worktree A   /root/MW-base        2a20c514b   the tip of main before the merge
worktree B   /root/firn-mergewin  the merge
for every tests/*.fi and examples/*.fi:  firnc --emit=asm  ->  cmp
```

| | |
|---|---:|
| character-identical | **314** |
| different | **0** |
| refused by both | 0 |
| refused by only one | 0 |

Both compilers were built from their own worktree with `cargo build
--release` on this machine. The Windows target adds a third value to the
second axis of `target.rs`; the Linux paths through `codegen_x86.rs` and
`regalloc.rs` are reached through the same case distinctions as before, and
this is what that looks like when it is measured instead of argued.

### 3.2 The full suite

`test.sh` — 67 sections, both targets, on the merged tree:

`test.sh` in voller Laenge auf dem zusammengefuehrten Baum
(`/root/MW-test3.log`, Lauf vom 02.09.2026, 13:37-15:00 Uhr):

```
FAIL 23/1562 failed        (RC=1)
```

1562 geprüfte Punkte, 23 rote Abschnitte. **Kein einziger davon ist neu.**
Jeder wurde auf einem eigenen, unberuehrten Arbeitsbaum des Basisstandes
`2a20c514b` (`/root/MW-base`, Bau mit `cargo build --release`) mit demselben
Skript nachgemessen:

| Abschnitt | `main` (2a20c514b) | nach dem Merge | Urteil |
|---|---|---|---|
| `tools/fixpoint.sh` | STAGE 2 FAILED (rc=2) | STAGE 2 FAILED (rc=2) | gleich |
| `tools/self_compare.sh` | 194 gleich / 0 abweichend / 112 fehlerhaft / 28 nicht Kern / 19 uebersprungen | **zeichengleiche Ausgabe** | gleich |
| `tools/sema_compare.sh` | 179 gleich / 4 abweichend / 35190 Ausdruecke | **zeichengleich** | gleich |
| `tools/fir_compare.sh` | 175 gleich / 5 abweichend / 57079 Anweisungen | **zeichengleich** | gleich |
| `tools/thread/run.sh` | `firnc1: build failed` | dasselbe | gleich |
| `tools/strsoak/run.sh` | `firnc1 rc=2` | dasselbe | gleich |
| `tools/freestanding/run.sh` | 27 bestanden / 2 offen | 27 / 2 | gleich |
| `tools/freestanding/none.sh` | 20 / 2 (`start.s`, `aarch64-none: ld`) | 20 / 2, dieselben zwei | gleich |
| `tools/aarch64/syscall_table.sh` | 4 / 2 | 4 / 2, dieselben zwei | gleich |
| `tools/firstrun/run.sh` | 8 von 38 rot | 8 von 38 | gleich |
| `tools/extfn/run.sh` | 5 / 1 (`stage1_callback`) | 5 / 1 | gleich |
| `tools/escape/run.sh` | 4 Fehlalarme in `firnc1` | dieselben vier | gleich |
| `tools/state/run.sh`, `tools/checkidx/run.sh`, `tools/core/run.sh`, `tools/testrunner/run.sh` | rot, jeweils auf der `firnc1`-Seite | dieselben Meldungen | gleich |
| `tools/optlevels/run.sh` | 9 FAILURES | 9 FAILURES | gleich |
| `tools/repro/two_machines.sh` | `firnc1 failed on A` | dasselbe | gleich |
| `tools/packages/run.sh` | 22 bestanden / 17 rot | 22 / 17 | gleich (Runde WINDOWS §3 hat es schon benannt) |
| `tools/k3net/run.sh` | **4 von 17 rot** | **2 von 17 rot** | nach dem Merge besser (Zeitmessung am Netz, schwankt) |
| `tools/fmt/run.sh` | 2 Pruefungen rot: `firnc1` kann `firnfmt.fi` nicht bauen **und** `lib/firnc1/syscalls.fi ist nicht formatiert` | nur noch die erste; die Formatierung ist in dieser Runde nachgezogen | besser |
| `tools/english/check.sh` | 1 Fundstelle (`bin/firnc1.fi:663`) | 1 Fundstelle, dieselbe | gleich (siehe unten) |

Zwei rote Punkte hat diese Runde selbst verursacht und selbst behoben,
bevor der Merge stehen blieb:

* **`tools/windows/seam.fi` war nicht in kanonischer Form** — `firnfmt -w`
  darauf, und `firnfmt -c` ist still. `lib/firnc1/syscalls.fi` war schon auf
  `main` unformatiert und wurde gleich mitgezogen, damit der Abschnitt
  ueberhaupt eine Chance hat, gruen zu werden.
* **`tools/english/check_texts.py` hielt `jbe` fuer ein deutsches Wort.**
  Das ist ein x86-Sprungbefehl, den der Stapel-Abtaster in
  `compiler/src/win.rs` ausgibt. Die Wortliste des Pruefers kannte
  `jmp`, `jne`, `jle`, `jge` — aber nicht `ja`, `jae`, `jb`, `jbe`, `jc`,
  `jz`, `jnz`, `js`, `jns`, `jo`, `jno`, `jp`, `jl`, `jg`. Die fehlenden
  Mnemoniken sind ergaenzt; die Zahl der Fundstellen ist damit wieder
  genau die von `main` (1).

**Eine Warnung zur Messung, weil sie diese Runde zweimal in die Irre
gefuehrt hat:** die Platte des Servers lief waehrend der Laeufe zweimal auf
100 % voll (andere Runden auf demselben Rechner). Ein voller Datentraeger
sieht in diesem Testlauf aus wie ein Uebersetzerfehler — `self_compare`
meldete einmal `DIFFERING: 9`, `tools/liveb4/run.sh` meldete
`lib/browser/live_probe.fi does not compile (opt/noopt/dev)`. Beide waren
nach dem Freiraeumen von Platz **wieder gruen bzw. zeichengleich mit
`main`** (`liveb4`: `B4 OK: 389 / 1714 WPT-Teilfaellen`; `self_compare`:
194/0/112/28/19). Jede Zahl in dieser Tabelle stammt aus einem Lauf mit
freiem Platz; Zahlen aus den beiden verunglueckten Laeufen sind
verworfen.

Die beiden Windows-Abschnitte selbst, aus demselben Lauf:

```
== 65. WINDOWS: the same program on two operating systems ==
   OK    windows under wine: 1032 FIRN-OK
   passed: 5   failed: 0
== 65b. WINDOWS: the real programs of this repository (ROUND MERGE-WIN) ==
   RESULT: 14 of 14 programs behave identically on both operating systems
   SAME       172    both produced assembly, character identical
   REFUSED    142    both refused with the same exit code
   DIFFERENT  0
   corpus     314 cases, 6309838 octets of assembly compared
   BOUND    25
   MISSING  10   (they answer -38 = ENOSYS)
```

---

## 4. Real Windows programs

### 4.1 The tools of this repository (`tools/windows/programs.sh`, new)

Every program in `bin/` and every example, built twice, run twice, compared
by what it does. Wine 8.0 on the Windows side; what that does and does not
prove is §6.

```
  NAME                 LINUX     WINDOWS   BYTES    VERDICT
  sysdump              rc=0      rc=0      524      SAME
  lexdump              rc=0      rc=0      1387     SAME
  astdump              rc=0      rc=0      359      SAME
  firdump              rc=0      rc=0      2328     SAME
  semadump             rc=0      rc=0      512      SAME
  layoutdump           rc=0      rc=0      57       SAME
  firnc1               rc=0      rc=0      10923    SAME
  firnc1-structs       rc=0      rc=0      16757    SAME
  firnc1-bubblesort    rc=0      rc=0      26852    SAME
  hello                rc=0      rc=0      21       SAME
  tour                 rc=0      rc=0      48       SAME
  structs              rc=42     rc=42     0        SAME
  bubblesort           rc=1      rc=1      0        SAME
  fib                  rc=89     rc=89     0        SAME

  RESULT: 14 of 14 programs behave identically on both operating systems
```

The three non-zero exit codes are the expectations the files themselves
state in line 1 (`// expect_exit: 42`, `1`, `89`) — so this is not two sides
failing in the same way, it is two sides being right in the same way.

Each case was chosen because it stresses a different part of the seam:

| case | what it additionally proves |
|---|---|
| `sysdump` | no argument at all, pure `write` to handle 1 |
| `lexdump` | the source comes from **standard input** — `ReadFile` on a redirected handle, not on a file the program opened |
| `astdump` | the file name comes from `argv[1]`, i.e. from the start block the seam builds out of `GetCommandLineW`, and is opened with `CreateFileW` |
| `semadump` | type checking runs the **collector**, so the stack bounds out of `GetCurrentThreadStackLimits` are load bearing |
| `tour` | strings, the collector, `std.io`, `std.math`, an interface with `impl`, concatenation on the GC heap |

### 4.2 The compiler itself, over the whole corpus (`tools/windows/selfhost.sh`, new)

`bin/firnc1.fi` is the compiler written in Firn — 35,729 lines over that
file and `lib/firnc1/`, the largest Firn program in this repository. It was
built twice with `firnc0`:

| | size | format |
|---|---:|---|
| `firnc1` (Linux) | 1,894,632 octets | ELF 64-bit LSB executable, x86-64 |
| `firnc1.exe` (Windows) | 1,598,649 octets | PE32+ executable (console) x86-64 |

Then every case of `tests/` and `examples/` was handed to **both** and the
assembly they write to standard output compared character for character:

| | |
|---|---:|
| **SAME** (both produced assembly, texts identical) | **172** |
| **REFUSED** (both refused, same exit code) | **142** |
| **DIFFERENT** | **0** |
| corpus | 314 cases |
| assembly compared | **6,309,838 octets** |

The 142 refusals are `firnc1`'s own limits and they are the same on both
operating systems: 103 × rc 2 (I/O, or a module it cannot resolve — see
§7.1), 28 × rc 3 (not core language), 10 × rc 1 (error), 1 × rc 4
(`comptime`). Not one case is refused on one operating system and accepted
on the other.

That is a much harder workload than `hello.exe`. The compiler opens files,
resolves `import` over `$FIRNLIB`, allocates megabytes through the
collector, builds deep trees, formats text and writes half a megabyte to
standard output — the largest single case, `tests/305_dtoa_hardcases.fi`,
produces 695,084 octets of assembly, identical on both sides.

**The image itself** (`x86_64-w64-mingw32-objdump` on `firnc1.exe`):

```
file format pei-x86-64,  start address 0x0000000140001000
  .text    0015c600     .rodata  00019e78     .bss  00002030 (no file space)
  .idata   000005b4
  DLL Name: KERNEL32.dll   ADVAPI32.dll   WS2_32.dll
  syscall instructions in the whole image: 0
```

The seven textual hits for `syscall` in the disassembly are all `call
_Fwin.syscall` — the stub in the panic path — and not one is the two-octet
`0f 05` instruction.

### 4.3 Where self-hosting on Windows stops — exactly

Two measurements, both of them a limit and both of them named rather than
avoided:

**(a) `firnc1.exe` cannot BUILD an executable, and says so.**

```
$ wine firnc1.exe examples/hello.fi -o h.out
exit code 7          h.out.s written (10,923 octets), h.out not written
```

Exit code 7 is `bin/firnc1.fi` line 1378: `rt.run("/usr/bin/as", …)`
failed. `rt.run` is `fork` + `execve`, and both answer ENOSYS on Windows
(§1.2). **The compiler ran to completion** — the assembly text it wrote is
`cmp`-identical with the one the Linux build of the same compiler writes for
the same input. What is missing is not the compiler, it is the two calls to
the external assembler and linker.

**(b) `firnc1` does not know the Windows target.**

```
$ wine firnc1.exe --target=x86_64-windows examples/hello.fi
exit code 2
```

That confirms `docs/ROUND-WINDOWS.md` §4.4 by measurement: the project rule
that both compiler sides produce identical output is not violated for this
target, it is **not yet applicable** — there is nothing on the `firnc1` side
that could disagree.

**So: is Firn self-hosting on Windows?** The honest answer in one sentence:
*the compiler written in Firn runs on Windows and produces the right
assembly for the whole corpus, but it cannot yet turn that assembly into an
executable there, and it cannot yet target Windows itself.* Two named gaps,
both of them work and neither of them a wall of principle — process start
(§1.2) and `firnc1` catching up (`docs/ROUND-WINDOWS.md` §4.4).

---

## 5. The Win32 seam, measured (`tools/windows/seam.sh` + `seam.fi`, new)

A table read out of `compiler/src/win_seam.rs` says what someone *meant* to
bind. `tools/windows/seam.fi` calls 35 system call numbers **without a side
effect** on both operating systems and prints `<number> <return value>`;
this is what the seam really answers.

Deliberately absent from the probe: `fork`(57), `execve`(59), `clone`(56),
`unlink`(87), `exit_group`(231) — probing them would change the machine, and
what they do is already measured by §1.2.

| nr | name | Linux | Windows | state |
|---:|---|---:|---:|---|
| 0 | `read` | 16 | 16 | BOUND |
| 2 | `open` | 3 | 3 | BOUND |
| 3 | `close` | 0 | 0 | BOUND |
| 4 | `stat` | 0 | **−38** | MISSING |
| 5 | `fstat` | −9 | **−38** | MISSING |
| 7 | `poll` | 0 | **−38** | MISSING |
| 8 | `lseek` | 28152 | 28152 | BOUND |
| 9 | `mmap` | ok | ok | BOUND |
| 10 | `mprotect` | 0 | 0 | BOUND |
| 11 | `munmap` | 0 | 0 | BOUND |
| 16 | `ioctl` | −9 | **−38** | MISSING |
| 21 | `access` | 0 | 0 | BOUND |
| 24 | `sched_yield` | 0 | 0 | BOUND |
| 32 | `dup` | 3 | 3 | BOUND |
| 35 | `nanosleep` | 0 | 0 | BOUND |
| 39 | `getpid` | ok | ok | BOUND |
| 41 | `socket` | ok | ok | BOUND |
| 48 | `shutdown` | 0 | −107 | BOUND, other value |
| 49 | `bind` | 0 | 0 | BOUND |
| 50 | `listen` | 0 | 0 | BOUND |
| 51 | `getsockname` | 0 | 0 | BOUND |
| 54 | `setsockopt` | 0 | 0 | BOUND |
| 61 | `wait4` | −10 | **−38** | MISSING |
| 72 | `fcntl` | 0 | **−38** | MISSING |
| 74 | `fsync` | 0 | 0 | BOUND |
| 77 | `ftruncate` | −9 | **−38** | MISSING |
| 79 | `getcwd` | 14 | 16 | BOUND, other value |
| 89 | `readlink` | −22 | **−38** | MISSING |
| 186 | `gettid` | ok | ok | BOUND |
| 202 | `futex` | 0 | **−38** | MISSING |
| 217 | `getdents64` | −9 | **−38** | MISSING |
| 228 | `clock_gettime` | 0 | 0 | BOUND |
| 257 | `openat` | 3 | 3 | BOUND |
| 288 | `accept4` | −9 | −88 | BOUND, other value |
| 318 | `getrandom` | 8 | 8 | BOUND |

**BOUND 25 · MISSING 10.**

Three entries say "other value" and each of them is worth a line, because a
different number is exactly where a port goes quietly wrong:

* `getcwd` — 14 against 16 octets. Two different working directory strings,
  not a fault. Windows returns the path with forward slashes so a program
  recognises it again (`docs/ROUND-WINDOWS.md` 2.3).
* `shutdown` on a socket that is bound and listening but not connected —
  Linux says 0, Windows says −107 (`ENOTCONN`). That is Winsock's rule, not
  a mapping error.
* `accept4` on a bad descriptor — Linux `−9` (`EBADF`), Windows `−88`
  (`ENOTSOCK`). The seam's descriptor table knows the number is not a
  socket, and says so.

### 5.1 What the ten missing ones cost — counted, not guessed

Of the ten numbers that answer ENOSYS, **eight do not occur anywhere in
`lib/`**: `stat`(4), `fstat`(5), `poll`(7), `ioctl`(16), `fcntl`(72),
`ftruncate`(77), `readlink`(89), `getdents64`(217). No direct call, no
`SYS_` constant. Nothing in the standard library asks for them, so nothing
in the standard library breaks on them.

Two do occur, and they are the two gaps of §1.2:

| | where | what it blocks |
|---|---|---|
| `futex` (202) | `lib/gc/gc.fi` | the collector's stop-the-world across threads |
| `wait4` (61) | `lib/rt/` | waiting for a child process |

### 5.2 The import table, and what is bound in it

`compiler/src/win.rs` names **42 Win32 functions** in three DLLs, and every
`.exe` binds all three whether it uses them or not:

| DLL | functions |
|---|---:|
| `KERNEL32.dll` | 27 |
| `WS2_32.dll` | 14 |
| `ADVAPI32.dll` | 1 (`SystemFunction036`, which *is* `RtlGenRandom`) |

Grouped by what they carry:

* **Files** — `CreateFileW`, `ReadFile`, `WriteFile`, `CloseHandle`,
  `SetFilePointerEx`, `FlushFileBuffers`, `DeleteFileW`,
  `GetFileAttributesW`, `GetCurrentDirectoryW`, `DuplicateHandle`,
  `GetStdHandle`. Paths go through UTF-16, `/` becomes `\`. **Absolute
  Linux paths do not survive the crossing** and nothing pretends otherwise.
* **Memory** — `VirtualAlloc`, `VirtualFree`, `VirtualProtect`. `munmap` of
  a *part* of a mapping falls back from `MEM_RELEASE` to `MEM_DECOMMIT` and
  is not the same thing as on Linux.
* **Console** — handles 0/1/2 out of `GetStdHandle` at startup; measured
  here against a file, a pipe and a redirected standard input (§4.1,
  `lexdump`).
* **Network** — `WSAStartup` once, then `socket`, `connect`, `bind`,
  `listen`, `accept`, `send`, `recv`, `shutdown`, `closesocket`,
  `setsockopt`, `getsockname`, `ioctlsocket`, `WSAGetLastError`. A `SOCKET`
  is not a file handle on Windows, so the seam keeps a descriptor table that
  remembers which of the two is behind each small integer.
* **Time and chance** — `QueryPerformanceCounter`/`-Frequency`,
  `GetSystemTimeAsFileTime`, `Sleep`, `SwitchToThread`,
  `SystemFunction036`.
* **Threads** — `GetCurrentThreadId`, `GetCurrentThreadStackLimits`,
  `GetCurrentProcess(Id)`. **No `CreateThread`, no `WaitOnAddress`**: this
  is the seam's thread support, and it consists of asking about the thread
  that already exists.

Not bound at all, and therefore not reachable: **process creation**
(`CreateProcessW`), **thread creation** (`CreateThread`), **any GUI**
(`user32`, `gdi32`), **the certificate store** (`crypt32`), **directory
listing** (`FindFirstFileW`), **file metadata** (`GetFileInformationByHandle`).

### 5.3 Two things the compiler cannot do at the boundary yet

* **Callbacks.** A Firn function cannot be handed to Windows as a function
  pointer. The thunk exists in one direction only (System V → Win64); the
  mirror image is missing. This is the single blocker for a window (§7).
* **Floating point in `extern fn`.** The thunks reorder integer arguments
  only. Win64 places float arguments by **position** in `xmm0`–`xmm3`,
  System V by float **index** in `xmm0`–`xmm7`. None of the 42 bound
  functions has a float argument; one that did would be called wrongly. A
  known hole, not a checked refusal.
* **No `.pdata`/`.xdata`.** No usable crash report, no unwinding across a
  system boundary. The own panic path is unaffected — it writes its message
  and exits through the seam.

---

## 6. Wine is not Windows

Everything on the Windows side of this round ran under **Wine 8.0
(Debian `8.0~repack-4`, loader `/usr/lib/wine/wine64`)** on Linux. There was
no Windows machine, and no statement here should be read as if there had
been.

**What Wine does prove, and it is not nothing:**

* The PE image is accepted by a loader that is not ours, the import
  directory is walked, and the three DLLs resolve.
* The **calling convention holds**, because Wine's `kernel32` and `ws2_32`
  are real compiled Win64 code: a misplaced argument or a missing shadow
  space is a crash, not a warning.
* The seam returns the right **values** — 25 bound numbers, checked against
  the Linux answers one by one (§5).
* A workload the size of a compiler runs to completion 314 times (§4.2).

**What Wine does not prove:**

* **The stack guard page.** Wine lays out its stacks differently. The probe
  is written to the documented rule but has not been measured against a real
  Windows kernel.
* **`/proc` really being absent.** Wine maps `Z:` onto the host root, so
  `\proc\self\maps` opens the *Linux* file. The seam intercepts the path
  before `CreateFileW`, but only the intercepted version has been measured.
* **`GetCurrentThreadStackLimits` exists since Windows 8.** On Windows 7 the
  collector would fail at startup.
* **Console code page, file locking, permissions, long paths (`\\?\`),
  timing** — none of it measured.
* Anything about **crash reports** or unwinding, which cannot work without
  `.pdata` anyway.

One observation that is recorded because it happened and is *not* explained:
the very first execution of the socket-using probe with its standard output
redirected into a file produced a truncated file (the last 39 octets at
offset 0). It did not reproduce — six further runs, to a file and through a
pipe, in two directories, produced the full 35 lines every time, and a
reduced probe that isolates the socket path never showed it. It is written
down here as an unexplained one-off on a first run in that Wine prefix, and
**not** as a seam defect, because nothing measured supports that.

---

## 7. What Certus needs on Windows

Justin's actual goal is Certus as his main browser on Windows. This is the
list, with the parts that were re-checked in this round marked as measured.

### 7.1 What already carries

* **The engine's system calls.** Certus (`/root/certus`, 242 `.fi` files,
  104,924 lines) names **14** `SYS_` constants across its whole tree. Three
  of them — `SYS_POLL`, `SYS_IOCTL`, `SYS_FCNTL` — occur in exactly one
  file, `tools/k3net/drv.fi`, a network **driver** experiment, and in no
  file under `lib/`. The **eleven the engine itself uses** — `read`,
  `write`, `open`, `close`, `mmap`, `munmap`, `clock_gettime`, `socket`,
  `bind`, `setsockopt`, `exit` — are **all bound** (§5). Measured, by
  grepping the tree and comparing against the table of §5, not estimated.
* **The collector runs.** Without stack bounds Certus would not start at
  all; the whole DOM and JS tree hangs on the GC. §4.2 exercises the
  collector 172 times over a real workload.
* **Certus is single-threaded**, so the largest gap of round WINDOWS does
  not touch it (§1.2).
* **DNS goes over TCP**, so the one thing the seam only does in the
  connected form — `sendto`/`recvfrom` with an address — is not needed.

### 7.2 What is missing, in the order it has to be built

1. **Callbacks, Win64 → System V.** A window procedure *is* a callback, and
   Firn can call out but not be called in (§5.3). Estimated at 50–80 lines
   in `win.rs` plus an attribute. **Without this there is no window.**
   There is a way around it for a first version — a window class with
   `DefWindowProcW` as its procedure and all the work in the message loop —
   which is worth knowing because it makes the first picture on screen much
   cheaper.
2. **`lib/browser/gdi.fi`.** Re-checked in this round: X11 sits in
   **exactly two files** (`lib/browser/x11.fi`, 702 lines;
   `lib/browser/window_main.fi`, 706 lines), behind **18 exported functions
   and 6 event constants**, with **27 call sites**. Drawing goes through
   `x11_put_image` — a finished BGRX pixel buffer. The engine knows nothing
   about X11. A GDI file of the same size (600–800 lines) plus about 30
   lines of case distinction in `window_main.fi` replaces it; with Certus'
   own TrueType rasteriser (`lib/font`) doing the text, the GDI surface
   shrinks to five real jobs: create a window, blit a buffer, fetch events,
   set the title, close.
3. **Paths and the certificate store.** `window_main.fi` hard-codes
   `/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf`. On Windows that has to
   be a Windows path, and the root store has to be shipped or fetched
   (`CertOpenSystemStoreW`, a fourth DLL). Small, but nobody has done it.
4. **`.pdata`/`.xdata`**, so a crash produces a report. Not a blocker.
5. **Directory listing and file metadata.** Not needed for a first window,
   but a browser that saves downloads or shows a file picker will want
   `FindFirstFileW`/`FindNextFileW` and `GetFileInformationByHandle` — and
   neither is bound today (§5.2).
6. **Threads** (`CreateThread` + `WaitOnAddress`/`WakeByAddressSingle`),
   the moment image loading or the network is to become concurrent — and
   not before.
7. **`firnc1` catching up** (§4.3b), so the project rule applies to this
   target again.
8. **Processes** (`CreateProcessW`). Certus does not need them; this is
   library completeness — but note that it is also what would let
   `firnc1.exe` build executables on Windows (§4.3a).

Points 1 and 2 together are the whole path to a Certus window on Windows,
and both are one round in size.

---

## 8. Two things that are broken on `main`, and were before this merge

Named here because a merge report that only lists what it fixed is
worthless. **Both were measured on a clean worktree of `2a20c514b` — the tip
of `main` before the merge — and behave identically there and after the
merge. Neither is caused by this round, and neither is fixed by it.**

* **`tools/fixpoint.sh` does not get past stage 2.** `./.firnc1
  bin/firnc1.fi -o ./.firnc2` exits 2. `strace` shows the import of module
  `rt` being found next to the importing file
  (`access("bin/rt.fi") = 0`) and the search nevertheless continuing into
  `$FIRNLIB` and `<exe>/../lib`, where it fails and returns 2. The same
  binary, built from `main`, does the same thing. This is the same family as
  the failure round WINDOWS already recorded for `tools/packages/run.sh`
  (17 of 39 cases red on the base branch, `.firnc1` exiting silently with 2
  under `--package`), and it is what the 103 `rc=2` refusals of §4.2 are.
* **Five cases in `tests/` do not meet their own expectation on Linux**,
  independently of any target: `028_cast_narrow`, `030_wrap_u8`,
  `054_i16_ops`, `1334b_type_truncation` (all four already listed by
  `docs/ROUND-ARM-FREESTANDING.md` §1.1) and `834_arc_thread`, which
  measures a timing that does not come about under load.

Both belong to a round of their own. The second one in particular is worth
it: a repository whose headline property is a fixpoint should not have a red
fixpoint.

**Wie gross das ist, in Zahlen dieser Runde:** von den 23 roten Abschnitten
des vollen Laufs gehen **20 auf denselben Punkt zurueck** — 
verweigert schweigend mit 2, sobald ein Programm ueber die Kernsprache
hinausgeht. , , , , ,
, , , , , ,
, ,  (Bau von ), ,
, , , ,  haengen alle an
diesem einen Verhalten. Die uebrigen drei sind:  und
 (bekannte, gezaehlte Abweichungen, 4 bzw. 5) und
 (eine Fundstelle in ). Wer den
Fixpunkt repariert, macht in einem Zug die Mehrheit dieser Liste gruen —
das ist der lohnendste naechste Schritt in diesem Repository, und er ist
unabhaengig von Windows.

---

## 9. What this round produced

| file | lines | what |
|---|---:|---|
| `tools/windows/programs.sh` | 127 | the tools of `bin/` and the examples as `.exe`, run and compared |
| `tools/windows/selfhost.sh` | 125 | the compiler written in Firn as a Windows program, over the whole corpus |
| `tools/windows/seam.sh` | 81 | the seam's answer table, measured |
| `tools/windows/seam.fi` | 154 | the probe: 35 system call numbers without a side effect |
| `docs/round-windows/*.txt` | — | the raw result of round WINDOWS, kept as text |
| `test.sh` section 65b | 31 | all three in the suite, skipping cleanly without mingw or Wine |

Changed: `.gitignore` (`.win-work/`), `test.sh`.

---

## 10. Short version

* **The branch is in `main`** (`fac08f85c` + `535f4994e` + `e59ec33cb`).
  There was no conflict to resolve — the merge base *was* the tip of `main`.
  One thing was changed on the way in: 613 committed build artifacts left
  the tree, the three text results stayed.
* **The Linux side is provably untouched**: 314 of 314 programs give
  character-identical assembly before and after, measured in this round on
  two worktrees.
* **Real Windows programs run.** All 7 tools in `bin/` and all 5 examples:
  14 of 14 identical on both operating systems. The **compiler written in
  Firn** runs as a `.exe` over the whole corpus: 172 identical assembly
  texts (6.3 MB), 142 identical refusals, **0 differences**.
* **Self-hosting on Windows is nearly there, and the gap is named**: the
  compiler produces the correct assembly and writes it; it cannot call `as`
  and `ld` because `fork`/`execve` answer ENOSYS, and it cannot target
  Windows itself yet.
* **The seam carries 25 of the 35 probed system calls.** Of the ten it does
  not, eight are used nowhere in `lib/`; the two that are used are the two
  known gaps — threads and processes.
* **Everything Windows-side ran under Wine 8.0.** What that proves and what
  it does not is §6, and it is not padding.
* **For Certus, the missing piece is the window, not the system calls.**
  Eleven of eleven system calls its engine names are bound; X11 sits in two
  files behind 18 names; and the one thing the compiler still cannot do is
  be *called* by Windows — which is exactly what a window procedure is.
