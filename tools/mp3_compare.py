#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""mp3_compare.py -- misst zwei PCM-Dateien (s16le) gegeneinander.

Gibt aus: Zahl der Werte, groesster Absolutfehler, quadratisches Mittel
des Fehlers und den Signal-Rausch-Abstand in dB. Bit-genau heisst
max = 0. Alles andere ist ein Befund, keine Meinung.
"""
import sys
import array
import math


def lade(pfad):
    a = array.array('h')
    with open(pfad, 'rb') as f:
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
    summe = 0.0
    signal = 0.0
    erste = -1
    anzahl_ungleich = 0
    for i in range(n):
        d = a[i] - b[i]
        if d:
            anzahl_ungleich += 1
            if erste < 0:
                erste = i
        if abs(d) > maxf:
            maxf = abs(d)
        summe += d * d
        signal += float(a[i]) * a[i]
    rms = math.sqrt(summe / n)
    if summe == 0:
        snr = float('inf')
    else:
        snr = 10 * math.log10(signal / summe) if signal > 0 else 0.0
    print('werte    %d (Referenz %d, Pruefling %d)' % (n, len(a), len(b)))
    print('ungleich %d (%.4f %%), erster bei %d' % (anzahl_ungleich, 100.0 * anzahl_ungleich / n, erste))
    print('maxfehler %d' % maxf)
    print('rms       %.4f' % rms)
    print('snr       %.1f dB' % snr)
    return 0 if maxf == 0 else 1


sys.exit(main())
