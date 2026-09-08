#!/usr/bin/env bash
# ROUND PHI -- DIFFERENTIAL TEST: the same program, translated two ways,
# must ANSWER the same.
#
# The four level gate only ever notices a wrong answer when the program has
# an `expect_exit:`/`expect_out:` header AND the wrong answer happens to
# differ from the expected one. A miscompilation that turns 3 into 4 in a
# program whose header says nothing about it goes straight through.
#
# This asks a different question, and it needs no header at all:
#
#     does `-Ofast` answer the same as `--no-opt`?
#     does `-Ofast` answer the same as `-Ofast --no-pass=thread-bool`?
#     does `-Ofast` answer the same as `-Ofast --no-pass=inline`?
#
# `--no-opt` is the reference: no inliner, no threading, no mem2reg. Any
# difference between it and an optimized build is a compiler bug in the
# passes in between -- there is no other explanation, because the source is
# byte identical.
#
# Usage: diffpass.sh <listfile> [tag] [jobs]
#   env A_FLAGS / B_FLAGS override the two sides.
set -uo pipefail
ROOT=${DIFFROOT:-/root/firn-phi}
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
JOBS=${3:-6}
A_FLAGS=${A_FLAGS:---no-opt}
B_FLAGS=${B_FLAGS:---opt-level=release-fast}
export FIRNLIB=$ROOT/lib
W=${DIFFWORK:-$ROOT/.work/diff.$$}
mkdir -p "$W"

one() {
  local f="$1" ba bb oa ob ra rb
  ba="$W/a.$BASHPID.$RANDOM"; bb="$W/b.$BASHPID.$RANDOM"
  # Both sides must BUILD. A program that only builds on one side is not a
  # wrong answer, it is a build difference -- reported apart.
  if ! (cd "$ROOT" && timeout 300 "$FIRNC" $A_FLAGS "$f" -o "$ba") >/dev/null 2>&1; then
    echo "SKIP-A $f"; rm -f "$ba" "$bb"; return 0
  fi
  if ! (cd "$ROOT" && timeout 300 "$FIRNC" $B_FLAGS "$f" -o "$bb") >/dev/null 2>&1; then
    echo "BUILDDIFF $f (B fails, A builds)"; rm -f "$ba" "$bb"; return 0
  fi
  oa=$(cd "$ROOT" && timeout 120 "$ba" < /dev/null 2>/dev/null); ra=$?
  ob=$(cd "$ROOT" && timeout 120 "$bb" < /dev/null 2>/dev/null); rb=$?
  rm -f "$ba" "$bb"
  if [ "$ra" != "$rb" ]; then
    echo "DIFF-EXIT $f: A=$ra B=$rb"
  elif [ "$oa" != "$ob" ]; then
    echo "DIFF-OUT $f: A='${oa:0:120}' B='${ob:0:120}'"
  else
    echo "SAME"
  fi
}
export -f one
export ROOT FIRNC W FIRNLIB A_FLAGS B_FLAGS

xargs -a "$1" -d '\n' -P "$JOBS" -I{} bash -c 'one "$@"' _ {} > "$W/res" 2>/dev/null

same=$(grep -c '^SAME$' "$W/res")
skip=$(grep -c '^SKIP-A' "$W/res")
diff=$(grep -c '^DIFF' "$W/res")
bd=$(grep -c '^BUILDDIFF' "$W/res")
grep '^DIFF\|^BUILDDIFF' "$W/res" | sort
echo "--- ${2:-diff}: same=$same differ=$diff builddiff=$bd skipped=$skip  [A=$A_FLAGS B=$B_FLAGS]"
rm -rf "$W"
