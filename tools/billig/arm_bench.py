#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/billig/arm_bench.py -- WAS DER ARM64-REGISTERZUTEILER WIRKLICH BRINGT.

Dieselben elf Baenke, mit denen Runde 43 und Runde 90 den x86-Zuteiler
gemessen haben (`bench/firn/`), einmal mit dem alten und einmal mit dem
neuen Uebersetzer, beide fuer `--target=aarch64-linux`.

DREI ZAHLEN JE BANK, und jede sagt etwas anderes:

  1. STATISCHE BEFEHLE -- gezaehlt im erzeugten Assemblertext (`--emit=asm`),
     ohne Marken, Kommentare und Anweisungen. Exakt, deterministisch, und
     die einzige Zahl hier, die von der Maschine gar nicht abhaengt.

  2. AUSGEFUEHRTE BEFEHLE -- `qemu-aarch64 -singlestep -d cpu` schreibt je
     ausgefuehrtem Befehl eine Zeile mit `PC=`. Das ist die Zahl, die Runde
     43 auf x86 mit callgrind gemessen hat (-26,15 %), nur eben fuer ARM64.
     Sie ist exakt, aber sie kostet rund 90 000 Befehle je Sekunde -- eine
     Bank, die eine Milliarde Befehle laeuft, ist so nicht messbar. Deshalb
     steht ein Zeitlimit davor und es steht dabei, welche Bank es gerissen
     hat.

  3. DURCHSATZ -- Wanduhr unter `qemu-aarch64`, Bestwert aus N Laeufen.
     WICHTIG UND EHRLICH: das ist ein Emulator, kein Telefon. TCG uebersetzt
     Bloecke und fuehrt sie in x86-Code aus; die Zahl folgt der Zahl der
     ausgefuehrten Befehle viel enger als echte ARM64-Hardware es taete
     (kein Sprungvorhersager, keine Ausfuehrung ausser der Reihe, andere
     Cachehierarchie). Sie ist ein UNTERSCHIED zwischen zwei Fassungen
     desselben Programms auf demselben Emulator -- nicht mehr, aber auch
     nicht weniger.

Aufruf:
    python3 tools/billig/arm_bench.py --alt <firnc> --neu <firnc> \\
        [--stufe dev-fast] [--laeufe 5] [--icount-limit 120] [--json <datei>]
"""
import argparse
import json
import math
import os
import re
import subprocess
import sys
import time

HIER = os.path.dirname(os.path.abspath(__file__))
WURZEL = os.path.dirname(os.path.dirname(HIER))
BAENKE = ["fib", "sieve", "matmul", "bytecount", "bubblesort", "statemachine",
          "bitmap", "xxhash", "jsonscan", "memstride", "branchy"]

MARKE = re.compile(r"^\s*[.\w$]+:\s*$")
ANWEISUNG = re.compile(r"^\s*\.")
KOMMENTAR = re.compile(r"^\s*(//|#)")


def statische_befehle(pfad):
    n = 0
    for z in open(pfad, encoding="utf-8", errors="replace"):
        if not z.strip():
            continue
        if MARKE.match(z) or ANWEISUNG.match(z) or KOMMENTAR.match(z):
            continue
        n += 1
    return n


def uebersetze(firnc, quelle, ziel, stufe, asm=False):
    umg = dict(os.environ)
    umg["FIRNLIB"] = os.path.join(WURZEL, "lib")
    cmd = [firnc, "--target=aarch64-linux", "--opt-level=" + stufe]
    if asm:
        cmd.append("--emit=asm")
    cmd += ["-o", ziel, quelle]
    p = subprocess.run(cmd, capture_output=True, text=True, env=umg)
    if p.returncode != 0:
        return p.stderr[:400]
    return None


def wanduhr(binaer, laeufe):
    ts = []
    aus = None
    for _ in range(laeufe):
        t = time.perf_counter()
        p = subprocess.run(["qemu-aarch64", binaer], capture_output=True, timeout=3600)
        ts.append(time.perf_counter() - t)
        aus = (p.returncode, p.stdout)
    return min(ts), aus


def icount(binaer, limit):
    """Exakte Zahl ausgefuehrter Befehle -- oder None, wenn es zu lange dauert.

    Das Zaehlen selbst laeuft in `grep`, nicht in Python: qemu schreibt je
    ausgefuehrtem Befehl ZEHN Zeilen Registerzustand, und eine Bank mit ein
    paar Millionen Befehlen sind schon Gigabytes. Python liest so etwa
    zehntausend Befehle je Sekunde, `grep -c` einige hunderttausend.
    `pipefail` sorgt dafuer, dass das Zeitlimit von `timeout` (124) durch
    die Roehre kommt und nicht von `grep` verschluckt wird.
    """
    cmd = ("set -o pipefail; timeout %d qemu-aarch64 -singlestep -d cpu "
           "-D /dev/stdout %s 2>/dev/null | grep -c '^PC='" % (int(limit), binaer))
    p = subprocess.run(["bash", "-c", cmd], capture_output=True, text=True)
    if p.returncode != 0:
        return None
    try:
        return int(p.stdout.strip())
    except ValueError:
        return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--alt", required=True)
    ap.add_argument("--neu", required=True)
    ap.add_argument("--stufe", default="dev-fast")
    ap.add_argument("--laeufe", type=int, default=5)
    ap.add_argument("--icount-limit", type=float, default=120.0)
    ap.add_argument("--nur", default="")
    ap.add_argument("--json", default="")
    a = ap.parse_args()
    os.chdir(WURZEL)
    arbeit = "/root/BILLIG-arm"
    os.makedirs(arbeit, exist_ok=True)

    erg = {}
    print("%-14s %-26s %-27s %s"
          % ("Bank", "statische Befehle", "ausgefuehrte Befehle", "Wanduhr qemu (s)"))
    print("%-14s %8s %8s %7s  %8s %8s %7s  %9s %9s %s"
          % ("", "alt", "neu", "diff", "alt", "neu", "diff", "alt", "neu", "Faktor"))
    for name in BAENKE:
        if a.nur and a.nur not in name:
            continue
        quelle = os.path.join(WURZEL, "bench/firn", name + ".fi")
        if not os.path.exists(quelle):
            continue
        z = {}
        fehler = None
        for tag, firnc in (("alt", a.alt), ("neu", a.neu)):
            b = os.path.join(arbeit, "%s.%s" % (name, tag))
            s = b + ".s"
            fehler = uebersetze(firnc, quelle, s, a.stufe, asm=True)
            if fehler:
                break
            z["stat_" + tag] = statische_befehle(s)
            fehler = uebersetze(firnc, quelle, b, a.stufe)
            if fehler:
                break
            z["bin_" + tag] = b
        if fehler:
            print("%-14s UEBERSETZUNG FEHLGESCHLAGEN: %s" % (name, fehler.strip()[:120]))
            continue
        t_alt, o_alt = wanduhr(z["bin_alt"], a.laeufe)
        t_neu, o_neu = wanduhr(z["bin_neu"], a.laeufe)
        z["t_alt"], z["t_neu"] = t_alt, t_neu
        z["gleich"] = (o_alt == o_neu)
        z["ic_alt"] = icount(z["bin_alt"], a.icount_limit)
        z["ic_neu"] = icount(z["bin_neu"], a.icount_limit) if z["ic_alt"] else None
        erg[name] = {k: v for k, v in z.items() if not k.startswith("bin_")}
        if z["ic_alt"] and z["ic_neu"]:
            ic = "%8d %8d %6.1f%%" % (z["ic_alt"], z["ic_neu"],
                                      100.0 * (z["ic_neu"] - z["ic_alt"]) / z["ic_alt"])
        else:
            ic = "%25s" % "zu gross fuer singlestep"
        print("%-14s %8d %8d %6.1f%%  %s  %9.3f %9.3f %6.3fx%s"
              % (name, z["stat_alt"], z["stat_neu"],
                 100.0 * (z["stat_neu"] - z["stat_alt"]) / z["stat_alt"],
                 ic, t_alt, t_neu, t_alt / t_neu,
                 "" if z["gleich"] else "  !! UNGLEICHES ERGEBNIS"))
        sys.stdout.flush()
    if a.json:
        json.dump(erg, open(a.json, "w"), indent=1)
        print("geschrieben", a.json)
    q = [v["t_alt"] / v["t_neu"] for v in erg.values()]
    s_alt = sum(v["stat_alt"] for v in erg.values())
    s_neu = sum(v["stat_neu"] for v in erg.values())
    ics = [(v["ic_alt"], v["ic_neu"]) for v in erg.values()
           if v.get("ic_alt") and v.get("ic_neu")]
    print()
    print("statische Befehle gesamt: %d -> %d  (%+.2f %%)"
          % (s_alt, s_neu, 100.0 * (s_neu - s_alt) / s_alt))
    if ics:
        ia = sum(x for x, _ in ics)
        ib = sum(y for _, y in ics)
        print("ausgefuehrte Befehle (%d Baenke): %d -> %d  (%+.2f %%)"
              % (len(ics), ia, ib, 100.0 * (ib - ia) / ia))
    if q:
        print("Durchsatz unter qemu, geometrisches Mittel: %.3f x"
              % math.exp(sum(math.log(x) for x in q) / len(q)))
    alle_gleich = all(v["gleich"] for v in erg.values())
    print("Ergebnisse identisch in allen %d Baenken: %s" % (len(erg), "JA" if alle_gleich else "NEIN"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
