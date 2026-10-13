# Firn shared objects (C ABI `.so`) -- `firnc --shared`

x86-64 Linux. Windows DLL and aarch64 `.so` are not built by this path (G5 in GRAPHICS-STACK-E-REQUIREMENTS.md).

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
