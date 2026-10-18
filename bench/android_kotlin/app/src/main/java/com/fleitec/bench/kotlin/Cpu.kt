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

    // ------------------------------------------------------------------ kernels 4..9 (CPU_SPEC.md)

    const val HASH_N = 4_194_304
    const val JSON_N = 40_000
    const val STR_N = 200_000
    const val MAT_N = 256
    const val MAP_N = 300_000
    const val OBJ_N = 200_000

    private val SHA_K = intArrayOf(
        1116352408, 1899447441, -1245643825, -373957723, 961987163, 1508970993, -1841331548, -1424204075,
        -670586216, 310598401, 607225278, 1426881987, 1925078388, -2132889090, -1680079193, -1046744716,
        -459576895, -272742522, 264347078, 604807628, 770255983, 1249150122, 1555081692, 1996064986,
        -1740746414, -1473132947, -1341970488, -1084653625, -958395405, -710438585, 113926993, 338241895,
        666307205, 773529912, 1294757372, 1396182291, 1695183700, 1986661051, -2117940946, -1838011259,
        -1564481375, -1474664885, -1035236496, -949202525, -778901479, -694614492, -200395387, 275423344,
        430227734, 506948616, 659060556, 883997877, 958139571, 1322822218, 1537002063, 1747873779,
        1955562222, 2024104815, -2067236844, -1933114872, -1866530822, -1538233109, -1090935817, -965641998,
    )

    private fun shaBlock(h: IntArray, buf: ByteArray, off: Int, w: IntArray) {
        for (i in 0 until 16) {
            val p = off + i * 4
            w[i] = ((buf[p].toInt() and 255) shl 24) or ((buf[p + 1].toInt() and 255) shl 16) or
                ((buf[p + 2].toInt() and 255) shl 8) or (buf[p + 3].toInt() and 255)
        }
        for (i in 16 until 64) {
            val s0 = Integer.rotateRight(w[i - 15], 7) xor Integer.rotateRight(w[i - 15], 18) xor (w[i - 15] ushr 3)
            val s1 = Integer.rotateRight(w[i - 2], 17) xor Integer.rotateRight(w[i - 2], 19) xor (w[i - 2] ushr 10)
            w[i] = w[i - 16] + s0 + w[i - 7] + s1
        }
        var a = h[0]; var b = h[1]; var c = h[2]; var d = h[3]
        var e = h[4]; var f = h[5]; var g = h[6]; var hh = h[7]
        for (i in 0 until 64) {
            val bs1 = Integer.rotateRight(e, 6) xor Integer.rotateRight(e, 11) xor Integer.rotateRight(e, 25)
            val ch = (e and f) xor (e.inv() and g)
            val t1 = hh + bs1 + ch + SHA_K[i] + w[i]
            val bs0 = Integer.rotateRight(a, 2) xor Integer.rotateRight(a, 13) xor Integer.rotateRight(a, 22)
            val maj = (a and b) xor (a and c) xor (b and c)
            val t2 = bs0 + maj
            hh = g; g = f; f = e; e = d + t1; d = c; c = b; b = a; a = t1 + t2
        }
        h[0] += a; h[1] += b; h[2] += c; h[3] += d; h[4] += e; h[5] += f; h[6] += g; h[7] += hh
    }

    /** SHA-256 (FIPS 180-4) of buf[0, n); first digest word as unsigned 32 bit. */
    private fun sha256Word0(buf: ByteArray, n: Int): Long {
        val h = intArrayOf(
            1779033703, -1150833019, 1013904242, -1521486534, 1359893119, -1694144372, 528734635, 1541459225,
        )
        val w = IntArray(64)
        val full = n / 64
        for (bi in 0 until full) shaBlock(h, buf, bi * 64, w)
        val rem = n - full * 64
        val tail = ByteArray(128)
        System.arraycopy(buf, full * 64, tail, 0, rem)
        tail[rem] = 0x80.toByte()
        val tl = if (rem >= 56) 128 else 64
        val bits = n.toLong() * 8
        for (k in 0 until 8) tail[tl - 1 - k] = ((bits ushr (k * 8)) and 255).toByte()
        shaBlock(h, tail, 0, w)
        if (tl == 128) shaBlock(h, tail, 64, w)
        return h[0].toLong() and 0xFFFFFFFFL
    }

    /** FNV-1a 32 + SHA-256 over 4 MiB of LCG bytes; checksum = (fnv * 31 + sha_word0) mod M. */
    fun hash(): Long {
        val buf = ByteArray(HASH_N)
        var x = LCG_SEED
        for (i in 0 until HASH_N) {
            x = (x * 1103515245L + 12345L) and 0x7fffffffL
            buf[i] = ((x shr 16) and 255L).toByte()
        }
        var h = 2166136261L
        for (i in 0 until HASH_N) {
            h = ((h xor (buf[i].toLong() and 255L)) * 16777619L) and 0xFFFFFFFFL
        }
        val w0 = sha256Word0(buf, HASH_N)
        return (h * 31 + w0) % MOD
    }

    // ---- json: generate into a byte buffer, parse with a hand-written recursive-descent parser (counters only)
    private class ByteSink(cap: Int) {
        var a = ByteArray(cap)
        var n = 0
        fun put(b: Int) {
            if (n == a.size) a = a.copyOf(a.size * 2)
            a[n++] = b.toByte()
        }
        fun put(s: String) { for (ch in s) put(ch.code) }
        fun putInt(v: Long) {
            if (v == 0L) { put('0'.code); return }
            var t = v
            var digits = 0
            var q = t
            while (q > 0) { digits++; q /= 10 }
            if (n + digits > a.size) a = a.copyOf(maxOf(a.size * 2, n + digits))
            var p = n + digits - 1
            while (t > 0) { a[p--] = ('0'.code + (t % 10).toInt()).toByte(); t /= 10 }
            n += digits
        }
    }

    private class JsonCounters(val b: ByteArray) {
        var p = 0
        var num = 0L; var str = 0L; var obj = 0L; var arr = 0L; var tru = 0L
        private fun peek(): Int = b[p].toInt()
        private fun skip() { while (peek() == 32) p++ }
        private fun string() {
            p++
            val start = p
            while (peek() != 34) { if (peek() == 92) p++; p++ }
            str += (p - start)
            p++
        }
        fun value() {
            skip()
            val c = peek()
            when {
                c == 123 -> {
                    p++; obj++
                    skip()
                    if (peek() == 125) { p++; return }
                    var go = true
                    while (go) {
                        skip(); string(); skip(); p++
                        value(); skip()
                        val d = peek(); p++
                        if (d == 125) go = false
                    }
                }
                c == 91 -> {
                    p++; arr++
                    skip()
                    if (peek() == 93) { p++; return }
                    var go = true
                    while (go) {
                        value(); skip()
                        val d = peek(); p++
                        if (d == 93) go = false
                    }
                }
                c == 34 -> string()
                c == 116 -> { p += 4; tru++ }
                c == 102 -> p += 5
                c == 110 -> p += 4
                else -> {
                    var neg = false
                    if (c == 45) { neg = true; p++ }
                    var v = 0L
                    var dg = peek()
                    while (dg in 48..57) { v = v * 10 + (dg - 48); p++; dg = peek() }
                    if (neg) v = -v
                    num += v
                }
            }
        }
    }

    fun json(): Long {
        val sink = ByteSink(1 shl 16)
        var x = LCG_SEED
        sink.put('['.code)
        for (i in 0 until JSON_N) {
            x = (x * 1103515245L + 12345L) and 0x7fffffffL
            val a = x
            x = (x * 1103515245L + 12345L) and 0x7fffffffL
            val y = x
            if (i > 0) sink.put(','.code)
            sink.put("{\"id\":")
            sink.putInt(i.toLong())
            sink.put(",\"v\":")
            sink.putInt(a % 100000)
            sink.put(",\"name\":\"n")
            sink.putInt(i.toLong())
            sink.put("x\",\"f\":[")
            sink.putInt(y % 1000)
            sink.put(','.code)
            sink.putInt(y % 977)
            sink.put(','.code)
            sink.putInt((y / 1000) % 1000)
            sink.put("],\"ok\":")
            if (i % 2 == 0) sink.put("true}") else sink.put("false}")
        }
        sink.put(']'.code)
        sink.put(' '.code) // sentinel so peek() never leaves the array
        val jc = JsonCounters(sink.a)
        jc.value()
        return (jc.num + 7 * jc.str + 13 * jc.obj + 17 * jc.arr + 19 * jc.tru) % MOD
    }

    /** StringBuilder with a fresh temporary string per piece. */
    fun strbuild(): Long {
        val sb = StringBuilder()
        for (i in 0 until STR_N) {
            val piece = "item" + i.toString() + ";"
            sb.append(piece)
        }
        val n = sb.length
        var t = n.toLong() * 31
        for (j in 0 until n) t += sb[j].code.toLong() * ((j % 7) + 1)
        return t % MOD
    }

    /** 256x256 f64 matrix product, i-j-k order, no FMA; all values exact binary fractions. */
    fun matmul(): Long {
        val n = MAT_N
        val a = DoubleArray(n * n)
        val b = DoubleArray(n * n)
        val c = DoubleArray(n * n)
        for (i in 0 until n) for (j in 0 until n) {
            a[i * n + j] = ((i * 31 + j * 17) % 101).toDouble() * 0.25
            b[i * n + j] = ((i * 13 + j * 7) % 97).toDouble() * 0.5
        }
        for (i in 0 until n) for (j in 0 until n) {
            var sum = 0.0
            for (k in 0 until n) sum = sum + a[i * n + k] * b[k * n + j]
            c[i * n + j] = sum
        }
        var total = 0.0
        for (i in 0 until n * n) total = total + c[i]
        return (total * 8.0).toLong() % MOD
    }

    /** HashMap<Int, Int>: insert 300k LCG keys, then 600k lookups (hit + hit-or-miss). */
    fun hashmap(): Long {
        val n = MAP_N
        val keys = IntArray(n)
        val m = HashMap<Int, Int>()
        var x = LCG_SEED
        for (i in 0 until n) {
            x = (x * 1103515245L + 12345L) and 0x7fffffffL
            keys[i] = x.toInt()
            m[keys[i]] = i
        }
        var hits = 0L
        var total = 0L
        for (i in 0 until n) {
            val v = m[keys[(i.toLong() * 7919 % n).toInt()]]
            if (v != null) { hits++; total += v }
            val v2 = m[keys[i] xor 1]
            if (v2 != null) { hits++; total += v2 }
        }
        return (hits * 1000003 + total + m.size) % MOD
    }

    private class Item(val key: Long, val w: Long, val id: Long)

    /** ArrayList of 200k objects, sortWith(key, w), position-weighted id sum. */
    fun objsort(): Long {
        val list = ArrayList<Item>()
        var x = LCG_SEED
        for (i in 0 until OBJ_N) {
            x = (x * 1103515245L + 12345L) and 0x7fffffffL
            val key = x % 1000
            x = (x * 1103515245L + 12345L) and 0x7fffffffL
            list.add(Item(key, x, i.toLong()))
        }
        list.sortWith { p, q -> if (p.key != q.key) p.key.compareTo(q.key) else p.w.compareTo(q.w) }
        var t = 0L
        for (pos in 0 until OBJ_N) t += list[pos].id * ((pos % 251) + 1)
        return t % MOD
    }
}
