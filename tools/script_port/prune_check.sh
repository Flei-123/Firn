#!/bin/bash
# tools/script_port/prune_check.sh -- FIR pruning (compiler/src/prune.rs) must not change behaviour.
#
#   bash tools/script_port/prune_check.sh [STEP]     (STEP: every STEP-th test, default 6)
#
# Builds every STEP-th tests/*.fi with and without FIRN_PRUNE_FIR=1 (dev-fast), runs both and
# compares stdout+stderr+exit code. A link failure of the pruned build is reported (firn-run would
# fall back to the unpruned build). Run it through heavy: /root/jarvis/bin/heavy bash <this>.
set -u
cd "$(dirname "$0")/../.."
ROOT=$PWD
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
export FIRNLIB=$ROOT/lib
STEP=${1:-6}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
one() {
    local f=$1 n; n=$(basename "$f" .fi)
    case "$(head -1 "$f")" in *expect_out:*|*expect_exit:*) ;; *) echo "SKIP $n"; return ;; esac
    "$FIRNC" --opt-level=dev-fast "$f" -o "$W/$n.a" >/dev/null 2>&1 || { echo "SKIP $n (base compile)"; return; }
    if ! FIRN_PRUNE_FIR=1 "$FIRNC" --opt-level=dev-fast "$f" -o "$W/$n.b" >"$W/$n.log" 2>&1; then
        echo "LINKFAIL $n $(grep -m1 -o 'undefined reference to .*' "$W/$n.log")"; return
    fi
    local a b
    a=$(cd "$W" && timeout 120 "./$n.a" </dev/null 2>&1; echo "rc=$?")
    b=$(cd "$W" && timeout 120 "./$n.b" </dev/null 2>&1; echo "rc=$?")
    if [ "$a" == "$b" ]; then echo "OK $n $(stat -c %s "$W/$n.a") $(stat -c %s "$W/$n.b")"; else echo "DIFF $n"; fi
    rm -f "$W/$n.a" "$W/$n.b" "$W/$n.log"
}
export -f one
export W FIRNC ROOT
ls tests/*.fi | awk -v s="$STEP" 'NR%s==0' | xargs -P 4 -I{} bash -c 'one {}' >"$W/res.txt"
cut -d' ' -f1 "$W/res.txt" | sort | uniq -c
grep -E '^(DIFF|LINKFAIL)' "$W/res.txt"
! grep -q '^DIFF' "$W/res.txt"
