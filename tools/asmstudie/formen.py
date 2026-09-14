#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Zaehlt, welche Befehle mit welchen OPERANDENFORMEN Firn wirklich ausgibt.

Nicht geraten aus dem Quelltext der Codeerzeuger, sondern gemessen am
tatsaechlich erzeugten Assemblertext (tools/asmstudie/ernte/).

Eine "Form" ist Mnemonic + die Art jedes Operanden, nicht sein Wert:
  mov rax, qword ptr [rbp-8]   ->   mov r64, m64
  mov rax, 7                   ->   mov r64, imm
Damit wird aus einer Million Zeilen eine Liste, die man abarbeiten kann.
"""
import re, sys, os, collections, json

# ---------------------------------------------------------------- x86

X86_R64 = {"rax","rcx","rdx","rbx","rsp","rbp","rsi","rdi",
           "r8","r9","r10","r11","r12","r13","r14","r15"}
X86_R32 = {"eax","ecx","edx","ebx","esp","ebp","esi","edi",
           "r8d","r9d","r10d","r11d","r12d","r13d","r14d","r15d"}
X86_R16 = {"ax","cx","dx","bx","sp","bp","si","di",
           "r8w","r9w","r10w","r11w","r12w","r13w","r14w","r15w"}
X86_R8  = {"al","cl","dl","bl","spl","bpl","sil","dil",
           "r8b","r9b","r10b","r11b","r12b","r13b","r14b","r15b","ah","ch","dh","bh"}
X86_XMM = {f"xmm{i}" for i in range(16)}

def x86_op(o):
    o = o.strip()
    if not o:
        return None
    low = o.lower()
    # Speicheroperand mit Groessenwort
    m = re.match(r'^(byte|word|dword|qword|xmmword)\s+ptr\s*\[(.*)\]$', low)
    if m:
        size = {"byte":8,"word":16,"dword":32,"qword":64,"xmmword":128}[m.group(1)]
        return "m%d%s" % (size, _x86_mem(m.group(2)))
    if low.startswith("[") and low.endswith("]"):
        return "m?" + _x86_mem(low[1:-1])
    if low in X86_R64: return "r64"
    if low in X86_R32: return "r32"
    if low in X86_R16: return "r16"
    if low in X86_R8:  return "r8"
    if low in X86_XMM: return "xmm"
    if re.match(r'^-?(0x[0-9a-f]+|\d+)$', low):
        return "imm"
    if re.match(r'^[a-z_.$][\w.$@]*$', low, re.I):
        return "sym"      # Sprungmarke / Symbolname
    return "sonst:" + low

def _x86_mem(inner):
    """Klassifiziert die Adressform innerhalb der Klammern."""
    s = inner.strip()
    if s.startswith("rip"):
        return "(rip)"
    # Basis [+ Index*Skala] [+/- Verschiebung]
    hat_index = "*" in s
    m = re.search(r'([+-])\s*(0x[0-9a-f]+|\d+)\s*$', s)
    disp = None
    if m:
        v = int(m.group(2), 16) if m.group(2).startswith("0x") else int(m.group(2))
        if m.group(1) == "-": v = -v
        disp = v
    if disp is None:
        d = "d0"
    elif -128 <= disp <= 127:
        d = "d8"
    else:
        d = "d32"
    return "(b+i*s%s)" % d if hat_index else "(b%s)" % d

def zerlege_x86(zeile):
    z = zeile.split("#")[0].split("//")[0].strip()
    if not z or z.startswith(".") or z.endswith(":"):
        return None
    # Praefixe
    praefix = ""
    while True:
        m = re.match(r'^(lock|rep|repe|repne|repz|repnz)\s+', z, re.I)
        if not m: break
        praefix += m.group(1).lower() + " "
        z = z[m.end():]
    m = re.match(r'^([a-z][a-z0-9_.]*)\s*(.*)$', z, re.I)
    if not m: return None
    mn = m.group(1).lower()
    rest = m.group(2).strip()
    if not rest:
        return praefix + mn
    ops = []
    tiefe = 0; akt = ""
    for c in rest:
        if c == "[": tiefe += 1
        if c == "]": tiefe -= 1
        if c == "," and tiefe == 0:
            ops.append(akt); akt = ""
        else:
            akt += c
    ops.append(akt)
    kl = [x86_op(o) for o in ops]
    if any(k is None for k in kl): return None
    return praefix + mn + " " + ", ".join(kl)

# ---------------------------------------------------------------- a64

A64_X = {f"x{i}" for i in range(31)} | {"xzr","sp"}
A64_W = {f"w{i}" for i in range(31)} | {"wzr","wsp"}
A64_D = {f"d{i}" for i in range(32)}
A64_S = {f"s{i}" for i in range(32)}
A64_V = {f"v{i}" for i in range(32)}
A64_Q = {f"q{i}" for i in range(32)}

def a64_op(o):
    o = o.strip()
    if not o: return None
    low = o.lower()
    if low.startswith("[") :
        return "mem" + _a64_mem(low)
    if low in A64_X: return "Xn"
    if low in A64_W: return "Wn"
    if low in A64_D: return "Dn"
    if low in A64_S: return "Sn"
    if low in A64_Q: return "Qn"
    if re.match(r'^v\d+\.\w+$', low): return "Vn.T"
    if low.startswith("#"):
        return "#imm"
    # Schiebe-/Erweiterungsmodifikator
    if re.match(r'^(lsl|lsr|asr|ror|uxtb|uxth|uxtw|uxtx|sxtb|sxth|sxtw|sxtx)\b', low):
        return "shift"
    # Bedingung
    if low in {"eq","ne","cs","hs","cc","lo","mi","pl","vs","vc","hi","ls","ge","lt","gt","le","al"}:
        return "cond"
    if re.match(r'^:lo12:', low): return ":lo12:sym"
    if re.match(r'^:got(_lo12)?:', low): return ":got:sym"
    if re.match(r'^[a-z_.][\w.$@]*$', low, re.I): return "sym"
    return "sonst:" + low

def _a64_mem(s):
    s = s.strip()
    nach = ""
    if s.endswith("]!"):
        nach = "!"; s = s[:-2]
    elif s.endswith("]"):
        s = s[:-1]
    else:
        # Nachindizierung: [xN], #imm
        m = re.match(r'^\[([^\]]*)\]\s*,\s*(.*)$', s)
        if m:
            return "[b],#imm"
    inner = s.lstrip("[")
    teile = [t.strip() for t in inner.split(",")]
    if len(teile) == 1:
        return "[b]" + nach
    zweiter = teile[1]
    if zweiter.startswith("#"):
        return "[b,#imm]" + nach
    if re.match(r'^[wx]\d+$', zweiter):
        extra = ",shift" if len(teile) > 2 else ""
        return "[b,r%s]" % extra
    return "[b,?]"

def zerlege_a64(zeile):
    z = zeile.split("//")[0].split(";")[0].strip()
    if not z or z.startswith(".") or z.endswith(":"):
        return None
    m = re.match(r'^([a-z][a-z0-9_.]*)\s*(.*)$', z, re.I)
    if not m: return None
    mn = m.group(1).lower()
    rest = m.group(2).strip()
    if not rest: return mn
    ops = []; tiefe = 0; akt = ""
    for c in rest:
        if c == "[": tiefe += 1
        if c == "]": tiefe -= 1
        if c == "," and tiefe == 0:
            ops.append(akt); akt = ""
        else: akt += c
    ops.append(akt)
    kl = [a64_op(o) for o in ops]
    if any(k is None for k in kl): return None
    return mn + " " + ", ".join(kl)

# ---------------------------------------------------------------- Lauf

def lauf(verz, zerleger):
    formen = collections.Counter()
    mnemo = collections.Counter()
    unklar = collections.Counter()
    zeilen = 0
    for name in sorted(os.listdir(verz)):
        if not name.endswith(".s"): continue
        with open(os.path.join(verz, name), errors="replace") as fh:
            for zeile in fh:
                zeilen += 1
                f = zerleger(zeile)
                if f is None:
                    z = zeile.split("#")[0].strip()
                    if z and not z.startswith(".") and not z.endswith(":"):
                        unklar[z[:60]] += 1
                    continue
                formen[f] += 1
                mnemo[f.split()[0]] += 1
    return formen, mnemo, unklar, zeilen

if __name__ == "__main__":
    basis = os.path.dirname(os.path.abspath(__file__))
    ergebnis = {}
    for arch, zerleger in (("x86", zerlege_x86), ("a64", zerlege_a64)):
        verz = os.path.join(basis, "ernte", arch)
        formen, mnemo, unklar, zeilen = lauf(verz, zerleger)
        ergebnis[arch] = {
            "zeilen": zeilen,
            "formen": formen.most_common(),
            "mnemonics": mnemo.most_common(),
            "unklar": unklar.most_common(40),
        }
        print("=" * 70)
        print("%s  --  %d Zeilen gelesen, %d verschiedene Mnemonics, %d Formen"
              % (arch.upper(), zeilen, len(mnemo), len(formen)))
        print("=" * 70)
        for f, n in formen.most_common(1000):
            print("%9d  %s" % (n, f))
        if unklar:
            print("--- nicht zerlegt (Top 40) ---")
            for u, n in unklar.most_common(40):
                print("%9d  %s" % (n, u))
    with open(os.path.join(basis, "formen.json"), "w") as fh:
        json.dump(ergebnis, fh, indent=1)
