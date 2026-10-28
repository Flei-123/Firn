#!/usr/bin/env python3
"""ROUND KODIERER -- the counter check: own encoder against GNU as, octet by octet.

The idea: the old way sends the same assembler text through `as` and gets the
RIGHT octets. So we let both ways loose on the same input and compare.

What is compared
  * the content of .text, .data and .rodata, octet by octet,
  * the set of relocations (offset, symbol name, kind, addend) per section,
  * the defined global symbols with their value.

For x86-64 the line table (.debug_line, .debug_info, .debug_abbrev,
.debug_aranges) is compared too; .debug_str differs on purpose (producer).

On a deviation the instruction where it begins is reported -- with expected and
actual encoding and the nearest label, so that the place in the source can be
found.

Usage:
    compare.py <firnc> <file.s> [<file.s> ...]
    compare.py <firnc> --list <file-with-paths>
"""
import os, re, subprocess, sys, tempfile, json

COMPARED_SECTIONS = [".text", ".data", ".rodata", ".bss"]
DEBUG_SECTIONS = [".debug_line", ".debug_info", ".debug_abbrev", ".debug_aranges"]


# --------------------------------------------------------------------------
# Reading ELF (only as much as the comparison needs)
# --------------------------------------------------------------------------

def u(b, off, n):
    return int.from_bytes(b[off:off + n], "little")


class Elf:
    def __init__(self, raw):
        self.raw = raw
        if raw[:4] != b"\x7fELF":
            raise ValueError("not an ELF file")
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
        """The relocations of a section as a set, symbolic."""
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
        """Defined global symbols: name -> (section name, value)."""
        out = {}
        for s in self.symbols():
            if s["bind"] == 1 and s["type"] != 3 and s["shndx"] not in (0, 0xFFF1):
                nm = self.sh[s["shndx"]]["name"] if s["shndx"] < len(self.sh) else "?"
                out[s["name"]] = (nm, s["value"])
        return out


# --------------------------------------------------------------------------
# Diagnosis: which instruction stands at this place?
# --------------------------------------------------------------------------

OBJDUMP = "objdump"


def disasm_at(path, secname, offset, context=3):
    """Returns the disassembly around `offset` (list of lines)."""
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
    lo = max(0, idx - context)
    hi = min(len(rows), idx + context + 1)
    return ["%s %06x: %-28s %s" % ("->" if i == idx else "  ", rows[i][0],
                                   rows[i][1], rows[i][2])
            for i in range(lo, hi)]


def label_before(path, secname, offset):
    """The name of the last symbol before `offset` -- to find it again in the .s."""
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
    return "%s + %d" % (best[0], offset - best[1]) if best else "(no label)"


# --------------------------------------------------------------------------
# Check one file
# --------------------------------------------------------------------------

def check(firnc, s_path, tmpd, arch="x86"):
    result = {"file": s_path, "ok": True, "errors": [], "bytes": 0, "relocs": 0}
    a_o = os.path.join(tmpd, "as.o")
    b_o = os.path.join(tmpd, "internal.o")
    for p in (a_o, b_o):
        if os.path.exists(p):
            os.unlink(p)
    if arch == "x86":
        as_cmd = ["as", "--64", "-o", a_o, s_path]
        fc_cmd = [firnc, "--asm-intern", "--object-only", "-o", b_o, s_path]
    else:
        as_cmd = ["aarch64-linux-gnu-as", "-o", a_o, s_path]
        fc_cmd = [firnc, "--target=aarch64-linux", "--asm-intern",
                  "--object-only", "-o", b_o, s_path]

    r = subprocess.run(as_cmd, capture_output=True, text=True)
    if r.returncode != 0:
        result["ok"] = False
        result["errors"].append("as failed: " + r.stderr.strip()[:300])
        return result
    r = subprocess.run(fc_cmd, capture_output=True, text=True)
    if r.returncode != 0:
        result["ok"] = False
        result["errors"].append("encoder failed: "
                                + (r.stderr.strip() or r.stdout.strip())[:400])
        return result

    try:
        ea = Elf(open(a_o, "rb").read())
        eb = Elf(open(b_o, "rb").read())
    except Exception as ex:
        result["ok"] = False
        result["errors"].append("ELF unreadable: %s" % ex)
        return result

    for sec in COMPARED_SECTIONS + (DEBUG_SECTIONS if arch == "x86" else []):
        da, db = ea.data(sec), eb.data(sec)
        result["bytes"] += len(da)
        if da != db:
            result["ok"] = False
            n = min(len(da), len(db))
            i = 0
            while i < n and da[i] == db[i]:
                i += 1
            msg = ["SECTION %s deviates at offset 0x%x (as: %d octets, "
                   "encoder: %d octets)" % (sec, i, len(da), len(db)),
                   "  place: %s" % label_before(a_o, sec, i),
                   "  as      : " + da[i:i + 16].hex(" "),
                   "  encoder : " + db[i:i + 16].hex(" ")]
            msg.append("  --- as ---")
            msg += ["    " + x for x in disasm_at(a_o, sec, i)]
            msg.append("  --- encoder ---")
            msg += ["    " + x for x in disasm_at(b_o, sec, i)]
            result["errors"].append("\n".join(msg))

        ra = {r for r in ea.relocs(sec) if r[1] != ".debug_str"}
        rb = {r for r in eb.relocs(sec) if r[1] != ".debug_str"}
        result["relocs"] += len(ra)
        if ra != rb:
            result["ok"] = False
            missing = sorted(ra - rb)[:6]
            extra = sorted(rb - ra)[:6]
            msg = ["RELOCATIONS in %s deviate (as: %d, encoder: %d)"
                   % (sec, len(ra), len(rb))]
            for f in missing:
                msg.append("  missing : off=0x%x sym=%s kind=%d addend=%d" % f)
            for f in extra:
                msg.append("  extra   : off=0x%x sym=%s kind=%d addend=%d" % f)
            result["errors"].append("\n".join(msg))

    ga, gb = ea.global_defs(), eb.global_defs()
    if ga != gb:
        result["ok"] = False
        only_a = {k: v for k, v in ga.items() if gb.get(k) != v}
        only_b = {k: v for k, v in gb.items() if ga.get(k) != v}
        msg = ["GLOBAL SYMBOLS deviate (as: %d, encoder: %d)"
               % (len(ga), len(gb))]
        for k in sorted(only_a)[:6]:
            msg.append("  as      : %s = %s" % (k, only_a[k]))
        for k in sorted(only_b)[:6]:
            msg.append("  encoder : %s = %s" % (k, only_b[k]))
        result["errors"].append("\n".join(msg))

    return result


def main():
    args = sys.argv[1:]
    arch = "x86"
    if "--a64" in args:
        arch = "a64"
        args.remove("--a64")
        global OBJDUMP
        OBJDUMP = "aarch64-linux-gnu-objdump"
    quiet = "--quiet" in args
    if quiet:
        args.remove("--quiet")
    if not args:
        print(__doc__)
        return 2
    firnc = args[0]
    files = args[1:]
    if files and files[0] == "--list":
        files = [l.strip() for l in open(files[1]) if l.strip()]

    total = {"files": 0, "good": 0, "bad": 0, "bytes": 0, "relocs": 0}
    with tempfile.TemporaryDirectory() as tmpd:
        for f in files:
            r = check(firnc, f, tmpd, arch)
            total["files"] += 1
            total["bytes"] += r["bytes"]
            total["relocs"] += r["relocs"]
            if r["ok"]:
                total["good"] += 1
            else:
                total["bad"] += 1
                if not quiet:
                    print("=== %s ===" % f)
                    for e in r["errors"]:
                        print(e)
                    print()
    print(json.dumps(total))
    return 0 if total["bad"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
