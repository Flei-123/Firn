#!/usr/bin/env bash
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
export FIRNC="${FIRNC:-$ROOT/compiler/target/release/firnc}" OWN_ROOT="$ROOT" FIRNLIB="${FIRNLIB:-$ROOT/lib}"
WORK="${WORK:?set WORK}"
TARGETS="${TARGETS:-$(source "$ROOT/tools/ownership/targets.sh"; for n in $FVC_NAMES; do echo -n "fvc_$n "; done; echo openplan_derive)}"
source "$ROOT/tools/ownership/targets.sh"
ir() { grep -E 'Collected' "$1" | awk '{print $4}' | tail -n 1; }
printf "%-22s %14s %14s %14s %8s %s\n" target gcc off on on/off same
for t in $TARGETS; do
    for m in off on; do
        d="$WORK/$t/$m"
        mkdir -p "$d"
        if [ $m = off ]; then flag=1; else flag=0; fi
        FIRN_NO_SUPEROPT=$flag target_build "$t" on "$d" > "$d/build.log" 2>&1 || { echo "$t $m build failed"; continue; }
        target_run "$t" "$d" "$d/out" valgrind --tool=callgrind --callgrind-out-file=/dev/null > "$d/cg.log" 2>&1
        rm -f "$d/bin"
    done
    g=-
    if [ "${t#fvc_}" != "$t" ] && [ -f "$ROOT/bench/firn_vs_c/c/${t#fvc_}.c" ]; then
        gd="$WORK/$t/gcc"; mkdir -p "$gd"
        gcc -O2 -o "$gd/bin" "$ROOT/bench/firn_vs_c/c/${t#fvc_}.c" -lm 2>/dev/null &&
            valgrind --tool=callgrind --callgrind-out-file=/dev/null "$gd/bin" > /dev/null 2> "$gd/cg.log" && g=$(ir "$gd/cg.log")
        rm -f "$gd/bin"
    fi
    a=$(ir "$WORK/$t/off/cg.log"); b=$(ir "$WORK/$t/on/cg.log")
    same=no; cmp -s "$WORK/$t/off/out" "$WORK/$t/on/out" && same=yes
    r=$(python3 -c "print('%.5f' % (int('${b//,/}' or 0)/max(1,int('${a//,/}' or 1))))")
    printf "%-22s %14s %14s %14s %8s %s\n" "$t" "$g" "$a" "$b" "$r" "$same"
done
