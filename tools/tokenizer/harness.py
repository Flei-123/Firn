#!/usr/bin/env python3
"""Harness fuer den HTML5-Tokenizer aus lib/html/ gegen html5lib-tests.

WERKBANK, KEIN PRODUKT: dieses Skript enthaelt KEINE Tokenizer-Logik. Es
uebersetzt die Testfaelle in Auftraege, ruft das in Firn geschriebene Binary
genau einmal auf und vergleicht die Antwort mit der Erwartung.

Ehrlichkeitsregeln (Messlatte Punkt a):
  * JEDER Fall aus allen .test-Dateien wird gezaehlt — auch `xmlViolationTests`.
  * `doubleEscaped: true` entschluesselt input UND output zusaetzlich \\uXXXX.
  * `initialStates` und `lastStartTag` werden beachtet; ein Fall gilt nur als
    bestanden, wenn er in JEDEM seiner Startzustaende stimmt.
  * Nicht unterstuetzte Faelle (Antwort ["NICHT-UNTERSTUETZT"]) sind
    FEHLSCHLAEGE. Es gibt kein Ueberspringen und keine Filter.

Aufruf:  python3 tools/tokenizer/harness.py <binary> [--json datei] [--zeige N]
"""

import glob
import json
import os
import re
import struct
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
TESTDIR = os.path.join(ROOT, "testdata", "html5lib-tokenizer")

STATES = {
    "Data state": 0,
    "PLAINTEXT state": 1,
    "RCDATA state": 2,
    "RAWTEXT state": 3,
    "Script data state": 4,
    "CDATA section state": 5,
}


def unescape(text):
    """\\uXXXX-Entschluesselung fuer `doubleEscaped`-Faelle."""
    return re.sub(
        r"\\u([0-9A-Fa-f]{4})", lambda m: chr(int(m.group(1), 16)), text
    )


def unescape_token(tok):
    if isinstance(tok, str):
        return unescape(tok)
    if isinstance(tok, list):
        return [unescape_token(x) for x in tok]
    if isinstance(tok, dict):
        return {unescape(k): unescape_token(v) for k, v in tok.items()}
    return tok


def normalisiere(tokens):
    """Vergleichsform: Character-Token verschmelzen, ParseError entfernen,
    StartTag auf (name, attrs, self_closing) vereinheitlichen."""
    out = []
    for t in tokens:
        if t == "ParseError":
            continue
        if not isinstance(t, list):
            out.append(t)
            continue
        art = t[0]
        if art == "Character":
            if out and out[-1][0] == "Character":
                out[-1] = ["Character", out[-1][1] + t[1]]
            else:
                out.append(["Character", t[1]])
        elif art == "StartTag":
            attrs = t[2] if len(t) > 2 else {}
            self_closing = bool(t[3]) if len(t) > 3 else False
            out.append(["StartTag", t[1], attrs, self_closing])
        elif art == "EndTag":
            out.append(["EndTag", t[1]])
        else:
            out.append(list(t))
    return out


def lade_faelle():
    """Liefert [(datei, index, beschreibung, input, erwartung, states, lasttag)]."""
    faelle = []
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
            faelle.append(
                (
                    os.path.basename(pfad),
                    i,
                    t.get("description", ""),
                    ein,
                    normalisiere(erwartet),
                    states,
                    t.get("lastStartTag", ""),
                )
            )
    return faelle


def auftraege(faelle):
    roh = bytearray()
    plan = []  # (fall_index, state_name)
    for k, (_, _, _, ein, _, states, lasttag) in enumerate(faelle):
        for st in states:
            code = STATES.get(st)
            if code is None:
                raise SystemExit("unbekannter Startzustand: %r" % st)
            lt = lasttag.encode("utf-8", "surrogatepass")
            eb = ein.encode("utf-8", "surrogatepass")
            roh += struct.pack("<I", code)
            roh += struct.pack("<I", len(lt)) + lt
            roh += struct.pack("<I", len(eb)) + eb
            plan.append((k, st))
    return bytes(roh), plan


def main():
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    binary = sys.argv[1]
    json_out = None
    zeige = 0
    args = sys.argv[2:]
    while args:
        a = args.pop(0)
        if a == "--json":
            json_out = args.pop(0)
        elif a == "--zeige":
            zeige = int(args.pop(0))
        else:
            raise SystemExit("unbekannte Option %r" % a)

    faelle = lade_faelle()
    roh, plan = auftraege(faelle)
    p = subprocess.run([binary], input=roh, stdout=subprocess.PIPE)
    zeilen = p.stdout.decode("ascii", "replace").splitlines()
    if len(zeilen) != len(plan):
        print(
            "FEHLER: %d Antwortzeilen fuer %d Auftraege — das Binary ist "
            "abgebrochen (Exit %d)" % (len(zeilen), len(plan), p.returncode)
        )
        # Alles, was fehlt, gilt als Fehlschlag: mit leeren Zeilen auffuellen.
        zeilen += ["[]"] * (len(plan) - len(zeilen))

    ok_je_fall = [True] * len(faelle)
    grund = [None] * len(faelle)
    for (k, st), zeile in zip(plan, zeilen):
        try:
            got = json.loads(zeile)
        except ValueError:
            got = ["<unlesbar>"]
        if got == ["NICHT-UNTERSTUETZT"]:
            if ok_je_fall[k]:
                grund[k] = "zustand nicht umgesetzt"
            ok_je_fall[k] = False
            continue
        if normalisiere(got) != faelle[k][4]:
            if ok_je_fall[k]:
                grund[k] = "ausgabe weicht ab (%s)" % st
            ok_je_fall[k] = False

    je_datei = {}
    for k, f in enumerate(faelle):
        d = je_datei.setdefault(f[0], [0, 0])
        d[1] += 1
        if ok_je_fall[k]:
            d[0] += 1

    gesamt = len(faelle)
    bestanden = sum(1 for x in ok_je_fall if x)
    print("Datei                          bestanden /  gesamt    Quote")
    print("-" * 62)
    for name in sorted(je_datei):
        p_, g_ = je_datei[name]
        print("%-30s %6d / %6d   %6.2f %%" % (name, p_, g_, 100.0 * p_ / g_))
    print("-" * 62)
    print(
        "%-30s %6d / %6d   %6.2f %%"
        % ("GESAMT", bestanden, gesamt, 100.0 * bestanden / gesamt)
    )

    if zeige:
        print("\nErste %d Fehlschlaege:" % zeige)
        n = 0
        for k, f in enumerate(faelle):
            if ok_je_fall[k]:
                continue
            print("  %s #%d  %s  [%s]" % (f[0], f[1], f[2][:60], grund[k]))
            n += 1
            if n >= zeige:
                break

    if json_out:
        with open(json_out, "w", encoding="utf-8") as fh:
            json.dump(
                {
                    "suite": "html5lib-tokenizer",
                    "total": gesamt,
                    "passed": bestanden,
                    "failed": gesamt - bestanden,
                    "rate": bestanden / gesamt,
                    "files": {
                        k: {"passed": v[0], "total": v[1]} for k, v in je_datei.items()
                    },
                },
                fh,
                indent=1,
            )
    return 0


if __name__ == "__main__":
    sys.exit(main())
