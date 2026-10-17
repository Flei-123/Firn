#!/usr/bin/env bash
# tools/vspy/langprobe.sh -- compile (and run) tiny Firn probe files, one line of verdict each.
#   bash tools/vspy/langprobe.sh [--msg] file.fi|file.probe...
# OK  = compiled and ran (first stdout line shown); ERR = firnc message (first 2 lines, --msg: all 6).
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
export FIRNLIB=${FIRNLIB:-$ROOT/lib}
full=0; [ "$1" = "--msg" ] && { full=1; shift; }
D=$(mktemp -d "${TMPDIR:-/tmp}/langprobe.XXXXXX"); trap 'rm -rf "$D"' EXIT
for f in "$@"; do
  n=$(basename "$f"); n=${n%.fi}; n=${n%.probe}
  if "$FIRNC" --opt-level=dev-fast "$f" -o "$D/$n" >"$D/$n.log" 2>&1; then
    out=$(timeout 10 "$D/$n" 2>&1 | head -3 | tr '\n' '|'); 
    echo "OK   $n  out=$out"
  else
    if [ $full = 1 ]; then echo "ERR  $n"; head -8 "$D/$n.log" | sed 's/^/       /'; else echo "ERR  $n  $(head -2 "$D/$n.log" | tr '\n' ' ' | cut -c1-230)"; fi
  fi
done
