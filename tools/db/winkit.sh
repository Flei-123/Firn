#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/db/winkit.sh -- a KIT that runs the lib/db checks on ANOTHER machine (a real Windows PC).
#
#   bash tools/db/winkit.sh [windows|linux] [out.zip]
#
# The checks (tools/db/run.sh) need bash and this tree; a real Windows PC has neither, and this
# server cannot put binary files on one. So the kit is a zip that needs ONLY Python 3 there
# (its `sqlite3` module is the judge):
#
#   sql_probe(.exe)    the program under test: runs SQL scripts through lib/db
#   check_select.py    106 SELECTs against SQLite
#   check_dml.py       INSERT/UPDATE/DELETE/DDL/transactions, random statements, files read both ways
#   check_crash.py     the process killed in the middle of a commit (TerminateProcess), recovery by SQLite and lib/db
#   check_lock.py      SQLite and lib/db processes on one file (LockFileEx against SQLite's own Windows locks)
#   check_hostile.py   damaged files: no crash, no hang
#   run.py             runs them all, prints "ok"/"FAIL" per check; exit 0 when everything passed
#
# Unzip it, run `py run.py` (Windows) or `python3 run.py` (Linux).
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
TARGET=${1:-windows}
OUT=${2:-$ROOT/build/db-$TARGET-kit.zip}
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB=$ROOT/lib
[ -x "$FIRNC" ] || { echo "no compiler"; exit 1; }
case "$TARGET" in
    windows) EXT=.exe; TF="--target=x86_64-windows" ;;
    linux) EXT=""; TF="" ;;
    *) echo "target: windows or linux"; exit 2 ;;
esac
W=$(mktemp -d "${TMPDIR:-/tmp}/db-winkit.XXXXXX")
trap 'rm -rf "$W"' EXIT
K=$W/db-kit
mkdir -p "$K" "$(dirname "$OUT")"
"$FIRNC" $TF --opt-level=dev-fast -o "$K/sql_probe$EXT" tools/db/sql_probe.fi 2>"$W/build.err" \
    || { echo "build failed:"; grep -v RWX "$W/build.err" | head; exit 1; }
for f in check_select check_dml check_crash check_lock check_hostile; do
    cp "tools/db/$f.py" "$K/$f.py"
done
cat > "$K/run.py" <<'EOF'
#!/usr/bin/env python3
# runs every check of the kit against the sql_probe next to this file
import os, subprocess, sys, time

here = os.path.dirname(os.path.abspath(__file__))
probe = os.path.join(here, "sql_probe.exe" if os.name == "nt" else "sql_probe")
if os.name != "nt":
    os.chmod(probe, 0o755)   # a zip does not keep the executable bit
rounds = "300"   # damaged files; the full run on the server uses 1500
checks = [("SELECT against SQLite", ["check_select.py", probe]),
          ("statements, transactions, files read both ways", ["check_dml.py", probe]),
          ("a commit killed at every event", ["check_crash.py", probe]),
          ("locks against SQLite's own", ["check_lock.py", probe]),
          ("damaged files", ["check_hostile.py", probe, rounds])]
only = [a for a in sys.argv[1:] if not a.startswith("-")]
fails = 0
for name, args in checks:
    if only and not any(o in args[0] for o in only):
        continue
    t0 = time.time()
    print("-- " + name, flush=True)
    r = subprocess.run([sys.executable, os.path.join(here, args[0])] + args[1:], capture_output=True, text=True, errors="replace")
    lines = (r.stdout + r.stderr).strip().splitlines()
    for l in lines[-6:]:
        print("   " + l)
    if r.returncode != 0:
        fails += 1
        print("   FAIL %s (exit %d)" % (name, r.returncode))
    else:
        print("   ok (%.0f s)" % (time.time() - t0))
print("db kit: %d check groups failed" % fails)
sys.exit(1 if fails else 0)
EOF
cat > "$K/README.txt" <<EOF
lib/db test kit for $TARGET, made $(date -u +%Y-%m-%d)

Needs: Python 3 (Windows: the "py" launcher). Nothing is installed.

    py run.py                (Windows)
    python3 run.py           (Linux)
    py run.py check_crash    (one group)

What it does: sql_probe$EXT is the embedded database of the Firn tree (lib/db, a SQLite 3
compatible file format). Python's own sqlite3 module is the judge: statements are run through
both and compared; a commit is killed in the middle and both engines must find the file
consistent; SQLite and lib/db processes fight for one file; damaged files must give an error,
never a crash or a hang. Each group prints a short result; the last line is the count and the
exit code is 0 when everything passed.

If Windows Defender or SmartScreen blocks sql_probe.exe, allow the folder.
Takes a few minutes. Please send the output of "py run.py" back.
EOF
( cd "$W" && python3 -c "
import sys, zipfile, os
z = zipfile.ZipFile(sys.argv[1], 'w', zipfile.ZIP_DEFLATED)
for root, dirs, files in os.walk('db-kit'):
    for f in sorted(files):
        p = os.path.join(root, f)
        z.write(p, p)
z.close()" "$OUT" ) || exit 1
echo "kit: $OUT ($(du -k "$OUT" | cut -f1) KiB)"
