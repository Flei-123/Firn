# Firn vs C vs Python: Python side

CPU seconds (user+sys), median of up to 3 runs (runs > 25 s: single sample); `*` = extrapolated from a scaled run (see notes). Checksums verified against the C output. Lines = non-empty non-comment; tokens = Python `tokenize` / `firnc --emit=tokens` (without Eof) / regex tokenizer for C.

| task | C -O2 s | py (idiomatic) s | py x C | pure py s | pure x C | fi lines/tok | c | py | pure |
|---|---|---|---|---|---|---|---|---|---|
| primes | 0.19 | 0.29 | 1.6 | 6.62 | 35 | 48/330 | 21/143 | 8/78 | 15/105 |
| qsort | 0.22 | 2.57 | 12 | 9.39 | 42 | 107/788 | 64/522 | 8/68 | 53/415 |
| mandelbrot | 0.26 | 5.50 | 21 | 31.83 | 123 | 54/388 | 29/198 | 16/195 | 22/124 |
| nbody | 0.56 | 68.75* | 122 | 42.88* | 76 | 164/1398 | 85/982 | 41/561 | 77/743 |
| fannkuch | 0.28 | 15.53 | 56 | 37.60 | 136 | 103/624 | 45/353 | 25/185 | 47/247 |
| spectral_norm | 0.16 | 0.33 | 2.0 | 28.19 | 172 | 93/655 | 48/428 | 9/114 | 33/277 |
| matmul | 0.34 | 0.73 | 2.1 | 135.26 | 392 | 62/479 | 28/284 | 8/91 | 21/209 |
| fib | 0.24 | 0.02 | 0.1 | 25.78* | 106 | 33/253 | 12/77 | 5/39 | 6/48 |
| bitops | 0.82 | 265.40* | 323 | 414.65* | 505 | 49/400 | 26/220 | 11/112 | 21/200 |
| floatloop | 1.26 | 48.27* | 38 | 46.42* | 37 | 46/338 | 15/121 | 8/80 | 8/80 |
| fnv_hash | 0.59 | 68.57* | 116 | 106.29* | 180 | 77/605 | 25/194 | 17/195 | 16/108 |
| sha256 | 0.14 | 0.40 | 2.9 | 83.89* | 595 | 154/1454 | 67/974 | 13/172 | 62/713 |
| jsonscan | 0.26 | 6.11 | 23 | 31.33* | 121 | 206/1466 | 114/873 | 25/276 | 97/520 |
| strbuild | 0.18 | 7.92 | 44 | 12.51 | 69 | 123/911 | 52/446 | 15/140 | 30/195 |
| memcpy_loops | 0.16 | 1.28 | 8.0 | 52.38 | 329 | 98/748 | 28/365 | 18/249 | 26/233 |
| binary_trees | 0.30 | 1.72 | 5.6 | 9.87 | 32 | 98/799 | 55/400 | 11/158 | 33/219 |
| hashmap | 0.26 | 4.70 | 18 | 9.84 | 38 | 117/846 | 56/431 | 18/155 | 45/298 |
| soa_access | 0.24 | 2.14 | 9.1 | 24.72 | 105 | 125/1053 | 49/591 | 15/208 | 47/406 |
| crc32 | 0.81 | 0.73 | 0.9 | 93.38* | 115 | 97/727 | 38/298 | 16/171 | 22/164 |
| wordcount | 0.34 | 7.16 | 21 | 29.95 | 89 | 122/889 | 54/453 | 26/273 | 45/308 |

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
