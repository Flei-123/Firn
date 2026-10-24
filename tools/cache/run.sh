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
ev() { grep -o 'cache: [a-z]*' "$1" | grep -v 'cache: obj' | tr '\n' ' '; }
evo() { grep -o 'cache: [a-z]*' "$1" | tr '\n' ' '; }
objfiles() { ls "$FIRN_CACHE_DIR/objects" 2>/dev/null | grep -c '\.o$'; }

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

echo "== 12. assembled object cache =="
rm -rf "$FIRN_CACHE_DIR"
sed -i 's/return [0-9]*/return 5/' util.fi
"$FIRNC" main.fi -o o_cold 2> o_cold.log
[ "$(evo o_cold.log)" = "cache: miss cache: store cache: objsave " ]; check "cold exe = miss+store+objsave" $?
[ "$(objfiles)" -eq 1 ]; check "one object file stored" $?
"$FIRNC" main.fi -o o_warm 2> o_warm.log
[ "$(evo o_warm.log)" = "cache: hit cache: objload " ]; check "warm exe = hit+objload" $?
cmp -s o_cold o_warm; check "object hit executable identical" $?
"$FIRNC" --no-cache main.fi -o o_plain 2> /dev/null
cmp -s o_warm o_plain; check "object hit identical to uncached" $?
./o_warm; [ $? -eq 12 ]; check "object hit executable runs (exit 12)" $?
[ ! -e o_warm.s ] && [ ! -e o_warm.o ]; check "no stray .s/.o after object hit" $?
"$FIRNC" main.fi -o o_other 2> o_other.log
[ "$(evo o_other.log)" = "cache: hit cache: objload " ]; check "another -o = objload" $?
cmp -s o_other o_plain; check "identical under another name" $?
"$FIRNC" --keep-asm main.fi -o o_keep0 2> /dev/null
rm -f o_keep0.s
"$FIRNC" --keep-asm main.fi -o o_keep 2> o_keep.log
[ "$(evo o_keep.log)" = "cache: hit cache: objsave " ]; check "--keep-asm reads the asm instead" $?
[ -s o_keep.s ] && cmp -s o_keep o_plain; check "--keep-asm keeps .s, executable identical" $?
"$FIRNC" --no-cache --keep-asm main.fi -o o_keep2 2> /dev/null
cmp -s o_keep.s o_keep2.s; check ".s identical to uncached" $?
"$FIRNC" -c --profile=kernel main.fi -o ok1.o 2> ok1.log
"$FIRNC" -c --profile=kernel main.fi -o ok2.o 2> ok2.log
[ "$(evo ok2.log)" = "cache: hit cache: objload " ]; check "kernel object = hit+objload" $?
"$FIRNC" --no-cache -c --profile=kernel main.fi -o ok3.o 2> /dev/null
cmp -s ok1.o ok2.o && cmp -s ok2.o ok3.o; check "kernel object identical (miss, hit, uncached)" $?
"$FIRNC" -c --target=aarch64-linux main.fi -o oa1.o 2> /dev/null
"$FIRNC" -c --target=aarch64-linux main.fi -o oa2.o 2> oa2.log
[ "$(evo oa2.log)" = "cache: hit cache: objload " ]; check "aarch64 object = hit+objload" $?
"$FIRNC" --no-cache -c --target=aarch64-linux main.fi -o oa3.o 2> /dev/null
cmp -s oa1.o oa2.o && cmp -s oa2.o oa3.o; check "aarch64 object identical (miss, hit, uncached)" $?
"$FIRNC" statics.fi -o os1 2> /dev/null
"$FIRNC" statics.fi -o os2 2> os2.log
[ "$(evo os2.log)" = "cache: hit cache: objload " ]; check "static program = objload" $?
cmp -s os1 os2; check "static program identical on object hit" $?
"$FIRNC" --asm-intern -c main.fi -o oi1.o 2> oi1.log
[ "$(evo oi1.log)" = "cache: miss cache: store cache: objsave " ]; check "integrated assembler = own entry" $?
"$FIRNC" --asm-intern -c main.fi -o oi2.o 2> oi2.log
[ "$(evo oi2.log)" = "cache: hit cache: objload " ]; check "integrated assembler objload" $?
"$FIRNC" --no-cache --asm-intern -c main.fi -o oi3.o 2> /dev/null
cmp -s oi1.o oi2.o && cmp -s oi2.o oi3.o; check "integrated assembler object identical" $?
REAL_AS=$(command -v as)
mkdir -p "$T/fakebin"
printf '#!/bin/sh\nif [ "$1" = "--version" ]; then echo "GNU assembler (fake) 9.9"; exit 0; fi\nexec %s "$@"\n' "$REAL_AS" > "$T/fakebin/as"
chmod +x "$T/fakebin/as"
PATH="$T/fakebin:$PATH" "$FIRNC" main.fi -o ov1 2> ov1.log
[ "$(evo ov1.log)" = "cache: hit cache: objsave " ]; check "other assembler version = object miss" $?
PATH="$T/fakebin:$PATH" "$FIRNC" main.fi -o ov2 2> ov2.log
[ "$(evo ov2.log)" = "cache: hit cache: objload " ]; check "other assembler version cached on its own" $?
"$FIRNC" main.fi -o ov3 2> ov3.log
[ "$(evo ov3.log)" = "cache: hit cache: objload " ]; check "original assembler object still there" $?
cmp -s ov1 o_plain && cmp -s ov2 o_plain && cmp -s ov3 o_plain; check "all identical to uncached" $?
rm -rf "$FIRN_CACHE_DIR"
"$FIRNC" main.fi -o od0 2> /dev/null
f=$(ls "$FIRN_CACHE_DIR"/objects/*.o | head -1)
printf 'garbage' > "$f"
"$FIRNC" main.fi -o od1 2> od1.log
[ "$(evo od1.log)" = "cache: hit cache: objsave " ]; check "damaged object = rebuilt from asm" $?
cmp -s od1 o_plain; check "rebuilt executable identical" $?
"$FIRNC" main.fi -o od2 2> od2.log
[ "$(evo od2.log)" = "cache: hit cache: objload " ]; check "object healed" $?
"$FIRNC" env.fi -o on1 2> on1.log
[ "$(evo on1.log)" = "cache: miss cache: skip " ]; check "uncacheable program stores no object" $?
ls "$FIRN_CACHE_DIR/objects" | grep -q '^\.tmp-'; [ $? -ne 0 ]; check "no temporary files left" $?
rm -rf "$FIRN_CACHE_DIR"
for i in 1 2 3 4 5 6; do
    ( "$FIRNC" main.fi -o pp$i 2> /dev/null; echo $? > pp$i.rc ) &
done
wait
ok=0
for i in 1 2 3 4 5 6; do [ "$(cat pp$i.rc)" = "0" ] && cmp -s pp$i o_plain || ok=1; done
check "6 parallel cold builds equal the uncached executable" $ok
"$FIRNC" main.fi -o pp_after 2> pp_after.log
[ "$(evo pp_after.log)" = "cache: hit cache: objload " ]; check "objload after the parallel run" $?
rm -rf "$FIRN_CACHE_DIR"
FIRN_CACHE_MAX_MB=0 "$FIRNC" main.fi -o oc0 2> /dev/null
[ "$(objfiles)" -eq 0 ]; check "cap 0 MB keeps no object either" $?

echo "== 13. firnc1 accepts --no-cache =="
"$FIRNC" --no-cache "$ROOT/bin/firnc1.fi" -o "$T/firnc1" 2> /dev/null
check "firnc1 builds" $?
FIRNLIB="$ROOT/lib" "$T/firnc1" --no-cache main.fi -o c1_plain 2> /dev/null
check "firnc1 --no-cache builds" $?
./c1_plain; [ $? -eq 12 ]; check "firnc1 --no-cache executable runs (exit 12)" $?

echo
echo "cache proof: $PASS ok, $FAIL failed"
[ "$FAIL" -eq 0 ]
