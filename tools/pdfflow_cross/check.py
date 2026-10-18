#!/usr/bin/env python3
"""Cross-check pdf.pdfflow against poppler: every word comes out of the PDF in the order it went in
(pdftotext -raw), the page count equals pdfinfo's.
usage: check.py <firnc> <firnlib> <firn repo root> [cases]"""
import os, random, re, shutil, subprocess, sys, tempfile

firnc, firnlib, root = sys.argv[1], sys.argv[2], sys.argv[3]
cases = int(sys.argv[4]) if len(sys.argv) > 4 else 40
d = tempfile.mkdtemp(prefix="pdfcross-")
bad = 0
try:
    env = dict(os.environ, FIRNLIB=firnlib)
    exe = os.path.join(d, "probe")
    r = subprocess.run([firnc, "--opt-level=dev-fast", "-o", exe, os.path.join(root, "tools/pdfflow_cross/probe.fi")],
                       capture_output=True, text=True, env=env)
    if r.returncode:
        print(r.stderr[:500]); sys.exit(2)
    rnd = random.Random(11)
    syll = ["ka", "lo", "mi", "ren", "tu", "sa", "bro", "ix", "ol", "den", "far", "gu"]  # english: ok

    def word():
        w = "".join(rnd.choice(syll) for _ in range(rnd.randint(1, 4)))
        return w if rnd.random() > .05 else w * 12  # a few over-long words

    for k in range(cases):
        if k % 2 == 0:
            lines = [" ".join(word() for _ in range(rnd.randint(0, 60))) for _ in range(rnd.randint(1, 40))]
            content = "P\n" + "\n".join(lines)
            want = " ".join(lines).split()
        else:
            rows = [[" ".join(word() for _ in range(rnd.randint(1, 7))) for _ in range(3)] for _ in range(rnd.randint(1, 40))]
            content = "T\n" + ";".join(",".join(c) for c in rows)
            want = [w for row in rows for c in row for w in c.split()]
        with open(os.path.join(d, "in.txt"), "w") as f:
            f.write(content)
        # the probe reads tests/data/... relative to the cwd: link the repo's tests into the scratch dir
        if not os.path.exists(os.path.join(d, "tests")):
            os.symlink(os.path.join(root, "tests"), os.path.join(d, "tests"))
        out = subprocess.run([exe], cwd=d, capture_output=True, text=True)
        m = re.search(r"pages (\d+)", out.stdout)
        txt = subprocess.run(["pdftotext", "-raw", os.path.join(d, "out.pdf"), "-"], capture_output=True, text=True).stdout
        info = subprocess.run(["pdfinfo", os.path.join(d, "out.pdf")], capture_output=True, text=True).stdout
        pm = re.search(r"Pages:\s+(\d+)", info)
        got = txt.split()
        if not m or not pm or m.group(1) != pm.group(1):
            bad += 1; print("PAGES", k, m and m.group(1), pm and pm.group(1))
        elif "".join(got) != "".join(want) or len(got) < len(want):  # over-long words are cut into pieces
            bad += 1; print("TEXT", k, len(got), len(want))
    print(f"checked {cases} documents, {bad} deviations")
    sys.exit(1 if bad else 0)
finally:
    shutil.rmtree(d, ignore_errors=True)
