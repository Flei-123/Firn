#!/usr/bin/env bash
# Proof for the compile cache of --target=wasm32-browser (docs/CACHE.md).
# Usage: bash tools/cache/run_wasm.sh
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
FAIL=0
PASS=0
check() { if [ "$2" = "0" ]; then PASS=$((PASS + 1)); else echo "  FAIL  $1"; FAIL=$((FAIL + 1)); fi; }
export FIRN_CACHE_LOG=1
export FIRN_CACHE_DIR="$T/cache"
export FIRNLIB="$ROOT/lib"
unset FIRN_NO_CACHE
W="$T/w"
mkdir -p "$W"
cp tools/cache/cases/* "$W/"
cd "$W"
WA="--target=wasm32-browser"
ev() { grep -o 'cache: [a-z]*' "$1" | grep -v 'cache: obj' | tr '\n' ' '; }
entries() { ls "$FIRN_CACHE_DIR/objects" 2>/dev/null | grep -c '\.wasm$'; }
runwasm() { node "$ROOT/tools/wasm/run.mjs" "$1" > /dev/null 2>&1; echo $?; }

echo "== 1. miss, then hit, byte-identical to uncached =="
"$FIRNC" $WA main.fi -o cold.wasm 2> cold.log
check "cold build" $?
[ "$(ev cold.log)" = "cache: miss cache: store " ]; check "cold = miss+store" $?
"$FIRNC" $WA main.fi -o warm.wasm 2> warm.log
[ "$(ev warm.log)" = "cache: hit " ]; check "warm = hit" $?
"$FIRNC" $WA --no-cache main.fi -o plain.wasm 2> plain.log
[ "$(ev plain.log)" = "cache: off " ]; check "--no-cache = off" $?
cmp -s cold.wasm warm.wasm; check "hit module identical" $?
cmp -s cold.wasm plain.wasm; check "uncached module identical" $?
[ "$(runwasm warm.wasm)" = "12" ]; check "hit module runs (exit 12)" $?
"$FIRNC" $WA main.fi -o sub.wasm 2> /dev/null
cmp -s cold.wasm sub.wasm; check "different -o name still a hit, same bytes" $?
mkdir -p outdir
"$FIRNC" $WA main.fi -o outdir/x.wasm 2> o.log
[ "$(ev o.log)" = "cache: hit " ] && cmp -s cold.wasm outdir/x.wasm; check "hit into another directory" $?
FIRN_NO_CACHE=1 "$FIRNC" $WA main.fi -o envoff.wasm 2> envoff.log
[ "$(ev envoff.log)" = "cache: off " ]; check "FIRN_NO_CACHE = off" $?

echo "== 2. import change = miss =="
n=$(entries)
sed -i 's/return 5/return 6/' util.fi
"$FIRNC" $WA main.fi -o changed.wasm 2> changed.log
[ "$(ev changed.log)" = "cache: miss cache: store " ]; check "import change = miss" $?
[ "$(entries)" -eq $((n + 1)) ]; check "new entry added" $?
"$FIRNC" $WA --no-cache main.fi -o changed_plain.wasm 2> /dev/null
cmp -s changed.wasm changed_plain.wasm; check "changed module identical to uncached" $?
[ "$(runwasm changed.wasm)" = "13" ]; check "changed module runs (exit 13)" $?
"$FIRNC" $WA main.fi -o changed2.wasm 2> changed2.log
[ "$(ev changed2.log)" = "cache: hit " ]; check "second build of the changed tree = hit" $?
sed -i 's/return 6/return 5/' util.fi
"$FIRNC" $WA main.fi -o back.wasm 2> back.log
[ "$(ev back.log)" = "cache: hit " ] && cmp -s back.wasm cold.wasm; check "reverting the import = hit on the first entry" $?

echo "== 3. flags and target =="
"$FIRNC" $WA --opt-level=release-safe main.fi -o rs.wasm 2> rs.log
[ "$(ev rs.log)" = "cache: miss cache: store " ]; check "other opt level = miss" $?
"$FIRNC" $WA --opt-level=release-safe main.fi -o rs2.wasm 2> rs2.log
[ "$(ev rs2.log)" = "cache: hit " ] && cmp -s rs.wasm rs2.wasm; check "other opt level = hit afterwards" $?
"$FIRNC" main.fi -o native 2> nat.log
[ "$(ev nat.log)" = "cache: miss cache: store " ]; check "native build of the same source = own entry" $?
"$FIRNC" $WA main.fi -o again.wasm 2> again.log
[ "$(ev again.log)" = "cache: hit " ] && cmp -s again.wasm cold.wasm; check "wasm entry untouched by the native build" $?
FIRN_CACHE_PROOF=x "$FIRNC" $WA main.fi -o fenv.wasm 2> fenv.log
[ "$(ev fenv.log)" = "cache: miss cache: store " ]; check "FIRN* environment variable = miss" $?
"$FIRNC" $WA --emit=asm main.fi -o t.wat 2> wat.log
[ "$(ev wat.log)" = "cache: off " ]; check "--emit=asm on wasm is not cached" $?
"$FIRNC" $WA --timings main.fi -o tm.wasm 2> tm.log
[ "$(ev tm.log)" = "cache: off " ]; check "--timings is not cached" $?

echo "== 4. compiler binary =="
mkdir -p bin
cp "$FIRNC" bin/firnc
printf 'x' >> bin/firnc
bin/firnc $WA main.fi -o c2.wasm 2> c2.log
[ "$(ev c2.log)" = "cache: miss cache: store " ]; check "other compiler binary = miss" $?
cmp -s c2.wasm cold.wasm; check "other binary, same module" $?

echo "== 5. include file and environment constants (hidden inputs) =="
"$FIRNC" $WA inc_wasm.fi -o i1.wasm 2> i1.log
[ "$(ev i1.log)" = "cache: miss cache: store " ]; check "include: miss+store (not skipped)" $?
"$FIRNC" $WA inc_wasm.fi -o i2.wasm 2> i2.log
[ "$(ev i2.log)" = "cache: hit " ] && cmp -s i1.wasm i2.wasm; check "include: hit" $?
printf 'hello world\n' > data.txt
"$FIRNC" $WA inc_wasm.fi -o i3.wasm 2> i3.log
[ "$(ev i3.log)" = "cache: miss cache: store " ]; check "include file changed = miss" $?
"$FIRNC" $WA --no-cache inc_wasm.fi -o i3p.wasm 2> /dev/null
cmp -s i3.wasm i3p.wasm && ! cmp -s i1.wasm i3.wasm; check "include change reaches the module" $?
"$FIRNC" $WA env_wasm.fi -o e1.wasm 2> e1.log
[ "$(ev e1.log)" = "cache: miss cache: store " ]; check "env const: miss+store" $?
"$FIRNC" $WA env_wasm.fi -o e2.wasm 2> e2.log
[ "$(ev e2.log)" = "cache: hit " ] && cmp -s e1.wasm e2.wasm; check "env const: hit" $?
FIRN_CACHE_PROOF=other "$FIRNC" $WA env_wasm.fi -o e3.wasm 2> e3.log
[ "$(ev e3.log)" = "cache: miss cache: store " ]; check "env const value changed = miss" $?
FIRN_CACHE_PROOF=other "$FIRNC" $WA --no-cache env_wasm.fi -o e3p.wasm 2> /dev/null
cmp -s e3.wasm e3p.wasm && ! cmp -s e1.wasm e3.wasm; check "env change reaches the module" $?

echo "== 6. damaged entry, failed build =="
for g in "$FIRN_CACHE_DIR/objects"/*.wasm; do
    b=$(stat -c %s "$g")
    printf '\377' | dd of="$g" bs=1 seek=$((b - 2)) conv=notrunc 2> /dev/null
done
"$FIRNC" $WA main.fi -o dmg.wasm 2> dmg.log
[ "$(ev dmg.log)" = "cache: miss cache: store " ]; check "damaged entry = miss, rewritten" $?
cmp -s dmg.wasm cold.wasm; check "module after damaged entry identical" $?
n=$(entries)
"$FIRNC" $WA bad.fi -o bad.wasm 2> /dev/null
[ $? -ne 0 ] && [ "$(entries)" -eq "$n" ]; check "failed build stores nothing" $?

echo "== 7. parallel builds =="
rm -rf "$FIRN_CACHE_DIR"
for i in 1 2 3 4 5 6 7 8; do
    "$FIRNC" $WA main.fi -o par$i.wasm 2> par$i.log &
done
wait
ok=0
for i in 1 2 3 4 5 6 7 8; do cmp -s par$i.wasm cold.wasm || ok=1; done
check "eight parallel builds all identical" $ok
"$FIRNC" $WA main.fi -o par_after.wasm 2> par_after.log
[ "$(ev par_after.log)" = "cache: hit " ] && cmp -s par_after.wasm cold.wasm; check "hit after the parallel run" $?
rm -rf "$FIRN_CACHE_DIR"
FIRN_CACHE_MAX_MB=0 "$FIRNC" $WA main.fi -o cap0.wasm 2> /dev/null
[ "$(entries)" -eq 0 ]; check "cap 0 MB keeps no wasm entry" $?

echo "== 8. resolve manifest (hit without resolving) =="
cat > shadow.fi <<'EOT'
import std.num

fn main() -> i32 {
    return 3
}
EOT
"$FIRNC" $WA shadow.fi -o s1.wasm 2> s1.log
[ "$(ev s1.log)" = "cache: miss cache: store " ]; check "shadow case: miss+store" $?
"$FIRNC" $WA shadow.fi -o s2.wasm 2> s2.log
[ "$(ev s2.log)" = "cache: hit " ] && cmp -s s1.wasm s2.wasm; check "shadow case: hit" $?
ls "$FIRN_CACHE_DIR/objects"/*.rm > /dev/null 2>&1; check "manifest file written" $?
mkdir -p std
cp "$ROOT"/lib/std/*.fi std/
"$FIRNC" $WA shadow.fi -o s3.wasm 2> s3.log
[ "$(ev s3.log)" = "cache: miss cache: store " ]; check "new file that shadows an import = miss" $?
"$FIRNC" $WA --no-cache shadow.fi -o s3p.wasm 2> /dev/null
cmp -s s3.wasm s3p.wasm; check "shadowed build identical to uncached" $?
rm -rf std
"$FIRNC" $WA shadow.fi -o s4.wasm 2> s4.log
[ "$(ev s4.log)" = "cache: hit " ] && cmp -s s1.wasm s4.wasm; check "shadow removed = hit on the first entry" $?
printf '// touched\n' >> shadow.fi
"$FIRNC" $WA shadow.fi -o s5.wasm 2> s5.log
[ "$(ev s5.log)" = "cache: miss cache: store " ]; check "edited root file = miss" $?
touch shadow.fi
"$FIRNC" $WA shadow.fi -o s6.wasm 2> s6.log
[ "$(ev s6.log)" = "cache: hit " ] && cmp -s s5.wasm s6.wasm; check "touch without change = hit" $?
FIRNLIB="$T/nolib" "$FIRNC" $WA shadow.fi -o s7.wasm 2> s7.log
[ "$(ev s7.log)" = "cache: miss cache: store " ] || [ $? -ne 0 ]; check "other FIRNLIB = not a hit of the old entry" $?

echo
echo "wasm cache proof: $PASS ok, $FAIL failed"
[ "$FAIL" -eq 0 ]
