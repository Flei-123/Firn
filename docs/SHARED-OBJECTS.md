# Firn shared objects (C ABI `.so`) -- `firnc --shared`

x86-64 Linux `.so` (full), Windows DLL (`--target=x86_64-windows`) and aarch64 `.so` (`--target=aarch64-linux`) with the limits in
the last section ("Windows DLL and aarch64 .so (G5)").

## Build (one command)

```
firnc --shared [--header=lib.h] [--opt-level=release-fast] -o libfoo.so foo.fi
```

* `--shared` = `--pic` + object + `ld -shared -Bsymbolic -z noexecstack -soname <file>` with a version script.
  Visible symbols: exactly the `#[export_c]` functions (bare names, no module prefix). Everything else is local.
* No `fn main` needed. Temp files (`.o`, `.ver`) are removed. `#[link_lib(c)]` etc. become `DT_NEEDED`.
* `--header=lib.h` writes a C header from the `#[export_c]` signatures (also usable with `-c`): `<stdint.h>` scalars,
  `size_t` for `usize`, structs as plain C structs (Firn lays fields out like C), pointers to unknown types as
  `void*`, function values / tuples as `void*`, `&T` parameters as `T*`. C++ users get `extern "C"`.
* Proof: `tools/stack_e/run.sh` (gate 134).

## Threads and GC inside a `.so` (G1 fixed)

The Firn runtime keeps a per-thread block (own free lists, stack bounds, state). In an executable it sits behind
`fs`; **in a shared object it sits behind `gs`** (`--pic` on x86-64 Linux: `mov rax, gs:0`, `arch_prctl(ARCH_SET_GS)`).
glibc owns `fs` of every host thread (stack protector canary, errno, TLS); the `.so` never touches it.
Executables are unchanged byte for byte.

Rules:

* The thread that first reaches the runtime (first `thread_start`/`jobs.create`/GC call, or `thread_attach()`) becomes the
  Firn "main thread" and gets the `gs` base. It must outlive every other host thread (its stack is the one the collector
  knows from `gc_init`; it cannot detach).
* **Other host threads (G1b, `thread_attach`)**: every further host thread calls `thread_attach()` ONCE before any other
  runtime use (it works with `gs = 0`; it gets its own table entry, stack bounds from `/proc/self/maps`, its own `gs`
  base) and `thread_host_detach()` before it ends (free lists back, entry recycled, `gs` cleared). Both are plain Firn
  functions: export them (`#[export_c] fn se_attach() -> u64 { ... }`). A second `thread_attach()` on the same thread
  returns true and changes nothing. A host thread that was not attached crashes on its first runtime access (`gs = 0`).
* **Host mode**: `thread_attach()` returns with the thread counted as BLOCKED (its stack is scanned, the collector does
  not wait for it while it runs host code that never reaches a safepoint). Every export that touches the runtime on such a
  thread brackets its work: `thread_blocking_out()` ... `thread_blocking_an()`. Without the bracket, a thread idling in host
  code would stall every stop of the world. Proof: `tools/stack_e/host_attach.c` / `host_attach.py` (4 host threads, two
  waves so entries are recycled, one thread idles in host code while the others collect; jobs with 0/3 workers, GC chains,
  threads of their own), 3 opt levels, C and ctypes.
* Threads started by the library itself (`thread_start`, `jobs`) are fine, also while the host runs threads of its own.
* The collector scans the Firn threads and the stack of the Firn main thread; host memory is not scanned.
  Do not keep the only reference to a `gc` object in host-owned memory.
* A C callback is a bare code address. Wrap it with `std.cfn` (`cfn.rec(addr)`, `cfn.as_u1(&rec)` ...), call it
  from the thread that received it.

## Windows DLL and aarch64 .so (G5)

```
firnc --shared --target=x86_64-windows [--header=f.h] -o foo.dll foo.fi     # PE DLL, x86_64-w64-mingw32-ld --dll
firnc --shared --target=aarch64-linux  [--header=f.h] -o libfoo.so foo.fi   # aarch64-linux-gnu-ld -shared
```

**Windows DLL.** Exports are Win64 entry thunks under the bare `#[export_c]` names (`win.rs::win64_thunk_asm`, the callback-thunk code the other
way round: `rcx, rdx, r8, r9, stack` -> System V registers, shadow space, callee-saved xmm6-15 kept); the Firn function itself keeps its internal
symbol. The export list is a temporary `.def`. Entry `__firn_dll_entry` (DllMain shape): on `DLL_PROCESS_ATTACH` it runs the seam init (standard
handles) only, always returns TRUE. **There is no `_start`, so the collector does NOT start itself** (same as the Linux `.so`): a library that uses
`gc` types exports an init that calls `gc_init()` (idempotent) and the host calls it once before any other export (`wd_init` in `tools/stack_e/lib_win.fi`).
Threads and jobs work in a DLL (`tools/stack_e/lib_rt.fi`: heap_alloc, `jobs` with 0 and 3 workers, GC on 4 threads) because the Windows thread block
lives in the TEB slot, not behind a host register. Limits (compile errors, not miscompiles): up to 6 integer / pointer arguments per export, no
`f32` / `f64` / `v128` parameters. Return values: integer / pointer in `rax`. Not done: `thread_attach` from host threads of their own (only proven on the
Linux x86-64 `.so`), floats / by-value structs in exports, a `.lib` import library (use `LoadLibrary` / `GetProcAddress`).
Proof: gate 134 -> `tools/stack_e/run_g5.sh` (mingw host `host_win.c` under Wine, dev-fast + release-fast, plus the runtime probe, plus a float export
that must be refused). **Wine only; a real Windows is untested.**

**aarch64 .so.** Same C ABI (AAPCS64 is already Firn's calling convention), no `_start`, symbols as in the x86-64 `.so` (version script). The one real
difference is the thread register: Firn keeps the thread block pointer in `tpidr_el0`, which in a `.so` belongs to the HOST (glibc TLS: errno, stack
protector, locale). In `--shared` builds the pointer lives in a private word of the library (`__firn_thread_ptr`) and **`thread_start` / `jobs` workers
fail with `-ENOSYS`** (no second thread block can exist). So an aarch64 `.so` is single-threaded for the runtime: GC (stop-the-world with one thread), heap,
statics, all work; `thread_attach` from several host threads is NOT supported. `__cpu_features()` reads the auxiliary vector that `_start` saves, which a
`.so` never sees: NEON/crypto code that asks falls back to the baseline. Proof: `host_a64.c` (aarch64-linux-gnu-gcc, `-fstack-protector-all`, checks
that the host's TLS and `errno` survive the calls) under `qemu-aarch64 -L /usr/aarch64-linux-gnu`, dev-fast + release-fast. **qemu only; a real aarch64
machine is untested.**
