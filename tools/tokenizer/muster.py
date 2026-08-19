#!/usr/bin/env python3
"""Dynamisch gewichtete INSTRUKTIONSMUSTER eines Firn-Binaries (Runde 51).

WARUM: `tools/tokenizer/profil.py` beantwortet „welche FUNKTION kostet?".
Diese Datei beantwortet die Frage daneben — „welche FORM von Code kostet?".
Beides zusammen zeigt erst, wo der Uebersetzer schlecht arbeitet.

Die Lehre aus Runde 43 (§5) war: **statische Haeufigkeit ist keine Schaetzung
des Gewinns** — dort war ein statischer Zaehler um den Faktor acht zu
optimistisch. Deshalb verbindet dieses Werkzeug die Disassemblierung mit der
INSTRUKTIONSGENAUEN callgrind-Ausgabe und gewichtet jedes Muster mit seiner
echten Ausfuehrungszahl.

Erzeugen der Eingaben:

    objdump -d --no-show-raw-insn BINARY > dis.txt
    valgrind --tool=callgrind --dump-instr=yes --cache-sim=no --branch-sim=no \
             --callgrind-out-file=cg.out BINARY < eingabe

Aufruf:

    python3 tools/tokenizer/muster.py dis.txt cg.out

FALLEN beim Lesen der callgrind-Datei (beide haben hier schon zugeschlagen):
  * Mit `--dump-instr=yes` beginnt eine Kostenzeile mit der ADRESSE, die auch
    relativ sein kann (`+12`, `-4`, `*`). Wer das nicht aufloest, bekommt ein
    plausibel aussehendes, falsches Profil.
  * Die Zeile unmittelbar nach `calls=` sind die INKLUSIVEN Kosten des
    Aufrufs und gehoeren nicht zu den Selbstkosten der aufrufenden Funktion.

Gefunden hat dieses Werkzeug die drei groessten Posten der Runde 51:
`setcc`-Ketten (17,13 %), Store+Reload derselben Zelle (10,27 %) und
unbedingte Spruenge hinter bedingten (3,05 %).
"""
import re
import sys
from collections import defaultdict


def lies_dis(pfad):
    """addr -> (Mnemonic, Operanden, Symbol)"""
    code = {}
    reihe = []
    sym = "?"
    for z in open(pfad, errors="replace"):
        m = re.match(r"^([0-9a-f]+) <(.+)>:", z)
        if m:
            sym = m.group(2)
            continue
        m = re.match(r"^\s+([0-9a-f]+):\t(\S+)\s*(.*)$", z.rstrip("\n"))
        if not m:
            continue
        a = int(m.group(1), 16)
        code[a] = (m.group(2), m.group(3).strip(), sym)
        reihe.append(a)
    reihe.sort()
    return code, reihe


def lies_kosten(pfad):
    """addr -> Ir (Selbstkosten)"""
    kosten = defaultdict(int)
    letzte = 0
    inaufruf = False
    for z in open(pfad, errors="replace"):
        if z.startswith(("calls=", "jump=", "jcnd=")):
            inaufruf = z.startswith("calls=")
            continue
        m = re.match(r"^(0x[0-9a-f]+|\+\d+|-\d+|\*)\s+(\S+)\s+(\d+)", z)
        if not m:
            if z.startswith(("fn=", "fl=", "cfn=", "cfl=", "cob=", "ob=")):
                inaufruf = False
            continue
        p = m.group(1)
        if p.startswith("0x"):
            a = int(p, 16)
        elif p == "*":
            a = letzte
        else:
            a = letzte + int(p)
        letzte = a
        if inaufruf:
            inaufruf = False   # inklusive Kosten des Aufrufs, nicht Selbstkosten
            continue
        kosten[a] += int(m.group(3))
    return kosten


def ist_bedingt(mn):
    return mn.startswith("j") and mn != "jmp"


def stamm(r):
    r = r.lstrip("%")
    fest = {
        "al": "rax", "ax": "rax", "eax": "rax", "bl": "rbx", "bx": "rbx", "ebx": "rbx",
        "cl": "rcx", "cx": "rcx", "ecx": "rcx", "dl": "rdx", "dx": "rdx", "edx": "rdx",
        "sil": "rsi", "si": "rsi", "esi": "rsi", "dil": "rdi", "di": "rdi", "edi": "rdi",
    }
    if r in fest:
        return fest[r]
    if re.match(r"^r\d+[dwb]$", r):
        return r[:-1]
    return r


def main():
    code, reihe = lies_dis(sys.argv[1])
    kosten = lies_kosten(sys.argv[2])
    ges = sum(kosten.values())
    if ges == 0:
        print("keine Kosten gefunden — wurde --dump-instr=yes gesetzt?")
        return 1
    print(f"Gesamt (Selbstkosten aus der Instruktionsdatei): {ges:,}")

    muster = defaultdict(lambda: [0, 0])   # Name -> [Ir, Stellen]

    def zaehle(name, ir):
        m = muster[name]
        m[0] += ir
        m[1] += 1

    for i, a in enumerate(reihe):
        mn, ops, sym = code[a]
        nxt = reihe[i + 1] if i + 1 < len(reihe) else None

        # (1) unbedingter Sprung direkt hinter bedingtem -> Blocklayout
        if ist_bedingt(mn) and nxt is not None and code[nxt][0] == "jmp" and code[nxt][2] == sym:
            zaehle("jmp direkt hinter jcc (Blocklayout)", kosten.get(nxt, 0))

        # (2) setcc-Kette statt direktem Sprung
        if mn.startswith("set"):
            kette = [a]
            j = i + 1
            treffer = False
            while j < len(reihe) and j <= i + 5:
                b = reihe[j]
                mb = code[b][0]
                kette.append(b)
                if mb == "test":
                    if j + 1 < len(reihe) and ist_bedingt(code[reihe[j + 1]][0]):
                        treffer = True
                    break
                if mb not in ("movzbl", "movzwl", "mov"):
                    break
                j += 1
            if treffer:
                zaehle("setcc-Kette statt direktem Sprung",
                       sum(kosten.get(x, 0) for x in kette))

        # (3) Speichern und sofort wieder Laden derselben Zelle
        if mn == "mov" and ops.startswith("%") and "," in ops:
            q, z = ops.rsplit(",", 1)
            if "(%rbp)" in z and "(" not in q and nxt is not None:
                m2, o2, _ = code[nxt]
                if m2 in ("mov", "movzbl", "movzwl", "movslq") and o2.startswith(z + ","):
                    zaehle("Store+Reload derselben Zelle",
                           kosten.get(a, 0) + kosten.get(nxt, 0))

        # (4) Adressrechnung, die in den Speicheroperanden koennte
        if mn == "lea" and nxt is not None:
            m = re.match(r"^(-?0x[0-9a-f]+)?\((%r[a-z0-9]+)(,(%r[a-z0-9]+),(\d))?\),(%r[a-z0-9]+)$", ops)
            if m and code[nxt][0].startswith("mov") and f"({m.group(6)})" in code[nxt][1]:
                zaehle("lea + Zugriff (Adressierungsmodus ungenutzt)", kosten.get(a, 0))

        # (5) Rahmenverwaltung
        if mn in ("push", "pop", "ret"):
            zaehle("Rahmenverwaltung (push/pop/ret)", kosten.get(a, 0))
        elif mn == "call":
            zaehle("Rahmenverwaltung (call)", kosten.get(a, 0))
        elif mn == "mov" and ops in ("%rsp,%rbp", "%rbp,%rsp"):
            zaehle("Rahmenverwaltung (rsp<->rbp)", kosten.get(a, 0))
        elif mn == "mov" and "," in ops:
            q, z = ops.rsplit(",", 1)
            gesichert = ("%r12", "%r13", "%r14", "%r15", "%rbx")
            if (q in gesichert and "(%rbp)" in z) or (z in gesichert and "(%rbp)" in q):
                zaehle("Rahmenverwaltung (callee-saved sichern/holen)", kosten.get(a, 0))

    print("\n== Muster (dynamisch gewichtet) ==")
    for name, (ir, n) in sorted(muster.items(), key=lambda x: -x[1][0]):
        print(f"  {name:<48} {ir:>14,} Ir  {100 * ir / ges:6.2f}%   {n:>6} Stellen")

    proMn = defaultdict(int)
    proN = defaultdict(int)
    for a in reihe:
        proMn[code[a][0]] += kosten.get(a, 0)
        proN[code[a][0]] += 1
    print("\n== Top-Mnemonics ==")
    for mn, ir in sorted(proMn.items(), key=lambda x: -x[1])[:20]:
        print(f"  {mn:<12} {ir:>14,} Ir  {100 * ir / ges:6.2f}%   {proN[mn]:>6} statisch")

    print("\n== Datenbewegungen nach Art ==")
    art = defaultdict(int)
    artn = defaultdict(int)
    for a in reihe:
        mn, ops, _ = code[a]
        if not mn.startswith("mov") or "," not in ops:
            continue
        q, z = ops.rsplit(",", 1)
        f = lambda o: "Speicher" if "(" in o else ("Konstante" if o.startswith("$") else "Register")
        art[f"{f(q)} -> {f(z)}"] += kosten.get(a, 0)
        artn[f"{f(q)} -> {f(z)}"] += 1
    for name, ir in sorted(art.items(), key=lambda x: -x[1]):
        print(f"  {name:<28} {ir:>14,} Ir  {100 * ir / ges:6.2f}%   {artn[name]:>6} statisch")
    return 0


if __name__ == "__main__":
    sys.exit(main())
