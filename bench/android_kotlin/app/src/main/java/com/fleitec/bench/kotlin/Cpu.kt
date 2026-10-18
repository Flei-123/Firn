package com.fleitec.bench.kotlin

/**
 * CPU benchmark kernels. The algorithms and checksums are fixed by the Firn definition
 * (examples/bench_android/CPU_SPEC.md, cpu_ref.py) and bench/EXPECTED_REF.txt, so the Firn+fUi app
 * and this app must print identical checksums. No fused multiply-add, evaluation order as written.
 */
object Cpu {
    const val PRIME_LIMIT = 5_000_000
    const val SORT_COUNT = 1_000_000
    const val LCG_SEED = 12345L
    const val MANDEL_W = 800
    const val MANDEL_H = 600
    const val MANDEL_ITER = 200
    const val MOD = 1_000_000_007L

    /** Sieve of Eratosthenes below [limit]; checksum = (count * 1000003 + sum of primes) mod 1e9+7. */
    fun primes(limit: Int = PRIME_LIMIT): Long {
        val composite = BooleanArray(limit)
        var count = 0L
        var sum = 0L
        var i = 2
        while (i < limit) {
            if (!composite[i]) {
                count++
                sum += i
                var j = i.toLong() * i
                while (j < limit) {
                    composite[j.toInt()] = true
                    j += i
                }
            }
            i++
        }
        return (count * 1000003L + sum) % MOD
    }

    /**
     * 31-bit LCG: x = (x * 1103515245 + 12345) & 0x7fffffff, seed 12345; the value is the state AFTER
     * the update (first call = first element). Sorts [count] values ascending;
     * checksum = (sum a[i] * ((i mod 251) + 1)) mod 1e9+7.
     */
    fun sort(count: Int = SORT_COUNT): Long {
        val values = IntArray(count)
        var s = LCG_SEED
        for (i in values.indices) {
            s = (s * 1103515245L + 12345L) and 0x7fffffffL
            values[i] = s.toInt()
        }
        values.sort()
        var t = 0L
        for (i in values.indices) t += values[i].toLong() * ((i % 251) + 1)
        return t % MOD
    }

    /**
     * Mandelbrot 800x600, 200 iterations: dx = 3.5/800.0, dy = 2.0/600.0, ci = -1.0 + py*dy,
     * cr = -2.5 + px*dx; loop: zr2 = zr*zr; zi2 = zi*zi; stop if zr2+zi2 > 4.0;
     * zi = (2.0*zr)*zi + ci; zr = (zr2 - zi2) + cr; count++. Checksum = sum of all iteration counts.
     */
    fun mandelbrot(): Long {
        val dx = 3.5 / 800.0
        val dy = 2.0 / 600.0
        var sum = 0L
        for (py in 0 until MANDEL_H) {
            val ci = -1.0 + py * dy
            for (px in 0 until MANDEL_W) {
                val cr = -2.5 + px * dx
                var zr = 0.0
                var zi = 0.0
                var n = 0
                while (n < MANDEL_ITER) {
                    val zr2 = zr * zr
                    val zi2 = zi * zi
                    if (zr2 + zi2 > 4.0) break
                    zi = (2.0 * zr) * zi + ci
                    zr = (zr2 - zi2) + cr
                    n++
                }
                sum += n
            }
        }
        return sum
    }
}
