#!/usr/bin/env python3
"""tools/ucd/verify_bidi.py -- die Gegenrechnung zur Bidi-Tabelle.

Ein ZWEITER Zerleger fuer dieselben fuenf Dateien der UCD, in einer
zweiten Sprache, ohne eine Zeile aus tools/ucd/pack_bidi.fi. Er rechnet
fuer jeden der 1.114.112 Codepunkte aus, was in der Tabelle stehen MUSS,
und vergleicht es mit dem, was tools/ucd/probe_bidi.fi ueber
lib/str/ucd_bidi.fi aus der Tabelle HERAUSLIEST.

Stimmt eine einzige Zeile nicht, endet es mit 1 und nennt sie.
tools/ucd/build_bidi.sh faelscht zur Gegenprobe absichtlich eine Zeile
und verlangt, dass dieses Skript dann anschlaegt -- eine Pruefung, die
nicht scheitern kann, beweist nichts.

Aufruf:  python3 tools/ucd/verify_bidi.py <ausgabe von probe_bidi> <ucd-ordner>
"""
import os
import sys

CLASSES = "L R AL EN ES ET AN CS NSM BN B S WS ON LRE LRO RLE RLO PDF LRI RLI FSI PDI".split()
LONG = {
    "Left_To_Right": "L", "Right_To_Left": "R", "Arabic_Letter": "AL",
    "European_Number": "EN", "European_Separator": "ES",
    "European_Terminator": "ET", "Arabic_Number": "AN",
    "Common_Separator": "CS", "Nonspacing_Mark": "NSM",
    "Boundary_Neutral": "BN", "Paragraph_Separator": "B",
    "Segment_Separator": "S", "White_Space": "WS", "Other_Neutral": "ON",
    "Left_To_Right_Embedding": "LRE", "Left_To_Right_Override": "LRO",
    "Right_To_Left_Embedding": "RLE", "Right_To_Left_Override": "RLO",
    "Pop_Directional_Format": "PDF", "Left_To_Right_Isolate": "LRI",
    "Right_To_Left_Isolate": "RLI", "First_Strong_Isolate": "FSI",
    "Pop_Directional_Isolate": "PDI",
}
JOIN = "U R L D C T".split()
MAXCP = 0x110000


def rng(s):
    s = s.strip()
    if ".." in s:
        a, b = s.split("..")
        return int(a, 16), int(b, 16)
    return int(s, 16), int(s, 16)


def expected(d):
    cls = [0] * MAXCP
    text = open(os.path.join(d, "DerivedBidiClass.txt"), encoding="utf-8").read()
    for line in text.split("\n"):
        if line.startswith("# @missing:"):
            r, name = line[len("# @missing:"):].split(";")
            a, b = rng(r)
            for x in range(a, b + 1):
                cls[x] = CLASSES.index(LONG[name.strip()])
    for line in text.split("\n"):
        body = line.split("#")[0].strip()
        if not body:
            continue
        r, c = body.split(";")
        a, b = rng(r)
        for x in range(a, b + 1):
            cls[x] = CLASSES.index(c.strip())

    tmark = set()
    canon = {}
    forms = {}
    ligs = {}
    for line in open(os.path.join(d, "UnicodeData.txt"), encoding="utf-8"):
        f = line.rstrip("\n").split(";")
        if len(f) < 6:
            continue
        cp = int(f[0], 16)
        if f[2] in ("Mn", "Me", "Cf"):
            tmark.add(cp)
        dec = f[5].split()
        if not dec:
            continue
        if dec[0].startswith("<"):
            tag = dec[0][1:-1]
            parts = [int(x, 16) for x in dec[1:]]
            if tag in ("isolated", "final", "initial", "medial") and cp < 0x10000:
                k = ("isolated", "final", "initial", "medial").index(tag)
                if len(parts) == 1 and parts[0] < 0x10000:
                    forms.setdefault(parts[0], [0, 0, 0, 0])[k] = cp
                elif len(parts) == 2 and k < 2 and max(parts) < 0x10000:
                    ligs.setdefault(tuple(parts), [0, 0])[k] = cp
        elif len(dec) == 1 and cp < 0x10000 and int(dec[0], 16) < 0x10000:
            canon[cp] = int(dec[0], 16)

    join = {}
    for line in open(os.path.join(d, "ArabicShaping.txt"), encoding="utf-8"):
        body = line.split("#")[0].strip()
        if not body:
            continue
        f = [x.strip() for x in body.split(";")]
        join[int(f[0], 16)] = JOIN.index(f[2])

    mirror = {}
    for line in open(os.path.join(d, "BidiMirroring.txt"), encoding="utf-8"):
        body = line.split("#")[0].strip()
        if not body:
            continue
        a, b = body.split(";")
        mirror[int(a, 16)] = int(b, 16)
    btype = {}
    for line in open(os.path.join(d, "BidiBrackets.txt"), encoding="utf-8"):
        body = line.split("#")[0].strip()
        if not body:
            continue
        a, b, t = [x.strip() for x in body.split(";")]
        a = int(a, 16)
        btype[a] = 1 if t == "o" else 2
        mirror.setdefault(a, int(b, 16))

    out = []
    lo = 0

    def octet(x):
        j = join.get(x)
        if j is None:
            j = 5 if x in tmark else 0
        return cls[x] | (j << 5)

    cur = octet(0)
    for x in range(1, MAXCP + 1):
        here = octet(x) if x < MAXCP else None
        if here != cur:
            out.append("O %d %d %d" % (lo, x - 1, cur))
            lo = x
            cur = here
    for x in sorted(set(mirror) | set(btype)):
        t = btype.get(x, 0)
        cn = canon.get(x, x) if t else x
        out.append("M %d %d %d %d" % (x, mirror.get(x, x), t, cn))
    for base in sorted(forms):
        f = forms[base]
        out.append("F %d %d %d %d %d" % (base, f[0], f[1], f[2], f[3]))
    for (a, b) in sorted(ligs):
        v = ligs[(a, b)]
        if v[0] and v[1] and 0x600 <= a < 0x700 and 0x600 <= b < 0x700:
            out.append("G %d %d %d %d" % (a, b, v[0], v[1]))
    # Die Reihenfolge der G-Zeilen ist die der Befragung: a aussen, b innen.
    g = [l for l in out if l.startswith("G ")]
    g.sort(key=lambda l: (int(l.split()[1]), int(l.split()[2])))
    out = [l for l in out if not l.startswith("G ")] + g
    return out


def main():
    got = open(sys.argv[1]).read().split("\n")
    got = [l for l in got if l]
    want = expected(sys.argv[2])
    counts = {}
    for l in want:
        counts[l[0]] = counts.get(l[0], 0) + 1
    if got == want:
        print("   gleich: %d Bereiche (O), %d Spiegel/Klammern (M), "
              "%d Formen (F), %d Ligaturen (G) -- alle 1114112 Codepunkte"
              % (counts.get("O", 0), counts.get("M", 0), counts.get("F", 0),
                 counts.get("G", 0)))
        return 0
    n = 0
    for i in range(max(len(got), len(want))):
        a = got[i] if i < len(got) else "<fehlt>"
        b = want[i] if i < len(want) else "<fehlt>"
        if a != b:
            print("   VERSCHIEDEN in Zeile %d: Tabelle '%s', UCD '%s'" % (i + 1, a, b))
            n += 1
            if n >= 5:
                break
    return 1


if __name__ == "__main__":
    sys.exit(main())
