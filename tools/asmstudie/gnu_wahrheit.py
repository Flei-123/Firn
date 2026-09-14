#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""
Die WAHRHEIT von GNU as holen.

Fuer jeden Testfall wird der Befehl EINZELN assembliert und die erzeugten
Oktette aus der Abschnittsausgabe von objdump gelesen. Einzeln, weil sonst
nicht zu trennen waere, welche Oktette zu welchem Befehl gehoeren -- und weil
ein einziger Syntaxfehler sonst die ganze Datei killt.

Damit das nicht Stunden dauert, kommen alle Faelle in EINE Datei, jeder Befehl
zwischen zwei Marken. objdump gibt dann pro Befehl die Oktette aus, und ueber
die Adressdifferenz weiss ich, wo einer aufhoert.

Sprungziele sind der Sonderfall: `jmp markeA` bekommt eine Relokation, die
Oktette enthalten dann 0. Das ist RICHTIG so -- ich vergleiche nur das, was
ohne Linker feststeht, und merke mir die Relokationsstellen getrennt.
"""
import json, os, re, subprocess, sys, tempfile

HIER = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HIER)
import formen_liste as FL

AS_X86 = ["as", "--64"]
AS_A64 = ["aarch64-linux-gnu-as"]
OD_X86 = ["objdump"]
OD_A64 = ["aarch64-linux-gnu-objdump"]


def assemblieren(faelle, arch):
    """Gibt {index: (bytes_hex, fehler)} zurueck."""
    ergebnis = {}
    tmp = tempfile.mkdtemp(prefix="gnuwahr_")
    src = os.path.join(tmp, "a.s")
    obj = os.path.join(tmp, "a.o")

    # Erst versuchen: alle auf einmal. Faelle, die as ablehnt, fallen danach
    # einzeln durch.
    def bau(indizes):
        zeilen = []
        if arch == "x86":
            zeilen.append(".intel_syntax noprefix")
        zeilen.append(".text")
        for i in indizes:
            zeilen.append("MARKE_%d:" % i)
            zeilen.append("    " + faelle[i][1])
        zeilen.append("MARKE_ENDE:")
        # Sprungziel, damit `markeA` existiert. Weit genug weg fuer rel32,
        # aber das interessiert nur x86; bei a64 ebenso.
        zeilen.append("markeA:")
        zeilen.append("    nop" if arch == "x86" else "    nop")
        with open(src, "w") as f:
            f.write("\n".join(zeilen) + "\n")
        cmd = (AS_X86 if arch == "x86" else AS_A64) + [src, "-o", obj]
        p = subprocess.run(cmd, capture_output=True, text=True)
        return p

    alle = list(range(len(faelle)))
    p = bau(alle)
    schlecht = set()
    if p.returncode != 0:
        # Welche Zeilen mag as nicht?
        for m in re.finditer(r"a\.s:(\d+):", p.stderr):
            ln = int(m.group(1))
            # Zeilennummer -> Index zurueckrechnen ist fehleranfaellig; ich
            # parse stattdessen die Marke aus dem Kontext neu, indem ich die
            # Datei lese.
            schlecht.add(ln)
        with open(src) as f:
            zeilen = f.read().split("\n")
        bad_idx = set()
        for ln in schlecht:
            # die Marke ueber der fehlerhaften Zeile finden
            for k in range(ln - 1, -1, -1):
                mm = re.match(r"MARKE_(\d+):", zeilen[k])
                if mm:
                    bad_idx.add(int(mm.group(1)))
                    break
        for i in bad_idx:
            ergebnis[i] = (None, "as lehnt ab")
        rest = [i for i in alle if i not in bad_idx]
        p = bau(rest)
        if p.returncode != 0:
            return ergebnis, "as scheitert weiterhin: " + p.stderr[:2000]
        alle = rest

    # Symboltabelle: Adresse jeder MARKE_n
    od = (OD_X86 if arch == "x86" else OD_A64)
    p = subprocess.run(od + ["-t", obj], capture_output=True, text=True)
    adr = {}
    for line in p.stdout.split("\n"):
        m = re.match(r"^([0-9a-f]+)\s+.*\s(MARKE_(?:\d+|ENDE))$", line.strip())
        if m:
            adr[m.group(2)] = int(m.group(1), 16)

    # Die Oktette des .text-Abschnitts am Stueck
    p = subprocess.run(od + ["-s", "-j", ".text", obj], capture_output=True, text=True)
    roh = bytearray()
    basis = None
    for line in p.stdout.split("\n"):
        m = re.match(r"^\s*([0-9a-f]+)\s((?:[0-9a-f]{2,8}\s){1,4})", line)
        if m:
            a = int(m.group(1), 16)
            if basis is None:
                basis = a
            hexteil = m.group(2).replace(" ", "")
            roh.extend(bytes.fromhex(hexteil))
    if basis is None:
        basis = 0

    # Relokationen merken: dort steht ein Platzhalter, kein echter Wert
    p = subprocess.run(od + ["-r", obj], capture_output=True, text=True)
    relok = set()
    for line in p.stdout.split("\n"):
        m = re.match(r"^([0-9a-f]+)\s+(\S+)", line.strip())
        if m and not m.group(2).startswith("OFFSET"):
            relok.add(int(m.group(1), 16))

    geordnet = sorted(alle, key=lambda i: adr.get("MARKE_%d" % i, 1 << 60))
    for n, i in enumerate(geordnet):
        k = "MARKE_%d" % i
        if k not in adr:
            ergebnis[i] = (None, "keine Marke")
            continue
        start = adr[k]
        if n + 1 < len(geordnet):
            ende = adr.get("MARKE_%d" % geordnet[n + 1], start)
        else:
            ende = adr.get("MARKE_ENDE", start)
        b = bytes(roh[start - basis:ende - basis])
        hat_relok = any(start <= r < ende for r in relok)
        ergebnis[i] = (b.hex(), "relok" if hat_relok else None)
    return ergebnis, None


def main():
    d = FL.laden()
    arch = sys.argv[1] if len(sys.argv) > 1 else "x86"
    if arch == "x86":
        faelle = FL.x86_faelle([f for f, _ in d["x86"]["formen"]])
    else:
        faelle = FL.a64_faelle([f for f, _ in d["a64"]["formen"]])
    erg, fehler = assemblieren(faelle, arch)
    if fehler:
        print("FEHLER:", fehler)
    aus = []
    for i, (form, text) in enumerate(faelle):
        hexs, anm = erg.get(i, (None, "fehlt"))
        aus.append({"form": form, "text": text, "hex": hexs, "anm": anm})
    ziel = os.path.join(HIER, "wahrheit_%s.json" % arch)
    with open(ziel, "w") as f:
        json.dump(aus, f)
    ok = sum(1 for a in aus if a["hex"])
    abg = sum(1 for a in aus if a["anm"] == "as lehnt ab")
    rel = sum(1 for a in aus if a["anm"] == "relok")
    print("%s: %d Faelle, %d mit Oktetten, %d von as abgelehnt, %d mit Relokation"
          % (arch, len(aus), ok, abg, rel))
    print("-> %s" % ziel)
    formen_ok = len(set(a["form"] for a in aus if a["hex"]))
    print("Formen mit mindestens einem gueltigen Fall: %d" % formen_ok)


if __name__ == "__main__":
    main()
