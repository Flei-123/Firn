#!/usr/bin/env bash
# tools/modreload/run.sh -- hot-reload prototype: build mod v1, start the host,
# rebuild the mod as v2 (different code), then v3, while the host keeps running.
# Expected: the tag changes v1 -> v2 -> v3, the tick counter keeps counting
# (state lives in the host), `damage` grows by the new step.
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
export FIRNLIB="$(pwd)/lib"
build_mod() { # $1 tag $2 step $3 out.so
    sed "s/MOD_TAG/$1/; s/MOD_STEP/$2/" tools/modreload/mod.fi > "$D/mod_$1.fi"
    "$FIRNC" --pic --opt-level=dev-fast -c -o "$D/mod_$1.o" "$D/mod_$1.fi" || return 1
    printf '{ global: mod_*; local: *; };\n' > "$D/exp.ver"
    ld -shared -Bsymbolic --version-script="$D/exp.ver" -z noexecstack -soname "$3" -o "$D/$3" "$D/mod_$1.o"
}
build_mod v1 5 mod_v1.so || exit 1
"$FIRNC" --opt-level=dev-fast -o "$D/host" tools/modreload/host.fi || exit 1
T0=$(date +%s%N)
"$D/host" "$D/mod_v1.so" "$D/mod_v2.so" "$D/mod_v3.so" "$D/trigger" > "$D/out.txt" &
HP=$!
sleep 0.3
build_mod v2 7 mod_v2.so || exit 1
T1=$(date +%s%N)
touch "$D/trigger"
sleep 0.3
build_mod v3 100 mod_v3.so || exit 1
touch "$D/trigger"
wait $HP
echo "rebuild v2 (compile+link): $(( (T1-T0)/1000000 )) ms incl. 300 ms wait"
cat "$D/out.txt"
exp=$'v1 tick=1 damage=5\nv1 tick=2 damage=10\nv1 tick=3 damage=15\nv2 tick=4 damage=22\nv2 tick=5 damage=29\nv2 tick=6 damage=36\nv3 tick=7 damage=136\nv3 tick=8 damage=236\nv3 tick=9 damage=336'
if [ "$(cat "$D/out.txt")" = "$exp" ]; then echo "RESULT modreload: ok"; else echo "RESULT modreload: FAIL"; exit 1; fi
