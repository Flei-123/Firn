#!/usr/bin/env bash
# ROUND INLINE -- the correctness gate: every test program with an
# expect_exit:/expect_out: header, built and run in FOUR build levels
# (release-fast, release-safe, dev-fast, --no-opt), exit code and stdout
# checked. That is the core of test.sh step 3, and the part that can see a
# collector or an inliner bug.
#
# Usage: gate.sh <listfile> [tag]
set -uo pipefail
ROOT=${GATEROOT:-/root/firn-inline}
FIRNC=$ROOT/compiler/target/release/firnc
export FIRNLIB=$ROOT/lib
W=$ROOT/.work/gate.$$
mkdir -p "$W"
trap 'rm -rf "$W"' EXIT
ok=0; bad=0
while read -r f; do
  [ -z "$f" ] && continue
  hdr=$(head -1 "$ROOT/$f")
  case "$hdr" in
    *expect_exit:*) kind=exit; exp=${hdr#*expect_exit: } ;;
    *expect_out:*)  kind=out;  exp=${hdr#*expect_out: } ;;
    *) continue ;;
  esac
  for lvl in release-fast release-safe dev-fast noopt; do
    if [ "$lvl" = noopt ]; then flag=--no-opt; else flag=--opt-level=$lvl; fi
    if ! (cd "$ROOT" && timeout 300 "$FIRNC" $flag "$f" -o "$W/b") >/dev/null 2>&1; then
      echo "BUILDFAIL [$lvl] $f"; bad=$((bad+1)); continue
    fi
    out=$(cd "$ROOT" && timeout 120 "$W/b" 2>/dev/null); rc=$?
    if [ "$kind" = exit ]; then
      if [ "$rc" = "$exp" ]; then ok=$((ok+1)); else echo "FAIL [$lvl] $f: exit $rc want $exp"; bad=$((bad+1)); fi
    else
      if [ "$out" = "$exp" ]; then ok=$((ok+1)); else echo "FAIL [$lvl] $f: out '$out' want '$exp'"; bad=$((bad+1)); fi
    fi
  done
done < "$1"
echo "--- ${2:-gate}: pass=$ok fail=$bad"
