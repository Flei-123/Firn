#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Uebersetzt die Diagnose- und Ausgabetexte beider Uebersetzer ins Englische.

Die Tabelle `messages.tsv` bildet den EXAKTEN Rumpf eines Zeichenkettenliterals
(so wie er im Quelltext zwischen den Anfuehrungszeichen steht, mit Maskierungen)
auf den englischen Rumpf ab. Ersetzt wird nur bei VOLLSTAENDIGER Uebereinstimmung
— nie ein Teilstueck; sonst zerfaellt der Text unkontrolliert.

Fuer Firn-Quellen (`*.fi`) zieht das Werkzeug ausserdem die beiden Laengenzahlen
mit, die dort neben jedem Literal stehen:

  1. die Deklaration   `var m: [u8; N] = "..."`   -> N = Bytelaenge des Literals
  2. die Aufrufstelle  `(&m[0]) as u64, K`        -> K wird gemaess seiner alten
     Bedeutung neu gesetzt: war K die volle Laenge, bleibt es die volle Laenge;
     war es die Laenge ohne abschliessende `\\0`, bleibt es das; war es die
     Laenge ohne Auffuellzeichen (Leerzeichen am Ende), bleibt es das.
     Passt K in kein Schema, wird es GEMELDET und nicht angefasst.

Die Bytelaenge wird ueber die dekodierte Zeichenkette bestimmt: `\\0`, `\\n`,
`\\t`, `\\\\`, `\\"`, `\\xNN` sind je ein Byte, Zeichen ausserhalb von ASCII
zaehlen mit ihren UTF-8-Bytes (z. B. `—` = 3).
"""
import os, re, glob

HIER = os.path.dirname(os.path.abspath(__file__))
WURZEL = os.path.dirname(os.path.dirname(HIER))
ESC = set('0ntr\\"\'')


def bytelaenge(rumpf: str) -> int:
    """Bytelaenge des Literals, dessen Quelltextrumpf `rumpf` ist."""
    n, i = 0, 0
    while i < len(rumpf):
        c = rumpf[i]
        if c == '\\' and i + 1 < len(rumpf):
            k = rumpf[i + 1]
            if k == 'x':
                i += 4; n += 1; continue
            if k == 'u' and i + 2 < len(rumpf) and rumpf[i + 2] == '{':
                j = rumpf.index('}', i)
                n += len(chr(int(rumpf[i + 3:j], 16)).encode()); i = j + 1; continue
            if k in ESC:
                i += 2; n += 1; continue
            i += 2; n += 2; continue
        n += len(c.encode('utf-8')); i += 1
    return n


def sichtbar(rumpf: str) -> str:
    """Rumpf ohne abschliessendes `\\0`."""
    return rumpf[:-2] if rumpf.endswith('\\0') else rumpf


def tabelle():
    t = {}
    for zeile in open(os.path.join(HIER, 'messages.tsv'), encoding='utf-8'):
        zeile = zeile.rstrip('\n')
        if not zeile.strip() or ('\t' not in zeile and zeile.startswith('#')):
            continue
        de, en = zeile.split('\t')
        t[de] = en
    return t


LIT = re.compile(r'"((?:[^"\\\n]|\\.)*)"')
DECL = re.compile(r'(\[u8;\s*)(\d+)(\s*\]\s*=\s*)"((?:[^"\\\n]|\\.)*)"')
RUF = re.compile(r'(\(&(\w+)\[0\]\)\s*as\s*u64,\s*)(\d+)')
VAR = re.compile(r'\bvar\s+(\w+)\s*:\s*\[u8;\s*\d+\s*\]\s*=\s*"((?:[^"\\\n]|\\.)*)"')


def rust(t, dateien):
    n = 0
    for f in dateien:
        s = open(f, encoding='utf-8').read()

        def ers(m):
            nonlocal n
            if m.group(1) in t:
                n += 1
                return '"' + t[m.group(1)] + '"'
            return m.group(0)
        neu = LIT.sub(ers, s)
        if neu != s:
            open(f, 'w', encoding='utf-8').write(neu)
    return n


def firn(t, dateien):
    getroffen, offen = 0, []
    for f in dateien:
        if os.path.islink(f) or f.endswith('gctext.fi'):
            continue
        zeilen = open(f, encoding='utf-8').read().split('\n')
        stand = {}          # Variablenname -> alter Rumpf der NAECHSTGELEGENEN Deklaration
        aus = []
        for zeile in zeilen:
            for m in VAR.finditer(zeile):
                stand[m.group(1)] = m.group(2)

            def ers(m):
                nonlocal getroffen
                rumpf = m.group(4)
                if rumpf not in t:
                    return m.group(0)
                getroffen += 1
                neu = t[rumpf]
                return m.group(1) + str(bytelaenge(neu)) + m.group(3) + '"' + neu + '"'
            z2 = DECL.sub(ers, zeile)

            def ruf(m):
                v, k = m.group(2), int(m.group(3))
                if v not in stand or stand[v] not in t:
                    return m.group(0)
                a, b = stand[v], t[stand[v]]
                if k == bytelaenge(a):
                    z = bytelaenge(b)
                elif k == bytelaenge(sichtbar(a)):
                    z = bytelaenge(sichtbar(b))
                elif k == bytelaenge(sichtbar(a).rstrip(' ')):
                    z = bytelaenge(sichtbar(b).rstrip(' '))
                else:
                    offen.append((f, v, k, bytelaenge(a), bytelaenge(b)))
                    return m.group(0)
                return m.group(1) + str(z)
            z2 = RUF.sub(ruf, z2)

            def rest(m):
                nonlocal getroffen
                if m.group(1) in t and '[u8;' not in z2[max(0, m.start() - 40):m.start()]:
                    getroffen += 1
                    return '"' + t[m.group(1)] + '"'
                return m.group(0)
            z2 = LIT.sub(rest, z2)
            aus.append(z2)
        neu_text = '\n'.join(aus)
        if neu_text != '\n'.join(zeilen):
            open(f, 'w', encoding='utf-8').write(neu_text)
    return getroffen, offen


def main():
    os.chdir(WURZEL)
    t = tabelle()
    rs = sorted(glob.glob('compiler/src/*.rs'))
    fi = sorted(set(glob.glob('lib/**/*.fi', recursive=True) +
                    glob.glob('bin/*.fi') +
                    glob.glob('tools/**/*.fi', recursive=True)))
    a = rust(t, rs)
    b, offen = firn(t, fi)
    print('ersetzt: rust', a, ' firn', b)
    if offen:
        print('UNKLARE LAENGEN AN AUFRUFSTELLEN (von Hand pruefen):')
        for o in offen:
            print('  ', o)


if __name__ == '__main__':
    main()
