#!/usr/bin/env python3
"""Baut den Eingabekorpus fuer die Durchsatzmessung.

Quelle sind die Eingaben der html5lib-Faelle (nur die im Data-Zustand
startenden, ohne doubleEscaped-Sonderfaelle), vielfach aneinandergehaengt.
Es entstehen ZWEI Dateien mit demselben Inhalt:

  <korpus.html>     roher Text  — fuer die Referenzimplementierung
  <korpus.auftrag>  EIN Auftrag im Protokoll aus tools/tokenizer/PROTOKOLL.md
"""
import glob
import json
import os
import struct
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
ZIEL_MB = float(os.environ.get("KORPUS_MB", "4"))


def main():
    html_pfad, auftrag_pfad = sys.argv[1], sys.argv[2]
    stuecke = []
    for pfad in sorted(glob.glob(os.path.join(ROOT, "testdata", "html5lib-tokenizer", "*.test"))):
        d = json.load(open(pfad, encoding="utf-8"))
        liste = d.get("tests") or d.get("xmlViolationTests", [])
        for t in liste:
            if t.get("doubleEscaped") or t.get("initialStates"):
                continue
            stuecke.append(t["input"])
    text = "\n".join(stuecke)
    while len(text.encode("utf-8", "surrogatepass")) < ZIEL_MB * 1048576:
        text += "\n" + text
    roh = text.encode("utf-8", "surrogatepass")
    open(html_pfad, "wb").write(roh)
    with open(auftrag_pfad, "wb") as fh:
        fh.write(struct.pack("<I", 0) + struct.pack("<I", 0))
        fh.write(struct.pack("<I", len(roh)) + roh)
    print("   Korpus: %.2f MB" % (len(roh) / 1048576))


if __name__ == "__main__":
    main()
