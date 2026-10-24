#!/usr/bin/env bash
# Proof for the compile cache of firnc0 (roadmap r59, docs/CACHE.md).
# Usage: bash tools/cache/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml || exit 1
fi
FAIL=0
PASS=0
check() { if [ "$2" = "0" ]; then PASS=$((PASS + 1)); else echo "  FAIL  $1"; FAIL=$((FAIL + 1)); fi; }
export FIRN_CACHE_LOG=1
export FIRN_CACHE_DIR="$T/cache"
unset FIRN_NO_CACHE
W="$T/w"
mkdir -p "$W"
cp tools/cache/cases/* "$W/"
cd "$W"
objects() { ls "$FIRN_CACHE_DIR/objects" 2>/dev/null | grep -c '\.fc$'; }
ev() { grep -o 'cache: [a-z]*' "$1" | tr '\n' ' '; }

echo "== 1. miss, then hit, byte-identical to uncached =="
"$FIRNC" main.fi -o cold 2> cold.log
check "cold build" $?
[ "$(ev cold.log)" = "cache: miss cache: store " ]; check "cold = miss+store" $?
"$FIRNC" main.fi -o warm 2> warm.log
[ "$(ev warm.log)" = "cache: hit " ]; check "warm = hit" $?
"$FIRNC" --no-cache main.fi -o plain 2> plain.log
[ "$(ev plain.log)" = "cache: off " ]; check "--no-cache = off" $?
cmp -s cold warm; check "hit executable identical" $?
cmp -s cold plain; check "uncached executable identical" $?
./warm; [ $? -eq 12 ]; check "hit executable runs (exit 12)" $?
"$FIRNC" --emit=asm main.fi -o a1.s 2> /dev/null
"$FIRNC" --emit=asm main.fi -o a2.s 2> a2.log
"$FIRNC" --no-cache --emit=asm main.fi -o a3.s 2> /dev/null
cmp -s a1.s a2.s && cmp -s a1.s a3.s; check "asm identical (miss, hit, uncached)" $?
FIRN_NO_CACHE=1 "$FIRNC" main.fi -o envoff 2> envoff.log
[ "$(ev envoff.log)" = "cache: off " ]; check "FIRN_NO_CACHE = off" $?

echo "== 2. import change = miss =="
n=$(objects)
sed -i 's/return 5/return 6/' util.fi
"$FIRNC" main.fi -o changed 2> changed.log
[ "$(ev changed.log)" = "cache: miss cache: store " ]; check "import change = miss" $?
[ "$(objects)" -eq $((n + 1)) ]; check "new entry added" $?
"$FIRNC" --no-cache main.fi -o changed_plain 2> /dev/null
cmp -s changed changed_plain; check "changed executable identical to uncached" $?
./changed; [ $? -eq 13 ]; check "changed executable runs (exit 13)" $?
"$FIRNC" main.fi -o changed2 2> changed2.log
[ "$(ev changed2.log)" = "cache: hit " ]; check "second build of the changed tree = hit" $?
sed -i 's/return 6/return 5/' util.fi
"$FIRNC" main.fi -o back 2> back.log
[ "$(ev back.log)" = "cache: hit " ]; check "reverting the import = hit on the first entry" $?
cmp -s back cold; check "reverted executable identical to the first" $?

echo "== 3. flag change = miss =="
"$FIRNC" --emit=asm --target=aarch64-linux main.fi -o arm1.s 2> arm1.log
[ "$(ev arm1.log)" = "cache: miss cache: store " ]; check "target change = miss" $?
"$FIRNC" --emit=asm --target=aarch64-linux main.fi -o arm2.s 2> arm2.log
[ "$(ev arm2.log)" = "cache: hit " ]; check "same target again = hit" $?
cmp -s arm1.s arm2.s; check "aarch64 asm identical on hit" $?
"$FIRNC" --emit=asm --target=aarch64-linux --no-cache main.fi -o arm3.s 2> /dev/null
cmp -s arm1.s arm3.s; check "aarch64 asm identical to uncached" $?
cmp -s a1.s arm1.s; [ $? -ne 0 ]; check "x86 and aarch64 entries differ" $?
FIRN_NO_LOADPRE=1 "$FIRNC" --emit=asm main.fi -o env1.s 2> env1.log
[ "$(ev env1.log)" = "cache: miss cache: store " ]; check "FIRN_NO_* switch = miss" $?
"$FIRNC" --emit=asm --pic main.fi -o pic1.s 2> pic1.log
[ "$(ev pic1.log)" = "cache: miss cache: store " ]; check "--pic = miss" $?

echo "== 4. output name does not matter =="
"$FIRNC" main.fi -o other_name 2> other.log
[ "$(ev other.log)" = "cache: hit " ]; check "different -o = hit" $?
cmp -s other_name cold; check "identical under another name" $?

echo "== 5. kernel profile object, link state survives a hit =="
"$FIRNC" -c --profile=kernel main.fi -o k1.o 2> k1.log
"$FIRNC" -c --profile=kernel main.fi -o k2.o 2> k2.log
[ "$(ev k2.log)" = "cache: hit " ]; check "kernel object build = hit" $?
cmp -s k1.o k2.o; check "kernel object identical on hit" $?
"$FIRNC" statics.fi -o s1 2> s1.log
"$FIRNC" statics.fi -o s2 2> s2.log
[ "$(ev s2.log)" = "cache: hit " ]; check "program with a static = hit" $?
cmp -s s1 s2; check "static program identical (page aligned link kept)" $?
"$FIRNC" --no-cache statics.fi -o s3 2> /dev/null
cmp -s s1 s3; check "static program identical to uncached" $?

echo "== 6. not cacheable: build time environment, include =="
"$FIRNC" env.fi -o e1 2> e1.log
[ "$(ev e1.log)" = "cache: miss cache: skip " ]; check "__env_or program is not stored" $?
"$FIRNC" inc.fi -o i1 2> i1.log
[ "$(ev i1.log)" = "cache: miss cache: skip " ]; check "__include_str program is not stored" $?

echo "== 7. errors are never cached =="
n=$(objects)
"$FIRNC" bad.fi -o bad 2> bad.log
[ $? -ne 0 ]; check "broken program fails" $?
[ "$(objects)" -eq "$n" ]; check "failure stores nothing" $?

echo "== 8. concurrent builds of one program =="
rm -rf "$FIRN_CACHE_DIR"
pids=""
for i in 1 2 3 4 5 6 7 8; do
    ( "$FIRNC" main.fi -o par$i 2> par$i.log; echo $? > par$i.rc ) &
    pids="$pids $!"
done
wait $pids
ok=0
for i in 1 2 3 4 5 6 7 8; do [ "$(cat par$i.rc)" = "0" ] && cmp -s par$i cold || ok=1; done
check "8 parallel builds all succeed and equal the cold build" $ok
[ "$(objects)" -eq 1 ]; check "exactly one entry afterwards" $?
ls "$FIRN_CACHE_DIR/objects" | grep -q '^\.tmp-'; [ $? -ne 0 ]; check "no temporary files left" $?
"$FIRNC" main.fi -o after 2> after.log
[ "$(ev after.log)" = "cache: hit " ]; check "hit after the parallel run" $?

echo "== 9. damaged entry = miss, rebuilt =="
f=$(ls "$FIRN_CACHE_DIR"/objects/*.fc | head -1)
printf 'garbage' > "$f"
"$FIRNC" main.fi -o dmg 2> dmg.log
[ "$(ev dmg.log)" = "cache: miss cache: store " ]; check "damaged entry = miss + store" $?
cmp -s dmg cold; check "rebuild identical" $?
"$FIRNC" main.fi -o dmg2 2> dmg2.log
[ "$(ev dmg2.log)" = "cache: hit " ]; check "entry healed" $?

echo "== 10. size cap =="
rm -rf "$FIRN_CACHE_DIR"
for v in 1 2 3 4 5 6; do
    sed -i "s/return [0-9]*/return $v/" util.fi
    FIRN_CACHE_MAX_MB=0 "$FIRNC" main.fi -o cap 2> /dev/null
done
[ "$(objects)" -eq 0 ]; rc=$?; check "cap 0 MB keeps nothing" $rc
rm -rf "$FIRN_CACHE_DIR"
for v in 1 2 3 4 5 6; do
    sed -i "s/return [0-9]*/return $v/" util.fi
    "$FIRNC" main.fi -o cap 2> /dev/null
done
[ "$(objects)" -eq 6 ]; check "default cap keeps all 6 entries" $?

echo "== 11. unwritable cache dir does not break the build =="
FIRN_CACHE_DIR=/proc/nope/cache "$FIRNC" main.fi -o ro 2> ro.log
check "build with unwritable cache dir succeeds" $?

echo
echo "cache proof: $PASS ok, $FAIL failed"
[ "$FAIL" -eq 0 ]
