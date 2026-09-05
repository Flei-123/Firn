#!/usr/bin/env python3
"""RUNDE KODIERER -- die Gegenprobe: eigener Kodierer gegen GNU as, Oktett fuer Oktett.

Der Gedanke: der alte Weg schickt denselben Assemblertext durch `as` und
bekommt die RICHTIGEN Oktette. Also lassen wir beide Wege auf dieselbe
Eingabe los und vergleichen.

Verglichen wird
  * der Inhalt von .text, .data und .rodata, Oktett fuer Oktett,
  * die Menge der Umsetzungen (Offset, Symbolname, Art, Zusatz) je Abschnitt,
  * die definierten globalen Symbole mit ihrem Wert.

NICHT verglichen wird die Fehlersuchinformation (.debug_*): der interne Weg
erzeugt sie nicht (siehe docs/RUNDE-KODIERER.md, "Was fehlt").

Bei einer Abweichung wird der Befehl gemeldet, bei dem sie anfaengt -- mit
erwarteter und tatsaechlicher Kodierung und der naechstgelegenen Marke,
damit die Stelle im Quelltext auffindbar ist.

Aufruf:
    vergleich.py <firnc> <datei.s> [<datei.s> ...]
    vergleich.py <firnc> --liste <datei-mit-pfaden>
"""
import os, re, subprocess, sys, tempfile, json

VERGLICHENE_ABSCHNITTE = [".text", ".data", ".rodata", ".bss"]

# RUNDE KODIERER II -- die Fehlersuchabschnitte. `.debug_str` steht NICHT
# dabei: dort steht als letzte Zeichenkette der Erzeuger, und der eigene
# Weg schreibt ehrlich `firnc` hin statt `GNU AS`. Alles davor (Name des
# Hauptquelltextes, Arbeitsverzeichnis) wird trotzdem geprueft, siehe
# `pruefe_debug_str`.
DWARF_ABSCHNITTE = [".debug_line", ".debug_info", ".debug_abbrev",
                    ".debug_aranges"]


# --------------------------------------------------------------------------
# ELF lesen (nur so viel, wie der Vergleich braucht)
# --------------------------------------------------------------------------

def u(b, off, n):
    return int.from_bytes(b[off:off + n], "little")


class Elf:
    def __init__(self, raw):
        self.raw = raw
        if raw[:4] != b"\x7fELF":
            raise ValueError("kein ELF")
        shoff = u(raw, 0x28, 8)
        shentsize = u(raw, 0x3A, 2)
        shnum = u(raw, 0x3C, 2)
        shstrndx = u(raw, 0x3E, 2)
        self.sh = []
        for i in range(shnum):
            o = shoff + i * shentsize
            self.sh.append({
                "name_off": u(raw, o, 4),
                "type": u(raw, o + 4, 4),
                "flags": u(raw, o + 8, 8),
                "offset": u(raw, o + 0x18, 8),
                "size": u(raw, o + 0x20, 8),
                "link": u(raw, o + 0x28, 4),
                "info": u(raw, o + 0x2C, 4),
                "align": u(raw, o + 0x30, 8),
                "entsize": u(raw, o + 0x38, 8),
            })
        strtab = self.sh[shstrndx]
        base = strtab["offset"]
        for s in self.sh:
            e = raw.index(b"\0", base + s["name_off"])
            s["name"] = raw[base + s["name_off"]:e].decode()
        self.byname = {s["name"]: s for s in self.sh}
        self._symbols = None

    def data(self, name):
        s = self.byname.get(name)
        if s is None or s["type"] == 8:  # NOBITS
            return b""
        return self.raw[s["offset"]:s["offset"] + s["size"]]

    def symbols(self):
        if self._symbols is not None:
            return self._symbols
        st = self.byname.get(".symtab")
        if st is None:
            self._symbols = []
            return self._symbols
        strt = self.sh[st["link"]]
        out = []
        n = st["size"] // 24
        for i in range(n):
            o = st["offset"] + i * 24
            nm_off = u(self.raw, o, 4)
            info = self.raw[o + 4]
            shndx = u(self.raw, o + 6, 2)
            val = u(self.raw, o + 8, 8)
            if nm_off == 0:
                nm = ""
            else:
                e = self.raw.index(b"\0", strt["offset"] + nm_off)
                nm = self.raw[strt["offset"] + nm_off:e].decode()
            typ = info & 0xF
            bind = info >> 4
            if typ == 3:  # SECTION
                nm = self.sh[shndx]["name"] if shndx < len(self.sh) else "?"
            out.append({"name": nm, "bind": bind, "type": typ,
                        "shndx": shndx, "value": val})
        self._symbols = out
        return out

    def relocs(self, secname):
        """Menge der Umsetzungen eines Abschnitts, symbolisch."""
        rs = self.byname.get(".rela" + secname)
        if rs is None:
            return set()
        syms = self.symbols()
        out = set()
        n = rs["size"] // 24
        for i in range(n):
            o = rs["offset"] + i * 24
            off = u(self.raw, o, 8)
            info = u(self.raw, o + 8, 8)
            add = u(self.raw, o + 16, 8)
            if add >= 1 << 63:
                add -= 1 << 64
            sym = info >> 32
            kind = info & 0xFFFFFFFF
            nm = syms[sym]["name"] if sym < len(syms) else "?%d" % sym
            out.add((off, nm, kind, add))
        return out

    def global_defs(self):
        """Definierte globale Symbole: Name -> (Abschnittsname, Wert)."""
        out = {}
        for s in self.symbols():
            if s["bind"] == 1 and s["type"] != 3 and s["shndx"] not in (0, 0xFFF1):
                nm = self.sh[s["shndx"]]["name"] if s["shndx"] < len(self.sh) else "?"
                out[s["name"]] = (nm, s["value"])
        return out


# --------------------------------------------------------------------------
# Diagnose: welcher Befehl steht an dieser Stelle?
# --------------------------------------------------------------------------

OBJDUMP = "objdump"


def disasm_at(path, secname, offset, umgebung=3):
    """Gibt die Zerlegung um `offset` herum zurueck (Liste von Zeilen)."""
    try:
        out = subprocess.run(
            [OBJDUMP, "-d", "-j", secname, "--insn-width=16", path],
            capture_output=True, text=True, timeout=120).stdout
    except Exception:
        return []
    rows = []
    for line in out.splitlines():
        m = re.match(r"^\s*([0-9a-f]+):\t([0-9a-f ]+?)\t(.*)$", line)
        if m:
            rows.append((int(m.group(1), 16), m.group(2).strip(), m.group(3).strip()))
    if not rows:
        return []
    idx = 0
    for i, (a, _, _) in enumerate(rows):
        if a <= offset:
            idx = i
        else:
            break
    lo = max(0, idx - umgebung)
    hi = min(len(rows), idx + umgebung + 1)
    return ["%s %06x: %-28s %s" % ("->" if i == idx else "  ", rows[i][0],
                                   rows[i][1], rows[i][2])
            for i in range(lo, hi)]


def marke_vor(path, secname, offset):
    """Der Name des letzten Symbols vor `offset` -- zum Wiederfinden im .s."""
    try:
        e = Elf(open(path, "rb").read())
    except Exception:
        return "?"
    sec = e.byname.get(secname)
    if sec is None:
        return "?"
    idx = e.sh.index(sec)
    best = None
    for s in e.symbols():
        if s["shndx"] == idx and s["type"] != 3 and s["name"] and s["value"] <= offset:
            if best is None or s["value"] > best[1]:
                best = (s["name"], s["value"])
    return "%s + %d" % (best[0], offset - best[1]) if best else "(keine Marke)"


# --------------------------------------------------------------------------
# Eine Datei pruefen
# --------------------------------------------------------------------------

def pruefe_debug_str(da, db):
    """`.debug_str` haelt Name, Arbeitsverzeichnis und Erzeuger.

    Der Erzeuger ist die EINE Stelle, an der der eigene Weg mit Absicht
    von `as` abweicht: `as` schreibt `GNU AS 2.40`, wir schreiben `firnc`.
    Alles davor muss stimmen -- inklusive der Versaetze, denn auf die
    zeigt `.debug_info`.
    """
    if not da and not db:
        return None
    ta = da.split(b"\0")
    tb = db.split(b"\0")
    if len(ta) != len(tb) or len(ta) < 3:
        return ("ABSCHNITT .debug_str hat eine andere Form (as: %r, "
                "Kodierer: %r)" % (da[:80], db[:80]))
    if ta[:-2] != tb[:-2]:
        return ("ABSCHNITT .debug_str weicht vor dem Erzeuger ab\n"
                "  as       : %r\n  Kodierer : %r" % (da[:80], db[:80]))
    if not ta[-2].startswith(b"GNU AS") or not tb[-2].startswith(b"firnc"):
        return ("ABSCHNITT .debug_str: unerwartete Erzeugerangabe "
                "(as: %r, Kodierer: %r)" % (ta[-2], tb[-2]))
    return None


def decoded_line(obj, arch):
    """Die Zeilentabelle, wie ein Werkzeug sie SIEHT -- nicht wie sie
    kodiert ist. `readelf` fuehrt den Automaten aus und gibt die Matrix
    aus; daraus wird eine Liste (Datei, Zeile, Adresse, Sicht, stmt)."""
    rd = "readelf" if arch == "x86" else "aarch64-linux-gnu-readelf"
    r = subprocess.run([rd, "--debug-dump=decodedline", obj],
                       capture_output=True, text=True)
    zeilen = []
    for l in r.stdout.splitlines():
        l = l.rstrip()
        if not l or l.startswith(("Contents", "File name", "CU:")) or l.endswith(":"):
            continue
        if not l.startswith(" ") and not l[0].isalnum():
            continue
        t = l.split()
        if len(t) < 3:
            continue
        # Datei, Zeile, Adresse [, Sicht] [, x]
        if not (t[1].isdigit() or t[1] == "-"):
            continue
        if not t[2].startswith("0x"):
            continue
        zeilen.append(tuple(t))
    return zeilen


def pruefe(firnc, s_path, tmpd, arch="x86"):
    ergebnis = {"datei": s_path, "ok": True, "fehler": [], "bytes": 0,
                "relocs": 0, "dwarf_bytes": 0, "dwarf_rows": 0}
    a_o = os.path.join(tmpd, "as.o")
    b_o = os.path.join(tmpd, "intern.o")
    for p in (a_o, b_o):
        if os.path.exists(p):
            os.unlink(p)
    # RUNDE KODIERER II: dieselbe Pfadabbildung wie `main.rs::assemble`,
    # sonst schriebe `as` sein Arbeitsverzeichnis in die Zeilentabelle und
    # der eigene Weg nicht -- ein Unterschied, der nichts mit dem Kodierer
    # zu tun haette.
    karte = "--debug-prefix-map=%s=." % os.getcwd()
    if arch == "x86":
        as_cmd = ["as", "--64", karte, "-o", a_o, s_path]
        fc_cmd = [firnc, "--asm-intern", "--nur-obj", "-o", b_o, s_path]
    else:
        as_cmd = ["aarch64-linux-gnu-as", karte, "-o", a_o, s_path]
        fc_cmd = [firnc, "--target=aarch64-linux", "--asm-intern",
                  "--nur-obj", "-o", b_o, s_path]

    r = subprocess.run(as_cmd, capture_output=True, text=True)
    if r.returncode != 0:
        ergebnis["ok"] = False
        ergebnis["fehler"].append("as scheiterte: " + r.stderr.strip()[:300])
        return ergebnis
    r = subprocess.run(fc_cmd, capture_output=True, text=True)
    if r.returncode != 0:
        ergebnis["ok"] = False
        ergebnis["fehler"].append("Kodierer scheiterte: "
                                  + (r.stderr.strip() or r.stdout.strip())[:400])
        return ergebnis

    try:
        ea = Elf(open(a_o, "rb").read())
        eb = Elf(open(b_o, "rb").read())
    except Exception as ex:
        ergebnis["ok"] = False
        ergebnis["fehler"].append("ELF unlesbar: %s" % ex)
        return ergebnis

    for sec in VERGLICHENE_ABSCHNITTE:
        da, db = ea.data(sec), eb.data(sec)
        ergebnis["bytes"] += len(da)
        if da != db:
            ergebnis["ok"] = False
            n = min(len(da), len(db))
            i = 0
            while i < n and da[i] == db[i]:
                i += 1
            msg = ["ABSCHNITT %s weicht ab bei Versatz 0x%x (as: %d Oktette, "
                   "Kodierer: %d Oktette)" % (sec, i, len(da), len(db)),
                   "  Ort: %s" % marke_vor(a_o, sec, i),
                   "  as       : " + da[i:i + 16].hex(" "),
                   "  Kodierer : " + db[i:i + 16].hex(" ")]
            msg.append("  --- as ---")
            msg += ["    " + x for x in disasm_at(a_o, sec, i)]
            msg.append("  --- Kodierer ---")
            msg += ["    " + x for x in disasm_at(b_o, sec, i)]
            ergebnis["fehler"].append("\n".join(msg))

        ra, rb = ea.relocs(sec), eb.relocs(sec)
        ergebnis["relocs"] += len(ra)
        if ra != rb:
            ergebnis["ok"] = False
            fehlt = sorted(ra - rb)[:6]
            zuviel = sorted(rb - ra)[:6]
            msg = ["UMSETZUNGEN in %s weichen ab (as: %d, Kodierer: %d)"
                   % (sec, len(ra), len(rb))]
            for f in fehlt:
                msg.append("  fehlt  : off=0x%x sym=%s art=%d zusatz=%d" % f)
            for f in zuviel:
                msg.append("  zuviel : off=0x%x sym=%s art=%d zusatz=%d" % f)
            ergebnis["fehler"].append("\n".join(msg))

    # --- die Fehlersuchinformation ---------------------------------
    for sec in DWARF_ABSCHNITTE:
        da, db = ea.data(sec), eb.data(sec)
        ergebnis["dwarf_bytes"] += len(da)
        if da != db:
            ergebnis["ok"] = False
            n = min(len(da), len(db))
            i = 0
            while i < n and da[i] == db[i]:
                i += 1
            ergebnis["fehler"].append("\n".join([
                "ABSCHNITT %s weicht ab bei Versatz 0x%x (as: %d Oktette, "
                "Kodierer: %d Oktette)" % (sec, i, len(da), len(db)),
                "  as       : " + da[i:i + 24].hex(" "),
                "  Kodierer : " + db[i:i + 24].hex(" "),
            ]))
        ra, rb = ea.relocs(sec), eb.relocs(sec)
        if ra != rb:
            ergebnis["ok"] = False
            msg = ["UMSETZUNGEN in %s weichen ab (as: %d, Kodierer: %d)"
                   % (sec, len(ra), len(rb))]
            for f in sorted(ra - rb)[:6]:
                msg.append("  fehlt  : off=0x%x sym=%s art=%d zusatz=%d" % f)
            for f in sorted(rb - ra)[:6]:
                msg.append("  zuviel : off=0x%x sym=%s art=%d zusatz=%d" % f)
            ergebnis["fehler"].append("\n".join(msg))

    f = pruefe_debug_str(ea.data(".debug_str"), eb.data(".debug_str"))
    if f:
        ergebnis["ok"] = False
        ergebnis["fehler"].append(f)

    # Die ausgewertete Tabelle, unabhaengig von der Kodierung: beide
    # Programme durch `readelf` laufen lassen und Zeile fuer Zeile
    # vergleichen. Waere die Kodierung anders, aber gleichwertig, faende
    # der Oktettvergleich oben einen Unterschied und dieser hier nicht --
    # deshalb stehen beide da.
    if ea.data(".debug_line"):
        za = decoded_line(a_o, arch)
        zb = decoded_line(b_o, arch)
        ergebnis["dwarf_rows"] += len(za)
        if za != zb:
            ergebnis["ok"] = False
            msg = ["AUSGEWERTETE ZEILENTABELLE weicht ab (as: %d Zeilen, "
                   "Kodierer: %d Zeilen)" % (len(za), len(zb))]
            for i in range(max(len(za), len(zb))):
                x = za[i] if i < len(za) else None
                y = zb[i] if i < len(zb) else None
                if x != y:
                    msg.append("  Zeile %d:  as = %s" % (i, x))
                    msg.append("            eigen = %s" % (y,))
                    break
            ergebnis["fehler"].append("\n".join(msg))

    ga, gb = ea.global_defs(), eb.global_defs()
    if ga != gb:
        ergebnis["ok"] = False
        nur_a = {k: v for k, v in ga.items() if gb.get(k) != v}
        nur_b = {k: v for k, v in gb.items() if ga.get(k) != v}
        msg = ["GLOBALE SYMBOLE weichen ab (as: %d, Kodierer: %d)"
               % (len(ga), len(gb))]
        for k in sorted(nur_a)[:6]:
            msg.append("  as       : %s = %s" % (k, nur_a[k]))
        for k in sorted(nur_b)[:6]:
            msg.append("  Kodierer : %s = %s" % (k, nur_b[k]))
        ergebnis["fehler"].append("\n".join(msg))

    return ergebnis


def main():
    args = sys.argv[1:]
    arch = "x86"
    if "--a64" in args:
        arch = "a64"
        args.remove("--a64")
        global OBJDUMP
        OBJDUMP = "aarch64-linux-gnu-objdump"
    still = "--still" in args
    if still:
        args.remove("--still")
    if not args:
        print(__doc__)
        return 2
    firnc = args[0]
    dateien = args[1:]
    if dateien and dateien[0] == "--liste":
        dateien = [l.strip() for l in open(dateien[1]) if l.strip()]

    gesamt = {"dateien": 0, "gut": 0, "schlecht": 0, "bytes": 0, "relocs": 0,
              "dwarf_bytes": 0, "dwarf_rows": 0}
    with tempfile.TemporaryDirectory() as tmpd:
        for f in dateien:
            r = pruefe(firnc, f, tmpd, arch)
            gesamt["dateien"] += 1
            gesamt["bytes"] += r["bytes"]
            gesamt["relocs"] += r["relocs"]
            gesamt["dwarf_bytes"] += r["dwarf_bytes"]
            gesamt["dwarf_rows"] += r["dwarf_rows"]
            if r["ok"]:
                gesamt["gut"] += 1
            else:
                gesamt["schlecht"] += 1
                if not still:
                    print("=== %s ===" % f)
                    for e in r["fehler"]:
                        print(e)
                    print()
    print(json.dumps(gesamt))
    return 0 if gesamt["schlecht"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
