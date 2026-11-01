import sys, subprocess, glob, os, re, collections
tool, d = sys.argv[1], sys.argv[2]
def words(t): return collections.Counter(re.findall(r"\w+", t.lower()))
ok = bad = 0
for f in sorted(glob.glob(d + "/*.pdf")):
    pw = "geheim" if ".pw." in f else ""
    ref = subprocess.run(["pdftotext", "-upw", pw, "-enc", "UTF-8", f, "-"], capture_output=True).stdout.decode("utf-8", "replace")
    env = dict(os.environ, PDF_PASSWORD=pw) if pw else dict(os.environ)
    got = subprocess.run([tool, "t", f, "0"], capture_output=True, env=env, timeout=60).stdout.decode("utf-8", "replace")
    rw, gw = words(ref), words(got)
    n = sum(rw.values())
    common = sum((rw & gw).values())
    # page 1 only vs whole doc: compare against first page ref
    ref1 = subprocess.run(["pdftotext", "-upw", pw, "-f", "1", "-l", "1", "-enc", "UTF-8", f, "-"], capture_output=True).stdout.decode("utf-8", "replace")
    r1, = [words(ref1)]
    n1 = sum(r1.values()); c1 = sum((r1 & gw).values())
    good = n1 > 0 and c1 >= 0.95 * n1
    print(("OK  " if good else "BAD ") + os.path.basename(f), c1, n1)
    ok += good; bad += (not good)
print("ok", ok, "bad", bad)
