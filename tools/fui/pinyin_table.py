#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/fui/pinyin_table.py -- GENERATES lib/fui/pinyin.fi FROM THE UNIHAN DATABASE.
#
# WHY THIS SCRIPT EXISTS. Chinese cannot be calculated like
# Hangul and cannot be mapped by a handful of rules like romaji: every
# syllable has dozens of characters, and which of them a
# person means only a dictionary knows. The BUILT-IN input method
# of fUi therefore carries a small syllable table -- syllable without tone
# to characters, sorted by frequency --, and this table
# is generated here from a verifiable source and NOT written by hand.
# Written by hand it would be an opinion.
#
# THE SOURCE. Unihan (Unicode 15.0), field kHanyuPinlu: readings with
# frequencies from the Xiandai Hanyu Pinlu Cidian (1990), 3800
# characters. Exactly these characters go into the table, each under
# each of its readings with the frequency of THAT reading. On equal
# count the simplified form stands before the traditional one (recognised by the
# field kSimplifiedVariant in Unihan_Variants) -- the source counts both
# forms together, and the simplified one is that of the People's Republic.
#
# WHAT THE TABLE CANNOT DO, and this also stands in the head of ime.fi:
# it knows only SINGLE characters. Words and sentences ("zhongguo" ->
# 中国 in one step) need a dictionary with word sequences; that is
# supplied by the platform input method, not by this table.
#
# Call (on a Debian with the package unicode-data):
#   python3 tools/fui/pinyin_table.py > lib/fui/pinyin.fi
import bz2, re, sys, unicodedata, collections

QUELLE = "/usr/share/unicode/"

def lies(name):
    with bz2.open(QUELLE + name, "rt", encoding="utf-8") as f:
        for z in f:
            if not z.startswith("#") and z.strip():
                yield z.rstrip("\n").split("\t")

# Which characters are the TRADITIONAL form of another? They have
# a kSimplifiedVariant that points to another character.
traditionell = set()
for cp, feld, wert in lies("Unihan_Variants.txt.bz2"):
    if feld == "kSimplifiedVariant":
        eigen = cp
        if any(v.split("<")[0] != eigen for v in wert.split()):
            traditionell.add(chr(int(cp[2:], 16)))

silben = collections.defaultdict(dict)
for cp, feld, wert in lies("Unihan_Readings.txt.bz2"):
    if feld != "kHanyuPinlu":
        continue
    z = chr(int(cp[2:], 16))
    for m in re.finditer(r"(\S+?)\((\d+)\)", wert):
        lesung, zahl = m.group(1), int(m.group(2))
        d = unicodedata.normalize("NFD", lesung).replace("ü", "v")
        d = "".join(c for c in d if not unicodedata.combining(c))
        if not re.fullmatch("[a-z]+", d):
            sys.exit("unerwartete Lesung %r bei %s" % (lesung, cp))
        silben[d][z] = max(silben[d].get(z, 0), zahl)

# Each row of the table is ONE initial letter; an entry is
# "syllable:character" and ends with a space. All characters lie
# in the basic plane and are three bytes long in UTF-8 -- ime.fi relies
# on that, and tools/fui/ime_main.fi checks it.
zeilen = collections.defaultdict(list)
for s in sorted(silben):
    zs = sorted(silben[s], key=lambda c: (-silben[s][c], c in traditionell, c))
    for c in zs:
        if len(c.encode("utf-8")) != 3:
            sys.exit("Zeichen %r ist nicht drei Bytes lang" % c)
    zeilen[s[0]].append(s + ":" + "".join(zs) + " ")

n_silben = len(silben)
n_zeichen = sum(len(v) for v in silben.values())

aus = sys.stdout
aus.write("""// SPDX-License-Identifier: MPL-2.0
// lib/fui/pinyin.fi -- THE SYLLABLE TABLE OF THE BUILT-IN PINYIN INPUT.
//
// THIS FILE IS GENERATED. It is produced with tools/fui/pinyin_table.py
// from Unihan (Unicode 15.0, field kHanyuPinlu), and whoever wants to change it
// changes the script and not this file -- otherwise on the next
// generation something different stands there than was here. The data are under the
// Unicode licence, see THIRD_PARTY.md.
//
// WHAT IT CONTAINS: %d syllables without tone, together %d characters,
// each syllable with its characters by frequency (the most frequent first).
// The `ue` after l and n is called `v` here as on every pinyin keyboard
// ("lv" -> 绿, "nv" -> 女), because a keyboard has no ue.
//
// WHY ONE ROW PER INITIAL LETTER. A string literal stands on one line
// in Firn, and a single line of twelve kilobytes
// nobody reads any more. This way every row stays one letter, and the search
// in ime.fi goes only over the one block that comes into question.
//
// WHY `static`: a literal in a function arises on every
// call byte by byte in the frame (SPEC 14.1.str, S8). As `static`
// the table lies once in .rodata and costs nothing on lookup.

export {
    py_block, PY_SILBEN, PY_ZEICHEN,
}

// The two numbers that tools/fui/ime_main.fi holds against the table.
const PY_SILBEN: usize = %d
const PY_ZEICHEN: usize = %d

""" % (n_silben, n_zeichen, n_silben, n_zeichen))

buchstaben = sorted(zeilen)
for b in buchstaben:
    text = "".join(zeilen[b])
    laenge = len(text.encode("utf-8"))
    aus.write("static PY_%s: [u8; %d] = \"%s\"\n" % (b.upper(), laenge, text))

aus.write("""
// THE BLOCK OF AN INITIAL LETTER. The pointer to the row comes back
// and in `*n` its length; 0 means: no syllable begins with this
// letter (i, u and v do not exist at the start of a syllable).
fn py_block(b: u8, n: *mut usize) -> u64 {
""")
for b in buchstaben:
    text = "".join(zeilen[b])
    laenge = len(text.encode("utf-8"))
    aus.write("    if b == %d as u8 {\n        *n = %d\n        return (&PY_%s[0]) as u64\n    }\n"
              % (ord(b), laenge, b.upper()))
aus.write("    *n = 0\n    return 0\n}\n")
