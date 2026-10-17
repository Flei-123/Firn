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
| 01_http_json_api | 43 | 58 | 58 | 1.35 | 345 | 563 | 1.63 | 5 | 6 | 116.0 | 3.7 | 4205 |
| 02_csv_json_report | 21 | 105 | 100 | 4.76 | 242 | 1012 | 4.18 | 3 | 9 | 39.1 | 1.6 | 823 |
| 03_find_rename | 33 | 78 | 60 | 1.82 | 401 | 535 | 1.33 | 1 | 5 | 80.8 | 13.7 | 1203 |
| 05_download_extract | 16 | 37 | 37 | 2.31 | 183 | 402 | 2.2 | 1 | 8 | 130.9 | 6.4 | 5712 |
| 06_sqlite_query | 15 | 35 | 35 | 2.33 | 79 | 327 | 4.14 | 1 | 6 | 44.1 | 2.3 | 4067 |
| 08_web_scraper | 38 | 27 | 27 | 0.71 | 329 | 278 | 0.84 | 1 | 4 | 45.8 | 3.7 | 2406 |
| 09_cli_args | 31 | 32 | 32 | 1.03 | 244 | 336 | 1.38 | 1 | 2 | 94.1 | 3.9 | 856 |
| 10_async_client | 10 | 26 | 26 | 2.6 | 89 | 218 | 2.45 | 3 | 5 | 233.0 | 1278.0 | 4284 |
| 11_image_resize_convert | 8 | 24 | 11 | 1.38 | 73 | 99 | 1.36 | 1 | 2 | 137.1 | 17.8 | 3079 |
| 12_pdf_create | 55 | 71 | 25 | 0.45 | 546 | 248 | 0.45 | 2 | 4 | 36.7 | 5.9 | 1382 |
| 13_markdown_to_html | 63 | 11 | 11 | 0.17 | 618 | 90 | 0.15 | 2 | 4 | 44.3 | 0.9 | 1233 |
| 14_timer_scheduler | 25 | 43 | 43 | 1.72 | 202 | 342 | 1.69 | 1 | 2 | 532.4 | 386.2 | 918 |
| 15_parallel_map_files | 17 | 68 | 64 | 3.76 | 168 | 540 | 3.21 | 2 | 7 | 156.4 | 48.5 | 1580 |
| 16_fui_list_button | 14 | 26 | 26 | 1.86 | 114 | 279 | 2.45 | 1 | 3 | -1 | -1 | 7511 |
| 17_websocket_chat | 24 | 21 | 21 | 0.88 | 217 | 178 | 0.82 | 3 | 5 | 45.7 | 2.2 | 2059 |
| 18_logging | 25 | 41 | 41 | 1.64 | 234 | 359 | 1.53 | 1 | 5 | 127.3 | 3.6 | 1073 |
| 19_tests | 41 | 96 | 50 | 1.22 | 555 | 501 | 0.9 | 3 | 4 | 94.0 | 3.4 | 1014 |
| 20_canvas_game | 33 | 67 | 56 | 1.7 | 385 | 409 | 1.06 | 0 | 2 | 48.0 | 2.9 | 3218 |
| p01_dataclass_sort | 10 | 20 | 20 | 2.0 | 74 | 216 | 2.92 | 1 | 2 | 51.2 | 0.8 | 572 |
| p02_wordcount | 8 | 43 | 25 | 3.12 | 80 | 236 | 2.95 | 1 | 4 | 25.2 | 1.0 | 736 |
| p03_parse_errors | 7 | 15 | 15 | 2.14 | 37 | 89 | 2.41 | 0 | 2 | 34.3 | 0.6 | 524 |

Summary (`table.py --stats`): `tasks=18 median_factor_now=1.67 median_factor_first=1.94 le1.5_now=8 le1.5_first=6`.
Task 07 (mail) and 04 (regex) were still being finished when this table was generated; see `examples/vs_python/results_*.tsv` for the live numbers (`python3 tools/vspy/table.py`).
Ratios < 1 = Firn shorter. "first pass" = first working Firn version with the libs that existed on 2026-10-17 morning; "now" = after this round's libs.
Run times: Firn programs run 5-100x faster than `python3` (start 1-4 ms vs 25-45 ms + imports); exception: 10 (concurrent GETs, 1278 ms vs 233 ms, being fixed) and 14 (timer test is wall-clock bound).

## 4. Findings per task class

| Class | Tasks | Lines now (Firn/Py) | Why |
|---|---|---|---|
| **At Python level or better** | 08 scraper, 12 PDF, 13 markdown, 17 websocket, 09 CLI | 0.17 - 1.03 | A lib with a facade API exists (`html.soup`, `pdf.pdfflow`, `markdown`, `ws`, `std.args`): one call per Python call. |
| **Close (1.2 - 1.8x)** | 01 HTTP API, 03 find/rename, 11 image, 14 timer, 18 logging, 19 tests, 20 game | 1.2 - 1.8 | Facade exists, rest is explicit types, error handling (`catch 0` on every I/O call, `main` cannot `try`) and callbacks instead of closures. |
| **Far (2 - 5x)** | 02 CSV/JSON report, 05 download/extract, 06 SQLite, 10 async, 15 parallel map, p02 wordcount | 2.3 - 4.8 | Glue code Python gets for free: dict/`Counter`, `sorted(key=)`, group-by, `for row in rows`, comprehensions, f-string formats, closures for callbacks. |

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

<!-- GAPS -->

## 7. Language / tooling check (Python niveau)

<!-- LANG -->

## 8. Next steps

<!-- NEXT -->
