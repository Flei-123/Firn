# Suite runner (roadmap r26)

`tools/testrunner` is a standalone Rust tool (std only, no dependency on the compiler). It does what section 3 and 4 of
`test.sh` do -- compile every program, run it, compare against the expectation in line 1 -- and reports the result so a
machine can read it. `firnc --test` (round 94, `lib/test/runner.fi`) is the other half: it runs the `#[test]`
functions of one file, one process per case. The suite runner calls it for every file that has `#[test]` and no `main`.

## Use

```sh
cargo build --release --manifest-path tools/testrunner/Cargo.toml
testrunner [--root DIR] [--compiler PATH] [--format text|json|jsonl|junit] [--out FILE]
           [--filter TEXT] [--levels opt,noopt,devfast,safe] [--jobs N] [--timeout SECONDS] [--progress] [--quiet]
```

| Option | Default | Meaning |
|---|---|---|
| `--root` | `.` | repository root; `tests/`, `tests/opt/`, `examples/`, `tests/neg/` below it |
| `--compiler` | `ROOT/compiler/target/release/firnc` | compiler binary |
| `--levels` | all four | `opt` = release-fast, `noopt`, `devfast` = dev-fast, `safe` = release-safe |
| `--jobs` | 4 | files in parallel; the levels of one file run one after the other (tests that use a fixed temp path would collide otherwise). Timing sensitive tests (threads, shell) can still flake under load: `--jobs 1` is the exact `test.sh` behaviour |
| `--timeout` | 120 | seconds per compile and per run; the child is killed and the case fails |
| `--filter` | none | substring of the relative path |
| `--out` | stdout | write the report to a file |

Exit code: 0 all passed, 1 at least one failure, 2 usage or environment error (also: nothing selected, so an empty run is
never green). Skips do not fail the run.

## Rules (identical to `test.sh`)

- line 1 `// expect_exit: N` or `// expect_out: TEXT` (trailing newlines of the output are ignored, exit code must be 0)
- `// only_mode: opt` in the first five lines: run only at `opt`; `// not_mode: opt`: do not run at `opt` (both reported as skipped)
- a file with `#[test]` and no `fn main` runs once, as `firnc --test --opt-level=dev-fast`, one case per test function
- `tests/neg/*.fi`: `// expect_error: L:C TEXT` (optional line 2 `// expect_error_count: N`); the compile must fail with a
  clean message that has the position, the text and a `^` marker

## Report

Every case has: `name`, `level` (also under the old key `mode`), `kind` (`run`, `negative`, `unit`), `status`
(`pass`, `fail`, `skip`), `duration_ms`, `duration_us`, `file`, `line`, `col`, `message`.

- `json`: one object, `{"suite","total","passed","failed","skipped","rate","cases":[...]}`; `total` counts executed cases (no skips)
- `jsonl`: one case object per line, then `{"summary":{...}}`
- `junit`: `testsuites` with one `testsuite` per build level; `testcase` has `classname`, `name`, `file`, `line`, `time`;
  a failure carries `file:line:col: message`
- `text`: `PASS`/`SKIP`/`FAIL` lines, then `N of M passed`

`file:line:col` of a failure, in this order: the position in the panic message the program printed itself (the line table
agrees with it, see `docs/DEBUGGER.md`), the first position in a compiler error, the position the unit test runner reports
for a `#[test]` function, else line 1 (the expectation header). The order of the cases is the order of discovery,
independent of `--jobs`.

## Proof

`bash tools/testrunner/suite.sh` (section 53b of `test.sh`) runs the runner over `tools/testrunner/suite_cases`, a tree
whose result is known per case, and checks counts, keys, positions against the real source line, jsonl, junit (parsed as
XML), skips, `--levels`, `--jobs` independence, exit codes, the timeout, and that the scratch directory is gone. A shifted
line number must make the position check strike.
