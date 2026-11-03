# Peak RSS, CPU time and binary size: Firn vs gcc -O2 vs rustc -O

Measured with `bench/ram_compare.py` (3 runs, median; `/usr/bin/time` as launcher so the parent process does not inflate ru_maxrss). Firn: release-fast, `--ownership=off` and `--ownership=on`. Sizes in bytes. A `-` means no reference program or an RSS below the accounting resolution (about 0.5 MB for tiny programs).

Run: `bash -c 'BENCH_RUNS=3 JOBS=3 OUT=/tmp/ram.json python3 bench/ram_compare.py'` (use `/root/jarvis/bin/heavy`).

| bench | firn off KB | firn on KB | gcc KB | rust KB | on/gcc | on/rust | off cpu | on cpu | gcc cpu | rust cpu | size firn | size gcc | size rust | out same |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| bitmap | 516 | 516 | - | 1628 | - | 0.32 | 0.04 | 0.04 | - | 0.03 | 2600 | - | 4523144 | yes |
| branchy | 516 | 516 | - | 1632 | - | 0.32 | 0.58 | 0.58 | - | 0.54 | 2304 | - | 4522672 | yes |
| bubblesort | 516 | 516 | - | 1640 | - | 0.31 | 0.07 | 0.07 | - | 0.03 | 3400 | - | 4523328 | yes |
| bytecount | 16384 | 16384 | - | 18032 | - | 0.91 | 0.13 | 0.12 | - | 0.2 | 3000 | - | 4524280 | yes |
| fib | 516 | 516 | - | 1640 | - | 0.31 | 0.01 | 0.01 | - | 0.02 | 1968 | - | 4522752 | yes |
| jsonscan | 26112 | 26112 | - | 27740 | - | 0.94 | 0.14 | 0.15 | - | 0.09 | 3072 | - | 4523632 | yes |
| matmul | 516 | 516 | - | 2144 | - | 0.24 | 0.02 | 0.02 | - | 0.02 | 3688 | - | 4524728 | yes |
| memstride | 261632 | 262144 | - | 263764 | - | 0.99 | 0.28 | 0.23 | - | 0.22 | 2296 | - | 4523272 | yes |
| sieve | 4608 | 4608 | - | 6256 | - | 0.74 | 0.05 | 0.02 | - | 0.03 | 2240 | - | 4523248 | yes |
| statemachine | 8192 | 8192 | - | 9808 | - | 0.84 | 0.13 | 0.12 | - | 0.08 | 2912 | - | 4523560 | yes |
| xxhash | 65536 | 65536 | - | 67184 | - | 0.98 | 0.22 | 0.22 | - | 0.19 | 3760 | - | 4523784 | yes |
| vec_sort | 4240 | 4240 | - | 5728 | - | 0.74 | 0.04 | 0.04 | - | 0.05 | 22400 | - | 4528208 | yes |
| sqrt | - | - | - | 1644 | - | - | 0.01 | 0.01 | - | 0.01 | 2608 | - | 4523288 | yes |
| vecsum | 1024 | 1024 | - | 2656 | - | 0.39 | - | - | - | - | 21072 | - | 4517512 | yes |
| vecstore | 1024 | 1024 | - | 2664 | - | 0.38 | - | - | - | - | 21008 | - | 4517552 | yes |
| vecsub | 1024 | 1024 | - | 2664 | - | 0.38 | 0.01 | 0.01 | - | - | 21032 | - | 4517712 | yes |
| vecforeign | 1536 | 1536 | - | 3196 | - | 0.48 | - | - | - | - | 21064 | - | 4517784 | yes |
| binary_trees | 22864 | 8192 | 9276 | 9840 | 0.88 | 0.83 | 0.37 | 0.2 | 0.24 | 0.29 | 62616 | 20256 | 4524344 | yes |
| bitops | 516 | 516 | 1072 | 1624 | 0.48 | 0.32 | 0.82 | 0.85 | 0.79 | 0.87 | 2392 | 15960 | 4522736 | yes |
| crc32 | 65536 | 65536 | 66652 | 67176 | 0.98 | 0.98 | 0.8 | 0.8 | 0.81 | 0.79 | 7296 | 16040 | 4524744 | yes |
| fannkuch | - | 516 | 1092 | 1648 | 0.47 | 0.31 | 0.29 | 0.29 | 0.27 | 0.24 | 7096 | 16104 | 4524672 | yes |
| floatloop | 516 | 516 | 1092 | 1628 | 0.47 | 0.32 | 1.2 | 1.26 | 1.26 | 1.27 | 2632 | 16016 | 4523320 | yes |
| hashmap | 65536 | 65536 | 66656 | 67156 | 0.98 | 0.98 | 0.31 | 0.26 | 0.26 | 0.26 | 2688 | 16016 | 4523136 | yes |
| mandelbrot | 516 | 516 | 1092 | 1644 | 0.47 | 0.31 | 0.25 | 0.25 | 0.25 | 0.23 | 2312 | 15968 | 4522856 | yes |
| memcpy_loops | 49152 | 49152 | 50220 | 50776 | 0.98 | 0.97 | 0.23 | 0.23 | 0.16 | 0.19 | 3336 | 16024 | 4529104 | yes |
| nbody | 532 | 516 | 1620 | 1644 | 0.32 | 0.31 | 0.62 | 0.6 | 0.54 | 0.56 | 11024 | 16200 | 4525696 | yes |
| primes | 19456 | 19456 | 20528 | 21080 | 0.95 | 0.92 | 0.17 | 0.26 | 0.19 | 0.19 | 6008 | 15992 | 4523184 | yes |
| qsort | 7680 | 7680 | 8788 | 9308 | 0.87 | 0.83 | 0.26 | 0.25 | 0.22 | 0.22 | 7008 | 16024 | 4524968 | yes |
| sha256 | 16384 | 16384 | 17472 | 18028 | 0.94 | 0.91 | 0.14 | 0.14 | 0.13 | 0.14 | 11072 | 16120 | 4525040 | yes |
| soa_access | 62464 | 62464 | 63556 | 64112 | 0.98 | 0.97 | 0.35 | 0.39 | 0.31 | 0.25 | 4640 | 16016 | 4526000 | yes |
| spectral_norm | 516 | 516 | 1620 | 1640 | 0.32 | 0.31 | 0.16 | 0.16 | 0.16 | 0.16 | 10336 | 16136 | 4524640 | yes |
| strbuild | 8204 | 8204 | 16656 | 9816 | 0.49 | 0.84 | 0.22 | 0.21 | 0.17 | 0.15 | 7120 | 16184 | 4525184 | yes |
| wordcount | 12800 | 12800 | 13892 | 14448 | 0.92 | 0.89 | 0.35 | 0.35 | 0.34 | 0.31 | 6992 | 16080 | 4524016 | yes |
| fnv_hash | 65536 | 65536 | 66636 | 67172 | 0.98 | 0.98 | 0.58 | 0.58 | 0.59 | 0.59 | 2640 | 16016 | 4523640 | yes |

Geomean RSS firn on / gcc: 0.68 (n=17); firn on / rust: 0.57 (n=33)

## Reading

- No benchmark of the suite needs more peak RSS than the gcc or rustc build, except GC programs with ownership off: `binary_trees` 22.9 MB vs 9.3 MB (gcc) vs 9.8 MB (rustc). With `--ownership=on` it is 8.2 MB and CPU drops from 0.37 s to 0.20 s (gcc 0.24 s).
- Plain programs (no `gc class`) sit 0.5-1.5 MB below gcc/rustc because they carry no libc and no std runtime; the rest is the workload's own data.
- Binary size: 2-22 KB for plain programs, 58-63 KB with the GC runtime (`binary_trees`); gcc dynamic 16-20 KB, rustc 4.5 MB.
- Not covered: GC programs with a large live set. OpenPlan `derive` (300 pages) peaks at 228 MB (off) / 230 MB (on): the GC heap peaks at about 3x the live set (growth 2/1, see `docs/GC.md`).
