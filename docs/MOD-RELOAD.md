# Mod hot reload (R7) -- plan and prototype

**Verdict:** Firn can emit a loadable `.so` today (no language change). `firnc --pic -c` makes an ELF
object (the Android build uses this), `ld -shared -Bsymbolic` links it, `std.dynlib` loads, resolves
and unloads it. `tools/modreload/run.sh` proves it: mod v1 runs, the mod is rebuilt as v2 and v3 while
the host keeps running, and the output changes (`RESULT modreload: ok`).

## ABI (host <-> mod)

Plain C-ABI symbols (`#[export_c] #[no_gc]`); the dynamic symbol table is the entry table
(`ld --version-script` exports only `mod_*`).

| symbol | meaning |
|---|---|
| `mod_abi() -> u64` | ABI number; the host refuses a mod it does not know |
| `mod_init(state)` | first load; `state` = host-owned zeroed block |
| `mod_migrate(state, from_abi)` | later loads: upgrade the old block to the new layout |
| `mod_tick(state, out, cap) -> i64` | one tick (the prototype writes text into `out`) |
| `mod_shutdown(state)` | called before unload |

**State handover:** the mod keeps NO globals; everything lives in the host's `state` block, so it survives
`dlclose`. Layout changes go through `mod_migrate`. **Reload step:** wait for a tick boundary -> `mod_shutdown`
-> `dynlib.close` -> `dynlib.open_c(new path)` -> `mod_migrate` -> continue. Use a NEW file name per build
(`mod_v2.so`): the loader may keep a mapped file of the same name.

## Measured

* compile + link of a small mod: ~250 ms (dev-fast, `ld -shared`); the reload itself is `dlclose` + `dlopen`.
* the host is a dynamic ELF (`#[link_lib(c)]`, +0.4 ms start, docs/FFI.md); the mod itself uses no libc.

## Limits (honest)

* Linux x86-64 only so far. Windows: `dynlib.windows.fi` loads DLLs, but `firnc --pic -c` + a PE linker step for a DLL is not built.
* Mod code must be `#[no_gc]` and may not use the Firn heap/collector (the `.so` has no runtime of its own): data goes through the host's `state` and buffers.
* Host callbacks into the engine (draw, log) need a function-pointer table the host passes to `mod_init`; the mod calls it through a `#[extfn]`/function value -- not prototyped yet.
* Unload safety: pointers into the old `.so` (function values, strings in `.rodata`) must not outlive `dlclose`. The host must not keep them.
* Fallback if `dlclose` misbehaves: the PROCESS variant -- the mod is a separate program talking over a pipe; the host restarts it. Simple, slower, safe.

## Next

1. host callback table (`mod_init(state, api)`) and a text/draw call from the mod,
2. file watcher (inotify) that triggers rebuild + reload,
3. `tools/modreload` into the test suite, Windows DLL variant.
