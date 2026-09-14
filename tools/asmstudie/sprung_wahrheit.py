#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""
Spruenge und RIP-relative Adressen pruefen -- die Faelle, die man NICHT
einzeln pruefen kann.

Warum getrennt: die Oktette eines Sprungs haengen vom ABSTAND zum Ziel ab,
und der Abstand haengt davon ab, wie lang der Sprung wird. Ein Sprung, der
knapp nicht in ein Oktett passt, wird laenger -- und dadurch koennen andere
Spruenge ebenfalls laenger werden. Genau deshalb braucht ein Assembler zwei
Durchlaeufe.

Aufbau des Versuchs: eine Datei, in der das Sprungziel in genau definiertem
Abstand steht (ueber .skip erzeugt). Dann laesst sich pruefen:
  - waehlt as rel8 oder rel32?
  - bei welchem Abstand kippt es?
  - welchen Wert schreibt es hinein (vom ENDE des Befehls gerechnet)?
"""
import json, os, re, subprocess, tempfile

HIER = os.path.dirname(os.path.abspath(__file__))

# Abstaende rund um die Kippstellen. rel8 reicht von -128 bis +127,
# gerechnet ab dem Ende des Sprungbefehls.
ABSTAENDE = [0, 1, 2, 5, 60, 120, 124, 125, 126, 127, 128, 129, 130, 200, 1000, 100000]

CC = ["jmp", "je", "jne", "jz", "jnz", "jl", "jle", "jg", "jge",
      "jb", "jbe", "ja", "jae", "jc", "jo", "jns", "call"]


def bauen(mnem, abstand, rueckwaerts):
    """Erzeugt eine Quelle mit genau diesem Abstand und gibt die Oktette
    des Sprungbefehls plus die Entscheidung rel8/rel32 zurueck."""
    if rueckwaerts:
        src = (".intel_syntax noprefix\n.text\n"
               "ZIEL:\n"
               "  .skip %d, 0x90\n"
               "SPRUNG:\n"
               "  %s ZIEL\n"
               "ENDE:\n" % (abstand, mnem))
    else:
        src = (".intel_syntax noprefix\n.text\n"
               "SPRUNG:\n"
               "  %s ZIEL\n"
               "ENDE:\n"
               "  .skip %d, 0x90\n"
               "ZIEL:\n"
               "  nop\n" % (mnem, abstand))
    tmp = tempfile.mkdtemp(prefix="sprung_")
    s = os.path.join(tmp, "a.s")
    o = os.path.join(tmp, "a.o")
    open(s, "w").write(src)
    p = subprocess.run(["as", "--64", s, "-o", o], capture_output=True, text=True)
    if p.returncode != 0:
        return None
    p = subprocess.run(["objdump", "-t", o], capture_output=True, text=True)
    adr = {}
    for line in p.stdout.split("\n"):
        m = re.match(r"^([0-9a-f]+)\s+.*\s(SPRUNG|ENDE|ZIEL)$", line.strip())
        if m:
            adr[m.group(2)] = int(m.group(1), 16)
    p = subprocess.run(["objdump", "-s", "-j", ".text", o], capture_output=True, text=True)
    roh = bytearray()
    basis = None
    for line in p.stdout.split("\n"):
        m = re.match(r"^\s*([0-9a-f]+)\s((?:[0-9a-f]{2,8}\s){1,4})", line)
        if m:
            a = int(m.group(1), 16)
            if basis is None:
                basis = a
            roh.extend(bytes.fromhex(m.group(2).replace(" ", "")))
    if basis is None:
        return None
    okt = bytes(roh[adr["SPRUNG"] - basis: adr["ENDE"] - basis])
    return {
        "mnem": mnem,
        "abstand": abstand,
        "rueckwaerts": rueckwaerts,
        "hex": okt.hex(),
        "laenge": len(okt),
        # der Abstand, den der Sprung ueberbruecken muss, ab ENDE des Befehls
        "rel": (adr["ZIEL"] - adr["ENDE"]) if not rueckwaerts else (adr["ZIEL"] - adr["ENDE"]),
    }


def rip_faelle():
    """RIP-relativ: der Wert im Befehl ist der Abstand vom naechsten Befehl
    zum Symbol. Auch das laesst sich nur im Zusammenhang pruefen."""
    out = []
    for abstand in [0, 8, 64, 1000]:
        src = (".intel_syntax noprefix\n.text\n"
               "SPRUNG:\n"
               "  lea rax, [rip+ZIEL]\n"
               "ENDE:\n"
               "  .skip %d, 0x90\n"
               "ZIEL:\n"
               "  .quad 0\n" % abstand)
        tmp = tempfile.mkdtemp(prefix="rip_")
        s = os.path.join(tmp, "a.s")
        o = os.path.join(tmp, "a.o")
        open(s, "w").write(src)
        p = subprocess.run(["as", "--64", s, "-o", o], capture_output=True, text=True)
        if p.returncode != 0:
            continue
        p = subprocess.run(["objdump", "-t", o], capture_output=True, text=True)
        adr = {}
        for line in p.stdout.split("\n"):
            m = re.match(r"^([0-9a-f]+)\s+.*\s(SPRUNG|ENDE|ZIEL)$", line.strip())
            if m:
                adr[m.group(2)] = int(m.group(1), 16)
        p = subprocess.run(["objdump", "-s", "-j", ".text", o], capture_output=True, text=True)
        roh = bytearray()
        basis = None
        for line in p.stdout.split("\n"):
            m = re.match(r"^\s*([0-9a-f]+)\s((?:[0-9a-f]{2,8}\s){1,4})", line)
            if m:
                a = int(m.group(1), 16)
                if basis is None:
                    basis = a
                roh.extend(bytes.fromhex(m.group(2).replace(" ", "")))
        okt = bytes(roh[adr["SPRUNG"] - basis: adr["ENDE"] - basis])
        out.append({"abstand": abstand, "hex": okt.hex(),
                    "rel": adr["ZIEL"] - adr["ENDE"]})
    return out


def main():
    erg = []
    for mnem in CC:
        for rw in (False, True):
            for a in ABSTAENDE:
                r = bauen(mnem, a, rw)
                if r:
                    erg.append(r)
    rip = rip_faelle()
    ziel = os.path.join(HIER, "wahrheit_sprung.json")
    json.dump({"spruenge": erg, "rip": rip}, open(ziel, "w"))
    print("Spruenge: %d Faelle -> %s" % (len(erg), ziel))
    print("RIP: %d Faelle" % len(rip))

    print("\n--- Wo kippt GNU as von rel8 auf rel32? (vorwaerts) ---")
    for mnem in ("jmp", "je", "call"):
        kurz = [r for r in erg if r["mnem"] == mnem and not r["rueckwaerts"] and r["laenge"] <= 2]
        lang = [r for r in erg if r["mnem"] == mnem and not r["rueckwaerts"] and r["laenge"] > 2]
        gk = max([r["abstand"] for r in kurz], default=None)
        kl = min([r["abstand"] for r in lang], default=None)
        print("  %-6s kurz bis Abstand %s, ab %s lang" % (mnem, gk, kl))

    print("\n--- Beispiele ---")
    for r in erg[:6]:
        print("  %-5s abstand %-7d rw=%-5s laenge %d  rel %-8d %s"
              % (r["mnem"], r["abstand"], r["rueckwaerts"], r["laenge"], r["rel"], r["hex"]))
    print("\n--- RIP ---")
    for r in rip:
        print("  abstand %-6d rel %-6d %s" % (r["abstand"], r["rel"], r["hex"]))


if __name__ == "__main__":
    main()
