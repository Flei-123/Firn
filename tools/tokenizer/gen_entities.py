#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
# tools/tokenizer/gen_entities.py -- produces lib/html/entities_data.fi.
#
# SOURCE: `html.entities.html5` from the Python standard library. That is the
# official WHATWG list of names (2,231 entries, with and without a semicolon).
# The table is NOT derived from the test data -- the generator lies in the
# tree and can be repeated at any time:
#
#     python3 tools/tokenizer/gen_entities.py
#
# Why generated Firn source text at all: stage 0 has neither
# string literals nor global fields (`const` only scalar). The table
# is therefore written as a sequence of u64 words into a memory area
# (lib/html/entities.fi holds it over mmap MAP_FIXED_NOREPLACE, once per
# process). For the layout see below and lib/html/entities.fi.
#
# Memory picture (byte offsets from the base, everything 8-byte aligned):
#   0            u64  magic -- set LAST
#   OFF_NAMEN    u8[] all names one after another, without a separator, sorted
#   OFF_LEN      u8[] length per entry (1..32)
#   OFF_WERT     u64[] replacement characters: cp1 | cp2 << 32   (cp2 == 0: only one)
#   OFF_POS      u32[] start of each name in the name field -- computed at run time
import html.entities
import os
import sys

WORTE_JE_FUNKTION = 400


def main() -> int:
    root = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
    target = os.path.normpath(os.path.join(root, "lib", "html", "entities_data.fi"))

    tab = html.entities.html5
    names = sorted(tab)          # code point order == the order of the binary search
    count = len(names)

    blob = bytearray()
    lens = bytearray()
    values = []
    for name in names:
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
            print("a value with more than two characters: " + name, file=sys.stderr)
            return 1
        values.append(cps[0] | (cps[1] << 32))

    def auf8(n: int) -> int:
        return (n + 7) // 8 * 8

    off_namen = 16
    off_len = off_namen + auf8(len(blob))
    off_wert = off_len + auf8(len(lens))
    off_pos = off_wert + 8 * count
    bytes_total = off_pos + 4 * count

    # Build the raw picture (only the generated part; OFF_POS comes into being at run time).
    bild = bytearray(off_pos)
    bild[off_namen:off_namen + len(blob)] = blob
    bild[off_len:off_len + len(lens)] = lens
    for i, v in enumerate(values):
        bild[off_wert + 8 * i:off_wert + 8 * i + 8] = v.to_bytes(8, "little")

    # Words from index 2 on (the first 16 bytes are magic + reserve).
    words = []
    for i in range(2, len(bild) // 8):
        w = int.from_bytes(bild[8 * i:8 * i + 8], "little")
        if w != 0:
            words.append((i, w))

    z = []
    a = z.append
    a("// lib/html/entities_data.fi — ERZEUGT von tools/tokenizer/gen_entities.py.")
    a("// NICHT VON HAND AENDERN. Quelle: html.entities.html5 (WHATWG-Namensliste,")
    a("// %d Eintraege). Neu erzeugen:  python3 tools/tokenizer/gen_entities.py" % count)
    a("//")
    a("// Die Tabelle ist eine Folge von u64-Woertern; lib/html/entities.fi legt")
    a("// den Speicherbereich an und ruft `lade` genau einmal je Prozess.")
    a("")
    a("export { ANZAHL, OFF_NAMEN, OFF_LEN, OFF_WERT, OFF_POS, BYTES, KENNUNG, lade }")
    a("")
    a("const ANZAHL: usize = %d" % count)
    a("const OFF_NAMEN: usize = %d" % off_namen)
    a("const OFF_LEN: usize = %d" % off_len)
    a("const OFF_WERT: usize = %d" % off_wert)
    a("const OFF_POS: usize = %d" % off_pos)
    a("const BYTES: usize = %d" % bytes_total)
    a("const KENNUNG: u64 = 0x464952_4E454E54")
    a("")
    a("fn w(p: *mut u8, i: usize, v: u64) {")
    a("    *((p as usize + i * 8) as *mut u64) = v")
    a("}")
    a("")

    parts = []
    for start in range(0, len(words), WORTE_JE_FUNKTION):
        nr = len(parts)
        parts.append(nr)
        a("fn teil%d(p: *mut u8) {" % nr)
        for i, v in words[start:start + WORTE_JE_FUNKTION]:
            a("    w(p, %d, 0x%016X)" % (i, v))
        a("}")
        a("")

    a("// Schreibt die gesamte Tabelle nach `p` (ohne Kennung und ohne OFF_POS).")
    a("fn lade(p: *mut u8) {")
    for nr in parts:
        a("    teil%d(p)" % nr)
    a("}")
    a("")

    with open(target, "w") as f:
        f.write("\n".join(z))
    print("%s: %d entries, %d bytes of table, %d words, %d part functions"
          % (target, count, bytes_total, len(words), len(parts)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
