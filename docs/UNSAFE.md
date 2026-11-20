# `unsafe` in Firn (N16)

## Syntax

- `unsafe { ... }` is a block statement. Bindings declared inside do not leave the block.
- `unsafe fn name(...)` makes the whole body an unsafe context.
- `unsafe` is a contextual word: it is a keyword only before `{` (statement) or `fn` (item). It stays a legal identifier elsewhere.
- Both `firnc0` and `firnc1` parse both forms.

## What needs `unsafe`

Reported by `firnc0` when the mode is not `off`:

- dereference of a raw pointer: `(*p)`, `*p = v`
- raw pointer casts, in either direction: `x as *mut T`, `p as u64`
- raw pointer arithmetic: `p + n`, `p - n`, `p - q`
- indexing a raw pointer: `p[i]`

Not covered yet: calls of an `unsafe fn` from safe code, `transmute`, MMIO, inline assembly, `Gc[T]` internals.

## Modes

`firnc --unsafe=off|warn|error` (default `off`, nothing changes for existing code).

- `warn`: prints `file:line:col: warning: ...` to stderr, build continues.
- `error`: every site outside `unsafe` is a compile error.
- `--unsafe-scope=<path part>` (repeatable): only files whose path contains the text are checked. Without it every file is checked, including the embedded GC runtime and every library module not yet migrated, so `error` without a scope fails for almost every program today.

## Measuring

`bash tools/safety/unsafe_audit.sh [dir ...]` compiles one wrapper per `lib/<dir>/<file>.fi` in `warn` mode and prints unique sites per file. It uses the real type checker (`rawptr_audit.sh` only greps).

## Migration

1. Pick a module, measure it with `unsafe_audit.sh <dir>`.
2. `FIRNC=<firnc> python3 tools/safety/unsafe_mark.py lib/<dir>/<file>.fi` puts `unsafe` in front of every top-level `fn` that contains a site and re-measures until the module is clean. Sites inside `impl` or `interface` blocks are reported as left over and need a hand edit (`unsafe { }` around the statement, or a rewrite without raw pointers).
3. Run the tests that import the module.
4. Add the path to `tools/safety/unsafe_migrated.txt`; `tools/safety/unsafe/run.sh` then enforces `--unsafe=error` for it.
5. Narrowing an `unsafe fn` into small `unsafe { }` blocks is a later cleanup, not part of the mechanical step.

## Compatibility

A migrated module contains `unsafe fn`, which compilers without this feature (older pins, the Certus fork until r68) cannot parse. Migrate only modules whose consumers build with a compiler that has `unsafe`.

## Status

- Migrated: `std/dict`, `std/math`, `std/jsonstr`, `std/statx`.
- Open: enforcement in `firnc1` (parse only today), default mode `warn`, then `error`, migration of all other modules (see roadmap).
