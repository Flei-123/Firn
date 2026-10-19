# Firn vs C on Xiaomi 14T Pro (aarch64, real device)

Firn release-fast vs gcc -O2 (static glibc), CPU s median of 5. All checksums match.

**Geomean Firn/gcc-O2: 1.66x** (n=20)

| benchmark | Firn s | gcc -O2 s | Firn/C |
|---|---|---|---|
| sha256 | 0.178 | 0.191 | 0.93x |
| mandelbrot | 0.240 | 0.235 | 1.02x |
| fnv_hash | 0.534 | 0.504 | 1.06x |
| floatloop | 1.257 | 1.161 | 1.08x |
| primes | 0.120 | 0.109 | 1.11x |
| nbody | 0.455 | 0.407 | 1.12x |
| soa_access | 0.130 | 0.111 | 1.17x |
| qsort | 0.192 | 0.163 | 1.18x |
| wordcount | 0.368 | 0.281 | 1.31x |
| hashmap | 0.273 | 0.193 | 1.42x |
| matmul | 0.498 | 0.347 | 1.44x |
| crc32 | 1.230 | 0.809 | 1.52x |
| bitops | 0.770 | 0.405 | 1.90x |
| jsonscan | 0.393 | 0.187 | 2.10x |
| strbuild | 0.312 | 0.132 | 2.37x |
| spectral_norm | 0.243 | 0.093 | 2.60x |
| fannkuch | 0.635 | 0.242 | 2.62x |
| binary_trees | 0.645 | 0.193 | 3.35x |
| memcpy_loops | 0.313 | 0.081 | 3.88x |
| fib | 0.750 | 0.183 | 4.09x |

Setup: Thermal Status 0 before/after; battery 37.0C before / 36.9C after; cpu7 idle freq 600 MHz (governor not readable, no root)

Reproduce: `OUT=<dir> bash xiaomi/build.sh` (needs aarch64-linux-gnu-gcc + firnc), push `<dir>` + `xiaomi/runner.c` (built static) + `xiaomi/bench.sh` to /data/local/tmp/firnbench, run `sh bench.sh`, aggregate with `xiaomi/agg.py`.
Caveat: Firn x86/qemu proxy ratio was 1.33x; the real device differs mostly in fib, memcpy_loops, spectral_norm, strbuild, binary_trees, fannkuch, jsonscan (>2x).
