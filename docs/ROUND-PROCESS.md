# Round PROCESS -- start programs, the desktop, a thread pool

Why: the first user of Firn as an application language beyond OpenPlan is a
Minecraft launcher (FleiLauncher, Firn + fUi). A launcher starts `java` with
a long argument list and a changed environment, reads what the game prints,
kills it, opens a browser, finds a Java and downloads 3000 files in
parallel. None of that existed as a library. Everything here is Firn, no libc.

| module | file(s) | what |
|---|---|---|
| `std.process` | `lib/std/process.fi`, `process.windows.fi` | commands, pipes, wait/kill/detach |
| `std.shell` | `lib/std/shell.fi` + `shellos.fi` / `shellos.windows.fi` | open URL/path, reveal, directories, Java discovery |
| `std.env` | `lib/std/env.fi`, `env.windows.fi` | read the environment, `which` |
| `std.pool` | `lib/std/pool.fi`, `pool.windows.fi` | worker threads + job queue |
| `std.cmdline` | `lib/std/cmdline.fi` | Windows command line quoting/splitting, UTF-8 <-> UTF-16 (pure) |
| `std.envblock` | `lib/std/envblock.fi` | merge of the child's environment (pure) |

`import std.process` on `--target=x86_64-windows` picks `process.windows.fi`
(the twin mechanism of `modules.rs`, same as `input.backend`): same names,
same behaviour, different system calls.

## std.process

```firn
var c: process.Command = process.command("java")       // PATH is searched
process.add_arg(&c, "-Xmx2G")
process.set_cwd(&c, "/home/me/.minecraft")
process.set_env(&c, "LANG", "C")                      // unset_env, clear_env
process.set_stdout(&c, process.STDIO_PIPE)            // INHERIT | PIPE | NULL; merge_stderr(&c)
process.set_detach(&c, true)                          // survive the launcher
process.set_no_window(&c, true)                       // Windows: CREATE_NO_WINDOW

var ch: process.Child = process.spawn(&c) catch | e | ...   // ProcError::NotFound/Permission/...
process.read_out(&ch, buf, cap)       // blocking: n > 0, 0 = end of file, < 0 = -errno
process.read_out_nb(&ch, buf, cap)    // -11 = nothing yet
process.write_in(&ch, p, n); process.close_in(&ch)
process.wait(&ch) / wait_timeout(&ch, ms) / try_wait / is_running
process.exit_code / term_signal / pid_of
process.kill(&ch) / terminate(&ch) / close(&ch)

process.run(&c)                                        // inherit stdio, wait, exit code
process.run_capture(&c, &out, &err, timeout_ms)        // both outputs, no deadlock, kill on timeout
process.run_io(&c, in_p, in_n, &out, &err, timeout_ms) // + input
```

Measured: 3 MiB through a child that echoes stdin to stdout, both directions at
once, on Linux and under Wine (`tests/2062`, `tests/2064`); 300000 octets on
stdout AND stderr; a failing `exec` is reported to the caller (an error pipe
that closes on a successful exec), not as exit code 127; pipe ends of the
library are close-on-exec, so a second child cannot keep the first one's stdin
open; writing to a dead child answers `-32` and does not kill the caller
(SIGPIPE blocked for the call and swallowed); 300 starts leave no descriptor.

Linux: `fork`/`execve`/`pipe2`/`dup2`/`wait4`/`kill`/`poll`/`setsid`. The child
between `fork` and `execve` makes system calls only (the argument vector and
environment are built before). Detach is a double fork with `setsid`.
Windows: `CreateProcessW` with the command line from `std.cmdline`,
`CreatePipe`, `SetHandleInformation`, `PeekNamedPipe`, `WaitForSingleObject`,
`TerminateProcess`.

### Windows quoting

`std.cmdline.win_quote_arg` writes an argument so that the rules of the C
runtime (and `CommandLineToArgvW`) read it back unchanged; `win_split_command_line`
is the reverse. `tests/2060` checks both against the documented examples and
by round trip for 18 nasty arguments (spaces, tabs, quotes, trailing
backslashes, JSON, UNC paths, non-ASCII).

### Found and fixed on the way

The Windows start-up code of the compiler (`win_seam.rs`, `__win_argv`) cut the
command line at spaces and quotes only and ignored backslash-quote, so
`java -Dx="a b" "{\"k\":1}"` arrived wrong in a Firn program built for
Windows. It now follows the C runtime rules (the same ones `std.cmdline`
writes). Seen by `tests/2064` A.

## std.shell

```firn
shell.open_url("https://example.org/")      // http(s)/mailto only on Windows; scheme://… on Linux
shell.open_path(p); shell.reveal_in_file_manager(p)
shell.find_in_path("git")                    // "" if absent
shell.home_dir() config_dir() data_dir() cache_dir() temp_dir() minecraft_dir()
shell.find_java(gui) java_candidates(gui, &out) java_major_version(path)   // 8, 17, 21 or -1
```

Linux: `xdg-open` (then `gio open`), detached; reveal uses the
`org.freedesktop.FileManager1.ShowItems` D-Bus call through `dbus-send`, and
opens the folder when that fails; directories follow the XDG variables.
Windows: `ShellExecuteW` (shell32; `explorer /select,` for reveal), directories
from `%APPDATA%`, `%LOCALAPPDATA%`, `%USERPROFILE%`, `%TEMP%`.
Refused: a URL with blanks/control characters/an odd scheme; on Windows every
scheme except http/https/mailto and every path with a program or script
extension (opening it would run it). A relative path starting with `-` becomes
`./-…`. `tests/2061` runs a fake `xdg-open` and checks the argument arrives
byte for byte with `$(...)` and backticks inside (no shell is involved).

## std.env

`get_into`/`get`/`has`/`list_into`/`find_executable`. Linux reads
`/proc/self/environ`, Windows `GetEnvironmentStringsW` (names compare without
case). Read only. `find_executable` is `which`: PATH in order, execute bit and
regular file on Linux; PATH + PATHEXT on Windows, never the current directory.

## std.pool

```firn
fn __thread_work(kind: u64, arg: u64) -> u64 {      // the one line of wiring
    if kind == pool.WORKER_KIND { return pool.worker_main(arg) }
    return 0
}
var p: pool.Pool = pool.pool_new(8, 256)             // workers, queue size
let j: u64 = pool.submit(&p, f, arg)                 // f: fn(u64) -> u64; blocks when the queue is full
pool.wait_idle(&p); pool.job_result(j); pool.job_free(&p, j)
pool.run_all(&p, f, args, n, results)                // n jobs, results collected
pool.shutdown(&p)
```

Why the wiring line: the thread runtime starts a thread with a kind and an
argument and calls the program's own `fn __thread_work`; there can be only one
per program. The queue is a thread channel; job records come from slabs of the
pool and are recycled. `tests/2065`: 5000 jobs each exactly once, 8 sleeping
jobs finish in parallel, a queue of 4 with 2000 jobs, GC allocation on 4
workers, 25 repetitions in all four optimisation levels. **No threads on the
Windows target yet** (the runtime is `clone` + `futex`): `pool.windows.fi`
keeps the API and runs each job inside `submit` (`worker_count` says 0).

## Compiler changes

* `compiler/src/win.rs`: six more imports in the Windows table
  (`CreatePipe`, `SetHandleInformation`, `WaitForSingleObject`,
  `TerminateProcess`, `PeekNamedPipe`, `ShellExecuteW`/`SHELL32.dll`). `GetProcAddress` plus a
  function value cannot be used: a Firn indirect call is System V (comment in
  `win.rs` at `GetModuleFileNameW`).
* `compiler/src/syscalls.rs`: AArch64 `fcntl`, `chdir`, `setsid`,
  `rt_sigtimedwait`, `pipe2` (and the browser answers "no" for them).
* `compiler/src/win_seam.rs`: the command line splitter (above).

## Tests

| test | what | where |
|---|---|---|
| `2060_std_cmdline` | quoting/splitting vs the published examples, round trip, UTF-16 | all |
| `2061_std_env_shell` | env, directories from a controlled environment, refusals, fake xdg-open, fake java | Linux, Windows (subset); aarch64: needs `binfmt_misc` (environment.txt, probe) |
| `2062_std_process` | echo/exit codes/stderr/3 MiB/300 KB deadlocks/cwd/env/timeout (uses `sh`, `cat`, `head`) | Linux, aarch64 |
| `2063_std_process_live` | wait_timeout, kill, non-blocking, stdin, dead reader, detach, fd leak | Linux, aarch64 |
| `2064_std_process_portable` | the same on both systems; the program starts itself as the child | Linux, Wine; aarch64: see 2061 |
| `2065_std_pool` | threads, parallelism, queue, GC | Linux, aarch64 |
| `2066_std_pool_portable` | results | Linux, Wine |
| `neg/2062_process_unhandled`, `neg/2065_pool_job_type` | error union / job type refused at compile time | |

## HONEST (what is not done)

* Windows has no pool threads (`CreateThread` needs the GC/thread table work).
* Windows `kill`/`terminate` are both `TerminateProcess`; no process groups or
  job objects, so grandchildren survive on both systems.
* Not run on a real Windows machine yet, only under Wine 8 (`ShellExecuteW`
  not exercised at all: Wine would start a host browser).
* `java_candidates` lists symlinked installs twice (`java-17-…` and
  `java-1.17.0-…` on Debian); no `realpath`.
* `.bat`/`.cmd` use cmd.exe's own argument rules (`std.cmdline` C1).
* The environment of a running process cannot be changed (E1).
* `wait` on a detached Linux child follows the pid with `kill(pid, 0)`; the
  exit code is unknown (-1).
