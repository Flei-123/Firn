# Firn vs C vs Python: Python side

Data: `py_results.md` / `py_results.json` (made by `py_measure.py`). All 20 tasks: idiomatic Python (`py/`) and pure
Python (`pure/`) print the SAME checksum as the C version (verified, 0 mismatches). Time = user+sys CPU, median of up to 3 runs
(runs > 25 s: one sample). `*` = extrapolated from a scaled run (verified against a C build with the same scaled constants,
then multiplied by the work ratio). Host load was 20-40, 6 jobs in parallel: noise is about +-10 % (floatloop py and pure are
the SAME code and measured 56.5 s vs 52.8 s).

Lines = non-empty, non-comment. "Firn" = `firn/<name>.fi` (written to match the C benchmark, e.g. own quicksort, own SHA-256),
so Firn-with-libs would be shorter than the numbers below where a lib exists (part b).

## a) Where Python needs the LEAST code because a lib/builtin does the work

| task | lib / builtin | py lines | pure py | .fi lines | C lines |
|---|---|---|---|---|---|
| qsort | `list.sort()` | 8 | 53 | 107 | 64 |
| sha256 | `hashlib.sha256` | 13 (9 are the LCG buffer helper) | 62 | 154 | 67 |
| matmul | numpy `@` (int64) | 8 | 21 | 62 | 28 |
| spectral_norm | numpy `@`, `np.indices` | 9 | 33 | 93 | 48 |
| fib | `functools.cache` | 5 | 6 | 33 | 12 |
| primes | numpy strided slice fill + `flatnonzero` | 8 | 15 | 48 | 21 |
| binary_trees | tuples + `sum(generator)` | 11 | 33 | 98 | 55 |
| crc32 | `zlib.crc32` (chained) | 16 (9 LCG helper) | 22 | 97 | 38 |
| hashmap | `dict` | 18 | 45 | 117 | 56 |
| strbuild | `",".join(parts)`, `str(int)` | 15 | 30 | 123 | 52 |
| jsonscan | `re.finditer` | 25 | 97 | 206 | 114 |
| wordcount | `collections.Counter` | 26 | 45 | 122 | 54 |
| soa_access | numpy column ops (`+=`, `^=`, `cumprod`, dot) | 15 | 47 | 125 | 49 |
| memcpy_loops | numpy slice assign / `arange` / `copy` | 18 | 26 | 98 | 28 |
| mandelbrot | numpy `where` + masks | 16 | 22 | 54 | 29 |
| fannkuch | slice assign `perm[:k+1] = perm[k::-1]`, `insert/pop` | 25 | 47 | 103 | 45 |
| bitops | `int.bit_count()` | 11 | 21 | 49 | 26 |
| nbody | `itertools.combinations` | 41 | 77 | 164 | 85 |
| fnv_hash, floatloop | none (sequential loop; no stdlib FNV) | 17 / 8 | 16 / 8 | 77 / 46 | 25 / 15 |

Python speed of the same code (idiomatic / pure, x C): lib-backed tasks are close to C (matmul 1.5x, crc32 1.4x, sha256 2.4x,
spectral_norm 2.8x, primes 3.7x), loop-bound ones are 40-480x (pure bitops 484x, matmul 285x, sha256 594x, fnv 198x).
Idiomatic loop tasks gain nothing from libs (floatloop 44x, fnv 110x, nbody 124x).

## b) Do these lib functions exist in Firn? (grep in lib/ + docs/LIBS.md; signatures seen, not all tried in a program)

| Python lib/builtin | Firn | where |
|---|---|---|
| `list.sort`, `sorted(key=)` | EXISTS | `std.vec` `vec_sort[T: Ord]`, `vec_sort_by`; stable + key: `std.pycoll` `sort_stable_by`, `sort_key_i64/f64/str` |
| `hashlib.sha256` | EXISTS | `std.crypto.sha256` `sha256(p, n, out)`, streaming `Sha256` |
| `zlib.crc32` (chained) | EXISTS | `std.deflate` `crc32(p, n)`, `crc32_more(tab, crc, p, n)` |
| FNV-1a | EXISTS | `std.hash` `fnv1a64`, `fnv1a64_more`, `fnv1a32` (Python has no stdlib FNV) |
| `dict` with int keys | EXISTS (low level) | `rt/map.fi` `Map[K,V]`: `map_set/get/get_or/has/increase` |
| `dict` / `Counter` with str keys | EXISTS | `std.dict` `Dict[V]`: `dict_inc`, `dict_top`, insertion-ordered |
| `str.split/join/replace/strip` | EXISTS | `std.pystr` `split`, `split_ws`, `join(Vec[str], sep)` |
| `re.finditer/findall/sub` | EXISTS | `regex.pyre` (`finditer_next`, `findall`, `sub`, `split`) |
| `json.loads/dumps` | EXISTS | `std.json`, `std.jsonstr` |
| `str(int)`, f-strings | EXISTS | f-strings, `io.fmt_u64`, `rt.buf_push_dec_u64` |
| `sum`, `map`, `filter`, `reduce`, comprehensions, `enumerate`, `zip` | EXISTS | `std.vecfn` `vec_sum/map/filter/reduce`; language: comprehensions, `enumerate`, `zip` |
| slices `v[a:b:c]`, slice assign, `insert`, `pop(0)` | EXISTS | `std.vec` `py_slice`, `vec_splice_raw`, `vec_insert`, `vec_remove` |
| `random` (Mersenne Twister, same numbers as CPython) | EXISTS | `std.rng` |
| `math.sqrt`, `isqrt`, `gcd` | EXISTS | `std.math` |
| tuples, GC objects (binary_trees) | EXISTS (language) | tuple support in both compilers; `gc class` |
| `mem copy / zero / set` | EXISTS (raw u64 addresses) | `rt.mem_copy`, `rt.mem_set`, `std.core.mem_zero` |

### MISSING in Firn (derived from this suite)

1. **`int.bit_count` / popcount** (+ clz/ctz): no lib function, no builtin; only the assembler knows `popcnt`. The benchmark uses a SWAR loop.
2. **ndarray / numpy-like** (typed 1-D/2-D array, elementwise ops, broadcasting, `@`/dot/matmul, `cumprod`, `sum` over arrays, `arange`,
   `where`/masks, `flatnonzero`): only a design draft `docs/NDARRAY.md` (r318, "nothing built"). Hits matmul, spectral_norm, mandelbrot,
   soa_access, memcpy_loops, primes. `std.stats` has `sum/mean` on `f64` only.
3. **f64 SIMD**: `std.simd` is f32 `v128` only (docs/NDARRAY.md: f64 needs new compiler intrinsics).
4. **Strided / range fill and bool-array helpers**: `vec_fill` fills the WHOLE vec element by element; no `fill_range(v, a, b, step, x)`,
   no `nonzero/flatnonzero`, no `count_true`. (primes: `sieve[i*i::i] = False`).
5. **`functools.cache` / memoize**: no generic memo helper (grep memoize/cache finds only unrelated caches).
6. **`itertools`**: no `combinations`, `permutations`, `product`, `accumulate`, `chain`, `pairwise` (nbody, fannkuch).
7. **Python-`bytes` toolbox**: `std.bytes` is a read/write cursor buffer, not `bytes`; missing `bytes.count/find/translate`, iterating
   bytes, `int.from_bytes/to_bytes` one-liners (sha256 result, memcpy_loops).
8. **`Dict`-style facade for integer keys** (`Dict[V]` is text-keyed only; `Map[K,V]` is low-level, no insertion order, no `dict_items`).
9. **LCG / bulk-random helper**: all 20 tasks hand-write the 64-bit LCG; a `std.lcg` or `rng` bulk fill (`fill_u8`, `fill_u64`) would remove
   ~5 lines per task. Python idiomatic versions need a 10-line numpy jump-ahead helper for the same (crc32/sha256/fnv).
10. **Modular fold** `h = h*31 + v` as a reduction (polynomial hash over a vec; vectorised in Python with `cumprod`+dot).

## Reading the numbers

- Python's lib advantage is large only where C-backed code covers the WHOLE task (sha256, crc32, matmul, sort, spectral_norm, primes):
  1.4-4x C. Everything with a sequential dependency (bitops, floatloop, fnv, nbody, fannkuch, mandelbrot) stays 40-480x C in any form.
- Firn already has the facade libs for the "Python wins" rows sha256, crc32, sort, dict, Counter, regex, json, split/join, random.
  The real gaps are numeric: ndarray (+matmul), popcount, strided fill, f64 SIMD.
- nbody: idiomatic (list-of-lists + `combinations`) is SLOWER than pure (separate lists): 69 s vs 41 s extrapolated, re-measured alone (jobs=1), so not load noise.
- Caveats: `jsonscan` idiomatic scans once and folds the 4 identical passes (pure function of the text); `soa_access` idiomatic computes
  one layout (AoS and SoA give identical data); `fib` idiomatic is memoised (0.03 s) and is NOT comparable to C's 331 M calls;
  `matmul` pure ran 127 s (single sample, a bit above the 120 s target, not extrapolated).
