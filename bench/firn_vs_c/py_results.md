# Firn vs C vs Python: Python side

CPU seconds (user+sys), median of up to 3 runs (runs > 25 s: single sample); `*` = extrapolated from a scaled run (see notes). Checksums verified against the C output. Lines = non-empty non-comment; tokens = Python `tokenize` / `firnc --emit=tokens` (without Eof) / regex tokenizer for C.

| task | C -O2 s | py (idiomatic) s | py x C | pure py s | pure x C | fi lines/tok | c | py | pure |
|---|---|---|---|---|---|---|---|---|---|
| primes | 0.23 | 0.84 | 3.7 | 8.36 | 37 | 48/330 | 21/143 | 8/78 | 15/105 |
| qsort | 0.24 | 4.91 | 21 | 10.56 | 45 | 107/788 | 64/522 | 8/68 | 53/415 |
| mandelbrot | 0.26 | 10.49 | 40 | 35.24 | 136 | 54/388 | 29/198 | 16/195 | 22/124 |
| nbody | 0.64 | 79.09* | 124 | 49.55* | 78 | 164/1398 | 85/982 | 41/561 | 77/743 |
| fannkuch | 0.29 | 17.98 | 61 | 41.15 | 140 | 103/624 | 45/353 | 25/185 | 47/247 |
| spectral_norm | 0.21 | 0.60 | 2.8 | 31.68 | 151 | 93/655 | 48/428 | 9/114 | 33/277 |
| matmul | 0.45 | 0.68 | 1.5 | 127.00 | 285 | 62/479 | 28/284 | 8/91 | 21/209 |
| fib | 0.28 | 0.03 | 0.1 | 28.93* | 105 | 33/253 | 12/77 | 5/39 | 6/48 |
| bitops | 0.92 | 277.34* | 300 | 447.15* | 484 | 49/400 | 26/220 | 11/112 | 21/200 |
| floatloop | 1.27 | 56.53* | 44 | 52.79* | 42 | 46/338 | 15/121 | 8/80 | 8/80 |
| fnv_hash | 0.62 | 68.18* | 110 | 123.02* | 198 | 77/605 | 25/194 | 17/195 | 16/108 |
| sha256 | 0.15 | 0.36 | 2.4 | 90.83* | 594 | 154/1454 | 67/974 | 13/172 | 62/713 |
| jsonscan | 0.32 | 6.65 | 21 | 34.13* | 107 | 206/1466 | 114/873 | 25/276 | 97/520 |
| strbuild | 0.20 | 9.40 | 47 | 14.67 | 73 | 123/911 | 52/446 | 15/140 | 30/195 |
| memcpy_loops | 0.17 | 0.89 | 5.2 | 61.15 | 358 | 98/748 | 28/365 | 18/249 | 26/233 |
| binary_trees | 0.34 | 1.92 | 5.7 | 9.56 | 28 | 98/799 | 55/400 | 11/158 | 33/219 |
| hashmap | 0.33 | 5.29 | 16 | 10.36 | 32 | 117/846 | 56/431 | 18/155 | 45/298 |
| soa_access | 0.29 | 2.74 | 9.3 | 28.78 | 98 | 125/1053 | 49/591 | 15/208 | 47/406 |
| crc32 | 0.81 | 1.17 | 1.4 | 108.17* | 133 | 97/727 | 38/298 | 16/171 | 22/164 |
| wordcount | 0.35 | 8.06 | 23 | 32.03 | 92 | 122/889 | 54/453 | 26/273 | 45/308 |

Size columns: code lines / tokens.

## Notes

- nbody idiomatic: scale 1/2, extrapolated x2.0
- nbody pure: scale 1/4, extrapolated x4.0
- fib pure: n=35, extrapolated x11.1 (exact call-count ratio 2*F(n+1)-1)
- bitops idiomatic: scale 1/4, extrapolated x4.0
- bitops pure: scale 1/10, extrapolated x10.0
- floatloop idiomatic: scale 1/2, extrapolated x2.0
- floatloop pure: scale 1/2, extrapolated x2.0
- fnv_hash idiomatic: scale 1/2, extrapolated x2.0
- fnv_hash pure: scale 1/4, extrapolated x4.0
- sha256 pure: scale 1/2, extrapolated x2.0
- jsonscan pure: scale 1/4, extrapolated x4.0
- crc32 pure: scale 1/4, extrapolated x4.0
