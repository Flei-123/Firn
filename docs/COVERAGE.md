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

## Suite behavior

`suite` decides per file: a `fn main` means a normal run, no `main` but `#[test]` functions means a `--test` run, neither means the file is skipped. A build or run failure of one program never aborts the suite. The summary on stderr lists `profiled`, `skipped` and `failed` counts and names each skipped and failed file. Exit 0 when at least one program was profiled, exit 2 when none was.

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

## Baseline

`tests/*.fi` split into 8 shards by file number modulo 8, each built with `--opt dev` and run through `/root/jarvis/bin/heavy` with `firncov suite -j 12` (two shards at a time, 16-25 minutes each at host load 30-40), then combined with `firncov merge`. Selection `--include lib/`. All 8 shards (998 files):

| Measure | Hit / total | Percent |
|---|---|---|
| Lines | 182542 / 268339 | 68.0 |
| Functions | 15338 / 23057 | 66.5 |

Suite summary over all shards: 967 profiled, 0 skipped, 11 failed (callgrind timeout on heavy `tests/*mc_*` programs, listed in the suite output). Failed programs add no hits, so the figure is a lower bound. The previous sample (62 programs) gave 35.2 % lines, 37.8 % functions.

Least covered lib modules (all 0 %, of at least 30 lines): `async/wsc`, `audio/dev`, `audio/pulse`, `audio/ring`, `audio/sink`, `browser/quirks_data`, `db/datetime`, `desktop/notify`, `font/cbdt`, `fui/shape`, `fui/style`, `generated/grapheme_tables`, `generated/linebreak_tables`, `gif/gif`, `gpu/gl_fbo`.

The denominator is every imported library function the compiler emits, so modules no test imports stay at 0 %.
