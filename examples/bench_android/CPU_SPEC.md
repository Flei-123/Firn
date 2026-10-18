# CPU benchmark — exact definition (both apps MUST implement exactly this)

All integer maths in 64-bit signed (Kotlin `Long`), doubles are IEEE-754 binary64, NO fused multiply-add, evaluate exactly in the written order.
M = 1_000_000_007.

## LCG (seed 12345)
`x = 12345; next(): x = (x * 1103515245 + 12345) & 0x7FFFFFFF; return x`   (31-bit, never negative)

## 1. primes
Sieve of Eratosthenes over [0, 5_000_000) (exclusive). count = number of primes, sum = sum of the primes (Long).
checksum = (count * 1000003 + sum) mod M.

## 2. sort
Array of 1_000_000 values `next()` (first call = first element). Sort ascending (any algorithm, e.g. built-in IntArray.sort / own).
checksum = (Σ_{i=0..n-1} a[i] * ((i mod 251) + 1)) mod M     (sum fits in Long).

## 3. mandel
w=800, h=600, maxit=200, dx = 3.5/800.0, dy = 2.0/600.0
for py in 0..h-1: ci = -1.0 + py*dy
  for px in 0..w-1: cr = -2.5 + px*dx; zr=0; zi=0; it=0
    while it < 200: zr2=zr*zr; zi2=zi*zi; if (zr2+zi2 > 4.0) break; zi = (2.0*zr)*zi + ci; zr = (zr2 - zi2) + cr; it++
    total += it
checksum = total (exact Long).

## Log line
`BENCH cpu primes=<ms>:<checksum> sort=<ms>:<checksum> mandel=<ms>:<checksum>`   ms = wall milliseconds from task start to task end.
Expected checksums: see EXPECTED.txt (computed by cpu_ref.py, pure Python).
Firn app (A) runs on the UI loop in slices (see README.md) and additionally logs `BENCH cpu_pure primes=<ms> sort=<ms> mandel=<ms> slice_ms=<n>` = pure compute time without UI frames in between.
