#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""
ARM64-Sprungreichweiten messen.

Anders als bei x86 wird ein Sprung hier nie laenger -- jeder Befehl ist vier
Oktette. Dafuer gibt es etwas Schlimmeres: er passt ab einer gewissen Weite
GAR NICHT MEHR, und dann muss der Uebersetzer einen Umweg bauen.

Die Reichweiten sind sehr unterschiedlich:
  b / bl      +-128 MiB   (26 Bit in Worten)
  b.<cond>    +-1 MiB     (19 Bit)
  cbz / cbnz  +-1 MiB     (19 Bit)
  tbz / tbnz  +-32 KiB    (14 Bit)  <- das ist die enge Stelle

Hier wird gemessen, wo as noch mitmacht und was es hineinschreibt.
"""
import json, os, re, subprocess, tempfile

HIER = os.path.dirname(os.path.abspath(__file__))
AS = "aarch64-linux-gnu-as"
OD = "aarch64-linux-gnu-objdump"

FAELLE = [
    ("b",      "b ZIEL"),
    ("bl",     "bl ZIEL"),
    ("b.eq",   "b.eq ZIEL"),
    ("b.ne",   "b.ne ZIEL"),
    ("b.cs",   "b.cs ZIEL"),
    ("b.cc",   "b.cc ZIEL"),
    ("b.ge",   "b.ge ZIEL"),
    ("b.hi",   "b.hi ZIEL"),
    ("b.lo",   "b.lo ZIEL"),
    ("b.vs",   "b.vs ZIEL"),
    ("cbz",    "cbz x0, ZIEL"),
    ("cbnz",   "cbnz x0, ZIEL"),
    ("cbnzw",  "cbnz w9, ZIEL"),
    ("tbz",    "tbz x0, #7, ZIEL"),
    ("tbnz",   "tbnz x28, #63, ZIEL"),
]

# Abstaende in Oktetten, rund um die Reichweitengrenzen
ABSTAENDE = [0, 4, 8, 64, 1024, 32764, 32768, 32772,
             1048572, 1048576, 1048580]


def messen(name, befehl, abstand, rueckwaerts):
    if rueckwaerts:
        src = (".text\nZIEL:\n  .skip %d, 0\nSPRUNG:\n  %s\nENDE:\n"
               % (abstand, befehl))
    else:
        src = (".text\nSPRUNG:\n  %s\nENDE:\n  .skip %d, 0\nZIEL:\n  nop\n"
               % (befehl, abstand))
    tmp = tempfile.mkdtemp(prefix="a64spr_")
    s = os.path.join(tmp, "a.s")
    o = os.path.join(tmp, "a.o")
    open(s, "w").write(src)
    p = subprocess.run([AS, s, "-o", o], capture_output=True, text=True)
    if p.returncode != 0:
        return {"mnem": name, "abstand": abstand, "rueckwaerts": rueckwaerts,
                "hex": None, "fehler": p.stderr.strip()[:120]}
    p = subprocess.run([OD, "-t", o], capture_output=True, text=True)
    adr = {}
    for line in p.stdout.split("\n"):
        m = re.match(r"^([0-9a-f]+)\s+.*\s(SPRUNG|ENDE|ZIEL)$", line.strip())
        if m:
            adr[m.group(2)] = int(m.group(1), 16)
    p = subprocess.run([OD, "-s", "-j", ".text", o], capture_output=True, text=True)
    roh = bytearray()
    basis = None
    for line in p.stdout.split("\n"):
        m = re.match(r"^\s*([0-9a-f]+)\s((?:[0-9a-f]{2,8}\s){1,4})", line)
        if m:
            a = int(m.group(1), 16)
            if basis is None:
                basis = a
            roh.extend(bytes.fromhex(m.group(2).replace(" ", "")))
    okt = bytes(roh[adr["SPRUNG"] - basis: adr["SPRUNG"] - basis + 4])
    return {"mnem": name, "befehl": befehl, "abstand": abstand,
            "rueckwaerts": rueckwaerts, "hex": okt.hex(),
            "rel": adr["ZIEL"] - adr["SPRUNG"]}


def main():
    erg = []
    for name, befehl in FAELLE:
        for rw in (False, True):
            for a in ABSTAENDE:
                erg.append(messen(name, befehl, a, rw))
    ziel = os.path.join(HIER, "wahrheit_a64spr.json")
    json.dump(erg, open(ziel, "w"))
    gut = [r for r in erg if r["hex"]]
    print("ARM64-Spruenge: %d Faelle, %d von as angenommen -> %s"
          % (len(erg), len(gut), ziel))

    print("\n--- Reichweite: groesster Abstand, den as noch annimmt (vorwaerts) ---")
    for name, _ in FAELLE:
        ok = [r["abstand"] for r in erg
              if r["mnem"] == name and not r["rueckwaerts"] and r["hex"]]
        nein = [r["abstand"] for r in erg
                if r["mnem"] == name and not r["rueckwaerts"] and not r["hex"]]
        print("  %-7s bis %-9s ab %s abgelehnt"
              % (name, max(ok) if ok else "-", min(nein) if nein else "nie"))


if __name__ == "__main__":
    main()
