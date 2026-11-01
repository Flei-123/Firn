import base64
import os
import re
import sys
import urllib.request

URL = "https://aomedia.googlesource.com/aom/+/refs/heads/main/av1/common/av1_inv_txfm1d.c?format=TEXT"
HDR = "// SPDX-License-Identifier: MPL-2.0\n"
MOD = "av1itxm"

GROUPS = [
    ("av1idct8", ["idct4", "idct8"]),
    ("av1idct16", ["idct16"]),
    ("av1idct32", ["idct32"]),
    ("av1idct64", ["idct64"]),
    ("av1iadst", ["iadst8", "iadst16"]),
]


def load_source(src):
    if src:
        return open(os.path.join(src, "av1_inv_txfm1d.c")).read()
    return base64.b64decode(urllib.request.urlopen(URL).read()).decode()


def load_cospi(outdir):
    text = open(os.path.join(outdir, "av1tabtx.fi")).read()
    body = text.split("COSPI_ARR: [i32; 256] = [")[1].split("]")[0]
    vals = [int(x) for x in re.findall(r"-?\d+", body)]
    assert len(vals) == 256
    return vals[128:192]


def split_functions(text):
    funcs = {}
    for m in re.finditer(r"^void av1_(\w+?)(?:_c)?\(", text, re.M):
        name = m.group(1)
        start = m.start()
        end = text.index("\n}\n", start)
        funcs[name] = text[start:end]
    return funcs


def ref(buf, base, idx):
    return "%s.li(%s, %d)" % (MOD, base[buf], int(idx))


def translate(name, body, cospi):
    base = {"0": "out", "1": "out"}
    lines = []

    def c(i, neg):
        v = cospi[int(i)]
        return -v if neg else v

    for raw in body.split("\n"):
        s = raw.strip()
        m = re.match(r"bf(\d) = (output|step);", s)
        if m:
            base[m.group(1)] = "out" if m.group(2) == "output" else "stp"
            continue
        m = re.match(r"bf(\d)\[(\d+)\] = input\[(\d+)\];", s)
        if m:
            lines.append("    %s.si(%s, %s, %s.li(inp, %s))" % (MOD, base[m.group(1)], m.group(2), MOD, m.group(3)))
            continue
        m = re.match(r"bf(\d)\[(\d+)\] = bf(\d)\[(\d+)\];", s)
        if m:
            lines.append("    %s.si(%s, %s, %s)" % (MOD, base[m.group(1)], m.group(2), ref(m.group(3), base, m.group(4))))
            continue
        m = re.match(r"bf(\d)\[(\d+)\] = -bf(\d)\[(\d+)\];", s)
        if m:
            lines.append("    %s.si(%s, %s, 0 - %s)" % (MOD, base[m.group(1)], m.group(2), ref(m.group(3), base, m.group(4))))
            continue
        m = re.match(r"bf(\d)\[(\d+)\] = clamp_value\((-?)bf(\d)\[(\d+)\] ([+-]) bf(\d)\[(\d+)\], stage_range\[stage\]\);", s)
        if m:
            a = ref(m.group(4), base, m.group(5))
            if m.group(3):
                a = "(0 - %s)" % a
            b = ref(m.group(7), base, m.group(8))
            lines.append("    %s.si(%s, %s, %s.cl(%s %s %s, r))" % (MOD, base[m.group(1)], m.group(2), MOD, a, m.group(6), b))
            continue
        m = re.match(r"bf(\d)\[(\d+)\] = half_btf\((-?)cospi\[(\d+)\], bf(\d)\[(\d+)\], (-?)cospi\[(\d+)\], bf(\d)\[(\d+)\], cos_bit\);", s)
        if m:
            w0 = c(m.group(4), m.group(3))
            w1 = c(m.group(8), m.group(7))
            a = ref(m.group(5), base, m.group(6))
            b = ref(m.group(9), base, m.group(10))
            lines.append("    %s.si(%s, %s, %s.btf(%d, %s, %d, %s))" % (MOD, base[m.group(1)], m.group(2), MOD, w0, a, w1, b))
            continue
        if re.match(r"bf\d\[\d+\] =", s) or "half_btf" in s or "clamp_value" in s:
            raise SystemExit("untranslated in %s: %s" % (name, s))
    out = ["fn %s(inp: u64, out: u64, stp: u64, r: i64) {" % name]
    out += lines
    out.append("}")
    return "\n".join(out) + "\n"


def main():
    outdir = sys.argv[1]
    src = None
    if "--src" in sys.argv:
        src = sys.argv[sys.argv.index("--src") + 1]
    cospi = load_cospi(outdir)
    funcs = split_functions(load_source(src))
    for fname, names in GROUPS:
        parts = [HDR, "import media.video.av1.%s\n" % MOD, "export { %s }\n" % ", ".join(names)]
        for n in names:
            parts.append(translate(n, funcs[n], cospi))
        with open(os.path.join(outdir, fname + ".fi"), "w") as f:
            f.write("\n".join(parts))


main()
