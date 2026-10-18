#!/usr/bin/env python3
# tools/fui/lay_ref.py -- reference table for lib/fui/lay.fi.
#
# Builds flexbox cases (hand-written + seeded random), lets REAL Chromium
# (headless) lay them out, and writes the measured rectangles to
# tools/fui/lay_cases.txt. tools/fui/lay_main.fi builds the same trees with
# lay.fi / scene.fi and compares: 0 px deviation. Only cases whose Chromium
# result is whole pixels are kept (flex.fi is integer arithmetic).
#
# Usage: python3 tools/fui/lay_ref.py [out.txt]      (needs /usr/bin/chromium)
import json, os, random, subprocess, sys, tempfile

OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(os.path.abspath(__file__)), "lay_cases.txt")
CHROME = os.environ.get("CHROMIUM", "/usr/bin/chromium")

JU = ["flex-start", "flex-end", "center", "space-between", "space-around", "space-evenly"]  # flex.JUSTIFY_*
AL = {0: "auto", 1: "flex-start", 2: "flex-end", 3: "center", 4: "stretch"}                # flex.AL_*
FIELDS = ["parent", "dir", "ju", "ai", "wrap", "gx", "gy", "pl", "pt", "pr", "pb", "w", "h", "grow", "shrink",
          "basis", "minw", "maxw", "minh", "maxh", "self", "ml", "mt", "mr", "mb"]

def node(parent=0, dir=0, ju=0, ai=4, wrap=0, gx=0, gy=0, pad=(0, 0, 0, 0), w=-1, h=-1, grow=0, shrink=1000,
         basis=-1, minw=0, maxw=0, minh=0, maxh=0, self_=0, mg=(0, 0, 0, 0)):
    return dict(parent=parent, dir=dir, ju=ju, ai=ai, wrap=wrap, gx=gx, gy=gy, pl=pad[0], pt=pad[1], pr=pad[2],
                pb=pad[3], w=w, h=h, grow=grow, shrink=shrink, basis=basis, minw=minw, maxw=maxw, minh=minh,
                maxh=maxh, self=self_, ml=mg[0], mt=mg[1], mr=mg[2], mb=mg[3])

def css(n, root):
    s = ["box-sizing:border-box", "display:flex", "flex-direction:%s" % ("row" if n["dir"] == 0 else "column"),
         "justify-content:%s" % JU[n["ju"]], "align-items:%s" % AL[n["ai"]],
         "flex-wrap:%s" % ("wrap" if n["wrap"] else "nowrap"), "column-gap:%dpx" % n["gx"], "row-gap:%dpx" % n["gy"],
         "padding:%dpx %dpx %dpx %dpx" % (n["pt"], n["pr"], n["pb"], n["pl"]),
         "margin:%dpx %dpx %dpx %dpx" % (n["mt"], n["mr"], n["mb"], n["ml"]), "overflow:visible"]
    if n["w"] >= 0: s.append("width:%dpx" % n["w"])
    if n["h"] >= 0: s.append("height:%dpx" % n["h"])
    if not root:
        s.append("flex-grow:%s" % (n["grow"] / 1000)); s.append("flex-shrink:%s" % (n["shrink"] / 1000))
        s.append("flex-basis:%s" % ("auto" if n["basis"] < 0 else "%dpx" % n["basis"]))
        if n["self"]: s.append("align-self:%s" % AL[n["self"]])
    if n["minw"]: s.append("min-width:%dpx" % n["minw"])
    if n["maxw"]: s.append("max-width:%dpx" % n["maxw"])
    if n["minh"]: s.append("min-height:%dpx" % n["minh"])
    if n["maxh"]: s.append("max-height:%dpx" % n["maxh"])
    return ";".join(s)

def html_case(ci, nodes):
    kids = {}
    for i, n in enumerate(nodes):
        if i: kids.setdefault(n["parent"], []).append(i)
    def emit(i):
        n = nodes[i]
        inner = "".join(emit(k) for k in kids.get(i, []))
        st = css(n, i == 0)
        if i == 0: st += ";position:absolute;left:0;top:0"
        return '<div data-c="%d" data-n="%d" style="%s">%s</div>' % (ci, i, st, inner)
    return '<div class="case" style="position:relative;width:0;height:0">' + emit(0) + "</div>"

def rand_node(r, parent, depth, root=False):
    dirv = r.randint(0, 1)
    n = node(parent=parent, dir=dirv, ju=r.randint(0, 5), ai=r.choice([1, 2, 3, 4, 4, 4]))
    return n

def gen_random(r):
    nodes = []
    root = node(parent=-1, dir=r.randint(0, 1), ju=r.randint(0, 5), ai=r.choice([1, 2, 3, 4, 4]),
                gx=r.choice([0, 0, 4, 8, 10]), gy=r.choice([0, 0, 4, 8, 10]),
                pad=tuple(r.choice([0, 0, 4, 8, 16]) for _ in range(4)),
                w=r.choice([200, 300, 360, 400]), h=r.choice([100, 200, 300, 480]))
    if r.random() < 0.15 and root["dir"] == 0: root["wrap"] = 1  # column wrap is not supported
    nodes.append(root)
    nk = r.randint(1, 6)
    for _ in range(nk):
        parent = 0
        mainw = r.choice([20, 40, 60, 80, 100, 120])
        n = node(parent=parent, w=r.choice([-1, mainw, mainw]), h=r.choice([-1, 20, 30, 40, 50]),
                 grow=r.choice([0, 0, 0, 1000, 2000]), shrink=r.choice([1000, 1000, 0, 2000]),
                 basis=r.choice([-1, -1, -1, 50, 70]), self_=r.choice([0, 0, 0, 1, 2, 3, 4]),
                 mg=tuple(r.choice([0, 0, 0, 4, 8]) for _ in range(4)),
                 minw=r.choice([0, 0, 0, 30]), maxw=r.choice([0, 0, 0, 90]),
                 minh=r.choice([0, 0, 0, 25]), maxh=r.choice([0, 0, 0, 35]))
        # margins + shrink differ (documented); keep shrink at the default there
        if any((n["ml"], n["mt"], n["mr"], n["mb"])): n["shrink"] = 1000
        nodes.append(n)
        # nested box with 1-3 kids
        if r.random() < 0.25:
            idx = len(nodes) - 1
            nodes[idx]["dir"] = r.randint(0, 1)
            nodes[idx]["ju"] = r.randint(0, 5)
            nodes[idx]["ai"] = r.choice([1, 3, 4])
            nodes[idx]["gx"] = r.choice([0, 4]); nodes[idx]["gy"] = r.choice([0, 4])
            for _ in range(r.randint(1, 3)):
                nodes.append(node(parent=idx, w=r.choice([10, 20, 30]), h=r.choice([10, 20]),
                                  grow=r.choice([0, 0, 1000])))
    return nodes

def hand_cases():
    c = []
    def root(**kw):
        d = dict(parent=-1, w=300, h=100); d.update(kw); return node(**d)
    def kid(**kw): return node(**kw)
    # row, three fixed buttons, gap, padding
    c.append([root(gx=12, pad=(10, 8, 10, 8), ai=4), kid(w=60, h=30), kid(w=80, h=30), kid(w=40, h=30)])
    # entry takes the rest
    c.append([root(ai=3), kid(w=60, h=30), kid(w=-1, h=30, grow=1000, basis=0), kid(w=50, h=30)])
    # column: default CSS (start, stretch)
    c.append([root(dir=1, h=200), kid(h=40), kid(h=50, w=100), kid(h=30)])
    # justify variants (12 px free per item distribution)
    for ju in range(6):
        c.append([root(ju=ju, w=300, h=60), kid(w=40, h=20), kid(w=40, h=20), kid(w=40, h=20)])
    # align-self
    c.append([root(ai=1, w=200, h=100), kid(w=40, h=20, self_=2), kid(w=40, h=20, self_=3), kid(w=40, h=20, self_=4),
              kid(w=40, h=20)])
    # shrink with min-content of fixed items (min(content, fixed) = 0)
    c.append([root(w=100), kid(w=80, h=20), kid(w=80, h=20)])
    # shrink 0
    c.append([root(w=100), kid(w=80, h=20, shrink=0), kid(w=80, h=20)])
    # clamping redistribution (max 40 frees space for the other)
    c.append([root(w=300), kid(w=50, h=20, grow=1000, maxw=100), kid(w=50, h=20, grow=1000)])
    c.append([root(w=300), kid(w=50, h=20, grow=1000, maxw=60), kid(w=50, h=20, grow=1000), kid(w=50, h=20, grow=1000)])
    # min wins over shrink
    c.append([root(w=150), kid(w=100, h=20, minw=90), kid(w=100, h=20)])
    # margins
    c.append([root(w=300, h=100), kid(w=50, h=30, mg=(10, 5, 10, 5)), kid(w=50, h=30, mg=(0, 20, 0, 0))])
    c.append([root(w=300, h=100, dir=1), kid(h=30, mg=(10, 5, 10, 5)), kid(h=30, w=100, mg=(7, 0, 0, 0))])
    c.append([root(w=300, h=100), kid(w=50, h=30, grow=1000, mg=(10, 0, 10, 0)), kid(w=50, h=30, grow=1000)])
    # wrap
    c.append([root(w=200, h=200, wrap=1, gx=10, gy=6, ai=1), kid(w=80, h=30), kid(w=80, h=30), kid(w=80, h=40),
              kid(w=80, h=30)])
    c.append([root(w=200, h=200, wrap=1, gx=10, gy=6, ai=4), kid(w=80, h=30), kid(w=80), kid(w=100, h=40)])
    # card: column in column, padding, stretch
    c.append([root(dir=1, w=360, h=400, pad=(16, 16, 16, 16), gy=12),
              kid(dir=1, pad=(12, 12, 12, 12), gy=8), kid(parent=1, h=20), kid(parent=1, h=40),
              kid(h=44)])
    # row with fixed width in stretch column: not stretched
    c.append([root(dir=1, w=200, h=100), kid(w=80, h=20), kid(h=20)])
    # max-width in stretch column
    c.append([root(dir=1, w=200, h=100), kid(h=20, maxw=120), kid(h=20, minw=250)])
    # column fills rest with grow
    c.append([root(dir=1, w=200, h=300), kid(h=40), kid(grow=1000), kid(h=50)])
    # basis in column
    c.append([root(dir=1, w=200, h=300, ju=2), kid(basis=60, w=100), kid(h=40, w=50)])
    return c

def main():
    r = random.Random(20261018)
    cases = hand_cases()
    for _ in range(900):
        cases.append(gen_random(r))
    # nodes numbered in creation order with explicit parents; parents must precede children (true by construction)
    body = "".join(html_case(i, c) for i, c in enumerate(cases))
    page = """<!doctype html><meta charset=utf-8><style>*{margin:0}body{margin:0}</style><body>%s<pre id=out></pre>
<script>
var res={};
document.querySelectorAll('[data-n]').forEach(function(e){
  var root=e.closest('.case').firstChild.getBoundingClientRect();
  var b=e.getBoundingClientRect();
  var k=e.dataset.c;(res[k]=res[k]||{})[e.dataset.n]=[b.left-root.left,b.top-root.top,b.width,b.height];
});
document.getElementById('out').textContent=JSON.stringify(res);
</script>""" % body
    with tempfile.TemporaryDirectory() as d:
        pth = os.path.join(d, "t.html")
        open(pth, "w").write(page)
        prof = os.path.join(d, "prof")
        o = subprocess.run([CHROME, "--headless=new", "--no-sandbox", "--disable-gpu", "--user-data-dir=" + prof,
                            "--virtual-time-budget=5000", "--dump-dom", "file://" + pth],
                           capture_output=True, text=True, timeout=120)
    dom = o.stdout
    a = dom.index('<pre id="out">') + len('<pre id="out">')
    res = json.loads(dom[a:dom.index("</pre>", a)].replace("&quot;", '"'))
    out_cases = []
    for ci, nodes in enumerate(cases):
        got = res[str(ci)]
        rects = []
        okc = True
        for ni in range(len(nodes)):
            rr = got[str(ni)]
            if any(abs(v - round(v)) > 1e-6 for v in rr): okc = False
            if rr[0] < -1e-6 or rr[1] < -1e-6: okc = False  # overflow centring goes negative in CSS; flex.fi clamps at 0
            rects.append([int(round(v)) for v in rr])
        # overflow with end/centre justification goes negative in CSS; flex.fi clamps the free space at 0
        r0 = nodes[0]
        if r0["ju"] in (1, 2, 4, 5) and not r0["wrap"]:
            kidsr = [(n, rr) for n, rr in zip(nodes, rects) if n["parent"] == 0]
            ms = 0 if r0["dir"] == 0 else 1
            used = sum(rr[2 + ms] for n, rr in kidsr) + (r0["gx"] if r0["dir"] == 0 else r0["gy"]) * max(0, len(kidsr) - 1)
            used += sum((n["ml"] + n["mr"]) if r0["dir"] == 0 else (n["mt"] + n["mb"]) for n, rr in kidsr)
            inner = (rects[0][2] - r0["pl"] - r0["pr"]) if r0["dir"] == 0 else (rects[0][3] - r0["pt"] - r0["pb"])
            if used > inner: okc = False
        hand = ci < len(hand_cases())
        if okc: out_cases.append((nodes, rects))
        elif hand: print("hand case %d is not whole pixels: %s" % (ci, rects), file=sys.stderr)
        if len(out_cases) >= 190: break
    lines = [str(len(out_cases))]
    for nodes, rects in out_cases:
        lines.append(str(len(nodes)))
        for n, rr in zip(nodes, rects):
            lines.append(" ".join(str(n[f]) for f in FIELDS) + " " + " ".join(map(str, rr)))
    open(OUT, "w").write("\n".join(lines) + "\n")
    print("wrote %d cases to %s" % (len(out_cases), OUT))

main()
