#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/windows/threadkit.sh -- a KIT that runs the thread and process-tree
# tests on a REAL Windows machine.
#
#   bash tools/windows/threadkit.sh [out.zip]
#
# This server cannot reach a real Windows PC (the helper transports text only),
# and every Windows result in this tree comes from Wine. So the kit is a zip
# that needs ONLY Python 3 on the other side:
#
#   <test>.exe     the test programs (tests/860, 861, 862, 834, 2064, 2065,
#                  2066, 2100, 2101), built with --target=x86_64-windows
#   manifest.json  what each one has to answer (the header of its source)
#   run.py         runs them, prints PASS/FAIL and the Windows version
#   README.txt
#
# Unzip, `py run.py`. Exit code 0 = everything passed. Please send the output
# back; a FAIL line says which program and what it printed.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
OUT=${1:-$ROOT/build/windows-threads-kit.zip}
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
export FIRNLIB=$ROOT/lib
[ -x "$FIRNC" ] || { echo "no compiler: $FIRNC"; exit 1; }
W=$(mktemp -d "${TMPDIR:-/tmp}/threadkit.XXXXXX")
trap 'rm -rf "$W"' EXIT
K=$W/windows-threads-kit
mkdir -p "$K" "$(dirname "$OUT")"
CASES="860_thread_basic 861_thread_gc 862_thread_local 834_arc_thread 2064_std_process_portable 2065_std_pool 2066_std_pool_portable 2100_std_process_tree 2101_threads_portable"
: > "$W/manifest.tsv"
for t in $CASES; do
    src=tests/$t.fi
    "$FIRNC" --target=x86_64-windows -o "$K/$t.exe" "$src" >"$W/$t.log" 2>&1 || { echo "build failed: $t"; cat "$W/$t.log"; exit 1; }
    hdr=$(head -1 "$src")
    case "$hdr" in
        *expect_exit:*) printf '%s\texit\t%s\n' "$t" "${hdr#*expect_exit: }" >> "$W/manifest.tsv" ;;
        *expect_out:*) printf '%s\tout\t%s\n' "$t" "${hdr#*expect_out: }" >> "$W/manifest.tsv" ;;
        *) echo "no expectation in $src"; exit 1 ;;
    esac
done
python3 - "$W/manifest.tsv" "$K/manifest.json" <<'PY'
import json, sys
rows = []
for line in open(sys.argv[1]):
    name, kind, exp = line.rstrip("\n").split("\t", 2)
    rows.append({"name": name, "kind": kind, "expect": exp})
json.dump(rows, open(sys.argv[2], "w"), indent=1)
PY
cat > "$K/run.py" <<'PY'
# Runs every program of the kit and compares with manifest.json.
import json, os, platform, subprocess, sys, time

here = os.path.dirname(os.path.abspath(__file__))
rows = json.load(open(os.path.join(here, "manifest.json")))
print("machine:", platform.platform(), platform.machine(), "cpus:", os.cpu_count())
bad = 0
for r in rows:
    exe = os.path.join(here, r["name"] + (".exe" if os.name == "nt" or os.environ.get("KIT_RUNNER") else ""))
    t0 = time.time()
    try:
        runner = os.environ.get("KIT_RUNNER", "").split()  # e.g. wine64, for a check on Linux
        p = subprocess.run(runner + [exe], capture_output=True, timeout=600, stdin=subprocess.DEVNULL)
        rc, out = p.returncode, p.stdout.decode("utf-8", "replace").strip()
    except subprocess.TimeoutExpired:
        rc, out = "timeout", ""
    except OSError as e:
        rc, out = "cannot start: %s" % e, ""
    if r["kind"] == "exit":
        ok = str(rc) == r["expect"]
        got = "exit %s" % rc
    else:
        ok = rc == 0 and out == r["expect"]
        got = "exit %s, printed %r" % (rc, out[:80])
    dt = time.time() - t0
    print("%s %-30s %5.1fs  %s" % ("PASS" if ok else "FAIL", r["name"], dt, "" if ok else got))
    bad += 0 if ok else 1
print("RESULT: %d of %d passed" % (len(rows) - bad, len(rows)))
sys.exit(1 if bad else 0)
PY
cat > "$K/README.txt" <<'TXT'
Firn Windows thread kit.
1. Unzip anywhere (a folder you may write to; the tests use %TEMP%).
2. Run:  py run.py     (or python run.py)
3. Send back the whole output.
It starts a few helper processes of its own (the programs start themselves as
children) and waits at most a few seconds for each. Antivirus may need a moment
for new .exe files.
TXT
(cd "$W" && python3 - "$OUT" <<'PY'
import os, sys, zipfile
out = sys.argv[1]
if os.path.exists(out):
    os.remove(out)
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    for f in sorted(os.listdir("windows-threads-kit")):
        z.write(os.path.join("windows-threads-kit", f), os.path.join("windows-threads-kit", f))
PY
)
echo "kit: $OUT ($(du -h "$OUT" | cut -f1))"
