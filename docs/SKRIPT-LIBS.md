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

### std.csv — `lib/std/csv.fi` (r313) — DONE (08.10.2026)
**Landed API differences to the sketch below:** `error` is a keyword in Firn, so the reader's error is `csv.error_code(&r)` (+ `error_row`, `error_line`, `error_errno`, `error_text`, constants `csv.E_NONE/E_UNTERMINATED/E_BAD_QUOTE/E_IO`); the writer's setters are `set_writer_delimiter/_quote/_doublequote` and `set_terminator`; `reader_str(s)`, `reader_fd_sized(fd, n)`, `set_quote`, `set_doublequote`, `set_strict` (default strict = errors, `false` = Python's default), `table_load(&reader)`, `has_cell`, `row_len`, `csv.NO_COL`. Proof: `tools/csv_cross/run.sh` (test.sh section 101), tests 2300–2302, `examples/csv_report.fi`.

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

### std.glob — `lib/std/glob.fi` (r314/r315) — DONE (08.10.2026)
**Landed API:** `glob.matches(pat, name)`, `glob.expand(pat, &out)`, `glob.expand_opts(pat, flags, &out)`, `glob.expand_in(root, pat, flags, &out)` (names relative to `root`, like `root_dir=`), `glob.nth(&out, i)`, `glob.count_names(&out)`, `glob.has_magic`, `glob.escape(s, &out)`; flags `GLOB_RECURSIVE`, `GLOB_HIDDEN`, `GLOB_FOLLOW` (`**` also walks symlinks to directories, max 40 hops; default: listed, not walked). `?` and `[..]` work on UTF-8 characters like Python's `str`. Proof: `tools/glob_cross/run.sh` (test.sh section 102), tests 2310/2311, `examples/glob_find.fi`.

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

**Built (08.10.2026, branch `w-fsx`).** `lib/std/fsx.fi`; tests `tests/2341_std_fsx_temp.fi` (TempDir/TempFile, defer on the error path, registry,
cleanup by a failing child, copy_file) and `tests/2342_std_fsx_tree.fi` (fnmatch, copy_tree, walk, paths); cross-check `tools/fsx_cross/run.sh`
(test.sh section 102: 500 random trees against `cp -a` and `shutil.copytree`, 150 files against `cp -p`, 6000 path pairs against `os.path`, 6000
patterns against `fnmatch.fnmatchcase`, plus a self-test that the comparison strikes); `examples/fsx_script.fi`. Differences from the draft above:
* The call is `fsx.copy_tree(from, to, ignore)` with `ignore` = fnmatch patterns separated by `|` (matched against entry names at every level),
  `fsx.copy_tree_opts(from, to, ignore, flags)` with `COPY_DIRS_EXIST_OK` / `COPY_FOLLOW_SYMLINKS`; the answer is the number of entries made.
* `walk(root, cb, ctx)`: `cb(path, kind, ctx) -> i32` answers `WALK_CONTINUE` / `WALK_SKIP` / `WALK_STOP`; sorted, pre-order, links not followed.
* Extra: `temp_dir_in(base, prefix)`, `temp_dir_keep`, `temp_file_new(prefix)` (armed in the registry), `normpath`, `cwd`, `fnmatch`, `registered_count`.
* `temp_dir_drop` / `temp_file_drop` answer `bool` (not an error union) so that `defer` can call them.
* `cleanup_all` is called by `testkit` (failed assertion and `finish`; testkit therefore imports fsx). It only removes what THIS process armed
  (the registry stores the pid), so a forked child cannot delete the parent's directories. The `firnc --test` runner itself does not call it
  (it has no imports by design); a test that fails through testkit does.
* `copy_file_range` is not used (no entry in the AArch64/browser tables of `compiler/src/syscalls.rs`; a two-line change there would allow it);
  `FICLONE` (reflink) is tried first, but only its fallback could be tested here (ext4).
* Names: the fnmatch of Python merges the chunks of a reversed range (`[]-[!a]`) in a way no other glob does; std.fsx treats a reversed range as
  empty, and the cross-check leaves such patterns out (about 0.5 %). `?` and `[...]` work on octets, not on characters.
* Verified: x86-64 in the four build levels, AArch64 (tests 2340–2342 under qemu), Windows under Wine (temp_dir, copy_file, copy_tree; the
  symlink calls were not exercised there).

### std.stats — `lib/std/stats.fi` (r316)
Over `*mut f64` + `n` and over `Vec[f64]`:
`mean` (Neumaier compensated, same result as `statistics.fmean` to 1 ulp on test data), `median`, `percentile(p)`
(linear interpolation = numpy default and `statistics.quantiles(method="inclusive")`), `variance`/`stdev` (sample, n−1),
`pvariance`/`pstdev`, `min`/`max`, `sum`, `histogram(data, bins, lo, hi, counts)`.
Sorting: `f64` gets a **total order** — `math.total_cmp(a: f64, b: f64) -> i32` (IEEE 754 totalOrder: −NaN < −inf < … < +inf < +NaN)
and, if the language lets `f64` satisfy `Ord`, `vec_sort[f64]` works. `median` of an empty set / NaN input: documented, never a trap.
Cross-check against `statistics` and `numpy` (if installed) on random data incl. ties, huge/small magnitudes.

**Status (08.10.2026): built** — `lib/std/stats.fi`, `math.total_cmp`, `impl Ord for f64` in `lib/rt/vec.fi`; tests `tests/2350`–`2353`, `tools/stats_cross`, `examples/stats_basic.fi`.
How the design came out (module-qualified names: `import std.stats`, then `stats.mean(p, n)`):

* `f64` order: `math.total_key(x) -> u64` (monotone bit map), `math.total_cmp(a, b) -> i32` (−1/0/1), `math.total_less(a, b)`;
  in `std.math` and `std.core` (`lib/math/core_math.fi`). `impl Ord for f64` (rt.vec) uses the same map, so **`vec_sort[f64]` works**
  and puts NaN at the ends, −0.0 before +0.0. The language does not make f64 a `Scalar`, so `vec_at/vec_min/vec_max/vec_index_of[f64]`
  stay closed — read elements with `vec_elem_ptr[f64]`, or use `stats.min_vec/max_vec`. A program with its own `impl Ord for f64`
  (a workaround) must drop it, the impl is program wide.
* Raw form `stats.f(p: *mut f64, n: usize, ...)` and Vec form `stats.f_vec(&v, ...)`: `sum mean sum_exact mean_exact min max median
  percentile(p, n, pct) variance stdev pvariance pstdev histogram(p, n, bins, lo, hi, counts: *mut u64) -> counted sort`.
* `median`/`percentile` work on a private copy (the data is never reordered); `median_inplace`/`percentile_inplace` (and
  `*_vec_inplace`) use the input as working space and leave it permuted, no allocation. Quickselect, O(n), O(n log n) worst case.
* `sum`/`mean` = Neumaier. Added beyond the draft: `sum_exact`/`mean_exact` (Shewchuk partials, **identical to `math.fsum` /
  `statistics.fmean`**), because Neumaier is not bounded when terms of 1e30 and 1 cancel (measured, below).
* Bad input: empty set → NaN (`sum` 0.0), variance with n < 2 → NaN, any NaN in the data → NaN, p outside [0, 100] or NaN → NaN, a null
  pointer is the empty set. Never a trap. Infinities follow IEEE.
* `percentile` is **bit-identical to `numpy.percentile`** (position `(n-1)*(p/100)` in doubles, numpy's `lerp`); `median` is
  `statistics.median`'s `(a+b)/2`. `histogram` is `numpy.histogram` (edges `i*step+lo`, last bin closed, NaN/outside not counted).
* Measured (tools/stats_cross/run.sh, 5200 data sets, 217,793 compared values, Python 3.11, numpy 2.4.6), maximum deviation in ulp:
  `sum_exact` 0, `mean_exact` 0 (vs `fsum`, `fmean`); `median` 0; `percentile` vs numpy 0; `min`/`max` 0; `variance`/`pvariance` ≤ 3,
  `stdev`/`pstdev` ≤ 2 (vs `statistics`, exact fractions); `mean` ≤ 1 vs `statistics.mean`, 0 vs `fmean`; `sum` 0 — except the family
  “cancellation” (±1e30-sized terms with a residue of 1): there Neumaier is off by up to 3e15 ulp, `sum_exact` is exact.
  `percentile` against `statistics.quantiles(inclusive)` (exact integer positions): worst 1.5·eps·(n·span+|x|) — the weight
  `(n-1)*(p/100)` of numpy's method carries an error of n·eps, so the result is not bit-equal for p that are not exact in binary.
* Other targets: the four tests build and pass for `--target=aarch64-linux` (qemu-aarch64) and `--target=x86_64-windows` (wine); `stats_cli` answers
  400 random data sets byte for byte like the x86-64 build (no fused multiply-add, no flush-to-zero surprises).

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

**Built (08.10.2026, branch `w-fsx`).** `lib/std/testkit.fi`, tests `tests/2340_std_testkit.fi` (every assert run failing in a forked child,
exact message compared), `tools/testkit/run.sh` (test.sh section 101: `firnc --test` over assertions, script style, assert in `main`),
`examples/testkit_script.fi`. Differences from the draft above, on purpose:
* **No caller position.** The language has no caller-location intrinsic, so a failed assert cannot print `file:line:col` of the assert line;
  `firnc --test` therefore reports the position of the **test function** (its fallback when the message has no ` at file:line:col`).
  Instead `testkit.context("label")` adds ` [label]` to the first line of every failure. Open: a compiler intrinsic (e.g. `__caller_line()`)
  would let the message carry the real position.
* The generic `assert_eq[T]` must be called **without** the `testkit.` prefix (`assert_eq[u8](a, b)`); a module-qualified generic call is refused by the
  compiler ("only direct function names can be called").
* Extra: `assert_false`, `fail`, `assert_eq_u64`, `assert_ne_i64`, generic `assert_eq[T: Int]` (generic names are global in Firn, so it is
  called without the `testkit.` prefix), `check_detail`, `finish_named(label)`, `checks_run`/`checks_failed`/`reset`.
* `assert_near`: `|a-b| <= eps` OR `|a-b| <= eps * max(|a|,|b|)`; equal infinities are near, NaN is near to nothing.

### JSON `sort_keys` — `lib/std/json.fi` (r315)
`json_write_opts(d, i, out, opts)` with flags `JSON_SORT_KEYS`, `JSON_ASCII`, indent width, separators; output byte-identical to
`json.dumps(sort_keys=True, indent=N, ensure_ascii=…)`. Sort = by UTF-8 bytes (Python: by code point; identical for valid UTF-8).
Cross-check: random documents → `json.dumps`.

**Status (08.10.2026): built** — `json_write_opts` in `lib/std/json.fi`; the old `json_write` / `json_write_pretty` are untouched.

* `var o: json.JsonOpts = json.json_opts_new()` is `json.dumps(x)` (ensure_ascii on, one line, separators `, ` and `: `);
  `json_opts_compact()` = `separators=(",", ":")`, `json_opts_indent(n)` = `indent=n`; `json_opts_set_indent(&o, n)` (n < 0: one line),
  `json_opts_set_indent_text(&o, p, n)` (`indent="\t"`), `json_opts_set_separators(&o, item, n, key, m)` (≤ 16 octets each, in the struct).
  `o.flags`: `JSON_SORT_KEYS`, `JSON_ASCII`, `JSON_PY_NONFINITE` (writes `Infinity` / `NaN` as Python; default `null`).
* `json_write_opts(&d, node, &out, &o) -> bool` (false only if the memory for a sort ran out; that object then comes out in document order).
  Sort: stable merge sort of the member indexes by UTF-8 octets (= Python's code point order, NOT UTF-16), O(n log n), one 8n octet block per
  sorted object; equal keys keep the document order.
* The float text is Python's `repr`: `1e+16`, `1e-05`, `100.0`, `-0.0`, `5e-324`. (The old writers print the shortest digits without that exponent rule.)
* Found by the cross-check and fixed on the way: (1) `lib/num/dtoa.fi` took the LARGER digit at an exact tie of two shortest candidates (ES5); ES2019,
  Python, V8 and Ryu take the EVEN one (`1576992552742323.25` is `...23.2`). tools/dtoa_vectors/gen.rs (Rust still rounds ties up) now applies the even rule
  on exact ties, found with a u128 expansion, so the 300,000-vector comparison stays an independent check. (2) The writer turned invalid UTF-8 in a string
  into `\ud800` (a lone surrogate, which reads back as something else); in ASCII mode it is now one `\ufffd` per bad octet. Valid UTF-8 is unaffected.
* Still different from Python (J3/J4/J7 in json.fi): integers beyond i64 become doubles; a lone surrogate escape is U+FFFD; a duplicate key is kept twice.
* Measured (tools/jsonsort_cross/run.sh): 3300 random documents × 11 variants = 35,390 outputs compared byte for byte with `json.dumps` (10,433 objects, 7,193 floats,
  15,497 integers, 45,435 strings and keys, nested up to 120 deep, quotes/controls/DEL/accents/CJK/emoji/U+2028, prefix keys, UTF-16-order traps): 0 differences.
  Writer fuzz (20,000 mutated / junk documents, 6,437 parse, 64,370 round trips parse → write → parse → write): 0 failures, no crash.
* Speed (release-fast, loaded server): 100,000 random floats in a 2 MB document, parse + 11 writes: 6.4 s, i.e. about 0.5 s per write; Python's `json.dumps` needs
  0.1 s. The remainder is the exact big-number digit generation of `dtoa` (about 5 µs per float); before the scratch memory of `dtoa` was handed in once per
  document (`json_write_opts` only) it was 3.6 s per write, almost all of it 2 mmap/munmap pairs per number. The old `json_write` still pays them.
* Other targets: tests 2360/2361 pass on aarch64-linux (qemu) and x86_64-windows (wine); `jsonsort_cli dumps` on 300 documents × 11 variants is byte for byte
  the x86-64 answer on both. The whole lib is `rt`/`str`/`num` only, so nothing is Linux specific.

### std.xml — `lib/std/xml.fi` (r319, optional, small)
Well-formed XML only, no DTD/XSD/XPath: elements, attributes, text, CDATA, comments skipped, five entities + numeric references,
namespaces as plain `prefix:name` strings. Flat node arrays (like `std.json`). `xml.parse`, `xml.child`, `xml.children_named`, `xml.attr`,
`xml.text`, errors with line/column. Cross-check: `xml.etree.ElementTree` on random documents; fuzz.

### Regex lookaround — `lib/regex/regex.fi` (was r320 in the sketch) — DONE (08.10.2026), answer: YES, polynomial
`(?=X)`, `(?!X)`, `(?<=X)`, `(?<!X)` are supported, without backreferences, in the Pike VM: a lookaround is a zero-width instruction that
runs a Pike pass of its own over its body (`I_LOOK`; the body is compiled inline, ending in its own `I_MATCH`). **The linear-time promise
changes for patterns that use lookaround — honestly:** without lookaround still O(n·m); a lookahead is O(k·n²·m) worst case (QUADRATIC:
`(?=.*z)a` over `aaaa…` reads the rest of the text from every position); a lookbehind must be bounded (≤ 1000 characters) and costs
O(n·w·m); nested lookarounds (max 3 deep) are memoised per (instruction, position), so the exponent stays 2. **Never exponential** — an
ambiguous body such as `(?=(a|a)*b)` is quadratic as well, where Python's `re` takes 2ⁿ. Measured (release-fast, "a"×n, no match, per
doubling of n; `tools/regex_look/run.sh` prints the table and fails above x6.5): see test.sh section 111. Refused: a capture group inside
a lookaround (`(?:…)` is fine), an unbounded lookbehind (`(?<=a+)`), a quantifier after a lookaround (`(?=a)*`), nesting deeper than 3,
backreferences. Proof: tests 2330 and 1914, `tools/regex_look/run.sh` (6,000 random patterns against Python `re`, all four stages, plus the
unchanged 20,000-pattern `tools/libmvp/check_regex.py`). The two OpenPlan patterns now run unchanged: `(?<![A-Za-z])de:` and
`[A-ZÄÖÜ]+(?![a-zäöüß])` (both are rows of `check_look.py` and of test 2330). Note: the sketch called this r320, the roadmap's r320 is
`png.image_pixel`; the regex item is a new roadmap point.

### firn-run and shebang (r317)
`#!` on line 1 is skipped by the compiler; `tools/script_port/firn-run` compiles once into a content-hash cache and execs.
Proposal in `docs/SCRIPTS.md`.

### ndarray (r318, DRAFT ONLY)
`docs/NDARRAY.md`: design from concrete needs (LogicLab solver, OpenPlan thermal, Certus media). Nothing is built.

## Merge rules
Small merges to `main`, one lib per merge, only with its own tests green in all four stages; rebase on `main` before merging
(conflicts in `test.sh` or `docs/` index files: keep both sides). Roadmap items r311–r320 are ticked by the one who lands them.
