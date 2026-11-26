# Firn stability promise

Status: draft for 1.0, enforced today by `tools/stability/run.sh` (test.sh section 151, release gate).

## What is stable (from 1.0)

A program that compiles and passes under an edition keeps compiling and keeps its observable behavior under every later compiler that supports that edition.

- **Syntax**: every construct accepted today in `tests/stable/*.fi` (frozen corpus). New syntax may only be added where the old grammar rejected the text.
- **Type and move rules**: accepted programs stay accepted. Rejected programs may become accepted (loosening is compatible); `tests/stable/neg/*.fi` pins the rejections that must stay rejections.
- **Semantics**: integer, float, string, struct, enum, match, generics, interfaces, closures, error unions, ownership and GC rules as written in `SPEC.md`. Where `SPEC.md` and the compiler disagree, the frozen corpus decides until the text is fixed.
- **Build levels**: `dev-fast`, `release-safe`, `release-fast` mean what `docs/SAFETY.md` says. Checked levels panic (exit 101) on overflow, out-of-range shifts, `MIN / -1`, `-MIN` and lossy narrowing casts; `release-fast` wraps by definition (never undefined behavior). `+% -% *%` and `+| -| *|` behave identically in every level. A program must not depend on the difference; `tests/stable/levels` pins both sides.
- **std core API**: every exported item of the modules in `tools/stability/core_modules.txt` (`std.core vec str map math bytes option io fs json time deque heap base64 hash text num bisect testkit`). The frozen list is `tools/stability/api_std.txt` (name, generics, parameters, return type, struct fields, enum variants).
- **Manifest format**: keys of `firn.package` (`package version edition start source public needs`) and their meaning.
- **CLI**: `--edition`, `--opt-level`, `--target`, `--package`, `--emit=exe|asm`, `-o`.

## What is not stable

- **Experimental libraries**: every module outside the std core list (`net`, `http`, `tls`, `db`, `pg`, `mysql`, `fui`, `photo`, `media`, `pdf`, `docview`, `scene3d`, `gpu`, `webfw`, `mc`, ...) and every `std.*` module not in the list. They may change in any release; breaking changes are noted in the release notes.
- **`std.rt` (internal part)**: only the 39 names in `tools/stability/rt_stable.txt` are stable (buffers, heap, raw loads/stores, file helpers, `finish`, `arg_*`); every other `std.rt` export (`run`, `spawn`, `wait_pid`, `env_value`, ...) is runtime internals. Prefer `io`, `fs`, `bytes`, `mem`.
- **Diagnostics**: error text, notes, ordering and positions of messages. Only the fact of rejection is stable.
- **Generated code**: assembly, FIR text, symbol names, object layout, compile time, `--emit` targets other than `exe` and `asm`, `--timings`, `--stats`.
- **Compiler internals**: `firnc1` source, `lib/firnc1`, pass names, `FIRN_*` environment switches, cache layout.
- **Performance numbers** and optimizer behavior.
- **Targets other than `x86_64-linux` and `aarch64-linux`** until each is listed as stable in this file.
- **Unspecified behavior** listed in `docs/SAFETY.md` under "Non-guarantees".

## Versioning

- Format: `MAJOR.MINOR.PATCH` of the compiler plus an **edition** (`2026`, ...).
- **PATCH**: bug fixes, faster code, better diagnostics. No change in accepted programs or results of correct programs.
- **MINOR**: new language features, new std items, new experimental libraries, new targets. Everything stable stays valid. Frozen API entries are only added, never removed or changed.
- **MAJOR**: not planned. A breaking change is only allowed through a new edition.
- **Editions**: a project selects its edition with `edition 2026` in `firn.package` or with `--edition=2026`. The flag wins over the manifest. Without either, the newest edition counts. Unknown editions are refused with `unknown edition 'X' (supported: ...)`. An edition changes only what this file lists as edition-dependent; all editions of one compiler can be mixed in one build only through packages (the root package's edition governs the build today).
- A new edition ships only with: the list of differences, an automatic migration check, and the previous edition kept working for at least 3 years.
- Compiler bug fixes that make an incorrectly accepted program fail are allowed in a PATCH release if the program violated the written rules; the fix lists the program shape in the release notes.

## Deprecation process

1. **Announce**: mark the item `#[deprecated]` (planned attribute) or list it in `docs/DEPRECATED.md` with the replacement and the first release of the notice.
2. **Warn**: from the next MINOR, the compiler warns on use. Warnings never change exit codes.
3. **Wait**: at least 2 MINOR releases and 12 months.
4. **Remove**: only in a new edition, never inside an edition. Experimental items may be removed after step 2.
- Security fixes may shorten the process; the notice states why.

## How the promise is enforced

- `tests/stable/*.fi`: frozen golden programs with expected exit code or output, compiled with `--edition=2026` in `dev-fast` and `release-fast`. `tests/stable/MANIFEST.sha256` pins file contents; a changed golden fails the gate.
- `tests/stable/neg/*.fi`: programs that must stay rejected (no crash, no acceptance).
- `tests/stable/levels/*.fi`: programs whose first line `// expect_levels: dev=101 dev-fast=101 release-safe=101 release-fast=7` lists the exit code per build level (overflow, shifts, narrowing, wrapping operators).
- `tools/stability/api_check.sh`: extracts the exported API of the std core modules (and the `std.rt` allow-list `rt_stable.txt`) and diffs it against `tools/stability/api_std.txt`. Removed or changed entries are a **breaking change** and fail the gate; new entries are reported and frozen with `--update`.
- 20.11.2026: `std.math` `frexp` / `ldexp` are plain `fn` again (they were `unsafe fn` only through a hand edit of the generated `lib/std/math.fi`; `std.core` always had them safe). The frozen list was updated; callers are not affected.
- 20.11.2026: `#[derive(..)]` on a generic enum is accepted (additive: a program that was rejected is now valid, no accepted program changes). The negative golden `tests/stable/neg/derive_generic_enum.fi` was removed on purpose and `tests/stable/6796_derive_generic_enum.fi` pins the new behaviour.
- 24.11.2026: character literals `'a'`, `'\n'`, `'\u{1F600}'` are accepted (additive: the lexer rejected every apostrophe before). A literal is an untyped integer literal holding the Unicode scalar value; `char` is a second spelling of `u32`. The negative golden `tests/stable/neg/1062_hint_apostrophe.fi` now uses `'xy'` (still rejected) and `tests/stable/6811_char_literals.fi` pins the new behaviour.
- 25.11.2026: (a) a `*self` method may be called on a value without an address (`make(3).sum()`, `(a + b).sum()`): additive, a rejected program is now valid. The negative goldens `tests/stable/neg/impl_receiver_without_address.fi`, `4804_method_without_receiver_param_a.fi` and `impl_without_receiver.fi` were removed on purpose and `tests/stable/6821_method_chain_temps.fi` pins the new behaviour. (b) `impl` functions without a receiver and calls `Type.name(args)` (`int.parse(s)`, `Counter.start()`, `Config.default()`): additive, the parser and the checker rejected both before; `tests/stable/6822_type_static_calls.fi`, `6823_parse_type_calls.fi`, `6824_derive_default_static.fi`.
- 20.11.2026: type arguments of generic calls and enum constructors are inferred from value arguments and payloads (additive: a program that was rejected is now valid, no accepted program changes). The negative golden `tests/stable/neg/generic_without_ty_args.fi` was removed on purpose and `tests/stable/6791_infer_args_core.fi` pins the new behaviour.
- Adding a golden: `bash tools/stability/freeze.sh` (copies new qualifying tests, never overwrites). Changing or deleting a golden requires a new edition and an entry in this file.
- Run: `bash tools/stability/run.sh` (or `bash test.sh --stage` of section 151).
- 26.11.2026: (a) field access through a struct pointer without `(*p)`: `p.n`, `self.n += 1` for `p: *T` / `*mut T` (same unsafe rule as `(*p).n`); additive, the checker rejected it before with "there is no automatic dereference". (b) an array index may be any integer type up to 64 bits (`v[i]` with `i: i32`, `u32`, `i64` ...); the value is converted to `usize`, a negative value fails the bounds check; additive, rejected before with "index must be of type usize". The negative golden `tests/stable/neg/4726_safety_index_must_be_usize.fi` was removed on purpose; `tests/stable/6840_auto_deref_field.fi` and `6841_index_any_integer.fi` pin the new behaviour.
- 26.11.2026: an f-string is accepted where a `str` is expected (`"a" + f"{n}"`, `let s: str = f".."`, arguments, `return`, struct fields) and every `std` module is imported implicitly by `name.member` (additive: programs that were rejected are now valid, no accepted program changes). The negative golden `tests/stable/neg/fstring_plus_str.fi` was removed on purpose; `tests/stable/6833_fstring_as_str.fi`, `6834_fstring_plus_str.fi` and `6830_std_auto_import.fi` pin the new behaviour. `std.time` gains `millis()` and `micros()`.
