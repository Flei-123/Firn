#!/usr/bin/env python3
"""Pruefstand fuer lib/html/entities.fi (Zeichenreferenzen) — Werkbank.

Dieses Skript enthaelt KEINE Tokenizer-Logik. Es baut aus den offiziellen
html5lib-Testdaten alle Faelle heraus, die sich allein mit dem
Zeichenreferenz-Teil entscheiden lassen (Data state, kein '<' in der Eingabe,
Erwartung besteht nur aus Character-Token), faehrt sie durch den in Firn
geschriebenen Pruefstand lib/html/entities_probe.fi und vergleicht.

Faelle, die dieser enge Ausschnitt nicht abdeckt, werden hier NICHT gezaehlt —
die verbindliche Gesamtzahl liefert allein tools/tokenizer/harness.py ueber
alle 6.810 Faelle. Dieses Skript ist ein Modulnachweis, keine Bilanz.

Aufruf:  python3 tools/tokenizer/pruefe_entities.py [binary]
"""

import glob
import json
import os
import struct
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
TESTDIR = os.path.join(ROOT, "testdata", "html5lib-tokenizer")
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import unescape, unescape_token, normalisiere  # noqa: E402


def faelle():
    """Alle Faelle, die reine Zeichenreferenz-Faelle im Data state sind."""
    raus = []
    for pfad in sorted(glob.glob(os.path.join(TESTDIR, "*.test"))):
        with open(pfad, encoding="utf-8") as fh:
            daten = json.load(fh)
        liste = daten.get("tests")
        if liste is None:
            liste = daten.get("xmlViolationTests", [])
        for i, t in enumerate(liste):
            ein = t["input"]
            erwartet = t["output"]
            if t.get("doubleEscaped"):
                ein = unescape(ein)
                erwartet = unescape_token(erwartet)
            states = t.get("initialStates") or ["Data state"]
            if states != ["Data state"]:
                continue
            if "<" in ein or "\0" in ein or "\r" in ein or "&" not in ein:
                continue
            erwartet = normalisiere(erwartet)
            if any(tok[0] != "Character" for tok in erwartet):
                continue
            raus.append((os.path.basename(pfad), i, t.get("description", ""),
                         ein, erwartet))
    return raus


def main():
    binary = sys.argv[1] if len(sys.argv) > 1 else os.path.join(
        ROOT, ".tokenizer-work", "entities_probe")
    if not os.path.exists(binary):
        print("nicht gebaut: " + binary)
        return 2
    liste = faelle()
    roh = bytearray()
    for _, _, _, ein, _ in liste:
        b = ein.encode("utf-8", "surrogatepass")
        # zustand, flaggen, len_lasttag, len_input (siehe PROTOKOLL.md)
        roh += struct.pack("<I", 0) + struct.pack("<I", 0) + struct.pack("<I", 0)
        roh += struct.pack("<I", len(b)) + b
    p = subprocess.run([binary], input=bytes(roh), stdout=subprocess.PIPE)
    zeilen = p.stdout.decode("ascii", "replace").splitlines()
    if len(zeilen) != len(liste):
        print("FEHLER: %d Antworten fuer %d Faelle" % (len(zeilen), len(liste)))
        return 1

    gut = 0
    schlecht = []
    for (datei, i, beschr, ein, erwartet), zeile in zip(liste, zeilen):
        try:
            # Antwortzeile: Tokenstrom TAB Parse-Fehlerliste (PROTOKOLL.md).
            # Der Pruefstand vergleicht nur den Tokenstrom.
            ist = normalisiere(json.loads(zeile.split("\t")[0]))
        except ValueError:
            ist = ["<kaputte antwort>"]
        if ist == erwartet:
            gut += 1
        else:
            schlecht.append((datei, i, beschr, ein, erwartet, ist))

    print("Zeichenreferenzen (lib/html/entities.fi), reine Data-state-Faelle")
    print("  bestanden: %d / %d" % (gut, len(liste)))
    for datei, i, beschr, ein, erwartet, ist in schlecht[:20]:
        print("  FEHL %s #%d %s" % (datei, i, beschr))
        print("       ein=%r" % ein)
        print("       soll=%s" % json.dumps(erwartet))
        print("       ist =%s" % json.dumps(ist))
    if schlecht:
        print("  ... %d Fehlschlaege" % len(schlecht))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
