#!/usr/bin/env python3
"""Baut einen Eingabekorpus fuer die Durchsatzmessung.

Es gibt ZWEI Korpora, weil ein einzelner in die Irre fuehrt:

  --quelle html5lib  (Vorgabe)
      Die Eingaben der html5lib-Faelle (nur die im Data-Zustand startenden,
      ohne doubleEscaped-Sonderfaelle), vielfach aneinandergehaengt.

      ACHTUNG, ehrlich benannt: dieser Korpus ist ABSICHTLICH PATHOLOGISCH
      und misst den schlechtesten Fall, nicht den Alltag. Die html5lib-Suite
      besteht fast nur aus Grenzfaellen — kaputte Tags, abgebrochene
      Zeichenreferenzen, Nullbytes, ungepaarte Surrogate, Doctype-Muell,
      tausende sehr kurze Eingaben. Der Anteil an Zustandswechseln je Byte
      liegt um ein Vielfaches ueber echtem HTML, und lange Textlaeufe — der
      Fall, den jeder Tokenizer schnell macht — fehlen praktisch ganz.
      Ein MB/s-Wert auf diesem Korpus ist damit KEINE Aussage darueber, wie
      schnell echte Seiten verarbeitet werden. Er wird trotzdem ausgewiesen,
      weil er genau die Arbeit misst, die die Testsuite prueft.

  --quelle realweb
      Acht am 14.08.2026 gespeicherte echte Seiten aus testdata/realweb/
      (Wikipedia, WHATWG-HTML-Standard, W3C, rustdoc, Hacker News), zusammen
      rund 4,6 MB, unveraendert wie ausgeliefert. Das ist der Korpus fuer die
      Frage "wie schnell ist das auf echtem HTML".

Es entstehen ZWEI Dateien mit demselben Inhalt:

  <korpus.html>     roher Text  — fuer die Referenzimplementierung
  <korpus.auftrag>  EIN Auftrag im Protokoll aus tools/tokenizer/LOG.md

Aufruf:  korpus.py <html-pfad> <auftrag-pfad> [--quelle html5lib|realweb]
"""
import glob
import json
import os
import struct
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
ZIEL_MB = float(os.environ.get("KORPUS_MB", "4"))


def korpus_html5lib():
    """Grenzfall-Korpus: alle html5lib-Eingaben, bis ZIEL_MB verdoppelt."""
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
    return text.encode("utf-8", "surrogatepass")


def korpus_realweb():
    """Echte Seiten aus testdata/realweb/, unveraendert aneinandergehaengt."""
    verz = os.path.join(ROOT, "testdata", "realweb")
    dateien = sorted(glob.glob(os.path.join(verz, "*.html")))
    if not dateien:
        sys.exit("korpus.py: keine Seiten in testdata/realweb/ gefunden")
    roh = b"\n".join(open(p, "rb").read() for p in dateien)
    # Die acht gespeicherten Seiten ergeben bereits > 4 MB; verdoppelt wird
    # nur, falls jemand Seiten entfernt — lieber messbar gross als still zu
    # klein gemessen.
    while len(roh) < ZIEL_MB * 1048576:
        roh += b"\n" + roh
    return roh


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    html_pfad, auftrag_pfad = sys.argv[1], sys.argv[2]
    quelle = "html5lib"
    if "--quelle" in sys.argv:
        quelle = sys.argv[sys.argv.index("--quelle") + 1]
    if quelle == "html5lib":
        roh = korpus_html5lib()
    elif quelle == "realweb":
        roh = korpus_realweb()
    else:
        sys.exit("korpus.py: unbekannte quelle '%s' (html5lib|realweb)" % quelle)

    open(html_pfad, "wb").write(roh)
    with open(auftrag_pfad, "wb") as fh:
        # zustand, flaggen, len_lasttag, len_input (siehe PROTOKOLL.md)
        fh.write(struct.pack("<I", 0) + struct.pack("<I", 0) + struct.pack("<I", 0))
        fh.write(struct.pack("<I", len(roh)) + roh)
    print("   Korpus (%s): %.2f MB" % (quelle, len(roh) / 1048576))


if __name__ == "__main__":
    main()
