#!/usr/bin/env bash
# Proof for the compile cache of firnc1 (roadmap r989, docs/CACHE.md).
# Usage: bash tools/cache/run_c1.sh
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC0=${FIRNC:-$ROOT/compiler/target/release/firnc}
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
export FIRNLIB="$ROOT/lib"
"$FIRNC0" --no-cache bin/firnc1.fi -o "$T/firnc1" || exit 1
C1="$T/firnc1"
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
ev() { grep -o 'cache: [a-z]*' "$1" | tr '\n' ' '; }
nfc() { ls "$FIRN_CACHE_DIR/objects" 2>/dev/null | grep -c '\.fc$'; }
nfo() { ls "$FIRN_CACHE_DIR/objects" 2>/dev/null | grep -c '\.o$'; }

echo "== 1. miss, then hit, byte-identical to uncached =="
"$C1" main.fi -o cold 2> cold.log; check "cold build" $?
[ "$(ev cold.log)" = "cache: miss cache: store cache: objsave " ]; check "cold = miss+store+objsave" $?
"$C1" main.fi -o warm 2> warm.log
[ "$(ev warm.log)" = "cache: hit cache: objload " ]; check "warm = hit+objload" $?
"$C1" --no-cache main.fi -o plain 2> plain.log
[ "$(ev plain.log)" = "cache: off " ]; check "--no-cache = off" $?
cmp -s cold warm; check "hit executable identical" $?
cmp -s cold plain; check "uncached executable identical" $?
cmp -s cold.s warm.s && cmp -s cold.s plain.s; check "assembler text file identical (miss, hit, uncached)" $?
./warm; [ $? -eq 12 ]; check "hit executable runs (exit 12)" $?
"$C1" main.fi > out1.s 2> /dev/null
"$C1" main.fi > out2.s 2> out2.log
[ "$(ev out2.log)" = "cache: hit " ]; check "stdout build hit" $?
cmp -s out1.s out2.s && cmp -s out1.s cold.s; check "stdout assembler identical" $?
FIRN_NO_CACHE=1 "$C1" main.fi -o envoff 2> envoff.log
[ "$(ev envoff.log)" = "cache: off " ]; check "FIRN_NO_CACHE = off" $?
"$C1" --timings main.fi -o tm 2> tm.log
[ "$(ev tm.log)" = "cache: off " ]; check "--timings = off" $?

echo "== 2. import change = miss =="
n=$(nfc)
sed -i 's/return 5/return 6/' util.fi
"$C1" main.fi -o changed 2> changed.log
[ "$(ev changed.log)" = "cache: miss cache: store cache: objsave " ]; check "import change = miss" $?
[ "$(nfc)" -eq $((n + 1)) ]; check "new entry added" $?
"$C1" --no-cache main.fi -o changed_plain 2> /dev/null
cmp -s changed changed_plain; check "changed executable identical to uncached" $?
./changed; [ $? -eq 13 ]; check "changed executable runs (exit 13)" $?
sed -i 's/return 6/return 5/' util.fi
"$C1" main.fi -o back 2> back.log
[ "$(ev back.log)" = "cache: hit cache: objload " ]; check "reverting the import = hit on the first entry" $?

echo "== 3. flag and environment change = miss =="
"$C1" --opt-level=release-fast main.fi -o fast1 2> fast1.log
[ "$(ev fast1.log)" = "cache: miss cache: store cache: objsave " ]; check "opt level change = miss" $?
"$C1" --opt-level=release-fast main.fi -o fast2 2> fast2.log
[ "$(ev fast2.log)" = "cache: hit cache: objload " ]; check "same opt level again = hit" $?
cmp -s fast1 fast2; check "release-fast executable identical on hit" $?
FIRN_EAGER_LOWER=1 "$C1" main.fi -o env1 2> env1.log
[ "$(ev env1.log)" = "cache: miss cache: store cache: objsave " ]; check "FIRN_* switch = miss" $?

echo "== 4. output name does not matter =="
"$C1" main.fi -o other_name 2> other.log
[ "$(ev other.log)" = "cache: hit cache: objload " ]; check "different -o = hit" $?
cmp -s other_name cold; check "identical under another name" $?

echo "== 5. kernel object, static =="
"$C1" -c --profile=kernel main.fi -o k1.o 2> k1.log
"$C1" -c --profile=kernel main.fi -o k2.o 2> k2.log
[ "$(ev k2.log)" = "cache: hit cache: objload " ]; check "kernel object build = hit" $?
cmp -s k1.o k2.o; check "kernel object identical on hit" $?
"$C1" statics.fi -o s1 2> s1.log
"$C1" statics.fi -o s2 2> s2.log
[ "$(ev s2.log)" = "cache: hit cache: objload " ]; check "program with a static = hit" $?
cmp -s s1 s2; check "static program identical" $?

echo "== 6. not cacheable: build time environment =="
"$C1" env.fi -o e1 2> e1.log
[ "$(ev e1.log)" = "cache: miss cache: skip " ]; check "__env_or program is not stored" $?

echo "== 7. errors are never cached =="
n=$(nfc)
"$C1" bad.fi -o bad 2> bad.log
[ $? -ne 0 ]; check "broken program fails" $?
[ "$(nfc)" -eq "$n" ]; check "failure stores nothing" $?

echo "== 8. damaged entries =="
"$C1" main.fi -o warmup 2> /dev/null
key=$(ls -t "$FIRN_CACHE_DIR/objects"/*.fc | head -1 | sed 's/\.fc$//')
"$C1" main.fi -o probe 2> probe.log
tf="$key.fc"
of=$(ls "$key".*.o | head -1)
cp "$of" "$T/obj.keep"
printf 'x' | dd of="$of" bs=1 seek=100 conv=notrunc 2> /dev/null
"$C1" main.fi -o d1 2> d1.log
"$C1" main.fi -o d2 2> d2.log
cmp -s d1 d2; check "damaged object leaves a correct executable" $?
cp "$T/obj.keep" "$of"
printf 'garbage' > "$tf"
"$C1" main.fi -o t1 2> t1.log
grep -q 'cache: miss' t1.log; check "damaged text entry = miss" $?
cmp -s t1 cold; check "executable after damaged text identical" $?
"$C1" main.fi -o t2 2> t2.log
grep -q 'cache: hit' t2.log; check "entry rewritten = hit" $?

echo "== 9. concurrent builds =="
rm -rf "$FIRN_CACHE_DIR"
pids=""
for i in 1 2 3 4 5 6 7 8; do
    ( "$C1" main.fi -o par$i 2> par$i.log; echo $? > par$i.rc ) &
    pids="$pids $!"
done
wait $pids
ok=0
for i in 1 2 3 4 5 6 7 8; do [ "$(cat par$i.rc)" = "0" ] && cmp -s par$i cold || ok=1; done
check "8 parallel builds all succeed and equal the cold build" $ok
[ "$(nfc)" -eq 1 ]; check "exactly one text entry afterwards" $?
ls "$FIRN_CACHE_DIR/objects" | grep -q '^\.tmp-'; [ $? -ne 0 ]; check "no temporary files left" $?

echo "== 10. size cap =="
rm -rf "$FIRN_CACHE_DIR"
FIRN_CACHE_MAX_MB=0 "$C1" main.fi -o cap0 2> cap0.log
[ "$(nfc)" -eq 0 ] && [ "$(nfo)" -eq 0 ]; check "cap 0 keeps nothing" $?
cmp -s cap0 cold; check "executable under cap 0 identical" $?

echo "== 11. unwritable cache directory =="
FIRN_CACHE_DIR=/proc/nope "$C1" main.fi -o nw 2> nw.log
[ $? -eq 0 ]; check "build succeeds" $?
cmp -s nw cold; check "executable identical" $?

echo "== 12. firnc1 as the compiler of itself, cached =="
cd "$ROOT"
"$C1" --no-cache bin/firnc1.fi -o "$T/self1" 2> /dev/null
"$C1" bin/firnc1.fi -o "$T/self2" 2> "$T/self2.log"
"$C1" bin/firnc1.fi -o "$T/self3" 2> "$T/self3.log"
grep -q 'cache: hit' "$T/self3.log"; check "self build second run = hit" $?
cmp -s "$T/self1" "$T/self2" && cmp -s "$T/self1" "$T/self3"; check "self build identical (uncached, miss, hit)" $?
cmp -s "$T/self1.s" "$T/self3.s"; check "self build assembler text identical" $?

echo "== 13. resolve manifest (hit without lexing) =="
R="$T/rm"
mkdir -p "$R/lib"
cd "$R"
printf 'import helper\nfn main() -> i32 {\n    return helper.val()\n}\n' > main.fi
printf 'fn val() -> i32 {\n    return 3\n}\n' > lib/helper.fi
export FIRN_CACHE_DIR="$R/cache"
nrm() { ls "$FIRN_CACHE_DIR/objects" 2>/dev/null | grep -c '\.rm$'; }
FIRNLIB="$R/lib" "$C1" main.fi -o m1 2> m1.log
[ "$(ev m1.log)" = "cache: miss cache: store cache: objsave " ]; check "manifest build = miss" $?
[ "$(nrm)" -eq 1 ]; check "one manifest written" $?
FIRNLIB="$R/lib" "$C1" main.fi -o m2 2> m2.log
[ "$(ev m2.log)" = "cache: hit cache: objload " ]; check "manifest build = hit" $?
cmp -s m1 m2; check "manifest hit executable identical" $?
printf 'fn val() -> i32 {\n    return 9\n}\n' > helper.fi
FIRNLIB="$R/lib" "$C1" main.fi -o m3 2> m3.log
[ "$(ev m3.log)" = "cache: miss cache: store cache: objsave " ]; check "shadowing file = miss" $?
./m3; [ $? -eq 9 ]; check "shadowed module is used (exit 9)" $?
FIRNLIB="$R/lib" "$C1" main.fi -o m4 2> m4.log
[ "$(ev m4.log)" = "cache: hit cache: objload " ]; check "after shadowing = hit again" $?
rm helper.fi
FIRNLIB="$R/lib" "$C1" main.fi -o m5 2> m5.log
[ "$(ev m5.log)" = "cache: hit cache: objload " ]; check "removing the shadow = hit on the first entry" $?
cmp -s m1 m5; check "executable back to the first one" $?
printf 'fn val() -> i32 {\n    return 4\n}\n' > lib/helper.fi
FIRNLIB="$R/lib" "$C1" main.fi -o m6 2> m6.log
[ "$(ev m6.log)" = "cache: miss cache: store cache: objsave " ]; check "module edit = miss" $?
./m6; [ $? -eq 4 ]; check "edited module is used (exit 4)" $?
printf 'x' >> "$FIRN_CACHE_DIR/objects/$(ls "$FIRN_CACHE_DIR/objects" | grep '\.rm$' | head -1)"
FIRNLIB="$R/lib" "$C1" main.fi -o m7 2> m7.log
[ "$(ev m7.log)" = "cache: hit cache: objload " ]; check "damaged manifest falls back to the full key" $?
cmp -s m6 m7; check "damaged manifest executable identical" $?
FIRNLIB="$R/lib" "$C1" main.fi -o m8 2> m8.log
[ "$(ev m8.log)" = "cache: hit cache: objload " ]; check "manifest rewritten after damage" $?
cd "$ROOT"

echo "firnc1 cache: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
