# macOS target (`--target=aarch64-macos`, `--target=x86_64-macos`)

Status: structural only. No Mac hardware is available; nothing built here has run on macOS.

## How it works
- Firn emits the same assembler text as for Linux. `compiler/src/darwin/text.rs` rewrites that text for Mach-O: `_` prefix on symbols, `.L` -> `L`, `adrp`/`:lo12:` -> `@PAGE`/`@PAGEOFF`, ELF sections -> `__TEXT,__const` / `__DATA,__bss`, `.file`/`.loc`/`.arch`/GNU notes dropped.
- Assembler: `clang -target arm64-apple-macos11 -mcpu=apple-m1` (or `x86_64-apple-macos10.15`), override with `FIRN_DARWIN_CC`. Linker: `ld64.lld` (override `FIRN_DARWIN_LD`), against a generated `libSystem.tbd` stub (`darwin/libsystem.rs`). Code signing: ld64.lld writes the ad-hoc signature for arm64.
- No raw syscalls: every `svc #0` / `syscall` becomes a call to `__Fmac.syscall` (`darwin/stub_a64.rs`, `stub_x86.rs`, table in `syscalls.rs`). The stub maps the Linux number to a libSystem function (write, read, open, close, lseek, mmap, munmap, mprotect, clock_gettime, nanosleep, getpid, sched_yield, fsync, ftruncate, unlink, mkdir, pipe, dup2, kill, exit), translates the Linux flag values (open, mmap, clock ids), turns `-1`+errno into `-errno`, passes the `open` mode on the stack (Apple variadic ABI). madvise, sigaltstack, rt_sigaction, rt_sigprocmask are no-ops. Everything else returns `-ENOSYS`.
- Entry: dyld calls `__start` as `main(argc, argv)`; the shim in `darwin/entry.rs` rebuilds the Linux `[argc][argv...][0][0]` stack block the Firn start code expects.
- Integrated assembler is off for macOS (it writes ELF only).

## Verification (server, no Mac)
- `bash tools/macos/run.sh`: compiles hello, str, hash map, threads, tour for both targets and checks the Mach-O headers with llvm-objdump/llvm-nm (file type, LC_MAIN, libSystem, platform, no RWX, only libSystem imports).
- `tools/macos/ci.yml`: proposal for a GitHub Actions `macos-14` job that runs the same programs for real. Not published.
- Needs `llvm-14` (llvm-objdump, llvm-nm), `clang`, `ld64.lld` on the server.

## Not tested / known gaps
- Nothing has been executed on macOS (arm64 or Rosetta).
- Threads: `clone`/`futex` are not mapped (-ENOSYS); thread programs link but cannot start threads. x86_64: `arch_prctl`/`fs:` thread pointer does not exist on macOS.
- Linux errno numbers are not translated from Darwin values (only the sign).
- stat/readdir/poll/socket calls are not mapped (Linux and Darwin struct layouts differ).
- `tpidr_el0` use of the Firn runtime on Apple Silicon is untested.
- Signal handlers (stack overflow message) are not installed.
- No `lib/@macos` platform layer: window/fUi programs do not build.
- aarch64: variadic C calls (only `open`) are handled in the stub; no other variadic calls exist.
