#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""mp3_compare.py -- measures two PCM files (s16le) against each other.

Prints: number of values, largest absolute error, mean square
of the error and the signal-to-noise ratio in dB. Bit-exact means
max = 0. Anything else is a finding, not an opinion.
"""
import sys
import array
import math


def lade(fpath):
    a = array.array('h')
    with open(fpath, 'rb') as f:
        roh = f.read()
    a.frombytes(roh[:len(roh) // 2 * 2])
    return a


def main():
    a = lade(sys.argv[1])
    b = lade(sys.argv[2])
    n = min(len(a), len(b))
    if n == 0:
        print('LEER')
        return 1
    maxf = 0
    total = 0.0
    signal = 0.0
    first_diff = -1
    count_unequal = 0
    for i in range(n):
        d = a[i] - b[i]
        if d:
            count_unequal += 1
            if first_diff < 0:
                first_diff = i
        if abs(d) > maxf:
            maxf = abs(d)
        total += d * d
        signal += float(a[i]) * a[i]
    rms = math.sqrt(total / n)
    if total == 0:
        snr = float('inf')
    else:
        snr = 10 * math.log10(signal / total) if signal > 0 else 0.0
    print('werte    %d (Referenz %d, Pruefling %d)' % (n, len(a), len(b)))
    print('ungleich %d (%.4f %%), erster bei %d' % (count_unequal, 100.0 * count_unequal / n, first_diff))
    print('maxfehler %d' % maxf)
    print('rms       %.4f' % rms)
    print('snr       %.1f dB' % snr)
    return 0 if maxf == 0 else 1


sys.exit(main())
