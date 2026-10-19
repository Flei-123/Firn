# LIB-GAPS-VS-PYTHON -- which Python stdlib/builtin features Firn still lacks

Status 2026-10-19. Method: every row was checked by `grep` over `lib/` (paths below are real), not guessed.
Complements `docs/FIRN-VS-PYTHON.md` (language gaps G01-G50). Effort: S < 1 day, M 1-3 days, L > 3 days.
Priority: P0 = common in scripts and blocks a short solution, P1 = clear pain, P2 = nice to have.
Callbacks are plain `fn` values (no capturing closures), generic fns are called bare (`f[T](..)`), non-generic ones module-qualified.

## 1. Inventory (Python feature | Firn today | gap | benefit | effort | priority)

| Python feature | Firn today | Gap | Benefit | Effort | Prio |
|---|---|---|---|---|---|
| `heapq` | **NEW `std.heap`** (CPython-identical array, `_by` callbacks, nsmallest/nlargest) | done | Dijkstra, top-k, schedulers | S | done |
| `random` | **NEW `std.rng`** (MT19937, bit-exact with CPython 3.11: random/randint/randrange/shuffle/choice/sample/randbytes) | `choices`, `triangular`, `betavariate`, `getstate` | tests, simulations, shuffles | M | done |
| `collections.deque` | **NEW `std.deque`** (ring buffer, maxlen, rotate, extend) | done | BFS, sliding windows | S | done |
| `collections.Counter`, `defaultdict` | `std.dict` (`dict_inc`, `dict_top`, `dict_ref`), `std.pycoll` (`count_by`, `group_by`) | `Counter` arithmetic (`+`, `-`, `elements`), non-text keys use `Map[K,V]` | wordcount, group-by | S | P2 |
| `collections.OrderedDict` | `std.dict` keeps insertion order | none | - | - | ok |
| `collections.namedtuple` / `dataclass` | plain `struct`, tuples | no derive eq/repr/order (G27) | records | L | P1 |
| `collections.ChainMap` | missing | rare | - | S | P2 |
| `bisect` | `vec_lower_bound`, `vec_binary_search`, `vec_sorted_insert` (`std.vec`) | no `bisect_right` / `upper_bound`, no `key=` | sorted lists | S | P1 |
| `itertools` | missing (no `chain`, `product`, `permutations`, `combinations`, `groupby`, `accumulate`, `islice`, `cycle`, `zip_longest`, `pairwise`, `batched`) | whole module (eager `Vec` forms + `next_permutation`) | combinatorics, windows | M | **P0** |
| `functools` (`reduce`, `lru_cache`, `partial`, `cmp_to_key`) | `vec_reduce` (`std.vecfn`) | `lru_cache`/memoize (needs captures), `partial` | memoization | M | P1 |
| `sorted(key=)`, `min/max/sum`, `any/all` | `std.pycoll` (`sort_key_*`, stable), `std.vec` (`vec_min/max/any/all`), `std.vecfn` (`vec_sum/product`) | `min(key=)` generic over key type | daily | S | ok |
| `zip`, `enumerate`, `reversed`, `range`, slices | language sugar (`for (i, x) in enumerate(v)`, `v[a:b]`, `vec_invert`) | `reversed()` as iteration, `zip` of 3 | daily | M (lang) | P2 |
| `set` ops (`|`, `&`, `-`, `^`, `issubset`) | `Set[K]` in `std.map`: add/has/remove/first/next only | `set_union/intersect/difference/symdiff/is_subset` | dedup, diffs | S | **P0** |
| `statistics` | `std.stats`: mean, median, percentile, variance, stdev, pvariance, histogram, exact sum | `mode`, `multimode`, `fmean`, `geometric_mean`, `harmonic_mean`, `correlation`, `covariance`, `linear_regression`, `NormalDist`, `quantiles` | data scripts | M | P1 |
| `math` | `std.math`: sqrt, pow, exp, ln, log2, log10, sin/cos/tan, atan/atan2, hypot, floor/ceil/trunc/round, gcd/lcm, isqrt, fmod, frexp/ldexp, near | `asin`, `acos`, `sinh/cosh/tanh`, `asinh..`, `expm1`, `log1p`, `cbrt`, `copysign`, `degrees/radians`, `factorial`, `comb`, `perm`, `prod`, `fsum`, `erf`, `gamma`, `isclose` rel/abs | formulas | S | **P0** |
| `decimal`, `fractions` | `num.bignum` (big ints) | `Fraction`, `Decimal` | finance, exact math | L | P2 |
| `numpy` (ndarray) | design only (`docs/NDARRAY.md`), `std.stats` on `*f64` | typed 1-D/2-D view, elementwise ops, reductions, matmul, LU | numeric code, LogicLab port | L | P1 |
| `datetime` | `std.time`: civil date/time, weekday, ISO-8601 format+parse, tz database, unix | `strftime/strptime` with format strings, `timedelta`, date arithmetic helpers, `isocalendar` | logs, reports | M | **P0** |
| `pathlib` | `std.fs`/`std.fsx`: `path_join`, `path_parent`, `path_name`, `normpath`, `abspath`, `relpath`, `walk`, `copy_tree`, `remove_tree`, `temp_dir` | no `Path` object (`/`, `.suffix`, `.stem`, `.with_suffix`, `.read_text`) | scripts | S | P1 |
| `re` | `lib/regex` + `std.pyre`/`std.pystr`: search, match, findall, finditer, sub, subn, split, groups | named-group dict helper, `re.compile` flags table | text | S | P2 |
| `str` methods | `std.pystr`: split/join/strip/lower/upper/find/replace/count/center/ljust/zfill/title/translate/splitlines | `partition`, `format_map`, `casefold`, `expandtabs` | text | S | P2 |
| `textwrap`, `shlex`, `difflib`, `string.Template` | missing | wrap/dedent, shell quoting, diff | CLI tools | M each | P2 |
| `json`, `csv`, `xml`, `base64` | `std.json`, `std.jsonstr`, `std.csv`, `std.xml`, `std.base64` | `json` streaming | ok | - | ok |
| `tomllib`, `configparser`, `yaml` | missing | TOML/INI/YAML readers | config files | M | P1 |
| `hashlib`, `hmac`, `secrets` | `std.hash`, `std.md5`, `std.crypto.*`: md5, sha1, sha256/384/512, blake2b, hmac, hkdf, argon2, random | sha3, blake2s, scrypt | ok | S | P2 |
| `struct` (pack/unpack) | `ld*/st*` in `std.str`, `std.bytes` | format-string `pack/unpack` | binary formats | M | P1 |
| `uuid`, `secrets.token_*` | `md5_offline_uuid`, `random_bytes` | `uuid4/uuid5` | ids | S | P2 |
| `unicodedata` | `lib/pack/ucd.fi` (bidi), `std.str` utf8 | normalize NFC/NFD, category, name | text | L | P2 |
| `zlib/gzip/bz2/lzma/zipfile/tarfile` | `lib/compress`, `std.extract`, `std.tar`, `lib/zip` | - | ok | - | ok |
| `sqlite3`, `logging`, `argparse`, `unittest` | `lib/db`, `std.log`, `std.args`, `std.testkit` | - | ok | - | ok |
| `subprocess`, `os`, `sys`, `tempfile`, `glob`, `shutil` | `std.process`, `std.env`, `std.args`, `std.fsx`, `std.glob`, `std.shell` | - | ok | - | ok |
| `socket`, `urllib`, `http.server`, `asyncio`, `threading`, `concurrent.futures` | `lib/net`, `net.web`, `lib/http`, `lib/async`, `lib/jobs` | - | ok | - | ok |
| `smtplib`, `imaplib` | `lib/mail` | - | ok | - | ok |
| `copy.deepcopy`, `pprint`, `timeit`, `operator` | missing (`vec_copy`, `monotonic_ns` exist) | `deepcopy` needs reflection (r23) | debug | M | P2 |

## 2. Top 10 (benefit / effort)

| # | Item | Effort | Why |
|---|---|---|---|
| 1 | DONE `std.heap` (heapq) | S | CPython-identical heap array, tests/4440 |
| 2 | DONE `std.rng` (random, MT19937 bit-exact) | M | same seed = same numbers as Python, tests/4441 |
| 3 | DONE `std.deque` (collections.deque) | S | O(1) both ends, maxlen window, tests/4442 |
| 4 | `std.itertools` (eager chain/product/permutations/combinations/accumulate/groupby/pairwise/batched + `next_permutation`) | M | biggest remaining stdlib hole, no equivalent anywhere |
| 5 | `std.math` extras (asin/acos/sinh/cosh/tanh/expm1/log1p/cbrt/copysign/factorial/comb/perm/fsum/isclose/degrees/radians) | S | 1 day, pure functions, cross-check against Python `math` |
| 6 | Set operations (`set_union/intersect/difference/symdiff/is_subset`) | S | `Set[K]` is unusable for dedup/diff tasks without them |
| 7 | `datetime` `strftime/strptime` + `timedelta` helpers | M | every log/report script formats dates |
| 8 | `std.stats` extras (mode, fmean, geometric/harmonic mean, correlation, linear_regression, quantiles) | M | data scripts, cross-check against `statistics` |
| 9 | `bisect` completeness (`upper_bound`, `insort_right`, key variants) + `Path` object over `std.fs` | S | tiny, removes boilerplate |
| 10 | typed ndarray (`docs/NDARRAY.md`), then TOML/INI reader and `struct.pack/unpack` | L | largest effort, largest numeric payoff |

## 3. What this round built

| Lib | Tests | Python cross-check |
|---|---|---|
| `lib/std/heap.fi` | `tests/4440_std_heap.fi` (18 checks x 4 opt levels) | heap ARRAY identical to `heapq` after 3000 mixed ops, heapify, nsmallest/nlargest with ties (`tools/libs_py_cross/heap_ref.py`) |
| `lib/std/rng.fi` | `tests/4441_std_rng.fi` (101 checks x 4 opt levels) | 7 seeds incl. 0, 2^32, 2^64-1: random, getrandbits(1..64), randint/randrange, shuffle, choice, sample (pool + set branch), randbytes bit-exact; gauss/expovariate 1e-9 (`tools/libs_py_cross/random_gen.py`) |
| `lib/std/deque.fi` | `tests/4442_std_deque.fi` (16 checks x 4 opt levels) | 4 runs (unbounded, maxlen 1/5/64), rotate, extend, extendleft, reverse vs `collections.deque` (`tools/libs_py_cross/deque_ref.py`) |

Notes: the module is called `rng`, not `random`, because the loader resolves modules by their last path component and `std.crypto.random` already owns `random`. Non-generic functions and types of a module are used qualified (`rng.Rng`, `rng.randint(&r, 1, 6)`); generic ones are bare (`rng_shuffle[T]`, `heap_push[T]`, `Deque[T]`).

## 4. Round 2 (w3libs) -- built 2026-10-19

| Lib | Tests | Python cross-check |
|---|---|---|
| `lib/std/itertools.fi` (eager chain/product/permutations/combinations/cwr/accumulate/groupby/islice/takewhile/dropwhile/filterfalse/compress/pairwise/batched/zip/zip_longest/cycle/repeat, next/prev_permutation) | `tests/4520_std_itertools.fi` (15 checks x 4 opt levels) | 10 aggregate (count, checksum) pairs vs CPython 3.11 `itertools` (`tools/libs_py_cross/itertools_ref.py`), 0 deviations |
| `lib/std/mathx.fi` (asin/acos/sinh/cosh/tanh/asinh/acosh/atanh/expm1/log1p/cbrt/erf/erfc/gamma/lgamma, copysign/remainder/fmod_py/degrees/radians/isclose/fsum, factorial/comb/perm/gcd_i64/lcm_i64/floordiv/floormod/mul_mod/pow_mod/mod_inv) | `tests/4521_std_mathx.fi` (22 checks), `tests/4522_std_mathx_int.fi` (8 checks) | ~750 float cases as CPython bit patterns (exact ones bit-equal, transcendental within 1e-11), 127 fsum vectors bit-exact; integer checksums over ~4500 cases (`mathx_gen.py`, `mathx_int_ref.py`) |
| `lib/std/setops.fi` (union/intersection/difference/symmetric_difference/is_subset/superset/disjoint/equal, update forms) | `tests/4523_std_setops.fi` | 400 random rounds vs CPython `set` |
| `lib/std/bisect.fi` (bisect_left/right with lo/hi, insort, `_by` callbacks) | `tests/4524_std_bisect.fi` | 300 random rounds vs `bisect` |
| `lib/std/statx.fi` (median_low/high, fmean, geometric/harmonic mean, covariance, correlation, linear_regression, quantiles both methods, mode/multimode) | `tests/4525_std_statx.fi` (3 checks) | 80 data sets vs `statistics`: order statistics bit-exact, means/regression within 1e-11 |

Still open from the table above: `datetime` strftime/strptime + timedelta, `pathlib`-style Path object, popcount/clz/ctz intrinsics (compiler + firnc1; a64 backend belongs to the fUi manager), typed ndarray (plan: `docs/NDARRAY-PLAN.md`), `NormalDist`, `functools.lru_cache`.
