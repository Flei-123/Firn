#!/usr/bin/env bash
# ROUND INLINE -- the JS engine must produce BYTE IDENTICAL output on base and
# on this branch, over every .js in tools/js/cases and tools/js/progs.
# Each file becomes one job in the run_main.fi job format.
set -uo pipefail
cd /root/firn-inline
A="${1:?engine A}"; B="${2:?engine B}"
W=.work/jsout.$$; mkdir -p "$W"; trap 'rm -rf "$W"' EXIT
same=0; diff=0
for f in tools/js/cases/*.js tools/js/progs/*.js; do
  [ -f "$f" ] || continue
  python3 - "$f" > "$W/job.bin" <<'PY'
import struct, sys
b = open(sys.argv[1], 'rb').read()
sys.stdout.buffer.write(struct.pack("<II", 0, len(b)) + b)
PY
  timeout 120 "$A" < "$W/job.bin" > "$W/a.out" 2>&1; ra=$?
  timeout 120 "$B" < "$W/job.bin" > "$W/b.out" 2>&1; rb=$?
  if [ "$ra" != "$rb" ]; then echo "EXITDIFF($ra/$rb) $f"; diff=$((diff+1)); continue; fi
  if cmp -s "$W/a.out" "$W/b.out"; then same=$((same+1)); else
    echo "OUTDIFF $f"; diff=$((diff+1)); fi
done
echo "--- js output: identical=$same different=$diff"
