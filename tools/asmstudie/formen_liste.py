#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""
Aus formen.json konkrete TESTFAELLE machen.

Eine "Form" wie `mov r64, m64(bd8)` ist eine Schablone. Zum Pruefen gegen GNU as
brauche ich echten Text. Diese Datei setzt fuer jede gemessene Form konkrete
Operanden ein -- und zwar MEHRERE Registerwahlen pro Form, weil genau dort die
Fehler sitzen: r8..r15 brauchen REX, rsp/rbp brauchen SIB bzw. erzwungenes disp,
und bei 8-Bit-Registern aendert REX die Bedeutung der Nummern 4..7.

Ausgabe: eine Liste von (form, text) -- Text in AT&T-freier Intel-Syntax,
so wie Firns Codegen ihn schreibt.
"""
import json, os, itertools

HIER = os.path.dirname(os.path.abspath(__file__))

# --- Registerwahlen, die die Kodierung wirklich belasten ---------------------
# low  : keine REX noetig
# high : braucht REX.B/R/X
# rsp  : index=4 -> SIB erzwungen
# rbp  : base=5  -> disp erzwungen (bd0 wird zu disp8)
# r12  : wie rsp, aber zusaetzlich REX.B
# r13  : wie rbp, aber zusaetzlich REX.B
R64 = ["rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi",
       "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15"]
R32 = ["eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi",
       "r8d", "r9d", "r10d", "r11d", "r12d", "r13d", "r14d", "r15d"]
R16 = ["ax", "cx", "dx", "bx", "sp", "bp", "si", "di",
       "r8w", "r9w", "r10w", "r11w", "r12w", "r13w", "r14w", "r15w"]
# 8-Bit: spl/bpl/sil/dil gibt es nur MIT REX -- das ist eine eigene Fehlerquelle
R8 = ["al", "cl", "dl", "bl", "spl", "bpl", "sil", "dil",
      "r8b", "r9b", "r10b", "r11b", "r12b", "r13b", "r14b", "r15b"]
XMM = ["xmm%d" % i for i in range(16)]

# Auswahl pro Rolle: ich nehme nicht alle 16x16, das waere Zahlenhuberei.
# Ich nehme die GEFAEHRLICHEN plus ein paar harmlose.
SEL64 = ["rax", "rcx", "rsp", "rbp", "rsi", "r8", "r12", "r13", "r15"]
SEL32 = ["eax", "ecx", "esp", "ebp", "esi", "r8d", "r12d", "r13d", "r15d"]
SEL16 = ["ax", "cx", "sp", "bp", "r8w", "r12w", "r15w"]
SEL8 = ["al", "cl", "dl", "bl", "spl", "bpl", "sil", "dil", "r8b", "r12b", "r15b"]
SELX = ["xmm0", "xmm1", "xmm7", "xmm8", "xmm15"]

# Basisregister fuer Speicheroperanden -- rsp/rbp/r12/r13 sind die Sonderfaelle
SELBASE = ["rax", "rcx", "rsp", "rbp", "rsi", "r8", "r12", "r13", "r15"]
SELIDX = ["rax", "rcx", "rdx", "rbp", "r8", "r12", "r15"]  # rsp geht NICHT als Index

IMM8 = ["0", "1", "127", "-1", "-128"]
IMM32 = ["128", "1000", "-129", "2147483647", "-2147483648"]
IMM64 = ["4294967296", "-4294967297", "1152921504606846976"]

GROESSE = {"m8": "byte ptr", "m16": "word ptr", "m32": "dword ptr", "m64": "qword ptr"}


def mem_varianten(mgr, adr, begrenzt=False):
    """Konkrete Speicheroperanden fuer eine gemessene Adressform."""
    p = GROESSE.get(mgr, "qword ptr")
    out = []
    if adr == "bd0":
        for b in (SELBASE[:4] if begrenzt else SELBASE):
            out.append("%s [%s]" % (p, b))
    elif adr == "bd8":
        for b in (SELBASE[:4] if begrenzt else SELBASE):
            for d in ("8", "-8", "127", "-128"):
                out.append("%s [%s%s%s]" % (p, b, "+" if not d.startswith("-") else "", d))
    elif adr == "bd32":
        for b in (SELBASE[:4] if begrenzt else SELBASE):
            for d in ("128", "-129", "100000"):
                out.append("%s [%s%s%s]" % (p, b, "+" if not d.startswith("-") else "", d))
    elif adr == "b+i*sd0":
        for b in (SELBASE[:4] if begrenzt else SELBASE):
            for i in (SELIDX[:3] if begrenzt else SELIDX):
                for s in ("1", "2", "4", "8"):
                    out.append("%s [%s+%s*%s]" % (p, b, i, s))
    elif adr == "rip":
        out.append("%s [rip+markeA]" % p)
    return out


def reg_liste(art, begrenzt=False):
    if art == "r64":
        return SEL64[:4] if begrenzt else SEL64
    if art == "r32":
        return SEL32[:4] if begrenzt else SEL32
    if art == "r16":
        return SEL16[:3] if begrenzt else SEL16
    if art == "r8":
        return SEL8[:5] if begrenzt else SEL8
    if art == "xmm":
        return SELX[:3] if begrenzt else SELX
    return []


def imm_liste(mnem, groesse):
    """Welche Sofortwerte sind fuer diese Breite sinnvoll?"""
    # Schiebeweiten sind KEINE beliebigen Zahlen: die Architektur nimmt nur
    # 0..63 (64 Bit) bzw. 0..31 (32 Bit). Groessere Werte lehnt as ab -- dann
    # haette ich eine Form ohne einen einzigen gueltigen Testfall.
    if mnem in ("shl", "shr", "sar", "sal", "rol", "ror", "rcl", "rcr"):
        if groesse == "r64" or groesse == "m64":
            return ["0", "1", "2", "7", "31", "32", "63"]
        if groesse in ("r32", "m32"):
            return ["0", "1", "2", "7", "31"]
        if groesse in ("r16", "m16"):
            return ["0", "1", "7", "15"]
        return ["0", "1", "7"]
    if mnem == "tbz":
        return ["0", "7", "31", "63"]
    if groesse == "r8" or groesse == "m8":
        return IMM8
    if groesse == "r16" or groesse == "m16":
        return ["0", "1", "127", "-128", "1000", "-1000"]
    if groesse == "r64" and mnem == "mov":
        # mov r64,imm ist der Sonderfall: passt es in 32 Bit -> B8+r, sonst REX.W B8+r imm64
        return IMM8 + IMM32 + IMM64
    return IMM8 + IMM32


def x86_faelle(formen):
    """Erzeugt (form, asmtext) fuer jede x86-Form."""
    faelle = []
    for form in formen:
        mnem = form.split()[0]
        rest = form[len(mnem):].strip()
        if mnem == "lock":
            # z.B. "lock xadd m64(bd0), r64"
            teile = form.split(None, 2)
            mnem = teile[0] + " " + teile[1]
            rest = teile[2] if len(teile) > 2 else ""
        ops = [o.strip() for o in rest.split(",")] if rest else []
        if any(o.startswith("sonst:") for o in ops):
            continue  # handgeschriebene Sonderzeilen (fs:0, lokale Marken) -- nicht Teil der Kodierbibliothek
        kand = []
        ok = True
        for o in ops:
            if o in ("r64", "r32", "r16", "r8", "xmm"):
                kand.append(reg_liste(o, begrenzt=len(ops) > 2))
            elif o == "imm":
                breite = next((x for x in ops if x.startswith(("r", "m")) and x != "imm"), "r64")
                breite = breite.split("(")[0]
                kand.append(imm_liste(mnem, breite))
            elif o == "sym":
                kand.append(["markeA"])
            elif o.startswith("m") and "(" in o:
                mgr = o.split("(")[0]
                adr = o.split("(")[1].rstrip(")")
                v = mem_varianten(mgr, adr, begrenzt=len(ops) > 1)
                if not v:
                    ok = False
                    break
                kand.append(v)
            elif o.startswith("m?"):
                adr = o.split("(")[1].rstrip(")")
                v = mem_varianten("m64", adr, begrenzt=len(ops) > 1)
                # lea hat keinen Groessen-Praefix
                v = [x.split("ptr", 1)[1].strip() for x in v]
                if not v:
                    ok = False
                    break
                kand.append(v)
            else:
                ok = False
                break
        if not ok:
            continue
        if not kand:
            faelle.append((form, mnem))
            continue
        for kombi in itertools.product(*kand):
            faelle.append((form, "%s %s" % (mnem, ", ".join(kombi))))
    return faelle


# ---------------------------------------------------------------- ARM64 -----
X = ["x0", "x1", "x2", "x9", "x15", "x16", "x28", "xzr"]
W = ["w0", "w1", "w2", "w9", "w15", "w28", "wzr"]
D = ["d0", "d1", "d7", "d15", "d31"]
S = ["s0", "s1", "s7", "s15", "s31"]
COND = ["eq", "ne", "cs", "cc", "mi", "pl", "vs", "vc",
        "hi", "ls", "ge", "lt", "gt", "le"]


def a64_regs(art):
    return {"Xn": X, "Wn": W, "Dn": D, "Sn": S}.get(art, [])


def a64_faelle(formen):
    faelle = []
    for form in formen:
        mnem = form.split()[0]
        rest = form[len(mnem):].strip()
        ops = [o.strip() for o in rest.split(",")] if rest else []
        # Speicheroperanden stehen als mem[...] evtl. mit Komma drin -> neu zusammensetzen
        neu, puffer = [], ""
        tiefe = 0
        for o in ops:
            tiefe += o.count("[") - o.count("]")
            puffer = o if not puffer else puffer + "," + o
            if tiefe == 0:
                neu.append(puffer)
                puffer = ""
        ops = neu
        kand = []
        ok = True
        breite = "x"
        for o in ops:
            if o in ("Xn", "Wn", "Dn", "Sn"):
                if o == "Wn":
                    breite = "w"
                kand.append(a64_regs(o)[:4] if len(ops) > 2 else a64_regs(o))
            elif o == "#imm":
                if mnem in ("movz", "movk", "movn"):
                    kand.append(["0", "1", "65535", "4660"])
                elif mnem in ("svc", "brk"):
                    kand.append(["0", "1", "65535"])
                elif mnem in ("and", "orr", "eor", "tst"):
                    # Bitmuster-Konstanten: NICHT jede Zahl ist gueltig
                    kand.append(["1", "3", "255", "4095"] if breite == "w" else
                                ["1", "3", "255", "4095", "-1" if False else "1152921504606846976"])
                elif mnem == "tbz":
                    kand.append(["0", "7", "31", "63"])
                else:
                    kand.append(["0", "1", "255", "4095"])
            elif o == "sym":
                kand.append(["markeA"])
            elif o == "cond":
                kand.append(COND)
            elif o == "shift":
                kand.append(["lsl #16", "lsl #32", "lsl #48"] if mnem in ("movz", "movk", "movn")
                            else ["lsl #1", "lsl #3", "uxtw"] if mnem == "add" else ["lsl #2"])
            elif o.startswith("mem["):
                inner = o[4:].rstrip("]")
                skal = {"ldr": 8, "str": 8, "ldrb": 1, "strb": 1, "ldrh": 2, "strh": 2,
                        "ldrsw": 4, "ldrsh": 2, "ldrsb": 1, "ldp": 8, "stp": 8}.get(mnem, 8)
                if breite == "w" and mnem in ("ldr", "str"):
                    skal = 4
                if inner == "b":
                    kand.append(["[%s]" % b for b in ("x0", "x1", "sp", "x28")])
                elif inner == "b,#imm":
                    v = []
                    for b in ("x0", "x1", "sp", "x28"):
                        for m in (0, 1, 2, 15):
                            v.append("[%s, #%d]" % (b, m * skal))
                    kand.append(v)
                elif inner == "b,#imm]!" or inner.endswith("]!"):
                    # Anpassung VOR dem Zugriff: `stp x29, x30, [sp, #-16]!`
                    # Das ist der Rahmenaufbau jeder Funktion -- 7744-mal
                    # in der Ernte, also alles andere als ein Randfall.
                    skal2 = 8 if mnem in ("ldp", "stp") else 1
                    kand.append(["[%s, #%d]!" % (b, m * skal2)
                                 for b in ("x0", "sp", "x28")
                                 for m in (0, -1, -2, -8, 7)])
                elif inner == "b,r,shift":
                    kand.append(["[x0, x1, lsl #3]", "[x28, x9, lsl #3]", "[x0, x1]"])
                else:
                    ok = False
                    break
            elif o.startswith("mem[b]"):
                # Nachtraegliche Anpassung: `ldrb w0, [x0], #1`. Der Versatz
                # steht NACH der Klammer und ist NICHT skaliert -- er zaehlt
                # in Oktetten, anders als beim unsigned offset.
                skal = {"ldp": 8, "stp": 8}.get(mnem, 1)
                if mnem in ("ldp", "stp"):
                    kand.append(["[%s], #%d" % (b, m * skal)
                                 for b in ("x0", "x1", "sp", "x28")
                                 for m in (0, 1, 2, -8)])
                else:
                    kand.append(["[%s], #%d" % (b, m)
                                 for b in ("x0", "x1", "sp", "x28")
                                 for m in (0, 1, 8, -8, 255)])
            elif o == ":lo12:sym":
                kand.append([":lo12:markeA"])
            else:
                ok = False
                break
        if not ok:
            continue
        if not kand:
            faelle.append((form, mnem))
            continue
        for kombi in itertools.product(*kand):
            # shift haengt an den vorigen Operanden ohne eigenes Komma? Nein -- as
            # will "movz x0, #1, lsl #16" mit Komma. Passt.
            faelle.append((form, "%s %s" % (mnem, ", ".join(kombi))))
    return faelle


def laden():
    with open(os.path.join(HIER, "formen.json")) as f:
        return json.load(f)


if __name__ == "__main__":
    d = laden()
    fx = x86_faelle([f for f, _ in d["x86"]["formen"]])
    fa = a64_faelle([f for f, _ in d["a64"]["formen"]])
    print("x86: %d Formen -> %d konkrete Faelle" % (len(d["x86"]["formen"]), len(fx)))
    print("a64: %d Formen -> %d konkrete Faelle" % (len(d["a64"]["formen"]), len(fa)))
    print("\n--- Beispiele x86 ---")
    for f, t in fx[:15]:
        print("  %-24s %s" % (f, t))
    print("\n--- Beispiele a64 ---")
    for f, t in fa[:15]:
        print("  %-24s %s" % (f, t))
