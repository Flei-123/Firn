import sys, subprocess, tempfile, os
from fontTools.ttLib import TTFont
from fontTools.pens.recordingPen import RecordingPen

def ref_glyphs(font):
    gs = font.getGlyphSet()
    out = []
    for name in font.getGlyphOrder():
        pen = RecordingPen()
        try:
            gs[name].draw(pen)
        except Exception:
            out.append(None)
            continue
        toks = []
        if any(op == "addComponent" for op, args in pen.value):
            out.append(None)
            continue
        for op, args in pen.value:
            if op == "moveTo":
                toks.append(("M", [args[0]]))
            elif op == "lineTo":
                toks.append(("L", [args[0]]))
            elif op == "curveTo":
                toks.append(("C", list(args)))
        out.append(toks)
    return out

def parse_dump(text):
    res = {}
    for line in text.splitlines():
        t = line.split()
        if not t or t[0][0] != "G":
            continue
        gid = int(t[0][1:])
        toks = []
        i = 1
        bad = False
        while i < len(t):
            if t[i] == "!":
                bad = True
                i += 1
                continue
            if t[i] == "M" or t[i] == "L":
                toks.append((t[i], [(int(t[i+1]), int(t[i+2]))]))
                i += 3
            elif t[i] == "C":
                v = [int(x) for x in t[i+1:i+7]]
                toks.append(("C", [(v[0], v[1]), (v[2], v[3]), (v[4], v[5])]))
                i += 7
            elif t[i] == "Z":
                i += 1
            else:
                i += 1
        res[gid] = (bad, toks)
    return res

def main():
    cffmain, otf = sys.argv[1], sys.argv[2]
    font = TTFont(otf)
    raw = font.getTableData("CFF ")
    d = tempfile.mkdtemp()
    p = os.path.join(d, "f.cff")
    open(p, "wb").write(raw)
    r = subprocess.run([cffmain, p], capture_output=True, text=True, timeout=300)
    got = parse_dump(r.stdout)
    ref = ref_glyphs(font)
    bad = 0
    for gid, toks in enumerate(ref):
        if toks is None:
            continue
        g = got.get(gid)
        if g is None:
            bad += 1
            print("missing", gid)
            continue
        failed, gt = g
        flat_r = []
        for k, pts in toks:
            for (x, y) in pts:
                flat_r.append((k, round(x * 100), round(y * 100)))
        flat_g = []
        for k, pts in gt:
            for (x, y) in pts:
                flat_g.append((k, x, y))
        ok = len(flat_r) == len(flat_g) and all(a[0] == b[0] and abs(a[1]-b[1]) <= 2 and abs(a[2]-b[2]) <= 2 for a, b in zip(flat_r, flat_g))
        if failed or not ok:
            bad += 1
            if bad <= 5:
                print("DIFF gid", gid, "failed" if failed else "", flat_r[:4], flat_g[:4])
    print("glyphs", len(ref), "bad", bad)
    return 1 if bad else 0

sys.exit(main())
