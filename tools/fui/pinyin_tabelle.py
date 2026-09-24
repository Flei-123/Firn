#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/fui/pinyin_tabelle.py -- ERZEUGT lib/fui/pinyin.fi AUS DER UNIHAN-DATENBANK.
#
# WARUM ES DIESES SKRIPT GIBT. Chinesisch laesst sich nicht rechnen wie
# Hangul und nicht ueber eine Handvoll Regeln abbilden wie Romaji: zu
# jeder Silbe gehoeren Dutzende Schriftzeichen, und welches davon ein
# Mensch meint, weiss nur ein Woerterbuch. Die EINGEBAUTE Eingabemethode
# von fUi traegt darum eine kleine Silbentabelle mit -- Silbe ohne Ton
# nach Schriftzeichen, sortiert nach Haeufigkeit --, und diese Tabelle
# wird hier aus einer nachpruefbaren Quelle erzeugt und NICHT von Hand
# geschrieben. Von Hand geschrieben waere sie eine Meinung.
#
# DIE QUELLE. Unihan (Unicode 15.0), Feld kHanyuPinlu: Lesungen mit
# Haeufigkeiten aus dem Xiandai Hanyu Pinlu Cidian (1990), 3800
# Schriftzeichen. Genau diese Zeichen kommen in die Tabelle, jedes unter
# jeder seiner Lesungen mit der Haeufigkeit DIESER Lesung. Bei gleicher
# Zahl steht die vereinfachte Form vor der traditionellen (erkannt am
# Feld kSimplifiedVariant in Unihan_Variants) -- die Quelle zaehlt beide
# Formen gemeinsam, und die vereinfachte ist die der Volksrepublik.
#
# WAS DIE TABELLE NICHT KANN, und das steht auch im Kopf von ime.fi:
# sie kennt nur EINZELNE Zeichen. Woerter und Saetze ("zhongguo" ->
# 中国 in einem Schritt) braucht ein Woerterbuch mit Wortfolgen; das
# liefert die Plattform-Eingabemethode, nicht diese Tabelle.
#
# Aufruf (auf einem Debian mit dem Paket unicode-data):
#   python3 tools/fui/pinyin_tabelle.py > lib/fui/pinyin.fi
import bz2, re, sys, unicodedata, collections

QUELLE = "/usr/share/unicode/"

def lies(name):
    with bz2.open(QUELLE + name, "rt", encoding="utf-8") as f:
        for z in f:
            if not z.startswith("#") and z.strip():
                yield z.rstrip("\n").split("\t")

# Welche Zeichen sind die TRADITIONELLE Form eines anderen? Die haben
# ein kSimplifiedVariant, das auf ein anderes Zeichen zeigt.
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

# Jede Zeile der Tabelle ist EIN Anfangsbuchstabe; ein Eintrag ist
# "silbe:zeichen" und endet mit einem Leerzeichen. Alle Zeichen liegen
# in der Grundebene und sind in UTF-8 drei Bytes lang -- ime.fi verlaesst
# sich darauf, und tools/fui/ime_main.fi prueft es nach.
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
// lib/fui/pinyin.fi -- DIE SILBENTABELLE DER EINGEBAUTEN PINYIN-EINGABE.
//
// DIESE DATEI IST ERZEUGT. Sie entsteht mit tools/fui/pinyin_tabelle.py
// aus Unihan (Unicode 15.0, Feld kHanyuPinlu), und wer sie aendern will,
// aendert das Skript und nicht diese Datei -- sonst stimmt beim naechsten
// Erzeugen etwas anderes, als hier stand. Die Daten stehen unter der
// Unicode-Lizenz, siehe THIRD_PARTY.md.
//
// WAS SIE ENTHAELT: %d Silben ohne Ton, zusammen %d Schriftzeichen,
// jede Silbe mit ihren Zeichen nach Haeufigkeit (das haeufigste zuerst).
// Das `ue` nach l und n heisst hier wie in jeder Pinyin-Tastatur `v`
// ("lv" -> 绿, "nv" -> 女), weil eine Tastatur kein ue hat.
//
// WARUM EINE ZEILE JE ANFANGSBUCHSTABE. Ein Zeichenketten-Literal steht
// in Firn auf einer Zeile, und eine einzige Zeile mit zwoelf Kilobyte
// liest niemand mehr. So bleibt jede Zeile ein Buchstabe, und die Suche
// in ime.fi geht nur ueber den einen Block, der in Frage kommt.
//
// WARUM `static`: ein Literal in einer Funktion entsteht bei jedem
// Aufruf Byte fuer Byte im Rahmen (SPEC 14.1.str, S8). Als `static`
// liegt die Tabelle einmal in .rodata und kostet beim Nachschlagen
// nichts.

export {
    py_block, PY_SILBEN, PY_ZEICHEN,
}

// Die beiden Zahlen, die tools/fui/ime_main.fi gegen die Tabelle haelt.
const PY_SILBEN: usize = %d
const PY_ZEICHEN: usize = %d

""" % (n_silben, n_zeichen, n_silben, n_zeichen))

buchstaben = sorted(zeilen)
for b in buchstaben:
    text = "".join(zeilen[b])
    laenge = len(text.encode("utf-8"))
    aus.write("static PY_%s: [u8; %d] = \"%s\"\n" % (b.upper(), laenge, text))

aus.write("""
// DER BLOCK EINES ANFANGSBUCHSTABENS. Zurueck kommt der Zeiger auf die
// Zeile und in `*n` ihre Laenge; 0 heisst: mit diesem Buchstaben beginnt
// keine Silbe (i, u und v gibt es am Silbenanfang nicht).
fn py_block(b: u8, n: *mut usize) -> u64 {
""")
for b in buchstaben:
    text = "".join(zeilen[b])
    laenge = len(text.encode("utf-8"))
    aus.write("    if b == %d as u8 {\n        *n = %d\n        return (&PY_%s[0]) as u64\n    }\n"
              % (ord(b), laenge, b.upper()))
aus.write("    *n = 0\n    return 0\n}\n")
