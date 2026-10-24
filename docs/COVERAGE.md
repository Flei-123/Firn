# firncov: line and function coverage for Firn programs

`tools/coverage/firncov` measures which source lines and functions of a Firn program (and of every library it imports) a run executes. The compiler is not changed: the program is built normally, run under `valgrind --tool=callgrind --dump-instr=yes`, and every executed instruction address is mapped back to a source line with the `.debug_line` table the compiler already emits (docs/DEBUGGER.md). Needs `valgrind` and `python3`.

    firncov run   [--opt LEVEL] [--test] [--timeout S] -o profile.json prog.fi [program args]
    firncov suite [--opt LEVEL] [--test] [-j N] -o profile.json file.fi|dir ...
    firncov merge -o out.json a.json b.json ...
    firncov report [--format text|lcov|json|annotate] [--missing] [--include P] [--exclude P]
                   [--annotate FILE --root DIR] [--fail-under PERCENT] profile.json ...

`FIRNC` selects the compiler (default `compiler/target/release/firnc`), `FIRNLIB` the library root (default `lib/`). Builds use `--no-cache`.

## What is counted

- **Line**: a source line owning at least one row of the line table (that is code). Covered = one of its instructions ran. Hit count = the highest execution count of any instruction of the line (what a loop line shows is the condition count; a line sharing a return epilogue can show the count of the epilogue).
- **Function**: a `DW_TAG_subprogram`; at optimized levels (the compiler writes none there) a text symbol, its range ending at the next symbol. Covered = its entry instruction ran; hits = entry count.
- Paths are the ones in the line table, relative to the directory `firnc` ran in. Run from the repo root and `--include lib/std` or `--include tests` selects a part.
- Every imported library function the compiler emits counts, also unused ones: a program that imports `std.str` shows `lib/std/str.fi` as mostly uncovered. Use `--include` for the files you care about.

## Build levels

Default is `--opt dev` (`--no-opt`): statement-precise lines, nothing removed. `dev-fast`, `release-safe`, `release-fast` work too, but the optimizer deletes dead functions and folds constants, so denominators shrink (`release-fast` may keep two lines of a small program). Use `dev` for coverage numbers.

## `#[test]` programs

`--test` builds with `firnc --test --no-run` and runs the test executable under `--trace-children=yes`; every per-case process writes its own callgrind file and all are merged, so a case that dies still counts. The synthetic `<test runner>` file is the generated runner.

## Formats

- `text`: per file `hit/total` lines and functions; `--missing` lists uncovered line ranges and function names.
- `lcov`: `TN/SF/FN/FNDA/FNF/FNH/DA/LF/LH`, usable by `genhtml`.
- `json`: totals only (`lines_hit`, `lines`, `functions_hit`, `functions`, percentages).
- `annotate`: gcov-style listing, `#####` marks an uncovered line.

`--fail-under N` exits 1 when the line percentage of the selection is below N. Exit 2 = bad input (missing source, build failure, no line table, unknown profile).

## Limits

- Line coverage only, no branch coverage: `a && b` is one line.
- Valgrind is about 50x slower than native; start suites through `/root/jarvis/bin/heavy`.
- `.debug_line` versions 2 to 4 only (what the assembler writes).

## Proof

`bash tools/coverage/run.sh` (test.sh 53d): exact uncovered lines and hit counts on a known program, all formats, merge equal to `suite`, the four build levels, `#[test]` with a crashing case, thresholds and filters, and counter-checks (bad profile, missing source, build failure, wrong expectation strikes).
