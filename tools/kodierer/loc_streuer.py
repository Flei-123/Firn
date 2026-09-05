#!/usr/bin/env python3
"""RUNDE KODIERER II -- Quellstellen in fremden Assemblertext streuen.

Wozu. Der Codeerzeuger schreibt `.loc` nur in einem schmalen Muster:
aufsteigende Zeilen aus EINER Datei, kleine Spaltenzahlen, nie ein
Ruecksprung ueber die Spanne des Sonderopcodes hinaus. Auf der ARM64-Seite
schreibt er ueberhaupt keine (`codegen_a64.rs`, Runde 80). Der Kodierer
muss aber auch die Faelle treffen, die dort NICHT vorkommen:

  * Zeilenspruenge, die nicht in einen Sonderopcode passen
    (`DW_LNS_advance_line` mit vorzeichenbehafteter LEB128),
  * Rueckspruenge im Zeilenzaehler,
  * Adressspruenge ueber `DW_LNS_const_add_pc` hinaus,
  * mehrere Quelldateien (`DW_LNS_set_file`),
  * Zeile 0 (`as` wirft sie weg),
  * mehrere `.loc` hintereinander auf derselben Adresse.

Also nehmen wir echten Assemblertext aus dem Baum und streuen `.loc`
hinein -- pseudozufaellig, aber mit festem Startwert, damit ein Fehler
wiederholbar ist. Beide Wege bekommen DENSELBEN Text; was danach noch
unterschiedlich ist, gehoert dem Kodierer.

Aufruf:
    loc_streuer.py <ein.s> <aus.s> [startwert]
"""
import random
import sys

# Befehlszeilen erkennt man daran, dass sie eingerueckt sind und nicht mit
# einem Punkt (Direktive) oder einem Doppelpunkt (Marke) anfangen.
def ist_befehl(zeile):
    if not zeile[:1].isspace():
        return False
    t = zeile.strip()
    if not t or t.startswith((".", "#", "//")):
        return False
    return not t.endswith(":")


def streue(text, seed=1):
    r = random.Random(seed)
    zeilen = text.splitlines()
    # Die schon vorhandenen `.file`-Nummern behalten -- die bestehenden
    # `.loc` zeigen darauf. Dahinter kommen zwei weitere Dateien, damit
    # `DW_LNS_set_file` wirklich vorkommt.
    hoechste = 0
    letzte_file = -1
    for i, l in enumerate(zeilen):
        t = l.strip()
        if t.startswith(".file "):
            teile = t.split()
            if len(teile) >= 3 and teile[1].isdigit():
                hoechste = max(hoechste, int(teile[1]))
                letzte_file = i
    zusatz = ['"./unter/b.fi"', '"/anderswo/c.fi"']
    kopf = [".file %d %s" % (hoechste + 1 + i, d) for i, d in enumerate(zusatz)]
    n_dateien = hoechste + len(zusatz)
    if hoechste == 0:
        # Text ganz ohne Quellangaben (ARM64): eine erste Datei anlegen.
        kopf = ['.file 1 "a.fi"'] + [".file %d %s" % (i + 2, d)
                                     for i, d in enumerate(zusatz)]
        n_dateien = 1 + len(zusatz)

    aus = []
    gestreut = False
    im_text = True
    zeile = 1
    for i, l in enumerate(zeilen):
        t = l.strip()
        aus.append(l)
        if not gestreut and (i == letzte_file or (letzte_file < 0 and t == ".text")):
            aus.extend(kopf)
            gestreut = True
            continue
        if not gestreut:
            continue
        # Nur in `.text` streuen -- `as` legt sonst eine zweite Folge an.
        if t.startswith(".section"):
            im_text = ".text" in t
        elif t == ".text":
            im_text = True
        elif t == ".data":
            im_text = False
        if not (im_text and ist_befehl(l) and r.random() < 0.35):
            continue
        # Die Quellstelle gehoert VOR den Befehl.
        befehl = aus.pop()
        for _ in range(r.choice([1, 1, 1, 2, 3])):
            sprung = r.choice([1, 1, 2, 5, -3, 40, -60, 300, -400, 0])
            zeile = max(1, zeile + sprung)
            z = 0 if r.random() < 0.05 else zeile
            aus.append("    .loc %d %d %d"
                       % (r.randint(1, n_dateien), z, r.randint(0, 90)))
        aus.append(befehl)
    return "\n".join(aus) + "\n"


def main():
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    seed = int(sys.argv[3]) if len(sys.argv) > 3 else 1
    with open(sys.argv[1], errors="replace") as f:
        text = f.read()
    with open(sys.argv[2], "w") as f:
        f.write(streue(text, seed))
    return 0


if __name__ == "__main__":
    sys.exit(main())
