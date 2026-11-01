import re, sys, os

WT = sys.argv[1]
SRC = open(WT + "/lib/webp/webp.fi").read().split("\n")
OUT = WT + "/lib/media/video/vp8/"
os.makedirs(OUT, exist_ok=True)

START = re.compile(r'^(#\[|fn |static |struct |const |export|import|enum )')
items = []
cur = None
for i, line in enumerate(SRC, 1):
    if START.match(line):
        if cur is not None and cur["kind"] == "attr":
            cur["lines"].append(line)
            cur["kind"] = "item"
            cur["start"] = cur["start"]
            m = re.match(r'(?:fn|static mut|static|struct|const)\s+([A-Za-z_0-9]+)', line)
            cur["name"] = m.group(1) if m else None
            continue
        if cur is not None:
            items.append(cur)
        if line.startswith("#["):
            cur = {"kind": "attr", "lines": [line], "start": i, "name": None}
        else:
            m = re.match(r'(?:fn|static mut|static|struct|const)\s+([A-Za-z_0-9]+)', line)
            cur = {"kind": "item", "lines": [line], "start": i, "name": m.group(1) if m else None}
    elif cur is not None:
        cur["lines"].append(line)
if cur is not None:
    items.append(cur)

by_name = {it["name"]: it for it in items if it["name"]}

def clean(lines):
    out = []
    for l in lines:
        s = l.strip()
        if s.startswith("//"):
            continue
        if "//" in l and '"' not in l:
            l = re.sub(r'\s*//.*$', '', l)
        out.append(l.rstrip())
    while out and out[-1] == "":
        out.pop()
    return out

def in_range(it, a, b):
    return it["name"] and a <= it["start"] <= b

mods = {}
def add(mod, names=None, rng=None):
    sel = []
    for it in items:
        if not it["name"]:
            continue
        if names and it["name"] in names:
            sel.append(it)
        elif rng and in_range(it, rng[0], rng[1]):
            sel.append(it)
    mods.setdefault(mod, [])
    for it in sel:
        if it not in mods[mod]:
            mods[mod].append(it)

add("vp8mem", names=["ld8", "st8", "ld8o", "st8o", "ld32", "st32", "ldi", "sti", "alloc", "free", "zero", "clamp255", "iabs", "s16"])
add("vp8bool", rng=(1107, 1233))
add("vp8tok", names=["ZIGZAG", "BANDS", "CAT6", "read_large", "read_block"])
add("vp8idct", names=["walsh", "mul35468", "mul20091", "addpx", "idct_full", "idct_dc", "recon_block"])
add("vp8intra", rng=(1571, 1933))
add("vp8lf", rng=(1934, 2285))
add("vp8kfmode", rng=(1226, 1294))

owner = {}
for m, its in mods.items():
    for it in its:
        owner[it["name"]] = m

IDENT = re.compile(r'(?<![\.A-Za-z0-9_])([A-Za-z_][A-Za-z0-9_]*)\b(?!\s*:(?!:))')

def qualify(text, mod):
    def rep(mo):
        n = mo.group(1)
        if n in owner and owner[n] != mod:
            return owner[n] + "." + n
        return n
    return IDENT.sub(rep, text)

for m, its in mods.items():
    its.sort(key=lambda x: x["start"])
    body = []
    deps = set()
    for it in its:
        body.append("\n".join(clean(it["lines"])))
    text = "\n\n".join(body) + "\n"
    q = qualify(text, m)
    for n, o in owner.items():
        if o != m and (o + "." + n) in q:
            deps.add(o)
    names = [it["name"] for it in its]
    head = ["// SPDX-License-Identifier: MPL-2.0"]
    if m == "vp8mem":
        head.append("import std.rt")
    for d in sorted(deps):
        head.append("import media.video.vp8." + d)
    head.append("")
    exp = "export { " + ", ".join(sorted(names)) + " }"
    open(OUT + m + ".fi", "w").write("\n".join(head) + "\n" + exp + "\n\n" + q)
    print(m, len(q.split("\n")), "deps", sorted(deps))
