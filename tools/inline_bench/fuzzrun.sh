#!/usr/bin/env bash
# ROUND PHI -- run the generated corpus at ONE build level and report every
# program whose exit is not 0.
#
# The generated programs are self checking (see genbool.py): the generator
# computed the answers in Python over the same integers, so exit 0 is right
# and any other exit names the assertion that broke. No golden file, no
# header parsing -- which is what makes it usable as a fuzz oracle.
#
# Usage: fuzzrun.sh <listfile> <flags...> ; env JOBS=n
set -uo pipefail
ROOT=${FUZZROOT:-/root/firn-phi}
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
JOBS=${JOBS:-6}
LIST="$1"; shift
FLAGS="$*"
export FIRNLIB=$ROOT/lib
W=$ROOT/.work/fuzz.$$
mkdir -p "$W"

one() {
  local f="$1" b rc out
  b="$W/b.$BASHPID.$RANDOM"
  if ! timeout 300 "$FIRNC" $FLAGS "$f" -o "$b" >/dev/null 2>&1; then
    echo "BUILDFAIL $f"; rm -f "$b"; return 0
  fi
  out=$(timeout 60 "$b" < /dev/null 2>/dev/null); rc=$?
  rm -f "$b"
  if [ "$rc" = 0 ]; then echo "OK"; else echo "WRONG $f: exit $rc (want 0)"; fi
}
export -f one
export ROOT FIRNC W FIRNLIB FLAGS

xargs -a "$LIST" -d '\n' -P "$JOBS" -I{} bash -c 'one "$@"' _ {} > "$W/res" 2>/dev/null
ok=$(grep -c '^OK$' "$W/res"); bad=$(grep -cv '^OK$' "$W/res")
grep -v '^OK$' "$W/res" | sort
echo "--- fuzz [$FLAGS]: ok=$ok bad=$bad"
rm -rf "$W"
