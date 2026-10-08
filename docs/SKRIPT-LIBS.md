# SKRIPT-LIBS — API design of the script libraries (r311–r320)

Design note written BEFORE the code (08.10.2026). Source of the need:
`docs/SKRIPT-ERSATZ.md` (what OpenPlan's ~120 Python scripts use). Goal: a Firn
program that does what a 40-line Python check script does should not need
100 lines of plumbing.

## House rules (all libs)

* One file per lib in `lib/std/` (`import std.csv` → `lib/std/csv.fi`), `export { ... }` list at the top,
  header comment with a usage snippet and a **HONEST** list (what it does NOT do), like `lib/std/process.fi`.
* Style of the existing std: free functions on a `*mut Struct` (`csv_next_row(&r)`), `str` for
  text in, `rt.Buf` for text out, error unions (`Err!T`) with `catch` for I/O, plain `bool`/count
  for pure parsers. No hidden global state, no allocation that is not freed by a documented `*_free`.
* Names: `snake_case`, prefix with the lib (`csv_*`, `glob_*`, `stats_*`) unless the module name already
  qualifies (`fsx.copy_file`).
* Everything must also build for `--target=aarch64-linux` and `x86_64-windows` where the OS layer allows
  it (file/process code may be Linux only; say so under HONEST, like `process.fi`).
* Code, comments, messages, test names: English. Docs may be German.
* Each lib comes with: (1) tests `tests/NNNN_std_<lib>*.fi` (`// expect_exit: 0`, run by `test.sh` in all
  four stages), (2) a cross-check against Python with random cases in `tools/<lib>_cross/run.sh`
  (+ section in `test.sh`), (3) a fuzz run for parsers (no crash, no hang, no leak; round-trip when
  the input is valid), (4) one example in `examples/`.
* Number ranges for tests: 2300–2399 (check `ls tests` first; take the next free numbers).

## Libraries

### std.csv — `lib/std/csv.fi` (r313)
Mirrors Python's `csv` (dialect: `delimiter`, `quotechar`, `doublequote`, `lineterminator`).
```
var r: csv.Reader = csv.reader(p, n)                  // over memory
csv.set_delimiter(&r, ';')                            // default ','
while csv.next_row(&r) {                              // false at end; error → csv.error(&r)
    let k: usize = csv.field_count(&r)
    let f: str = csv.field(&r, 0)                     // unescaped, valid until next_row
}
csv.reader_fd(fd)                                     // streaming: constant memory, any row length via growing buffer
var t: csv.Table = csv.table_parse(p, n)              // header + rows: csv.cell(&t, row, "name"), csv.rows(&t)
var w: csv.Writer = csv.writer(&out)                  // QUOTE_MINIMAL; csv.set_terminator(&w, "\n")
csv.put(&w, "a,b"); csv.put(&w, "x"); csv.end_row(&w)
```
Behaviour as Python: blank line = empty row (the Table skips it), `"` inside quotes doubled, CR/LF/CRLF,
quoted fields with newlines, a lone `\r` ends a row, unterminated quote = error (strict) with row number.
Cross-check: `csv.reader`/`csv.writer` on random tables with hostile characters.

### std.glob — `lib/std/glob.fi` (r314/r315)
```
glob.matches("*.json", "a.json")                      // fnmatch: * ? [abc] [a-z] [!x]; no path separators
glob.expand("library/*/*.json", &out)                 // sorted list, NUL-separated names in a rt.Buf; returns count
glob.expand_opts(pat, GLOB_RECURSIVE | GLOB_HIDDEN, &out)   // ** = zero or more directories
```
Rules as Python's `glob.glob(recursive=True)`: a leading `.` is not matched by `*`/`?`/`[` unless the pattern
starts with `.`; `**` only as a whole path component; results sorted bytewise (Python: sorted by caller);
no brace expansion, no `~`. Cross-check against `glob.glob` on random trees.

### std.fsx — `lib/std/fsx.fi` (r314)
```
var d: fsx.TempDir = fsx.temp_dir("trn_") catch | e | ...   // 0700, unique (getrandom), under shell.temp_dir()
defer fsx.temp_dir_drop(&d)                                  // remove_tree; idempotent; also keep(&d) to disarm
fsx.temp_dir_path(&d) → str
var f: fsx.TempFile = fsx.temp_file(&d, "x.json")            // file in the dir
fsx.copy_file(from, to) / fsx.copy_tree(from, to, ignore_glob) / fsx.walk(root, cb)
fsx.relpath(path, base) / fsx.abspath(path)
```
`copy_file` keeps the mode, writes via a temp name + rename (atomic), reflink/`copy_file_range` if the OS gives it.
`copy_tree` follows `shutil.copytree` (symlinks copied as links by default, `dirs_exist_ok`). `remove_tree` already
exists in `std.fs`; `fsx` only adds what is missing.
Auto-clean: `defer` for normal exits; additionally a process-wide registry cleaned by `fsx.cleanup_all()`
(called by the test runner and by `testkit.finish`) — a crash (signal) may leave the directory; HONEST says so.

### std.stats — `lib/std/stats.fi` (r316)
Over `*mut f64` + `n` and over `Vec[f64]`:
`mean` (Neumaier compensated, same result as `statistics.fmean` to 1 ulp on test data), `median`, `percentile(p)`
(linear interpolation = numpy default and `statistics.quantiles(method="inclusive")`), `variance`/`stdev` (sample, n−1),
`pvariance`/`pstdev`, `min`/`max`, `sum`, `histogram(data, bins, lo, hi, counts)`.
Sorting: `f64` gets a **total order** — `math.total_cmp(a: f64, b: f64) -> i32` (IEEE 754 totalOrder: −NaN < −inf < … < +inf < +NaN)
and, if the language lets `f64` satisfy `Ord`, `vec_sort[f64]` works. `median` of an empty set / NaN input: documented, never a trap.
Cross-check against `statistics` and `numpy` (if installed) on random data incl. ties, huge/small magnitudes.

### std.testkit — `lib/std/testkit.fi` (r311)
For `#[test]` functions and for plain `main` scripts.
```
testkit.assert_true(cond, "message")
testkit.assert_eq_i64(a, b)         // panic text: "assert_eq failed: left=3 right=4 at file:line"
testkit.assert_eq_str(a, b)         // prints first differing byte, line and a ± excerpt (a diff of the two texts)
testkit.assert_eq_bytes(p, n, q, m)
testkit.assert_near(a, b, eps)      // |a-b| <= eps (and relative), NaN != NaN
testkit.check(ok, "what")           // script style: prints "  ok    what" / "  FAIL  what", counts
testkit.finish() -> i32             // prints summary, returns 0/1 for `return` from main
```
Failures inside `#[test]` end the test through the panic path so `firnc --test` reports `file:line:col`.
`check`/`finish` give the output format of OpenPlan's Python `check()` so a port stays diff-able.

### JSON `sort_keys` — `lib/std/json.fi` (r315)
`json_write_opts(d, i, out, opts)` with flags `JSON_SORT_KEYS`, `JSON_ASCII`, indent width, separators; output byte-identical to
`json.dumps(sort_keys=True, indent=N, ensure_ascii=…)`. Sort = by UTF-8 bytes (Python: by code point; identical for valid UTF-8).
Cross-check: random documents → `json.dumps`.

### std.xml — `lib/std/xml.fi` (r319, optional, small)
Well-formed XML only, no DTD/XSD/XPath: elements, attributes, text, CDATA, comments skipped, five entities + numeric references,
namespaces as plain `prefix:name` strings. Flat node arrays (like `std.json`). `xml.parse`, `xml.child`, `xml.children_named`, `xml.attr`,
`xml.text`, errors with line/column. Cross-check: `xml.etree.ElementTree` on random documents; fuzz.

### Regex lookaround — `lib/regex/regex.fi` (r320, if feasible)
`(?=…)`, `(?!…)`, `(?<=…)`, `(?<!…)` without backreferences, so the "no catastrophic backtracking" promise holds
(worst case O(n·m·k), never exponential). Lookbehind: bounded length only. If it cannot be done without breaking the linear-time
guarantee, the answer is a documented "no" plus the two OpenPlan patterns rewritten.

### firn-run and shebang (r317)
`#!` on line 1 is skipped by the compiler; `tools/script_port/firn-run` compiles once into a content-hash cache and execs.
Proposal in `docs/SCRIPTS.md`.

### ndarray (r318, DRAFT ONLY)
`docs/NDARRAY.md`: design from concrete needs (LogicLab solver, OpenPlan thermal, Certus media). Nothing is built.

## Merge rules
Small merges to `main`, one lib per merge, only with its own tests green in all four stages; rebase on `main` before merging
(conflicts in `test.sh` or `docs/` index files: keep both sides). Roadmap items r311–r320 are ticked by the one who lands them.
