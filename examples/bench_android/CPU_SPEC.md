# CPU benchmark — exact definition (both apps MUST implement exactly this)

Nine kernels, run one after another. All integer maths in 64-bit signed (Kotlin `Long`) unless a type is named, doubles are IEEE-754 binary64, NO fused multiply-add, evaluate exactly in the written order.
M = 1_000_000_007. Reference: `cpu_ref.py` (pure Python, independent code: hashlib, json.loads, sorted, dict) -> `EXPECTED.txt`.

## Measurement (same in both apps)
- The whole run executes on ONE background thread: Firn = `std.pool` with one worker (`bcpu.cpu_start`), Kotlin = `Dispatchers.Default` (`withContext`). The UI thread only polls and paints.
- `wall ms` = kernel start -> kernel end on that thread, **data generation included**, no UI work in between, no JIT warm-up run (cold, one run per kernel, like a user tapping Run). `System.nanoTime()` / `time.monotonic_ns()`.
- Why: the first version cut the Firn run into 8 ms slices on the UI loop (Firn has no thread there), so `wall` contained 60 Hz frames between the slices: primes/sort/mandel wall 132/577/473 ms vs pure 31/301/243 ms (Xiaomi 14T Pro). That compared "UI-loop cooperative" against "background coroutine" — unfair. Now both are background threads; `pure == wall` (the `cpu_pure` log line is kept = wall).
- GC rule (Firn): the UI thread is not announced as BLOCKED inside `ALooper_pollOnce`, so a collection started by the worker (kernel 6 allocates strings) waits until the UI thread reaches a safepoint (next 16 ms tick, `thread_safepoint()` in the app's tick). Roadmap: announce blocked in `window/android.fi r_wait`.

## LCG (seed 12345)
`x = 12345; next(): x = (x * 1103515245 + 12345) & 0x7FFFFFFF; return x`   (31-bit, never negative; full period, so no repeats in < 2^31 draws)

## 1. primes
Sieve of Eratosthenes over [0, 5_000_000) (exclusive). count = number of primes, sum = sum of the primes (Long).
checksum = (count * 1000003 + sum) mod M.

## 2. sort
Array of 1_000_000 values `next()` (first call = first element). Sort ascending (any algorithm; Firn: own iterative quicksort on raw u32, Kotlin: IntArray.sort).
checksum = (Σ_{i=0..n-1} a[i] * ((i mod 251) + 1)) mod M     (sum fits in Long).

## 3. mandel
w=800, h=600, maxit=200, dx = 3.5/800.0, dy = 2.0/600.0
for py in 0..h-1: ci = -1.0 + py*dy
  for px in 0..w-1: cr = -2.5 + px*dx; zr=0; zi=0; it=0
    while it < 200: zr2=zr*zr; zi2=zi*zi; if (zr2+zi2 > 4.0) break; zi = (2.0*zr)*zi + ci; zr = (zr2 - zi2) + cr; it++
    total += it
checksum = total (exact Long).

## 4. hash (FNV-1a 32 + SHA-256)
N = 4_194_304 bytes: `buf[i] = (next() >> 16) & 255`, i = 0..N-1.
FNV-1a 32: `h = 2166136261; for b in buf: h = ((h ^ b) * 16777619) & 0xFFFFFFFF`.
SHA-256 (FIPS 180-4, hand-written 32-bit rounds, NOT a library call) over the same N bytes; w0 = first digest word as unsigned 32 bit (big endian bytes 0..3).
checksum = (h * 31 + w0) mod M.
Firn runs the rounds in u64 masked to 32 bit (the stdlib idiom), Kotlin in Int with `rotateRight`.

## 5. json
Text (no whitespace), n = 40_000 records, ONE `next()` pair per record (`a = next(); y = next()`), i = 0..n-1:
`[` + join(`,`, rec_i) + `]` with
`rec_i = {"id":<i>,"v":<a % 100000>,"name":"n<i>x","f":[<y % 1000>,<y % 977>,<(y / 1000) % 1000>],"ok":<true if i even else false>}`
(decimal integers, no padding). Generation into a byte buffer (no string objects) is part of the time, then parse with a hand-written recursive-descent parser over the bytes (objects, arrays, strings without escapes, integers, true/false/null; no tree is built, only the counters):
num = Σ all integer values; str = Σ lengths of all strings AND object keys; obj = number of objects; arr = number of arrays; tru = number of `true`.
checksum = (num + 7*str + 13*obj + 17*arr + 19*tru) mod M.

## 6. strbuild
Builder (Firn `rt.Buf`, Kotlin `StringBuilder`), for i in 0..199_999: `piece = "item" + toString(i) + ";"` (a fresh temporary string per piece, concatenated with `+`), append `piece`.
Then over the final ASCII text s (length L): `t = L * 31 + Σ_{j} s[j] * ((j mod 7) + 1)`.
checksum = t mod M.

## 7. matmul (f64)
n = 256, row-major. `A[i][j] = ((i*31 + j*17) mod 101) * 0.25`, `B[i][j] = ((i*13 + j*7) mod 97) * 0.5`.
`C[i][j] = Σ_{k=0..n-1} A[i][k]*B[k][j]` — accumulate `sum = sum + A*B` in k order, `sum` starts at 0.0 (no FMA). All values are exact binary fractions, so the result is exact.
`total = Σ C` in row-major order (double); checksum = (long)(total * 8.0) mod M.

## 8. hashmap
n = 300_000. `keys[i] = next()`; map: key -> i (Firn `std.map` Map[i64,i64], Kotlin `HashMap<Int,Int>` — idiomatic library map on each side).
Phase 1: insert keys[i] -> i for i in 0..n-1. Phase 2, for i in 0..n-1: lookup `keys[(i*7919) mod n]` (hit) and `keys[i] ^ 1` (hit or miss); on hit hits++ and total += value.
checksum = (hits * 1000003 + total + map.size) mod M.

## 9. objsort
n = 200_000. For i in 0..n-1: `key = next() % 1000`, then `w = next()`; object `Item{key, w, id=i}` pushed into a growable list (Firn `Vec[Item]` struct + `vec_push`, Kotlin `ArrayList<Item>` data class).
Sort with the standard library sort and comparator (key asc, then w asc — a total order, w is unique): Firn `vec_sort_by` (currently heap sort, stdlib-sort work by Part D will change it), Kotlin `sortWith`.
checksum = (Σ_pos item[pos].id * ((pos mod 251) + 1)) mod M.

## Log lines
`BENCH cpu primes=<ms>:<checksum> sort=<ms>:<checksum> mandel=<ms>:<checksum>`
`BENCH cpu2 hash=<ms>:<cs> json=<ms>:<cs> strbuild=<ms>:<cs> matmul=<ms>:<cs> hashmap=<ms>:<cs> objsort=<ms>:<cs>`
Expected checksums: see EXPECTED.txt. Host check: `FIRNLIB=lib firnc --opt-level=release-fast -o x examples/bench_android/cpu_main.fi && ./x` must print the nine lines of EXPECTED.txt.
