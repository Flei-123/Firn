import re
import sys
import os

src_root = sys.argv[1]
out_root = sys.argv[2]

text = open(os.path.join(src_root, "vpx_dsp/inv_txfm.c")).read()
text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
text = re.sub(r"//[^\n]*", "", text)
consts = open(os.path.join(src_root, "vpx_dsp/txfm_common.h")).read()


def body_of(name):
    m = re.search(r"\nvoid " + name + r"\(const tran_low_t \*input, tran_low_t \*output\) \{", text)
    assert m, name
    i = m.end()
    depth = 1
    j = i
    while depth:
        c = text[j]
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
        j += 1
    return text[i:j - 1]


def match_paren(s, i):
    depth = 0
    while True:
        if s[i] == "(":
            depth += 1
        elif s[i] == ")":
            depth -= 1
            if depth == 0:
                return i
        i += 1


def strip_calls(s, name, repl_open):
    while True:
        k = s.find(name + "(")
        if k < 0:
            return s
        o = k + len(name)
        c = match_paren(s, o)
        s = s[:k] + repl_open + s[o:c + 1] + s[c + 1:]


def expr(e):
    e = re.sub(r"\(tran_high_t\)|\(tran_low_t\)|\(int\)|\(int64_t\)|\(int16_t\)|\(int32_t\)", "", e)
    e = strip_calls(e, "WRAPLOW", "")
    e = strip_calls(e, "dct_const_round_shift", "rs")
    e = re.sub(r"input\[([^\]]+)\]", lambda mm: "vp9mem.ldi(inp, %d)" % eval(mm.group(1).replace("/", "//")), e)
    e = re.sub(r"\bcospi_(\d+)_64\b", r"COSPI_\1_64", e)
    e = re.sub(r"\bsinpi_(\d+)_9\b", r"SINPI_\1_9", e)
    e = re.sub(r"\s+", " ", e).strip()
    return e


def split_top(s, ch):
    out = []
    depth = 0
    cur = ""
    for c in s:
        if c in "([":
            depth += 1
        elif c in ")]":
            depth -= 1
        if c == ch and depth == 0:
            out.append(cur)
            cur = ""
        else:
            cur += c
    if cur.strip():
        out.append(cur)
    return out


def remove_if_blocks(b):
    while True:
        m = re.search(r"if \(!\(", b)
        if not m:
            return b
        o = b.index("{", m.start())
        c = b.index("}", o)
        b = b[:m.start()] + b[c + 1:]


def translate(name, fname):
    b = remove_if_blocks(body_of(name))
    b = b.replace("memset(output, 0, 4 * sizeof(*output));", "")
    out = []
    for stmt in split_top(b, ";"):
        st = stmt.strip()
        if not st:
            continue
        st = re.sub(r"\s+", " ", st)
        m = re.match(r"^(tran_high_t|tran_low_t|int|int16_t|int32_t|int64_t)\s+(.*)$", st)
        if m:
            for d in split_top(m.group(2), ","):
                d = d.strip()
                am = re.match(r"^(\w+)\[(\d+)\]$", d)
                if am:
                    out.append("    var %s: [i64; %s] = [0; %s]" % (am.group(1), am.group(2), am.group(2)))
                    continue
                em = re.match(r"^(\w+)\s*=\s*(.*)$", d)
                if em:
                    out.append("    var %s: i64 = %s" % (em.group(1), expr(em.group(2))))
                else:
                    out.append("    var %s: i64 = 0" % d)
            continue
        m = re.match(r"^output\[(\d+)\]\s*=\s*(.*)$", st)
        if m:
            out.append("    vp9mem.sti(out, %s, %s)" % (m.group(1), expr(m.group(2))))
            continue
        m = re.match(r"^([\w\[\]]+)\s*=\s*(.*)$", st)
        assert m, st
        out.append("    %s = %s" % (m.group(1), expr(m.group(2))))
    return "fn %s(inp: u64, out: u64) {\n%s\n}" % (fname, "\n".join(out))


head = "// SPDX-License-Identifier: MPL-2.0\n"
cs = []
for m in re.finditer(r"static const tran_coef_t (cospi_\d+_64|sinpi_\d+_9) = (\d+);", consts):
    cs.append("const %s: i64 = %s" % (m.group(1).upper(), m.group(2)))
names = re.findall(r"const (\w+):", "\n".join(cs))
os.makedirs(os.path.join(out_root, "media/video/vp9"), exist_ok=True)
open(os.path.join(out_root, "media/video/vp9/vp9cospi.fi"), "w").write(
    head + "export { %s }\n\n" % ", ".join(sorted(names)) + "\n".join(cs) + "\n")

groups = {
    "vp9i1d4": [("idct4_c", "idct4"), ("iadst4_c", "iadst4")],
    "vp9i1d8": [("idct8_c", "idct8"), ("iadst8_c", "iadst8")],
    "vp9i1d16": [("idct16_c", "idct16"), ("iadst16_c", "iadst16")],
    "vp9i1d32": [("idct32_c", "idct32")],
}
for mod, fns in groups.items():
    used = set()
    bodies = []
    for cname, fname in fns:
        t = translate(cname, fname)
        used |= set(re.findall(r"\b(?:COSPI_\d+_64|SINPI_\d+_9)\b", t))
        bodies.append(t)
    imports = "import media.video.vp9.vp9mem\n"
    if used:
        imports += "import media.video.vp9.vp9cospi\n"
    t = "\n\n".join(bodies)
    t = re.sub(r"\b((?:COSPI_\d+_64|SINPI_\d+_9))\b", r"vp9cospi.\1", t)
    t = t.replace("rs(", "vp9mem.rs(")
    exp = "export { %s }\n" % ", ".join(f for _, f in fns)
    open(os.path.join(out_root, "media/video/vp9/%s.fi" % mod), "w").write(head + imports + "\n" + exp + "\n" + t + "\n")
print("ok")
