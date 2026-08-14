#!/usr/bin/env python3
# tools/tokenizer/gen_entities.py — erzeugt lib/html/entities_data.fi.
#
# QUELLE: `html.entities.html5` aus der Python-Standardbibliothek. Das ist die
# offizielle WHATWG-Namensliste (2.231 Eintraege, mit und ohne Semikolon).
# Die Tabelle wird NICHT aus den Testdaten abgeleitet — der Erzeuger liegt im
# Baum und ist jederzeit wiederholbar:
#
#     python3 tools/tokenizer/gen_entities.py
#
# Warum ueberhaupt erzeugter Firn-Quelltext: Stufe 0 kennt weder
# Zeichenkettenliterale noch globale Felder (`const` nur skalar). Die Tabelle
# wird deshalb als Folge von u64-Woertern in einen Speicherbereich geschrieben
# (lib/html/entities.fi haelt ihn ueber mmap MAP_FIXED_NOREPLACE, einmal je
# Prozess). Layout siehe unten und lib/html/entities.fi.
#
# Speicherbild (Byte-Offsets ab Basis, alles 8-Byte-ausgerichtet):
#   0            u64  Kennung (Magic) — wird ZULETZT gesetzt
#   OFF_NAMEN    u8[] alle Namen hintereinander, ohne Trenner, sortiert
#   OFF_LEN      u8[] Laenge je Eintrag (1..32)
#   OFF_WERT     u64[] Ersatzzeichen: cp1 | cp2 << 32   (cp2 == 0: nur eines)
#   OFF_POS      u32[] Anfang je Name im Namensfeld — zur Laufzeit berechnet
import html.entities
import os
import sys

WORTE_JE_FUNKTION = 400


def main() -> int:
    wurzel = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
    ziel = os.path.normpath(os.path.join(wurzel, "lib", "html", "entities_data.fi"))

    tab = html.entities.html5
    namen = sorted(tab)          # Codepunkt-Ordnung == Ordnung der Binaersuche
    anzahl = len(namen)

    blob = bytearray()
    lens = bytearray()
    werte = []
    for name in namen:
        roh = name.encode("ascii")
        if len(roh) > 32:
            print("name zu lang: " + name, file=sys.stderr)
            return 1
        blob += roh
        lens.append(len(roh))
        cps = [ord(c) for c in tab[name]]
        if len(cps) == 1:
            cps.append(0)
        if len(cps) != 2:
            print("wert mit mehr als zwei zeichen: " + name, file=sys.stderr)
            return 1
        werte.append(cps[0] | (cps[1] << 32))

    def auf8(n: int) -> int:
        return (n + 7) // 8 * 8

    off_namen = 16
    off_len = off_namen + auf8(len(blob))
    off_wert = off_len + auf8(len(lens))
    off_pos = off_wert + 8 * anzahl
    bytes_gesamt = off_pos + 4 * anzahl

    # Rohbild bauen (nur der erzeugte Teil; OFF_POS entsteht zur Laufzeit).
    bild = bytearray(off_pos)
    bild[off_namen:off_namen + len(blob)] = blob
    bild[off_len:off_len + len(lens)] = lens
    for i, v in enumerate(werte):
        bild[off_wert + 8 * i:off_wert + 8 * i + 8] = v.to_bytes(8, "little")

    # Woerter ab Index 2 (die ersten 16 Byte sind Kennung + Reserve).
    worte = []
    for i in range(2, len(bild) // 8):
        w = int.from_bytes(bild[8 * i:8 * i + 8], "little")
        if w != 0:
            worte.append((i, w))

    z = []
    a = z.append
    a("// lib/html/entities_data.fi — ERZEUGT von tools/tokenizer/gen_entities.py.")
    a("// NICHT VON HAND AENDERN. Quelle: html.entities.html5 (WHATWG-Namensliste,")
    a("// %d Eintraege). Neu erzeugen:  python3 tools/tokenizer/gen_entities.py" % anzahl)
    a("//")
    a("// Die Tabelle ist eine Folge von u64-Woertern; lib/html/entities.fi legt")
    a("// den Speicherbereich an und ruft `lade` genau einmal je Prozess.")
    a("")
    a("export { ANZAHL, OFF_NAMEN, OFF_LEN, OFF_WERT, OFF_POS, BYTES, KENNUNG, lade }")
    a("")
    a("const ANZAHL: usize = %d" % anzahl)
    a("const OFF_NAMEN: usize = %d" % off_namen)
    a("const OFF_LEN: usize = %d" % off_len)
    a("const OFF_WERT: usize = %d" % off_wert)
    a("const OFF_POS: usize = %d" % off_pos)
    a("const BYTES: usize = %d" % bytes_gesamt)
    a("const KENNUNG: u64 = 0x464952_4E454E54")
    a("")
    a("fn w(p: *mut u8, i: usize, v: u64) {")
    a("    *((p as usize + i * 8) as *mut u64) = v")
    a("}")
    a("")

    teile = []
    for anfang in range(0, len(worte), WORTE_JE_FUNKTION):
        nr = len(teile)
        teile.append(nr)
        a("fn teil%d(p: *mut u8) {" % nr)
        for i, v in worte[anfang:anfang + WORTE_JE_FUNKTION]:
            a("    w(p, %d, 0x%016X)" % (i, v))
        a("}")
        a("")

    a("// Schreibt die gesamte Tabelle nach `p` (ohne Kennung und ohne OFF_POS).")
    a("fn lade(p: *mut u8) {")
    for nr in teile:
        a("    teil%d(p)" % nr)
    a("}")
    a("")

    with open(ziel, "w") as f:
        f.write("\n".join(z))
    print("%s: %d eintraege, %d byte tabelle, %d woerter, %d teilfunktionen"
          % (ziel, anzahl, bytes_gesamt, len(worte), len(teile)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
