#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/hub/scale.py -- was Gesamtprogramm-Uebersetzung bei N Abhaengigkeiten kostet.

DIE FRAGE, die diese Messung beantwortet (Runde FIRNHUB, Teil 4):

Firn uebersetzt GESAMTPROGRAMME. Ein importiertes Modul landet in derselben
Uebersetzungseinheit; es gibt keine getrennten Objektdateien und keine
Schnittstellendateien (SPEC Punkt 15). Bei zehn Abhaengigkeiten wird also
jedes Mal alles neu uebersetzt. Ab welcher Groesse ist das untragbar?

DER AUFBAU. Erzeugt werden kuenstliche Abhaengigkeitsbaeume: N Pakete mit je
`--lines` Zeilen Firn, dazu ein Wurzelprojekt, das jedes davon einbindet und
je eine Funktion daraus ruft. Gemessen wird mit `/usr/bin/time` die Wanduhr
und der groesste Speicherbedarf (RSS) des Compilers.

ZWEI VERGLEICHE, und der zweite ist der wichtige:

  1. N Pakete gegen 1 Paket MIT DERSELBEN GESAMTZEILENZAHL. Das trennt die
     Kosten der PAKETE von den Kosten der ZEILEN. Wenn beide gleich teuer
     sind, kostet ein Paket nichts ausser seinen Zeilen -- und dann ist das
     Problem nicht das Paketsystem, sondern die fehlende getrennte
     Uebersetzung.
  2. Was ein WIEDERAUFBAU nach einer Aenderung an EINER Zeile der Wurzel
     kostet. Das ist der Alltag: man aendert sein eigenes Programm, nicht
     seine Abhaengigkeiten. Genau diese Zahl waere bei getrennter
     Uebersetzung nahezu null und ist hier die volle Bauzeit.

Aufruf:
    python3 tools/hub/scale.py --out .hub-scale --lines 2000 --counts 1,5,10,25
"""

import argparse
import os
import pathlib
import shutil
import subprocess
import sys
import time


def package_source(name, lines):
    """Ein Modul mit ungefaehr `lines` Zeilen ECHTEM Code.

    Kein toter Fuellstoff: jede Funktion rechnet, ruft ihre Vorgaengerin und
    wird exportiert. Sonst wuerde die tote Code-Entfernung die Messung
    auffressen und wir wuerden messen, wie schnell der Optimierer Arbeit
    wegwirft, statt wie teuer Arbeit ist.
    """
    per = 6                      # Zeilen je Funktion
    count = max(1, lines // per)
    out = []
    names = [f"{name}_f{i}" for i in range(count)]
    out.append(f"export {{ {', '.join(names)} }}")
    out.append("")
    for i in range(count):
        out.append(f"fn {name}_f{i}(x: i64) -> i64 {{")
        if i == 0:
            out.append(f"    var a: i64 = x * 3 + {i + 1}")
        else:
            out.append(f"    var a: i64 = {name}_f{i - 1}(x) + {i + 1}")
        out.append("    if a % 2 == 0 {")
        out.append(f"        a = a - {i + 1}")
        out.append("    }")
        out.append("    return a")
        out.append("}")
        out.append("")
    return "\n".join(out) + "\n"


def build_tree(root, count, lines):
    """N Pakete + ein Wurzelprojekt, das alle einbindet."""
    shutil.rmtree(root, ignore_errors=True)
    root.mkdir(parents=True)
    for k in range(count):
        name = f"p{k}"
        d = root / name
        (d / "src").mkdir(parents=True)
        (d / "firn.pkg").write_text(
            f"package  {name}\nversion  1.0.0\nsource   src\npublic   {name}\n"
        )
        (d / "src" / f"{name}.fi").write_text(package_source(name, lines))
    app = root / "app"
    (app / "src").mkdir(parents=True)
    needs = "".join(f"needs    p{k}  ../p{k}\n" for k in range(count))
    (app / "firn.pkg").write_text(
        f"package  app\nversion  0.1.0\nmain     src/main.fi\nsource   src\n\n{needs}"
    )
    body = ["// erzeugt von tools/hub/scale.py"]
    for k in range(count):
        body.append(f"import p{k}")
    body.append("")
    body.append("fn main() -> i32 {")
    body.append("    var s: i64 = 0")
    for k in range(count):
        body.append(f"    s = s + p{k}.p{k}_f0(3)")
    body.append("    return (s % 7) as i32")
    body.append("}")
    (app / "src" / "main.fi").write_text("\n".join(body) + "\n")
    return app


def build_single(root, count, lines):
    """EIN Paket mit derselben Gesamtzeilenzahl -- die Vergleichsgroesse."""
    shutil.rmtree(root, ignore_errors=True)
    app = root / "app"
    (app / "src").mkdir(parents=True)
    (app / "firn.pkg").write_text(
        "package  app\nversion  0.1.0\nmain     src/main.fi\nsource   src\n"
    )
    for k in range(count):
        (app / "src" / f"p{k}.fi").write_text(package_source(f"p{k}", lines))
    body = ["// erzeugt von tools/hub/scale.py"]
    for k in range(count):
        body.append(f"import p{k}")
    body.append("")
    body.append("fn main() -> i32 {")
    body.append("    var s: i64 = 0")
    for k in range(count):
        body.append(f"    s = s + p{k}.p{k}_f0(3)")
    body.append("    return (s % 7) as i32")
    body.append("}")
    (app / "src" / "main.fi").write_text("\n".join(body) + "\n")
    return app


def count_lines(d):
    n = 0
    for p in pathlib.Path(d).rglob("*.fi"):
        n += len(p.read_text().splitlines())
    return n


def measure(firnc, project, out, repeats):
    """Wanduhr in Sekunden und groesster RSS in KiB, bester von `repeats`."""
    best_t = None
    best_m = None
    for _ in range(repeats):
        t0 = time.monotonic()
        r = subprocess.run(
            ["/usr/bin/time", "-f", "%M", "-o", "/tmp/.scale-rss",
             firnc, "--package", str(project), "-o", str(out)],
            capture_output=True,
        )
        dt = time.monotonic() - t0
        if r.returncode != 0:
            sys.stderr.write(r.stderr.decode()[:2000])
            raise SystemExit(f"Bau fehlgeschlagen: {project}")
        rss = int(pathlib.Path("/tmp/.scale-rss").read_text().strip().splitlines()[-1])
        if best_t is None or dt < best_t:
            best_t = dt
        if best_m is None or rss > best_m:
            best_m = rss
    return best_t, best_m


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--firnc", default="compiler/target/release/firnc")
    ap.add_argument("--out", default=".hub-scale")
    ap.add_argument("--lines", type=int, default=2000)
    ap.add_argument("--counts", default="1,5,10,25")
    ap.add_argument("--repeats", type=int, default=3)
    a = ap.parse_args()

    firnc = os.path.abspath(a.firnc)
    base = pathlib.Path(a.out).absolute()
    base.mkdir(parents=True, exist_ok=True)
    counts = [int(x) for x in a.counts.split(",")]

    print(f"# firnc: {firnc}")
    print(f"# Zeilen je Paket: {a.lines}, beste von {a.repeats} Laeufen")
    print()
    print("| Pakete | Zeilen gesamt | N Pakete: Zeit | RSS | 1 Paket: Zeit | RSS | Zeit/1000 Zeilen |")
    print("|---:|---:|---:|---:|---:|---:|---:|")
    rows = []
    for n in counts:
        tree = base / f"tree{n}"
        app = build_tree(tree, n, a.lines)
        lines = count_lines(tree)
        t_many, m_many = measure(firnc, app, base / f"bin{n}", a.repeats)

        flat = base / f"flat{n}"
        app2 = build_single(flat, n, a.lines)
        t_one, m_one = measure(firnc, app2, base / f"binflat{n}", a.repeats)

        per_k = t_many / (lines / 1000.0)
        rows.append((n, lines, t_many, m_many, t_one, m_one, per_k))
        print(f"| {n} | {lines:,} | {t_many:.2f} s | {m_many / 1024:.0f} MiB "
              f"| {t_one:.2f} s | {m_one / 1024:.0f} MiB | {per_k:.3f} s |")

    print()
    print("## Wiederaufbau nach einer Aenderung an EINER Zeile der Wurzel")
    print()
    print("| Pakete | Zeilen gesamt | Bauzeit |")
    print("|---:|---:|---:|")
    for n in counts:
        app = base / f"tree{n}" / "app"
        m = app / "src" / "main.fi"
        m.write_text(m.read_text() + "// eine Zeile mehr\n")
        t, _ = measure(firnc, app, base / f"bin{n}", a.repeats)
        lines = count_lines(base / f"tree{n}")
        print(f"| {n} | {lines:,} | {t:.2f} s |")


if __name__ == "__main__":
    main()
