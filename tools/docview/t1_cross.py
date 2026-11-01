import sys, subprocess, tempfile, os, glob
from fontTools.t1Lib import T1Font
from fontTools.pens.recordingPen import RecordingPen

def ref(path):
    f = T1Font(path)
    f.parse()
    gs = f.getGlyphSet()
    out = {}
    for name in gs.keys():
        pen = RecordingPen()
        try:
            gs[name].draw(pen)
        except Exception:
            continue
        if any(op == "addComponent" for op, a in pen.value):
            continue
        toks = []
        for op, args in pen.value:
            if op == "moveTo":
                toks.append(("M", args[0]))
            elif op == "lineTo":
                toks.append(("L", args[0]))
            elif op == "curveTo":
                for a in args:
                    toks.append(("C", a))
        out[name] = toks
    return out

def parse(text):
    res = {}
    for line in text.splitlines():
        t = line.split()
        if len(t) < 2 or t[0][0] != "G":
            continue
        name = t[1]
        toks = []
        bad = False
        i = 2
        while i < len(t):
            if t[i] == "!":
                bad = True
                i += 1
            elif t[i] in ("M", "L"):
                toks.append((t[i], (int(t[i+1]), int(t[i+2]))))
                i += 3
            elif t[i] == "C":
                for k in range(3):
                    toks.append(("C", (int(t[i+1+2*k]), int(t[i+2+2*k]))))
                i += 7
            else:
                i += 1
        res[name] = (bad, toks)
    return res

total = bad = 0
for path in sys.argv[2:]:
    try:
        r = ref(path)
    except Exception as e:
        print("refskip", path, e)
        continue
    p = subprocess.run([sys.argv[1], path], capture_output=True, text=True, timeout=300)
    if p.returncode != 0:
        print("FAILRUN", path, p.returncode)
        bad += 1
        continue
    g = parse(p.stdout)
    for name, toks in r.items():
        total += 1
        got = g.get(name)
        if got is None:
            bad += 1
            print("missing", path, name)
            continue
        fl, gt = got
        a = [(k, round(x*100), round(y*100)) for k, (x, y) in toks]
        b = [(k, x, y) for k, (x, y) in gt]
        ok = len(a) == len(b) and all(u[0] == v[0] and abs(u[1]-v[1]) <= 2 and abs(u[2]-v[2]) <= 2 for u, v in zip(a, b))
        if fl or not ok:
            bad += 1
            if bad <= 6:
                print("DIFF", os.path.basename(path), name, a[:3], b[:3])
print("glyphs", total, "bad", bad)
sys.exit(1 if bad else 0)
