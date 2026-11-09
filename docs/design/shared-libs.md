# Shared libraries with a versioned C ABI and an export table (r674)

Status: design note, Q2 2027, no code. Ecosystem milestone M8b.

## Question: is it possible today?

**Mostly yes.** `firnc --shared` already builds a C-ABI shared object (docs/SHARED-OBJECTS.md):

| exists | where |
|---|---|
| ELF `.so` on x86-64 and aarch64 Linux, PE DLL on x86-64 Windows | `firnc --shared [--target=...]` |
| Exports = exactly the `#[export_c]` functions, everything else local (version script, `-Bsymbolic`, `-soname <file>`) | `compiler/src/main.rs` |
| C header from the export signatures | `--header=lib.h` |
| Struct-by-value thunks for System V / AAPCS64 / Win64 | `compiler/src/so_thunk.rs`, `win.rs` |
| Own GC + thread block inside the library (`gs` base), host threads via `thread_attach` / export guard | docs/SHARED-OBJECTS.md |
| Gate with C hosts under qemu / wine | `tools/stack_e/run.sh` (gate 134) |

## What is missing

1. **Versioning.** The soname is the plain file name. There is no ABI version in the artifact: no symbol versions
   (the version script has one anonymous node), no exported `<lib>_abi_version()`, no `major.minor` rule. A host cannot
   tell an incompatible library from a compatible one before calling it.
2. **Export table.** Exports exist only as ELF symbols plus a generated C header. There is no machine-readable table
   (name, signature, since-version) inside the artifact, so a loader that does not link at build time (a plugin host,
   OrientOS `dlopen`) cannot check signatures.
3. **Two Firn libraries in one process.** Each `.so` carries its own runtime and claims `gs` (`arch_prctl(ARCH_SET_GS)`);
   a second Firn `.so` in the same process would take over the base of the first. Today: exactly ONE Firn runtime per
   process. Needs a test first (likely broken), then either a shared runtime or per-library thread blocks.
4. **Types across the boundary.** Only C scalars, pointers and C-layout structs cross. `str`, `Vec`, `Map`, `gc`
   objects and closures do not (the host does not scan GC memory, a callback is a bare address; `std.cfn` wraps it).
5. **Unload.** No `dlclose` story: runtime threads, GC roots and the `gs` registration are not torn down.
6. **Stable-ABI promise.** Firn struct layout equals C today, but nothing pins it (`--emit=layout` exists, no golden test
   per exported struct).
7. **Gate health.** `tools/stack_e` (wine DLL tests) was RED in the stable gate 0.1.4946 and is being fixed; the `.so`
   claim above is only as good as that gate.
8. **No `dlopen` consumer written in Firn on OrientOS** (see OrientOS D-04): the loader, relocation of a `-shared`
   object and the symbol lookup are Osum work, not compiler work.

## Proposed design

* **Version:** `#[export_c]` library gets a manifest attribute `#![abi(name = "foo", major = 1, minor = 3)]`. The compiler
  emits `foo_abi_version() -> u64` (`major << 32 | minor`) and uses `-soname libfoo.so.<major>` and symbol version node
  `FOO_<major>`. Rule: minor only adds exports; any removed or changed signature bumps major.
* **Export table:** a read-only data symbol `foo_abi_table` (tiny header: magic, count, then per export: name offset,
  signature string in the `--header` type vocabulary, `since_minor`) plus a section `.firn.abi` so a loader reads it
  without running code. The C header is generated from the same table.
* **Check tool:** `firn abi check old.so new.so` compares two tables (removed/changed = major bump required) and runs in
  the release gate for every shipped library.
* **Runtime:** step 1 = make a second Firn `.so` in a process an explicit compile error at load time (init export
  detects an existing runtime); step 2 = a shared-runtime mode where libraries import `__firn_rt_*` from a host Firn
  image.
* **Types:** keep C-only at the boundary; Firn-to-Firn passes `(ptr, len)` pairs and handles. A typed layer (strings,
  Vec) is a later step on top of the table.

## Steps

1. Test: load two Firn `.so` in one C host, record what breaks (small, decides item 3).
2. `abi` attribute + `foo_abi_version` + soname/symbol version in `--shared` (both compilers; tests in `tools/stack_e`).
3. `.firn.abi` table + header generation from it + `firn abi check`.
4. Runtime isolation (item 3) and unload (item 5).
5. OrientOS consumer: kernel/user loader reads `.firn.abi` before binding (see OrientOS D-04).
