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
way round); the Firn function itself keeps its internal symbol. The thunk reads the export's signature and re-places every argument for Firn's
(System V) convention: Win64 numbers slots by POSITION (slot `i` = `rcx/rdx/r8/r9` for an integer / pointer, `xmm0-3` for `f32` / `f64`, from slot 4 on
the caller's stack above the 32-octet shadow space), System V counts integers (`rdi rsi rdx rcx r8 r9`) and floats (`xmm0-7`) separately and puts the
rest on the stack in order of writing. The thunk first parks the four register slots in their own shadow space (so no source is clobbered), loads the
System V registers from there, copies the overflow to a fresh 16-aligned block and calls; callee-saved `rsi/rdi/xmm6-15` are kept. A float result is in
`xmm0` in both conventions, an integer one in `rax`. So any number of `i8..i64 / u8..u64 / usize / bool / pointer / f32 / f64` parameters work.
Entry `__firn_dll_entry` (DllMain shape): on `DLL_PROCESS_ATTACH` it runs the seam init (standard handles) only, always returns TRUE. **There is no
`_start`, so the collector does NOT start itself** (same as the Linux `.so`): a library that uses `gc` types exports an init that calls `gc_init()`
(idempotent) and the host calls it once before any other export (`wd_init` in `tools/stack_e/lib_win.fi`). Threads and jobs work in a DLL
(`tools/stack_e/lib_rt.fi`: heap_alloc, `jobs` with 0 and 3 workers, GC on 4 threads) because the Windows thread block lives in the TEB slot, not behind
a host register. **Host threads of their own (r506)**: `thread_attach()` / `thread_host_detach()` work in a DLL exactly as in the Linux `.so`
(export them, bracket runtime-using exports with `thread_blocking_out()` / `thread_blocking_an()`). Why it needs no extra code: the seam answers
`/proc/self/maps` out of `GetCurrentThreadStackLimits` of the CALLING thread (stack bounds), `arch_prctl(ARCH_SET_FS, tcb)` is `TlsSetValue` of the
shared slot for the calling thread only (`0` on detach), `gettid` is `GetCurrentThreadId`. A host thread that never attached reads an empty TLS slot and
crashes on its first runtime access (like the x86-64 `.so`). Proof: `host_attach_win.c` (4 `CreateThread` threads, two waves so entries are recycled,
one idles in host code during collections; jobs with 0/3 workers, GC chains), dev-fast + release-fast, 3 runs each. **`v128` (r506)**: a `v128`
parameter is a pointer to 16 octets in its Win64 integer slot (hidden pointer, the C side passes `__m128` that way); the thunk loads it into the next
`xmm0-7`. A `v128` result is `xmm0` in both conventions. Compile error only if a `v128` would not find an `xmm` register (more than 8 float / vector
parameters). **Structs by value (r506)**: parameters and results follow the Win64 rules, the thunk converts to Firn's word placement (`abi.rs`: up to 16 octets as eightbyte words, over 16 a pointer to a copy, result over 8 octets through a hidden pointer). A struct of 1, 2, 4 or 8 octets travels IN the integer slot (also when it holds floats, e.g. `{f32, f32}` or `{f64}`); every other size is a pointer to the caller's copy (the thunk loads exactly `size` octets, so 3/5/6/7-octet structs never read past the copy). A result of 1/2/4/8 octets comes back in `rax` (the thunk moves `xmm0` there for a float struct); other results use the hidden pointer in slot 0 (rcx), which shifts every other slot by one, and `rax` hands it back (the thunk stores a 3/5/6/7-octet word piecewise). Proof: `lib_struct.fi` + `host_struct.c` (sizes 1-8, 12, 16, 24, float words, mixed with scalars past the registers, results), dev-fast + release-fast under Wine. Limit: a `.lib` import library (use `LoadLibrary` / `GetProcAddress`).
Proof: gate 134 -> `tools/stack_e/run_g5.sh` (mingw host `host_win.c` under Wine, dev-fast + release-fast, plus the runtime probe, plus
`host_abi.c` against `lib_abi.fi`: `f64`/`f32` in every slot, 8 integers, 10 doubles, 14 mixed arguments, 9 integers with a float result, `v128` arguments and result; the same C host
runs against the Linux x86-64 `.so` and the aarch64 `.so`, whose native conventions already carry floats and stack arguments). **Wine only; a real Windows
is untested.**

**Structs by value in a Linux `.so` (x86-64 System V, aarch64 AAPCS64)** have NO thunk: Firn's placement equals the C ABI only for integer structs that
fit the remaining registers whole, and for results of at most 8 octets (rax / x0). `firnc --shared` therefore REFUSES (clear error, "pass a pointer
instead") a struct over 16 octets (C passes a copy, Firn a pointer), a struct that no longer fits the registers (C puts all of it on the stack, Firn splits
it), a result of 9-16 octets on x86-64 (rax:rdx) and every result over 8 octets on aarch64 (x8 / x0:x1), and on aarch64 every struct with float words
(homogeneous float aggregates are in s/d registers in C). Proof: `lib_struct_so.fi` + `host_struct_so.c` (accepted shapes, gcc / qemu) and the refusal of
`lib_struct.fi` in `run_g5.sh`. **A host thread that never called `thread_attach`**: on the aarch64 `.so` it runs as the Firn main thread (fallback to entry 0, only
safe while the host serialises it with the real main thread); in the DLL and the x86-64 `.so` it crashes on its first runtime access.

**aarch64 .so.** Same C ABI (AAPCS64 is already Firn's calling convention: floats in `d0-d7`, more than 8 per class on the stack), no `_start`, symbols
as in the x86-64 `.so` (version script). The one real difference is the thread register: Firn keeps the thread block pointer in `tpidr_el0`, which in a
`.so` belongs to the HOST (glibc TLS: errno, stack protector, locale). **Threads now work** with a register-free scheme (`codegen_a64.rs`,
`SHARED_TP_BASE`): the library keeps two private words, `__firn_tp_base` (the first thread block registered through `arch_prctl(ARCH_SET_FS, p)`, i.e.
entry 0 of the thread table = the main thread) and `__firn_tp_hi` (highest registered entry + 1, atomic max). `Op::ThreadSelf` scans entries 1..hi for the
one whose stack range `[FT_LO, FT_BOTTOM)` holds `sp` (`FT_LO` at offset 272 is new in `gc.fi`: `thread_start` sets it to the stack mapping, an attached
host thread to the base of its stack mapping from `/proc/self/maps`; both are cleared when the stack is released) and falls back to entry 0. So
`thread_start`, `jobs` workers and `thread_attach` / `thread_host_detach` host threads work like on the x86-64 `.so`. Cost: `ThreadSelf` is a short loop
(about 8 instructions per live thread) instead of one instruction. `thread_attach` ends the scanned stack range of a host thread at the host's thread pointer (`arch_prctl(ARCH_GET_FS)` = `mrs tpidr_el0`; a pthread keeps its control block above all frames), because the end of a `/proc/self/maps` line can include a neighbouring mapping that later disappears (found by a ~40 % segfault of `host_attach` under qemu). Limits: a host thread that never called `thread_attach` is taken for the main thread
(the x86-64 `.so` crashes on it instead); code running on a swapped coroutine stack on a non-main thread is not found by its range and falls back to the
main thread's block; `__cpu_features()` reads the auxiliary vector that `_start` saves, which a `.so` never sees: NEON/crypto code that asks falls back
to the baseline. Proof: `host_a64.c` (aarch64-linux-gnu-gcc, `-fstack-protector-all`, checks that the host's TLS and `errno` survive the calls), and the
same `host_rt.c` / `host_attach.c` as the x86-64 gate (Firn threads, jobs with 0/3 workers, GC on 4 threads, 4 attached pthreads in two waves) under
`qemu-aarch64 -L /usr/aarch64-linux-gnu`, dev-fast + release-fast. **qemu only; a real aarch64 machine is untested.**
