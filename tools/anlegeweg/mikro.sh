#!/usr/bin/env bash
# tools/anlegeweg/mikro.sh -- die Nanosekunden je Grundvorgang, drei Laeufe,
# BESTWERT je Zeile (die Maschine traegt Fremdlast).
#
#   bash tools/anlegeweg/mikro.sh            mit laufendem Sammler
#   bash tools/anlegeweg/mikro.sh --frei     ohne Sammlerarbeit (Grenze hoch)
set -euo pipefail
cd "$(dirname "$0")/../.."
FIRNC="compiler/target/release/firnc"
[ -x "$FIRNC" ] || cargo build --release --manifest-path compiler/Cargo.toml
export FIRNLIB="$PWD/lib"
LAEUFE="${LAEUFE:-3}"
NAME=/tmp/aw_mikro
if [ "${1:-}" = "--frei" ]; then
    export FIRN_AW_ENTFESSELT=1
    NAME=/tmp/aw_mikro_frei
fi
"$FIRNC" -o "$NAME" tools/anlegeweg/mikro.fi 2>&1 | grep -v 'RWX' || true
for i in $(seq 1 "$LAEUFE"); do
    "$NAME"
done | python3 -c '
import sys, re
best = {}
order = []
for line in sys.stdin:
    m = re.match(r"^(.*?)\s{2,}([\d.]+) ns$", line.rstrip())
    if not m:
        continue
    k, v = m.group(1).strip(), float(m.group(2))
    if k not in best:
        best[k] = v; order.append(k)
    else:
        best[k] = min(best[k], v)
for k in order:
    print("%-34s %8.3f ns" % (k, best[k]))
'
