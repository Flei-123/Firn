#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
# tools/english/lengths.py — prueft (und richtet) die Laengenangaben von
# Zeichenkettenpuffern in Firn:  var m: [u8; 42] = "…"
#
# Firn hat kein `len(literal)`: die Groesse des Feldes steht als Zahl da. Wird
# ein Text uebersetzt, MUSS die Zahl mitwandern — sonst passt der Puffer
# scheinbar und bricht spaeter. Dieses Werkzeug rechnet die Zahl aus dem
# Literal aus.
#
#   python3 tools/english/lengths.py            # nur melden
#   python3 tools/english/lengths.py --fix      # richtigstellen
#
# Nur `[u8; N] = "…"` und `[u16; N] = u"…"` werden angefasst; Felder, die
# absichtlich groesser sind als ihr Anfangswert, gibt es in Firn nicht (das
# Literal fuellt das Feld vollstaendig).
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
os.chdir(ROOT)

MUSTER = re.compile(r'\[\s*(u8|u16)\s*;\s*(\d+)\s*\]\s*=\s*(b?u?)("(?:[^"\\]|\\.)*")')
AUS = ('.git', 'target', '__pycache__', '.test-work', 'testdata', '.gc-meas-work',
       'lexneg')   # tests/lexneg enthaelt ABSICHTLICH kaputte Literale


def oktette(lit):
    """Dekodiert ein Firn-Literal (ohne Anfuehrungszeichen) zu Bytes."""
    s = lit[1:-1]
    aus = bytearray()
    i = 0
    while i < len(s):
        c = s[i]
        if c != '\\':
            aus += c.encode('utf-8')
            i += 1
            continue
        e = s[i + 1]
        i += 2
        einfach = {'n': 0x0A, 'r': 0x0D, 't': 0x09, '0': 0x00,
                   '\\': 0x5C, '"': 0x22, "'": 0x27}
        if e in einfach:
            aus.append(einfach[e])
        elif e == 'x':
            aus.append(int(s[i:i + 2], 16))
            i += 2
        elif e == 'u':
            if s[i] == '{':
                j = s.index('}', i)
                cp = int(s[i + 1:j], 16)
                i = j + 1
            else:
                cp = int(s[i:i + 4], 16)
                i += 4
            if 0xD800 <= cp <= 0xDFFF:      # ungepaartes Surrogat: WTF-8
                aus += bytes([0xE0 | (cp >> 12), 0x80 | ((cp >> 6) & 0x3F),
                              0x80 | (cp & 0x3F)])
            else:
                aus += chr(cp).encode('utf-8')
        else:
            raise ValueError('unbekannte maskierung \\' + e)
    return bytes(aus)


def einheiten(lit):
    """Laenge eines u"…"-Literals in UTF-16-Einheiten."""
    b = oktette(lit).decode('utf-8', 'surrogatepass')
    return len(b.encode('utf-16-le', 'surrogatepass')) // 2


def main(argv):
    fix = '--fix' in argv
    schlecht = 0
    for d, _, fs in os.walk('.'):
        if any(a in d.split(os.sep) for a in AUS):
            continue
        for f in fs:
            p = os.path.join(d, f)
            if not f.endswith('.fi') or os.path.islink(p):
                continue
            s = open(p, encoding='utf-8').read()
            neu = []
            pos = 0
            aend = 0
            for m in MUSTER.finditer(s):
                typ, n, praefix, lit = m.group(1), int(m.group(2)), m.group(3), m.group(4)
                try:
                    soll = einheiten(lit) if typ == 'u16' else len(oktette(lit))
                except ValueError as e:
                    print(f"{p}: {e}")
                    continue
                if soll == n:
                    continue
                zeile = s.count('\n', 0, m.start()) + 1
                print(f"{p}:{zeile}: [{typ}; {n}] -> {soll}   {lit[:50]}")
                schlecht += 1
                if fix:
                    neu.append(s[pos:m.start()])
                    neu.append(f"[{typ}; {soll}] = {praefix}{lit}")
                    pos = m.end()
                    aend += 1
            if fix and aend:
                neu.append(s[pos:])
                open(p, 'w', encoding='utf-8').write(''.join(neu))
    print(f"{schlecht} falsche laengenangaben" + (" (berichtigt)" if fix else ""))
    return 1 if schlecht and not fix else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
