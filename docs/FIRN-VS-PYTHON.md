# FIRN-VS-PYTHON -- when does Firn reach "Python-size" code?

Status 2026-10-17. Question (Boss): when can Firn libs be stacked on libs so that very little code does very much, like Python?
Method: 20 typical tasks, each written twice (idiomatic stdlib Python 3.11 vs. the shortest honest Firn using `lib/`),
same input, same output (`diff` against `expect.txt`), measured with `tools/vspy/measure.py`.
Everything below was run, not estimated. Machine was loaded (load 25-40): absolute times are rough, ratios are stable.

## 1. Answer in 6 lines

- **Libs: almost there.** Where a lib exists with a *facade* API (markdown, argparse-like, websocket, logging) Firn is as short as Python (factor 0.2-1.4).
- **Glue is the problem, not the libs.** Factor 2-5 comes from missing language ergonomics: no `for x in vec`, no default/named args, no comprehensions, f-string is not a `str`, no `Map[str,V]`, no capturing closures, no `Option`.
- **Facades were missing** for ~10 of 20 tasks (scraper, image, PDF flow, find, parallel map, blocking mail, dict, JSON building). This round added them (see section 5); the table shows before/after.
- **Run time:** a Firn binary starts in 1-4 ms, Python in 25-45 ms (+ imports up to 200 ms). Firn is 10-100x faster to run, but 0.5-7 s slower to *build* cold (`firn-run` caches: warm 3 ms).
- **Tooling:** script mode (`firn-run`, shebang), formatter, LSP, `--test`, lock files exist. Missing: a `firn` driver (prototype added), REPL (prototype, 0.7 s/input), package fetch, tutorial.
- **Rough estimate:** with the P0 items of section 6 done, typical scripts land at factor ~1.0-1.5 of Python lines (tokens ~1.3-2, because of explicit types).

## 2. Method

| What | How |
|---|---|
| Lines | non-blank, non-comment source lines (`task.py`, `task.fi`) |
| Tokens | Python `tokenize` (no NEWLINE/INDENT/COMMENT); Firn `firnc --emit=tokens` |
| Imports | number of `import` statements |
| Run time | median of 5-7 wall-clock runs; Firn = `--opt-level=dev-fast` binary |
| Compile | one cold `firnc` build (no cache) |
| Equality | `check`: Firn output == Python output == `expect.txt` (window task 16: Xvfb screenshot of both) |

Files: `examples/vs_python/NN_name/{task.py,task.fi,expect.txt}`, results `examples/vs_python/results_*.tsv`, runner `tools/vspy/measure.py`, table `tools/vspy/table.py`.
Python reference notes: 11 uses Pillow (installed), 13 and 08 use stdlib only (`markdown`/`bs4` would be 3-6 lines), 12 stdlib raw PDF (reportlab ~15 lines).

## 3. Result table (latest row per task)

| Task | Py lines | Firn lines first pass | Firn lines now | Factor now | Py tokens | Firn tokens | Factor | Py imports | Firn imports | Py run ms | Firn run ms | Firn compile ms |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 01_http_json_api | 43 | 58 | 57 | 1.33 | 345 | 569 | 1.65 | 5 | 6 | 217.6 | 9.4 | 8168 |
| 02_csv_json_report | 21 | 105 | 100 | 4.76 | 242 | 1012 | 4.18 | 3 | 9 | 168.3 | 3.1 | 2135 |
| 03_find_rename | 33 | 78 | 60 | 1.82 | 401 | 535 | 1.33 | 1 | 5 | 175.0 | 37.3 | 2194 |
| 04_regex_text | 20 | 58 | 58 | 2.9 | 230 | 631 | 2.74 | 2 | 7 | 171.1 | 8.8 | 2615 |
| 05_download_extract | 16 | 37 | 37 | 2.31 | 183 | 402 | 2.2 | 1 | 8 | 166.6 | 16.6 | 6560 |
| 06_sqlite_query | 15 | 35 | 35 | 2.33 | 79 | 327 | 4.14 | 1 | 6 | 64.8 | 4.0 | 3972 |
| 07_send_mail | 12 | 11 | 11 | 0.92 | 87 | 83 | 0.95 | 3 | 3 | 164.8 | 16.9 | 4657 |
| 08_web_scraper | 38 | 27 | 27 | 0.71 | 329 | 278 | 0.84 | 1 | 4 | 64.8 | 6.0 | 3413 |
| 09_cli_args | 31 | 32 | 32 | 1.03 | 244 | 336 | 1.38 | 1 | 2 | 64.7 | 3.5 | 773 |
| 10_async_client | 10 | 26 | 18 | 1.8 | 89 | 143 | 1.61 | 3 | 4 | 316.0 | 64.5 | 4301 |
| 11_image_resize_convert | 8 | 24 | 11 | 1.38 | 73 | 99 | 1.36 | 1 | 2 | 165.4 | 32.4 | 3873 |
| 12_pdf_create | 55 | 71 | 25 | 0.45 | 546 | 248 | 0.45 | 2 | 4 | 64.8 | 8.1 | 1599 |
| 13_markdown_to_html | 63 | 11 | 11 | 0.17 | 618 | 90 | 0.15 | 2 | 4 | 65.5 | 1.9 | 1408 |
| 14_timer_scheduler | 25 | 43 | 43 | 1.72 | 202 | 342 | 1.69 | 1 | 2 | 516.6 | 415.5 | 603 |
| 15_parallel_map_files | 17 | 68 | 64 | 3.76 | 168 | 540 | 3.21 | 2 | 7 | 164.9 | 64.4 | 1019 |
| 16_fui_list_button | 14 | 26 | 26 | 1.86 | 114 | 279 | 2.45 | 1 | 3 | -1 | -1 | 11231 |
| 17_websocket_chat | 24 | 21 | 20 | 0.83 | 217 | 166 | 0.76 | 3 | 4 | 64.8 | 3.9 | 3602 |
| 18_logging | 25 | 41 | 41 | 1.64 | 234 | 359 | 1.53 | 1 | 5 | 165.4 | 4.2 | 895 |
| 19_tests | 41 | 96 | 50 | 1.22 | 555 | 501 | 0.9 | 3 | 4 | 114.5 | 3.8 | 1088 |
| 20_canvas_game | 33 | 67 | 56 | 1.7 | 385 | 409 | 1.06 | 0 | 2 | 64.3 | 3.8 | 2774 |
| p01_dataclass_sort | 10 | 20 | 20 | 2.0 | 74 | 216 | 2.92 | 1 | 2 | 51.2 | 0.8 | 572 |
| p02_wordcount | 8 | 43 | 25 | 3.12 | 80 | 236 | 2.95 | 1 | 4 | 25.2 | 1.0 | 736 |
| p03_parse_errors | 7 | 15 | 15 | 2.14 | 37 | 89 | 2.41 | 0 | 2 | 34.3 | 0.6 | 524 |

Re-measured 18.10.2026 (vspy-text, 3 runs, host load 36-46: Py ms inflated; line/token numbers exact). Median unchanged 1.67.
Shrink expected from open features (notes only, no rewrite): 02/15 default+named args; 04 (58 lines) enumerate + f-string format specs (`{w:<10}`), `Counter`-like dict_top already used; 09/18 named args; 03 enumerate.

Summary (`table.py --stats`): `tasks=20 median_factor_now=1.67 median_factor_first=1.94 le1.5_now=9 le1.5_first=7`.
Live numbers: `python3 tools/vspy/table.py` (reads `examples/vs_python/**/results_*.tsv`).
Ratios < 1 = Firn shorter. "first pass" = first working Firn version with the libs that existed on 2026-10-17 morning; "now" = after this round's libs.
Run times: Firn programs run 5-100x faster than `python3` (start 1-4 ms vs 25-45 ms + imports); exception: 14 (timer test is wall-clock bound). Task 10 first looked 5x slower (1278 ms vs 233 ms): the cause was the Python test server (listen backlog 5 -> SYN retransmit), not Firn; fixed in the harness: 61 ms vs 174 ms.

## 4. Findings per task class

| Class | Tasks | Lines now (Firn/Py) | Why |
|---|---|---|---|
| **At Python level or better** | 07 mail, 08 scraper, 12 PDF, 13 markdown, 17 websocket, 09 CLI | 0.17 - 1.03 | A lib with a facade API exists (`html.soup`, `pdf.pdfflow`, `markdown`, `ws`, `std.args`): one call per Python call. |
| **Close (1.2 - 1.8x)** | 01 HTTP API, 03 find/rename, 10 async client, 11 image, 14 timer, 18 logging, 19 tests, 20 game | 1.2 - 1.8 | Facade exists, rest is explicit types, error handling (`catch 0` on every I/O call, `main` cannot `try`) and callbacks instead of closures. |
| **Far (2 - 5x)** | 02 CSV/JSON report, 05 download/extract, 06 SQLite, 15 parallel map, p02 wordcount | 2.3 - 4.8 | Glue code Python gets for free: dict/`Counter`, `sorted(key=)`, group-by, `for row in rows`, comprehensions, f-string formats, closures for callbacks. |

What makes Firn code long (counted over all 20 task files):

1. **No iteration protocol**: `for x in vec` / `for c in str` / `for i, x in enumerate()` do not exist; every loop is `while i < vec_len(..)`.
2. **No default/named args, no kwargs**: every option is a setter call or a struct field.
3. **f-string is not a `str`** (it is a `Fmt` for `io`) and has no format spec (`{x:>8.2f}`); `std.textfmt` and `io.fmt_string` are workarounds.
4. **Error handling is explicit**: `try`/`catch` per call; `main` cannot return an error union.
5. **No closures with capture** (only `gc fn`): timers, `sorted(key=)`, parallel map need a context struct.
6. **No generic containers with text keys, no `Option[T]`**: `Dict[V]` (new) is a lib workaround; `vec_at` needs `Scalar`.
7. **Qualified generic types do not parse** (`vec.Vec[i32]`), so libs export bare names (`dict_*`, `vec_*`).

Where Firn wins: run time (start 1-4 ms, no interpreter), a static binary without installed runtime, memory-safety errors caught at compile time (use-after-move, constant index), integer overflow panics with file:line.
Honest caveats: Python references use only the stdlib (except 11: Pillow; 13/08: no markdown/bs4 -> with those packages Python would be 3-6 lines, i.e. Firn 13 would be 2-4x longer). Token counts are higher than line counts suggest because of explicit types.

## 5. Libs added in this round

Landed on main in this round (all with tests on 4 opt levels; cross-checks against Python/Pillow/bs4/poppler with 0 deviations unless noted):

| Lib | Replaces (Python) | Notes |
|---|---|---|
| `std.args` | `argparse` | subcommands, flags, typed options, defaults, `--help`, error texts byte-equal to Python 3.11 (test 3800) |
| `std.log` | `logging` | named loggers, inherited level, stream + rotating file handler, `infof/errf` (3801, 3826) |
| `std.textfmt` | `'%.2f' % x`, `f"{s:<8}"`, `round()` | exact half-even on the binary value, 3000 cases x 4 levels vs Python (3730) |
| `io.fmt_string` / `text.dup` | `str(f"..")` | f-string to `str`; fix for `"" + s` aliasing |
| `net.web` | `urllib.request` | get/post/post_json/download (3860) |
| `http.server` listen/port | `http.server` | port 0, background use |
| `html.soup` | `bs4` + `select()` | parse/select/text/attr over the existing HTML5 parser + `css.sel`; 2400 selector runs vs bs4/soupsieve, fuzz 600 pages (3750) |
| `css.sel` | - | + `:nth-of-type` family, `:first/last/only-of-type`, `:empty` |
| `std.jsonstr` | `json.dumps(indent, sort_keys)` | `str` API instead of `ptr,len`; `dumps` with indent + sorted keys (3751) |
| `image.picture` | `PIL.Image` | open/resized/fit/cropped/gray/save by extension, PNG/JPEG/WebP/GIF decode, 400 cases vs Pillow (3740) |
| `pdf.pdfflow` | `reportlab.platypus` | paragraph with wrap, table, page break, JPEG pass-through, sticky error; 80 docs vs poppler (3741) |
| `paint.draw` | `PIL.ImageDraw` | rect/circle/line/rounded, save png/ppm (3742) |
| `std.dict` | `dict`, `Counter` | text keys, insertion order, `inc`, `keys_sorted`, `top`; 400 op sequences vs Python, 0 deviations (3820) |
| `fsx.find` | `glob` + `os.stat` filters | pattern + size + mtime, sorted (3821) |
| `jobs.map*` | `concurrent.futures.map` | result per index, no context struct (3822) |
| `std.vecfn` | `sum/map/filter/reduce` | plain `fn` callbacks (3823) |
| `std.testkit` | `unittest` subTest/parametrize/fixtures | table helpers, `sub_*`, fixtures, `assert_ne/lt/..` (3824, 3825) |
| `tools/firn/firn` | `python3 x.py`, `python -m ...` | driver: run/build/check/test/fmt/new/repl/eval/cache/doctor (shell prototype, 21/21 checks) |
| `tools/vspy/*` | - | measure.py, table.py, check_sys.sh, gui_shot.sh, langprobe.sh |

Later in the same round (see the worker reports in `projects_kb/firn/reports/*vspy-*`): `mail.send` (blocking SMTP), `ahttp.get_all`, `http.serve_background`, `ws.connect_url` (net2); `regex.findall`, `str` front split/join/lower (text); `firnc` finds the stdlib without `FIRNLIB`, `for x in vec`, default/named args (fnc).

## 6. Gap list (priority)

P0 = blocks a short solution of a very common task. P1 = clear pain, workaround exists. P2 = nice to have.
Effort S <1 day, M 1-3 days, L >3 days. Status `done` = landed on main in this round.

| ID | Prio | Area | Gap (evidence) | Fix | Effort | Status |
|---|---|---|---|---|---|---|
| G01 | P0 | tooling | `firnc` needs `FIRNLIB` (`import std.io` -> `cannot read dir/std/io.fi`) | sysroot lookup relative to the binary | S | worker vspy-fnc |
| G02 | P0 | language | `for x in vec` / `for c in str` rejected (`'for ... in' needs an array`) | iteration sugar (desugar to index loop) | M | worker vspy-fnc |
| G03 | P0 | language | DONE (r664): `fn f(a: i32, b: i32 = 2)` works, constant defaults filled at the call site (docs/DEFAULT-ARGS.md) | constant defaults filled at call site | M | worker dargs |
| G04 | P0 | language | DONE (r664): named args `f(b: 1, a: 4)` work on plain functions, generic templates and method calls (r704; methods matched by bare name) | compile-time resolution | M | worker dargs |
| G05 | P0 | lib | no blocking SMTP send (sans-IO only, ~40 lines callbacks) | `mail.send(host, port, from, to, msg)` | M | **done** (`mail.mail`: `mail.send`, 11 lines vs Python 12) |
| G06 | P0 | lib | no `regex.findall`, no `str.lower/split/join` front | Python-like `str` front + findall | S | worker vspy-text |
| G07 | P0 | lib | no scraping API (dom.api = browser DOM) | `html.soup` | M | **done** |
| G08 | P0 | lib | JSON API takes `u64` ptr/len; no `dumps(indent, sort_keys)` | `std.jsonstr` | S | **done** |
| G09 | P0 | lib | no Pillow-like image facade | `image.picture` | M | **done** |
| G10 | P0 | lib | PDF writer without flow layout, JPEG re-encoded | `pdf.pdfflow`, `pdf_image_jpeg` | M | **done** |
| G11 | P0 | lib | no argparse / logging | `std.args`, `std.log` | M | **done** |
| G12 | P0 | lib | no number formatting (`:>8.2f`, `round`) | `std.textfmt` | M | **done** |
| G13 | P0 | lib | no `Dict`/`Counter` with text keys | `std.dict` | M | **done** |
| G14 | P0 | lib | find by glob+size+mtime = 25 lines | `fsx.find` | S | **done** |
| G15 | P0 | lib | parallel map needs context struct + raw pointers | `jobs.map*` | M | **done** |
| G16 | P0 | lib | no sum/map/filter/reduce | `std.vecfn` | M | **done** |
| G17 | P0 | lib | tests: no fixtures, parametrize, subTest | `testkit` extensions | M | **done** |
| G18 | P0 | lib | no HTTP client/server convenience | `net.web`, `http.server` listen/port | M | **done** |
| G19 | P0 | lib | no drawing facade | `paint.draw` | S | **done** |
| G20 | P0 | bug | `"a" + f"{n}"` accepted, wrong result / hang; `"" + s` aliased | `text.dup` (done); type error for `str + Fmt` | S | dup **done**, Fmt check: vspy-fnc |
| G21 | P1 | language | f-string format spec: DONE `{x:>6}` `{x:.2f}` `{n:05}` `{n:x}` `{x:.1%}` (r665, firnc0+firnc1; no `,`/`e`/`g`); `{{` + nested quotes done (r664) | spec -> `io.fmt_spec_*` | M | **done** (tools/fmtspec_cross: 0 diffs vs Python); open: thousands `,`, `g`/`e` |
| G22 | P1 | language | `for (i, x) in enumerate(v)` and `for (x, y) in zip(a, b)` DONE (r666, firnc0+firnc1); no comprehension, no general tuple patterns in `for` | comprehension syntax later | M | enumerate/zip **done**; open: comprehensions, `for (a, b) in vec_of_tuples` |
| G23 | P1 | language | no `Option[T]` (generic enums not possible) | generic enums + `Option[T]` + `?` | L | open |
| G24 | P1 | language | capturing closures only as `gc fn` (+`try`); timers/sort keys need ctx struct | stack closures for non-escaping use | L | open |
| G25 | P1 | language | `mod.fn[T](..)` works (r701, both compilers, tests 3970-3974); still open: `mod.Type[T]` and generic fns are not `fn` values | qualified generics | S-M | partial |
| G26 | P1 | language | `main` cannot `try`; `catch | e | return x` rejected | `fn main() -> Err!i32`, `catch return` | M | open |
| G27 | P1 | language | no `derive(Eq, Debug, Hash, Ord, Clone)`; struct `==` and print missing | derive via comptime/reflection (r23) | L | open |
| G28 | P1 | language | slicing `s[1..3]`, negative index, `x in a`, `[]T` slice type, `.len()` on arrays | slice type + operators | L | open |
| G29 | P1 | language | no return-type inference, no generic argument inference (`mx[i32](..)` required) | local inference | M | open |
| G30 | P1 | language | struct shorthand `P { x, y }` / `..base`, `const N` as array length, multi-line / raw strings, `static` with str/Vec init | small parser/sema items | S each | open |
| G31 | P1 | language | no operator overloading (SPEC says no) | decision: keep rejecting or `impl Add` | M | decision |
| G32 | P1 | runtime | null dereference = SIGSEGV exit 139, no message | `panic: null dereference at file:line` in safe levels | M | open |
| G33 | P1 | compiler speed | `import std.io` alone = 0.49 s cold (no import: 10 ms); whole modules are optimised/emitted | reachability pruning before optimizer/codegen | M | open |
| G34 | P1 | tooling | REPL: prototype only (`firn repl`, 0.7 s per input) | prelude object + incremental backend cache, after G33 | L | open |
| G35 | P1 | tooling | packages: only local paths (`needs NAME ../path`); no git URL, fetch, registry, `add`, `update` | `needs NAME git:URL rev` -> `.firn/deps`, rev in `firn.lock`, `firn add/vendor` | L | open |
| G36 | P1 | tooling | `firn` driver is a Linux shell prototype | native Firn driver, Windows/macOS | M | open |
| G37 | P1 | lib | no WebP encoder (decoder only) | `lib/webp/encode.fi` (VP8L first) | M-L | open |
| G38 | P1 | lib | pdfflow needs a `.ttf` file (no base-14 fonts) | built-in Helvetica metrics | M | open |
| G39 | P1 | lib | db: no row iterator (`for r in rows`), `":memory:"` creates a file named `:memory:` | `db_query_each`, in-memory VFS | M | open |
| G40 | P1 | lib | group-by + sorted-by-key = 25 lines of insertion sort (02) | `std.group`, `vec_sort_by` with key fn | M | open |
| G41 | P1 | lib | HTTP server in a thread: spin loop, stop() waits up to 1 s; no user-data pointer | `serve_background`, `App.ud` | M | **done** (`__thread_work` line still mandatory: G49) |
| G42 | P1 | lib | 10: Firn 1278 ms vs Python 233 ms for 20 concurrent GETs | root cause was the Python test server (backlog 5 -> SYN retransmit), not Firn; `ahttp.get_all` added: 61 ms | M | **done** |
| G43 | P1 | lib | `--test` summary has timings (not diffable) | plain format in runner | S | open |
| G44 | P2 | language | shadowing `let x` twice rejected (hurts REPL) | allow | S | open |
| G45 | P2 | diagnostics | no hints: missing import (`did you forget import std.io?`), unmatched `{` (point to opener), unused `E!T` (`use try/catch`) | hints in sema/parser | S | open |
| G46 | P2 | tooling | no linter (unused var/import), no coverage; `firnfmt --help` missing | `firn lint` | M | open |
| G47 | P2 | docs | no tutorial/cookbook (only `tour.fi`, 37 lines); LIBS.md shows 6 names, no signatures/examples | 10-chapter tour from the vs_python pairs; usage snippet in each header | M | partly (std.str/math/cmdline: vspy-text) |
| G48 | P2 | lib | paint.draw without text/ellipse/polygon; pdfflow one font | extend | S-M | open |
| G49 | P1 | language | every program using threads/`serve_background` needs a hand-written `__thread_work` line (a lib cannot define it) | lib-owned thread kinds / auto dispatcher | M | open |
| G50 | P1 | lang | `ws.connect`-style error types `ws.WsError!bool` do not parse; `for x in ["a","b"]` needs a named array | qualified error types, literal in for-in | S-M | open |

## 7. Language / tooling check (Python niveau)

Evidence: `examples/vs_python/lang/` (79 probe files `.probe`, output in `PROBES.txt`, runner `tools/vspy/langprobe.sh`, driver test `bash tools/firn/test_firn.sh` 21/21).

| Item | Python | Firn today | Status |
|---|---|---|---|
| Script mode without build | `python3 x.py`, 25 ms | `firn-run x.fi` / shebang `#!/usr/bin/env firn-run`: warm 2-4 ms, cold 0.5-0.7 s (cache); `firn run x.fi` (driver) | **works** |
| One command (`run/build/check/test/fmt/new`) | `python -m ...`, `pip` | `tools/firn/firn` (shell prototype, Linux) | partial |
| REPL | `python` | `firn repl` / `firn eval '6*7'` rebuilds the whole program per input: 0.7 s | prototype |
| Package manager | pip + PyPI | `firn.package` with `needs NAME ../path [version]`, `--package`, `--lock/--locked` (hash pin), version check. No git URL, fetch, registry, `add`, `update` | partial |
| Stdlib found automatically | yes | needs `FIRNLIB` (fix running: G01) | gap |
| Type inference | yes | `let/var` inferred, generics only with `f[T](..)`, no return-type inference | partial |
| Short syntax | defaults, kwargs, slices, `in` | none of default args, named args, slices, `in`, negative index | gap |
| Comprehensions / iterators | yes | no `for x in vec`, no enumerate/zip, no generators; `std.vecfn` map/filter/sum as lib | gap |
| f-strings | `f"{x:>8.2f}"` | `f"{a + b}"` expressions work, but result is `Fmt` not `str`, no spec | partial |
| `with` / RAII | `with open()` | `defer` | works |
| dataclass | `@dataclass` (eq, repr, order) | plain `struct`; no `derive`, no `==`, no print | gap |
| Tuples / unpacking | yes | `let (q, r) = f()` | works |
| Enums + match | 3.10 `match` | `enum` with payload + `match` | works |
| Error handling | exceptions | `Err!T`, `try`, `catch` (explicit, verbose, `main` cannot `try`) | works, verbose |
| Optional | `None` | no `Option[T]` | gap |
| dict / set | built in | `Map[u32,V]`, `BMap[V]`, `Set`, new `Dict[V]` (text keys) | partial |
| Closures | lambda | no-capture closures; capture only `gc fn` | gap |
| Error messages | good | file:line:col, source line + `^`, several errors per run, `did you mean 'x'?`; 17 probes: 13 good, 3 ok, 1 bad (null dereference = SIGSEGV without text) | **good** |
| Docs / examples | huge | 123 examples, 180 docs, `tour.fi` (37 lines); no tutorial/cookbook; LIBS.md generated (802 modules, purpose + names, no signatures) | partial |
| Formatter / LSP / tests | black / pylsp / pytest | `firnfmt -c/-w`, `firnc --lsp` (hover/def/complete/rename), `firnc --test` (TAP/JSON) | works |
| Linter / coverage | pylint / coverage | none | gap |

Compile time (cold, `dev-fast`): hello 10 ms, any program importing `std.io` 0.5 s, typical task of this study 0.8-7 s (the libs are compiled whole, no pruning: G33). With the cache a script starts in 2-4 ms, so only the first run and every edit pay.

## 8. Next steps

1. **Language ergonomics first** (biggest lever for the 2-5x tasks): G02 `for x in vec` and G03/G04 default + named args are done (r664); next G22 (enumerate / tuple patterns), G21 (format spec -> `str`), G26 (`main` with `try`), G25 (qualified generics -> libs can use proper names).
2. **Compile speed** (G33): reachability pruning before optimizer/codegen. It makes cold script start and any REPL ~10x faster and is the precondition for G34.
3. **Packages** (G35): git URL + rev pin in `firn.lock`, `firn add/vendor`. Needed before a community lib ecosystem is possible.
4. **Option[T] / closures / derive** (G23, G24, G27): large (L), but they decide whether Firn code reads like Python for data-processing code. Needs a design decision per item (SPEC currently says no operator overloading, no generic enums).
5. **Re-run this study** after each block: `python3 tools/vspy/measure.py examples/vs_python/<task>` + `python3 tools/vspy/table.py`. Target: median line factor <= 1.5 (now ~1.7), no task above 3x.

Roadmap items: see ROADMAP in the JARVIS project (group "Python-Niveau"), one item per gap above.
