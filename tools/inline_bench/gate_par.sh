#!/usr/bin/env bash
# ROUND PHI -- the four level gate, in parallel.
#
# Same contract as gate.sh: every program with an expect_exit:/expect_out:
# header, built and run at release-fast, release-safe, dev-fast and --no-opt,
# exit code and stdout checked. 1340 runs on this corpus.
#
# gate.sh is sequential and takes the better part of an hour on a machine
# that carries a load average of 8 from other rounds. This one hands each
# (program, level) pair to xargs -P and each job gets its OWN scratch file --
# the trap round INLINE fell into with asmcmp.sh, where all parallel jobs
# wrote the same two names and overwrote each other, is the reason the
# binary path below contains $$ AND the job index.
#
# Usage: gate_par.sh <listfile> [tag] [jobs]
set -uo pipefail
ROOT=${GATEROOT:-/root/firn-phi}
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
JOBS=${3:-6}
export FIRNLIB=$ROOT/lib
W=${GATEWORK:-$ROOT/.work/gatepar.$$}
mkdir -p "$W"

# one line per (file, level)
: > "$W/jobs"
while read -r f; do
  [ -z "$f" ] && continue
  for lvl in release-fast release-safe dev-fast noopt; do
    printf '%s\t%s\n' "$f" "$lvl" >> "$W/jobs"
  done
done < "$1"

run_one() {
  local line="$1" f lvl hdr kind exp flag out rc
  f=${line%%$'\t'*}; lvl=${line##*$'\t'}
  hdr=$(head -1 "$ROOT/$f")
  case "$hdr" in
    *expect_exit:*) kind=exit; exp=${hdr#*expect_exit: } ;;
    *expect_out:*)  kind=out;  exp=${hdr#*expect_out: } ;;
    *) return 0 ;;
  esac
  exp=${exp%%$'\r'*}
  # ROUND PHI -- `// only_mode: opt` (round 72, test.sh line 262).
  #
  # Four programs in the corpus exist to DEMONSTRATE that `release-fast`
  # truncates and wraps instead of checking (SPEC 13, L9): they cast a
  # value that deliberately does not fit. dev-fast, release-safe and
  # --no-opt are RIGHT to panic on them with exit 101 -- that is the
  # checked behaviour those levels promise.
  #
  # `test.sh` has skipped them outside `opt` mode since round 72. The
  # round gate script never did, which is the whole of the "12 of 1340
  # fail identically on base" that rounds SAMMLER and INLINE both
  # reported and neither chased down. They are not miscompilations and
  # they are not firnc1 building sites: they are this script asking the
  # wrong question. 1340 - 12 = 1328 runs were ever really being made.
  if [ "$lvl" != release-fast ] && head -5 "$ROOT/$f" | grep -q '^// only_mode: opt$'; then
    echo "SKIP"; return 0
  fi
  if [ "$lvl" = noopt ]; then flag=--no-opt; else flag=--opt-level=$lvl; fi
  local b="$W/b.$BASHPID.$RANDOM"
  if ! (cd "$ROOT" && timeout 300 "$FIRNC" $flag "$f" -o "$b") >/dev/null 2>&1; then
    echo "BUILDFAIL [$lvl] $f"; rm -f "$b"; return 0
  fi
  out=$(cd "$ROOT" && timeout 120 "$b" 2>/dev/null); rc=$?
  rm -f "$b"
  if [ "$kind" = exit ]; then
    if [ "$rc" = "$exp" ]; then echo "OK"; else echo "FAIL [$lvl] $f: exit $rc want $exp"; fi
  else
    if [ "$out" = "$exp" ]; then echo "OK"; else echo "FAIL [$lvl] $f: out '$out' want '$exp'"; fi
  fi
}
export -f run_one
export ROOT FIRNC W FIRNLIB

xargs -a "$W/jobs" -d '\n' -P "$JOBS" -I{} bash -c 'run_one "$@"' _ {} > "$W/res" 2>/dev/null

ok=$(grep -c '^OK$' "$W/res")
skip=$(grep -c '^SKIP$' "$W/res")
bad=$(grep -Ecv '^(OK|SKIP)$' "$W/res")
grep -Ev '^(OK|SKIP)$' "$W/res" | sort
echo "--- ${2:-gate}: pass=$ok fail=$bad skipped=$skip (only_mode:opt)"
rm -rf "$W"
