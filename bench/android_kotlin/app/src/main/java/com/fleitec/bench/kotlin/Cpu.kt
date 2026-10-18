package com.fleitec.bench.kotlin

/**
 * CPU benchmark kernels. The algorithms and checksums are fixed by bench/EXPECTED_REF.txt so the
 * Firn+fUi app and this app must print identical checksums.
 */
object Cpu {
    const val PRIME_LIMIT = 5_000_000
    const val SORT_COUNT = 1_000_000
    const val LCG_SEED = 12345L
    const val MANDEL_W = 800
    const val MANDEL_H = 600
    const val MANDEL_ITER = 200

    /** Sieve of Eratosthenes below [limit]; checksum = number of primes. */
    fun primes(limit: Int = PRIME_LIMIT): Long {
        val composite = BooleanArray(limit)
        var count = 0L
        var i = 2
        while (i < limit) {
            if (!composite[i]) {
                count++
                var j = i.toLong() * i
                while (j < limit) {
                    composite[j.toInt()] = true
                    j += i
                }
            }
            i++
        }
        return count
    }

    /**
     * 31-bit LCG: s = (s * 1103515245 + 12345) & 0x7fffffff, starting at seed 12345; the value is
     * the state AFTER the update. Sorts [count] values; checksum h = (h * 31 + v) & 0xffffffff over
     * the sorted values.
     */
    fun sort(count: Int = SORT_COUNT): Long {
        val values = IntArray(count)
        var s = LCG_SEED
        for (i in values.indices) {
            s = (s * 1103515245L + 12345L) and 0x7fffffffL
            values[i] = s.toInt()
        }
        values.sort()
        var h = 0L
        for (v in values) h = (h * 31L + v) and 0xffffffffL
        return h
    }

    /**
     * Mandelbrot 800x600, 200 iterations: cr = -2.0 + 3.0*px/800.0, ci = -1.2 + 2.4*py/600.0,
     * iteration: zr2 = zr*zr; zi2 = zi*zi; stop if zr2+zi2 > 4.0; zi = 2.0*zr*zi + ci;
     * zr = zr2 - zi2 + cr; count++. Checksum = sum of all iteration counts.
     */
    fun mandelbrot(): Long {
        var sum = 0L
        for (py in 0 until MANDEL_H) {
            val ci = -1.2 + 2.4 * py / MANDEL_H
            for (px in 0 until MANDEL_W) {
                val cr = -2.0 + 3.0 * px / MANDEL_W
                var zr = 0.0
                var zi = 0.0
                var n = 0
                while (n < MANDEL_ITER) {
                    val zr2 = zr * zr
                    val zi2 = zi * zi
                    if (zr2 + zi2 > 4.0) break
                    zi = 2.0 * zr * zi + ci
                    zr = zr2 - zi2 + cr
                    n++
                }
                sum += n
            }
        }
        return sum
    }
}
