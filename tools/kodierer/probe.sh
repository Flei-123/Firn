#!/usr/bin/env bash
# Kleine Stichprobe der Abnahme -- fuer die Arbeit waehrend der Entwicklung.
#   bash tools/kodierer/probe.sh [--a64] <quelle.fi> ...
set -u
cd "$(dirname "$0")/../.."
FIRNC="compiler/target/release/firnc"
export FIRNLIB="$(pwd)/lib"
ARCH=x86; TFLAG=""; EXTRA=""
if [ "${1:-}" = "--a64" ]; then ARCH=a64; TFLAG="--target=aarch64-linux"; EXTRA="--a64"; shift; fi
STUFEN="${STUFEN:-dev-fast release-fast no-opt streu}"
JOBS="${JOBS:-6}"
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
n=0
for f in "$@"; do
    [ -f "$f" ] || continue
    for lvl in $STUFEN; do
        (
            lf="--opt-level=$lvl"
            [ "$lvl" = no-opt ] && lf="--no-opt"
            [ "$lvl" = streu ] && lf="--opt-level=release-fast"
            s="$W/$n.s"
            if "$FIRNC" $TFLAG --emit=asm $lf -o "$s" "$f" >/dev/null 2>&1; then
                if [ "$lvl" = streu ]; then
                    python3 tools/kodierer/loc_streuer.py "$s" "$s.x" "$n" \
                        >/dev/null 2>&1 && mv "$s.x" "$s"
                fi
                python3 tools/kodierer/vergleich.py $EXTRA "$FIRNC" "$s" >> "$W/log" 2>&1
            else
                echo "UEBERSPRUNGEN $f [$lvl]" >> "$W/log"
            fi
            rm -f "$s"
        ) &
        n=$((n + 1))
        [ $((n % JOBS)) -eq 0 ] && wait
    done
done
wait
grep -v '^{' "$W/log" | grep -v '^UEBERSPRUNGEN' | head -60
python3 - "$W/log" <<'PY'
import json, sys
g = {}
sk = 0
for l in open(sys.argv[1], errors="replace"):
    if l.startswith("{"):
        for k, v in json.loads(l).items():
            g[k] = g.get(k, 0) + v
    elif l.startswith("UEBERSPRUNGEN"):
        sk += 1
print("Einheiten %d  bitgleich %d  abweichend %d  uebersprungen %d"
      % (g.get("dateien", 0), g.get("gut", 0), g.get("schlecht", 0), sk))
print("Oktette %d  Umsetzungen %d  DWARF-Oktette %d  Zeilen %d"
      % (g.get("bytes", 0), g.get("relocs", 0), g.get("dwarf_bytes", 0),
         g.get("dwarf_rows", 0)))
PY
