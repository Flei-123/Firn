| benchmark | Firn fast | Firn safe | Firn dev | gcc -O2 | gcc -O3 | clang -O2 | fast/O2 | fast/O3 | safe/O2 |
|---|---|---|---|---|---|---|---|---|---|
| binary_trees | 2.675 s | 3.448 s | 5.505 s | 2.393 s | 2.495 s | - | 1.12x | 1.07x | 1.44x |
| bitops | 1.876 s | 3.739 s | 7.025 s | 2.946 s | 2.959 s | - | 0.64x | 0.63x | 1.27x |
| crc32 | 1.948 s | 1.837 s | 2.383 s | 1.871 s | 1.975 s | - | 1.04x | 0.99x | 0.98x |
| fannkuch | 2.027 s | 2.603 s | 1.693 s | 1.756 s | 1.800 s | - | 1.15x | 1.13x | 1.48x |
| fib | 3.845 s | 6.618 s | 9.018 s | 1.816 s | 1.862 s | - | 2.12x | 2.06x | 3.64x |
| floatloop | 8.033 s | 8.058 s | 7.995 s | 7.860 s | 8.765 s | - | 1.02x | 0.92x | 1.03x |
| fnv_hash | 1.598 s | 1.592 s | 2.459 s | 1.697 s | 1.649 s | - | 0.94x | 0.97x | 0.94x |
| hashmap | 0.679 s | 0.714 s | 1.782 s | 0.528 s | 0.731 s | - | 1.29x | 0.93x | 1.35x |
| jsonscan | 2.443 s | 3.836 s | 4.936 s | 1.447 s | 2.500 s | - | 1.69x | 0.98x | 2.65x |
| mandelbrot | 3.273 s | 3.283 s | 3.545 s | 3.222 s | 2.999 s | - | 1.02x | 1.09x | 1.02x |
| matmul | 2.919 s | 5.388 s | 5.634 s | 1.556 s | 1.604 s | - | 1.88x | 1.82x | 3.46x |
| memcpy_loops | 1.612 s | 1.704 s | 4.580 s | 0.569 s | 0.554 s | - | 2.83x | 2.91x | 2.99x |
| nbody | 13.340 s | 11.173 s | 11.524 s | 10.651 s | 11.861 s | - | 1.25x | 1.12x | 1.05x |
| primes | 0.677 s | 0.842 s | 0.928 s | 0.636 s | 0.562 s | - | 1.07x | 1.21x | 1.33x |
| qsort | 0.661 s | 0.841 s | 0.788 s | 0.478 s | 0.484 s | - | 1.38x | 1.36x | 1.76x |
| sha256 | 0.507 s | 1.123 s | 2.843 s | 0.925 s | 0.839 s | - | 0.55x | 0.60x | 1.21x |
| soa_access | 0.610 s | 0.630 s | 2.394 s | 0.656 s | 0.635 s | - | 0.93x | 0.96x | 0.96x |
| spectral_norm | 3.079 s | 3.929 s | 4.561 s | 2.233 s | 2.250 s | - | 1.38x | 1.37x | 1.76x |
| strbuild | 0.961 s | 1.462 s | 3.775 s | 0.512 s | 0.530 s | - | 1.88x | 1.81x | 2.86x |
| wordcount | 1.285 s | 1.408 s | 1.480 s | 0.917 s | 0.930 s | - | 1.40x | 1.38x | 1.54x |

Geomean Firn release-fast / gcc -O2: 1.24x; / gcc -O3: 1.18x; Firn release-safe / gcc -O2: 1.57x (n=20)
CPU: AMD EPYC 7571 32-Core Processor, firn a3d9feb3e, gcc 12.2.0, load at start 4.85 12.21 12.99, 1 runs (median of user+sys)
