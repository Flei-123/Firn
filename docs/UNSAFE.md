# `unsafe` in Firn (N16)

## Syntax

- `unsafe { ... }` is a block statement. Bindings declared inside do not leave the block.
- `unsafe fn name(...)` makes the whole body an unsafe context.
- `unsafe` is a contextual word: it is a keyword only before `{` (statement) or `fn` (item). It stays a legal identifier elsewhere.
- `unsafe fn` also works inside `impl` blocks.
- Both `firnc0` and `firnc1` parse and enforce both forms.

## What needs `unsafe`

Reported by `firnc0` and `firnc1` (same texts, same positions) when the mode is not `off`:

- dereference of a raw pointer: `(*p)`, `*p = v`
- raw pointer casts, in either direction: `x as *mut T`, `p as u64`
- raw pointer arithmetic: `p + n`, `p - n`, `p - q`
- indexing a raw pointer: `p[i]`

- a call of an `unsafe fn` (plain call or method call) from safe code
- an `asm(...)` block (x86-64 and aarch64)

Not covered yet: `transmute`, MMIO, `Gc[T]` internals, `unsafe` as an expression.

## Modes

`firnc --unsafe=off|warn|error` (default `off`, nothing changes for existing code).

- `warn`: prints `file:line:col: warning: ...` to stderr, build continues. The build cache is bypassed in this mode, otherwise a cache hit would hide the warnings.
- `error`: every site outside `unsafe` is a compile error.
- `--unsafe-scope=<path part>` (repeatable): only files whose path contains the text are checked. Without it every file is checked, including the embedded GC runtime and every library module not yet migrated, so `error` without a scope fails for almost every program today.

## Measuring

`bash tools/safety/unsafe_audit.sh [dir ...]` compiles one wrapper per `lib/<dir>/<file>.fi` in `warn` mode and prints unique sites per file. It uses the real type checker (`rawptr_audit.sh` only greps).

## Migration

1. Pick a module, measure it with `unsafe_audit.sh <dir>`.
2. `FIRNC=<firnc> python3 tools/safety/unsafe_mark.py lib/<dir>/<file>.fi` wraps the body of every `fn` that contains a site in `unsafe { }` (one-line functions too, methods in `impl` blocks too) and re-measures until the module is clean. The signature stays the same, so the std API gate does not change. Sites that only show up when a generic is instantiated by another module: run `AUDIT_SITES=<file> tools/safety/unsafe_audit.sh <dir>` first and give that file to the marker as `SITES_FILE=<file>`.
3. Run the tests that import the module.
4. Add the path to `tools/safety/unsafe_migrated.txt`; `tools/safety/unsafe/run.sh` then enforces `--unsafe=error` for it.
5. Narrowing an `unsafe fn` into small `unsafe { }` blocks is a later cleanup, not part of the mechanical step.

## Compatibility

A migrated module contains `unsafe fn`, which compilers without this feature (older pins, the Certus fork until r68) cannot parse. Migrate only modules whose consumers build with a compiler that has `unsafe`.

## Status

- Enforced by `firnc0` and `firnc1` (`--unsafe`, `--unsafe-scope`). The markers live in the syntax tree of `firnc1` (`S_BLOCK` flag, `fn_unsafe`); `firnc0` still keeps them in a span table in `unsafety.rs`.
- Migrated: all of `lib/std/*.fi` and `lib/std/crypto/*.fi` that build on their own (list: `tools/safety/unsafe_migrated.txt`). `unsafe fn` is kept for `std/dict`, `std/jsonstr`, parts of `std/math` and `std/statx` (first round); the rest uses wrapped bodies.
- Open: default mode `warn`, then `error`; other libraries (`lib/gc`, `lib/window`, `lib/compress`, `lib/async`, `lib/net`, ...).
