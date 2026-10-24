# firnlint

Token-based linter written in Firn (`tools/lint/`). It reuses the formatter scanner (`tools/lint/fmt.fi` links to `tools/fmt/fmt.fi`), so it never parses a program the compiler would reject differently: an unscannable source exits with 3.

    firnlint [--rules a,b] [--lib dir] file...
    firnlint --list

Output: `path:line: rule: message`. Exit 0 = clean, 1 = findings, 2 = I/O, 3 = a source does not scan.

## Rules

| rule | finds |
|---|---|
| `unused-import` | import whose module name is never mentioned and none of whose exported names occurs in the file (`--lib` is the fallback search directory for the module) |
| `duplicate-import` | two imports that bind the same name |
| `unused-variable` | `let`/`var` without side effects (no call, `try`, `catch`) that is never read |
| `shadowed-variable` | `let`/`var` that hides a live binding or parameter of the same function |
| `unreachable-code` | statement directly after `return`/`break`/`continue` in the same block |

A name counts as used when it appears as a word anywhere after the declaration (also inside `f"..."` text); field access `.x` and keys `x:` do not count. The rules are conservative: a miss is possible, a wrong finding is a bug.

## Adding a rule

1. `tools/lint/rules/<name>.fi` exporting `name()`, `summary()`, `run(c: *mut ctx.Ctx)`.
2. One `vec_push` line in `tools/lint/registry.fi`.
3. A fixture `tools/lint/cases/<name>.fi` plus `.expected` (generated from the output after review).

## Proof

`bash tools/lint/run.sh [--fast]`: fixtures, exit-code counter-checks, self-lint of the tool, whole-tree run (every refused file must also be rejected by the compiler), and an oracle that deletes flagged imports from sample programs and checks they still compile.
