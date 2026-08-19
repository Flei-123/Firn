#!/usr/bin/env python3
"""Robustheitsprobe der Baumkonstruktion auf ECHTEN Seiten.

Die acht Seiten in `testdata/realweb/` sind unveraenderte Kopien echter
Webseiten (0,03 bis 1,0 MB). Geprueft wird hier nur, was ohne fremde
Bibliothek pruefbar ist:

  * das Binary laeuft durch und meldet keinen `#KAPUTT`-Abbruch,
  * es kommt ein nichtleerer Baum heraus,
  * die Ausgabe ist reproduzierbar (run.sh vergleicht die drei Baustufen
    Byte fuer Byte gegeneinander).

Der VERGLEICH mit einer unabhaengigen Umsetzung steht nicht hier, sondern in
tools/html/orakel.py (braucht html5lib und damit Netz).

Aufruf:  python3 tools/html/realweb.py <binary>
"""

import glob
import hashlib
import os
import struct
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
KORPUS = os.path.join(ROOT, "testdata", "realweb")


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    binary = sys.argv[1]
    pfade = sorted(glob.glob(os.path.join(KORPUS, "*.html")))
    if not pfade:
        print("KEIN KORPUS in %s" % KORPUS)
        return 1
    daten = []
    for p in pfade:
        with open(p, encoding="utf-8", errors="replace") as fh:
            daten.append(fh.read())
    payload = b""
    for d in daten:
        roh = d.encode("utf-8", "surrogatepass")
        payload += struct.pack("<I", len(roh)) + roh

    p = subprocess.run([binary], input=payload, stdout=subprocess.PIPE,
                       stderr=subprocess.PIPE, timeout=900)
    if p.returncode != 0:
        print("BINARY ENDETE MIT %d" % p.returncode)
        return 1
    teile = p.stdout.decode("utf-8", "surrogatepass").split("#ENDE\n")
    if teile and teile[-1] == "":
        teile.pop()
    if len(teile) != len(pfade):
        print("ANTWORTZAHL FALSCH: %d fuer %d Seiten" % (len(teile), len(pfade)))
        return 1
    schlecht = 0
    for pfad, quelle, antwort in zip(pfade, daten, teile):
        zeilen = antwort.count("\n")
        kaputt = "#KAPUTT" in antwort
        summe = hashlib.sha256(antwort.encode("utf-8", "surrogatepass")).hexdigest()[:16]
        if kaputt or zeilen < 10:
            schlecht += 1
        print("%-26s %9d B ->%8d Zeilen  %s  %s"
              % (os.path.basename(pfad), len(quelle), zeilen, summe,
                 "KAPUTT" if kaputt else "ok"))
    print("%d Seiten, %d beanstandet" % (len(pfade), schlecht))
    return 1 if schlecht else 0


if __name__ == "__main__":
    sys.exit(main())
