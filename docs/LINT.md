# firnlint

Token-based linter written in Firn (`tools/lint/`). It reuses the formatter scanner (`tools/lint/fmt.fi` links to `tools/fmt/fmt.fi`), so it never parses a program the compiler would reject differently: an unscannable source exits with 3.

    firnlint [--rules a,b] [--lib dir] file...
    firnlint --list

Output: `path:line: rule: message`. Exit 0 = clean, 1 = findings, 2 = I/O, 3 = a source does not scan.

## Rules

| rule | finds |
|---|---|
| `unused-import` | import (except `pystr`, which the compiler needs for step slices) whose module name is never mentioned and none of whose exported names occurs in the file (`--lib` is the fallback search directory for the module) |
| `duplicate-import` | two imports that bind the same name |
| `unused-variable` | `let`/`var` without side effects (no call, `try`, `catch`) that is never read |
| `shadowed-variable` | `let`/`var` that hides a live binding or parameter of the same function |
| `unreachable-code` | statement directly after `return`/`break`/`continue` in the same block |
| `unused-private-item` | top-level `fn`/`static` of a file with an `export { ... }` list that is neither exported nor mentioned anywhere in the file (also in strings); skipped: attributed items, `main`, names starting with `_`; files without an export list export everything and are not checked |
| `var-never-mutated` | `var` with an initializer whose every use is a plain read (no `=`/`op=`, no `&x`/`&mut x`/`inout x`, no `.field`/`[i]`), so `let` would do |
| `self-assignment` | statement `place = place` with identical token sequences and no call |
| `constant-condition` | `if true`, `if false`, `while false` |
| `bool-comparison` | `==`/`!=` against the literal `true`/`false` |
| `empty-branch` | empty `else {}`; empty `if {}` without `else` whose condition has no call (a call may be kept for its side effect) |

A name counts as used when it appears as a word anywhere after the declaration (also inside `f"..."` text); field access `.x` and keys `x:` do not count. The rules are conservative: a miss is possible, a wrong finding is a bug.

## Adding a rule

1. `tools/lint/rules/<name>.fi` exporting `name()`, `summary()`, `run(c: *mut ctx.Ctx)`.
2. One `vec_push` line in `tools/lint/registry.fi`.
3. A fixture `tools/lint/cases/<name>.fi` plus `.expected` (generated from the output after review).

## Proof

`bash tools/lint/run.sh [--fast]`: fixtures, exit-code counter-checks, self-lint of the tool, whole-tree run (every refused file must also be rejected by the compiler), and an oracle that deletes flagged imports and turns flagged `var` into `let` in sample programs and checks they still compile.

`RULE=<rule> bash tools/lint/fix.sh LINT_BINARY lib/path/file.fi...` rewrites findings of one rule (default `var-never-mutated`; also `unused-import`, `bool-comparison`, `unused-private-item`) through `tools/lint/edit.py`. Per file the compile oracle (`tools/lint/ctx_compile.sh`) builds a tiny program that imports the module, with the right context: bare, with `std.rt`/`std.str` prelude, or with the window back end; Android files compile as `aarch64-android` objects, `*.windows.fi` and `@windows` files as `x86_64-windows`, `@web` as wasm. It checks before and after; if the batch fails it retries line by line (bottom up) and keeps only the edits that compile. Files that do not compile before the fix are skipped; so are `lib/rc/parts/*` and include-style files (`// Include:` header), whose generated test copies are edited by hand. `std.rt`/`std.str` imports are never removed outside bare mode. `ROUNDS=n` repeats a file (cascading unused items). Needs FIRNC when there is no compiler/target in the tree.
