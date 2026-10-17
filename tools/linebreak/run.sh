#!/usr/bin/env bash
# tools/linebreak/run.sh -- line breaks (lib/str/linebreak.fi, UAX #14, Unicode 15.0).
#   1. generated/linebreak_tables.fi is up to date (gen.py --check)
#   2. LineBreakTest.txt (official, 15.0.0): every case
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
export FIRNLIB="$(pwd)/lib"
rc=0
python3 -I tools/linebreak/gen.py --check || rc=1
"$FIRNC" --opt-level=dev-fast -o "$D/probe" tools/linebreak/probe.fi || { echo "FAIL compile probe"; exit 1; }
python3 -I - "$D/probe" "$D" "$(pwd)" <<'PY' || rc=1
import os, struct, subprocess, sys
probe, tmp, root = sys.argv[1:4]
cases, exp = [], []
for line in open(os.path.join(root, "tools/linebreak/data/LineBreakTest.txt"), encoding="utf-8"):
    line = line.split("#")[0].strip()
    if not line:
        continue
    s = b""; brk = []
    for t in line.split():
        if t == "÷":
            if s:
                brk.append(len(s))
        elif t == "×":
            pass
        else:
            s += chr(int(t, 16)).encode("utf-8", "surrogatepass")
    cases.append(s); exp.append(brk)
path = os.path.join(tmp, "cases.bin")
with open(path, "wb") as f:
    for c in cases:
        f.write(struct.pack("<I", len(c)) + c)
r = subprocess.run([probe, path], capture_output=True, text=True, timeout=300)
if r.returncode != 0:
    print("FAIL probe exit", r.returncode); sys.exit(1)
got = [[int(x) for x in l.split()] for l in r.stdout.split("\n")[:-1]]
bad = [i for i in range(len(cases)) if got[i] != exp[i]]
print("  LineBreakTest.txt: %d cases, %d differ" % (len(cases), len(bad)))
for i in bad[:8]:
    print("   FAIL", cases[i].hex(), got[i], exp[i])
sys.exit(1 if bad else 0)
PY
[ "$rc" = 0 ] && echo "LINEBREAK PASSED." || { echo "LINEBREAK FAILED."; exit 1; }
