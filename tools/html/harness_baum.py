#!/usr/bin/env python3
"""Laeufer fuer die HTML-Baumkonstruktion aus lib/browser/ (in Firn).

WERKBANK, KEIN PRODUKT: dieses Skript enthaelt KEINE Parserlogik. Es
uebersetzt die Faelle in Auftraege, ruft das in Firn geschriebene Binary
GENAU EINMAL auf und vergleicht die Antwort Zeile fuer Zeile mit der
Erwartung.

Datenformat: das `.dat`-Format der html5lib-`tree-construction`-Tests.
Bewusst genau dieses und kein eigenes — liegen die Originaldaten eines Tages
vor, laeuft dieser Laeufer ohne Aenderung dagegen (siehe docs/RUNDE54.md).

Ehrlichkeitsregeln:
  * JEDER Fall aus allen .dat-Dateien wird gezaehlt. Es gibt kein
    Ueberspringen und keine Filter.
  * `#document-fragment`-Faelle (Zerlegung mit Kontextelement) sind NICHT
    umgesetzt und zaehlen als FEHLSCHLAG, nicht als uebersprungen.
  * Ein `#KAPUTT`-Vermerk des Binaries ist ein Fehlschlag.
  * Verglichen wird der vollstaendige Baum, nicht ein Ausschnitt.

Aufruf: python3 tools/html/harness_baum.py <binary> [--json datei] [--zeige N]
                                           [--nur MUSTER]
"""

import glob
import json
import os
import struct
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
FAELLE = os.path.join(ROOT, "tools", "html", "cases")
LUECKEN = os.path.join(ROOT, "tools", "html", "luecken")


def lade_dat(pfad):
    """Liest eine .dat-Datei: Liste von (data, document, fragment_kontext)."""
    faelle = []
    with open(pfad, encoding="utf-8") as fh:
        text = fh.read()
    for block in text.split("\n#data\n"):
        block = block.lstrip("\n")
        if block.startswith("#data\n"):
            block = block[len("#data\n"):]
        if not block.strip():
            continue
        teile = {}
        aktuell = "data"
        teile[aktuell] = []
        for zeile in block.split("\n"):
            if zeile.startswith("#") and " " not in zeile.rstrip():
                aktuell = zeile[1:].strip()
                teile[aktuell] = []
                continue
            teile.setdefault(aktuell, []).append(zeile)
        data = "\n".join(teile.get("data", []))
        doc = teile.get("document", [])
        while doc and doc[-1] == "":
            doc.pop()
        kontext = "\n".join(teile.get("document-fragment", [])).strip() or None
        faelle.append((data, "\n".join(doc), kontext))
    return faelle


def lade_alle(muster=None, verzeichnis=None):
    alle = []
    for pfad in sorted(glob.glob(os.path.join(verzeichnis or FAELLE, "*.dat"))):
        if muster and muster not in os.path.basename(pfad):
            continue
        for i, (data, doc, kontext) in enumerate(lade_dat(pfad)):
            alle.append((os.path.basename(pfad), i, data, doc, kontext))
    return alle


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    binary = sys.argv[1]
    json_ziel = None
    zeige = 0
    muster = None
    verzeichnis = None
    args = sys.argv[2:]
    i = 0
    while i < len(args):
        if args[i] == "--json":
            json_ziel = args[i + 1]
            i += 2
        elif args[i] == "--zeige":
            zeige = int(args[i + 1])
            i += 2
        elif args[i] == "--nur":
            muster = args[i + 1]
            i += 2
        elif args[i] == "--luecken":
            verzeichnis = LUECKEN
            i += 1
        else:
            print("unbekannte option: %s" % args[i])
            return 2

    faelle = lade_alle(muster, verzeichnis)
    if not faelle:
        print("KEINE FAELLE GEFUNDEN in %s" % (verzeichnis or FAELLE))
        return 1

    payload = b""
    for _, _, data, _, _ in faelle:
        roh = data.encode("utf-8", "surrogatepass")
        payload += struct.pack("<I", len(roh)) + roh

    p = subprocess.run([binary], input=payload, stdout=subprocess.PIPE,
                       stderr=subprocess.PIPE, timeout=600)
    if p.returncode != 0:
        print("BINARY ENDETE MIT %d" % p.returncode)
        print(p.stderr.decode("utf-8", "replace")[:2000])
        return 1
    roh = p.stdout.decode("utf-8", "surrogatepass")
    teile = roh.split("#ENDE\n")
    if teile and teile[-1] == "":
        teile.pop()
    if len(teile) != len(faelle):
        print("ANTWORTZAHL FALSCH: %d Bloecke fuer %d Faelle" % (len(teile), len(faelle)))
        return 1

    je_datei = {}
    fehler = []
    bestanden = 0
    for (datei, idx, data, erwartet, kontext), antwort in zip(faelle, teile):
        ist = antwort.rstrip("\n")
        ok = (ist == erwartet) and kontext is None and "#KAPUTT" not in antwort
        st = je_datei.setdefault(datei, [0, 0])
        st[1] += 1
        if ok:
            bestanden += 1
            st[0] += 1
        else:
            grund = "fragment nicht umgesetzt" if kontext else "baum weicht ab"
            if "#KAPUTT" in antwort:
                grund = antwort.splitlines()[0]
            fehler.append((datei, idx, data, erwartet, ist, grund))

    breite = max(len(x) for x in je_datei) + 2
    print("%-*s %8s %8s %8s" % (breite, "Datei", "gut", "gesamt", "Quote"))
    print("-" * (breite + 28))
    for datei in sorted(je_datei):
        gut, ges = je_datei[datei]
        print("%-*s %8d %8d %7.2f %%" % (breite, datei, gut, ges, 100.0 * gut / ges))
    print("-" * (breite + 28))
    ges = len(faelle)
    print("%-*s %8d %8d %7.2f %%" % (breite, "GESAMT", bestanden, ges,
                                     100.0 * bestanden / ges))

    if zeige and fehler:
        print("\nErste %d Fehlschlaege:" % min(zeige, len(fehler)))
        for datei, idx, data, erwartet, ist, grund in fehler[:zeige]:
            print("\n--- %s #%d (%s)" % (datei, idx, grund))
            print("    input: %r" % data)
            print("    erwartet:")
            for z in erwartet.split("\n"):
                print("      " + z)
            print("    bekommen:")
            for z in ist.split("\n"):
                print("      " + z)

    if json_ziel:
        with open(json_ziel, "w", encoding="utf-8") as fh:
            json.dump({"passed": bestanden, "total": ges,
                       "je_datei": je_datei}, fh, indent=1)
    return 0 if bestanden == ges else 1


if __name__ == "__main__":
    sys.exit(main())
