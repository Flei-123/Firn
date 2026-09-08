#!/usr/bin/env bash
# ROUND INLINE -- compare the ASSEMBLER of base (firn-sammler) and new
# (firn-inline) for many programs. A refactor of the inliner that changes no
# decision must produce byte identical code. Anything else is a real change
# and has to be justified.
set -uo pipefail
LVL="${LVL:-release-fast}"
NEW=/root/firn-inline/compiler/target/release/firnc
BASE=/root/firn-sammler/compiler/target/release/firnc
OUT=/root/firn-inline/.work/asmcmp.$$
mkdir -p "$OUT"
same=0; diff=0; failboth=0; failone=0
list="$1"
while read -r f; do
  [ -z "$f" ] && continue
  b="$OUT/b.s"; n="$OUT/n.s"
  rm -f "$b" "$n"
  (cd /root/firn-sammler && FIRNLIB=/root/firn-sammler/lib timeout 300 "$BASE" --opt-level=$LVL --emit=asm "$f" -o "$b") >/dev/null 2>&1; rb=$?
  (cd /root/firn-inline  && FIRNLIB=/root/firn-inline/lib  timeout 300 "$NEW"  --opt-level=$LVL --emit=asm "$f" -o "$n") >/dev/null 2>&1; rn=$?
  if [ $rb -ne 0 ] && [ $rn -ne 0 ]; then failboth=$((failboth+1)); continue; fi
  if [ $rb -ne $rn ]; then failone=$((failone+1)); echo "EXITDIFF($rb/$rn) $f"; continue; fi
  if cmp -s "$b" "$n"; then same=$((same+1)); else diff=$((diff+1)); echo "ASMDIFF $f"; fi
done < "$list"
echo "---"
echo "identical=$same  different=$diff  both-failed=$failboth  exit-mismatch=$failone"
