#!/usr/bin/env bash
# tools/grapheme/run.sh -- grapheme clusters (lib/str/grapheme.fi, UAX #29, Unicode 17.0).
#   1. generated/grapheme_tables.fi is up to date (gen.py --check)
#   2. GraphemeBreakTest.txt (official, Unicode 17.0.0): every case
#   3. random sequences from a pool of all classes vs the Python `regex` module (\X),
#      if it can be imported or pip-installed (else: SKIP, said loudly)
#   4. random octets (invalid UTF-8 too): clusters tile the input, no zero step, no crash
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
export FIRNLIB="$(pwd)/lib"
rc=0
python3 -I tools/grapheme/gen.py --check || rc=1
"$FIRNC" --opt-level=dev-fast -o "$D/probe" tools/grapheme/probe.fi || { echo "FAIL compile probe"; exit 1; }
# optional reference: regex module (own copy under $D, nothing installed system-wide)
if ! python3 -I -c "import regex" 2>/dev/null; then
    timeout 90 python3 -m pip install -q --disable-pip-version-check --target "$D/pk" regex >/dev/null 2>&1
fi
python3 - "$D/probe" "$D" "$(pwd)" <<'PY' || rc=1
import os, random, struct, subprocess, sys
probe, tmp, root = sys.argv[1:4]
if os.path.isdir(os.path.join(tmp, "pk")):
    sys.path.append(os.path.join(tmp, "pk"))
try:
    import regex
except ImportError:
    regex = None

def run(cases):
    path = os.path.join(tmp, "cases.bin")
    with open(path, "wb") as f:
        for c in cases:
            f.write(struct.pack("<I", len(c)) + c)
    r = subprocess.run([probe, path], capture_output=True, text=True, timeout=300)
    if r.returncode != 0:
        print("FAIL probe exit", r.returncode); sys.exit(1)
    lines = r.stdout.split("\n")[:-1]
    return [[int(x) for x in l.split()] for l in lines]

fails = 0
# ---- 2. official test file
cases, exp = [], []
for line in open(os.path.join(root, "tools/grapheme/data/GraphemeBreakTest.txt"), encoding="utf-8"):
    line = line.split("#")[0].strip()
    if not line:
        continue
    toks = line.split()
    s = b""; lens = []; cur = 0
    for t in toks:
        if t == "÷":
            if cur:
                lens.append(cur); cur = 0
        elif t == "×":
            pass
        else:
            b = chr(int(t, 16)).encode("utf-8", "surrogatepass")
            s += b; cur += len(b)
    if cur:
        lens.append(cur)
    cases.append(s); exp.append(lens)
got = run(cases)
bad = [i for i in range(len(cases)) if got[i] != exp[i]]
print("  GraphemeBreakTest.txt: %d cases, %d differ" % (len(cases), len(bad)))
for i in bad[:5]:
    print("   FAIL", cases[i].hex(), got[i], exp[i])
fails += len(bad)

# ---- 3. random vs regex \X
pool = [0x41, 0x20, 0x0D, 0x0A, 0x09, 0x7F, 0x300, 0x301, 0x200D, 0x200C, 0xFE0F, 0xFE0E,
        0x1F3FB, 0x1F3FF, 0x1F468, 0x1F469, 0x1F467, 0x1F600, 0x2764, 0x1F1E6, 0x1F1E7,
        0x1F1E8, 0x1F1FA, 0x1F1F8, 0x1100, 0x1160, 0x11A8, 0xAC00, 0xAC01, 0xD7B0,
        0x0600, 0x0903, 0x093E, 0x0915, 0x094D, 0x0937, 0x0924, 0x09CD, 0x0995, 0x0E33,
        0x0E40, 0x20E3, 0x1F3F4, 0xE0067, 0xE007F, 0x00A9, 0x2640, 0x3042, 0x4E2D, 0xE9]
if regex is None:
    print("  SKIP random vs regex \\X: module `regex` not available (offline) -- NOT checked")
else:
    rnd = random.Random(29)
    cs, ex = [], []
    for _ in range(30000):
        n = rnd.randint(1, 9)
        txt = "".join(chr(rnd.choice(pool)) for _ in range(n))
        b = txt.encode("utf-8")
        cs.append(b)
        ex.append([len(m.encode("utf-8")) for m in regex.findall(r"\X", txt)])
    got = run(cs)
    bad = [i for i in range(len(cs)) if got[i] != ex[i]]
    print("  random vs regex \\X (regex %s): %d cases, %d differ" % (regex.__version__, len(cs), len(bad)))
    for i in bad[:5]:
        print("   DIFF", cs[i].decode().encode("unicode_escape"), got[i], ex[i])
    fails += len(bad)

# ---- 4. random octets
rnd = random.Random(7)
cs = []
for _ in range(20000):
    n = rnd.randint(0, 12)
    cs.append(bytes(rnd.choice([0x41, 0x0D, 0x0A, 0xC3, 0xA9, 0xE2, 0x80, 0x8D, 0xF0, 0x9F, 0x91, 0xA8, 0xFF, 0xC0, 0xED, 0xA0, 0x80]) if rnd.random() < 0.8 else rnd.randrange(256) for _ in range(n)))
got = run(cs)
bad = [i for i in range(len(cs)) if sum(got[i]) != len(cs[i]) or any(x <= 0 for x in got[i])]
print("  random octets: %d cases, %d do not tile the input" % (len(cs), len(bad)))
fails += len(bad)
print("RESULT grapheme: %s (%d failed)" % ("ok" if not fails else "FAIL", fails))
sys.exit(1 if fails else 0)
PY
exit $rc
