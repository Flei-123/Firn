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

* The thread that first reaches the runtime (first `thread_start`/`jobs.create`/GC call that initialises it) becomes
  the Firn "main thread" and gets the `gs` base. **Call a runtime-using `.so` from that one host thread.** Another host
  thread has `gs = 0` and crashes on the first runtime access (known limit, roadmap: `thread_attach`).
* Threads started by the library itself (`thread_start`, `jobs`) are fine, also while the host runs threads of its own.
* The collector scans the Firn threads and the stack of the Firn main thread; host memory is not scanned.
  Do not keep the only reference to a `gc` object in host-owned memory.
* A C callback is a bare code address. Wrap it with `std.cfn` (`cfn.rec(addr)`, `cfn.as_u1(&rec)` ...), call it
  from the thread that received it.
