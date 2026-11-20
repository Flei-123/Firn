#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
pass=0
fail=0

ok() { echo "PASS  $1"; pass=$((pass + 1)); }
bad() { echo "FAIL  $1"; fail=$((fail + 1)); }

for t in gcc objcopy; do
    command -v "$t" > /dev/null 2>&1 || { echo "SKIP: $t is missing"; exit 0; }
done
[ -x "$FIRNC" ] || { echo "SKIP: no firnc at $FIRNC"; exit 0; }

"$FIRNC" tools/cbind/firn_bindgen.fi -o "$W/bindgen" > "$W/build.log" 2>&1 || { cat "$W/build.log"; echo "FAIL  build firn-bindgen"; exit 1; }
BG="$W/bindgen"

layout_check() {
    local name=$1 header=$2
    shift 2
    "$BG" "$@" --emit layout "$header" > "$W/$name.pred" 2> "$W/$name.warn" || { bad "$name: bindgen layout"; return; }
    "$BG" "$@" --emit check-c "$header" -o "$W/$name.c" 2> /dev/null || { bad "$name: check-c"; return; }
    local inc=()
    for a in "$@"; do [ "$a" != "-I" ] && inc+=("-I$a"); done
    gcc -I "$ROOT" "${inc[@]}" -o "$W/$name.cbin" "$W/$name.c" 2> "$W/$name.gcc.log" || { bad "$name: gcc check build"; head -5 "$W/$name.gcc.log"; return; }
    "$W/$name.cbin" > "$W/$name.gcc"
    if diff -q "$W/$name.pred" "$W/$name.gcc" > /dev/null; then
        ok "$name: predicted layout == gcc ($(grep -c '^S ' "$W/$name.gcc") records, $(grep -c '^O ' "$W/$name.gcc") offsets)"
    else
        bad "$name: predicted layout differs from gcc"
        diff "$W/$name.pred" "$W/$name.gcc" | head -10
    fi
    "$BG" "$@" --emit check-fi "$header" -o "$W/$name.fi" 2> /dev/null || { bad "$name: check-fi"; return; }
    "$FIRNC" "$W/$name.fi" -o "$W/$name.fbin" > "$W/$name.firn.log" 2>&1 || { bad "$name: generated Firn does not compile"; head -8 "$W/$name.firn.log"; return; }
    "$W/$name.fbin" > "$W/$name.firnout"
    if diff -q "$W/$name.firnout" "$W/$name.gcc" > /dev/null; then
        ok "$name: Firn size_of/offsets == gcc"
    else
        bad "$name: Firn layout differs from gcc"
        diff "$W/$name.firnout" "$W/$name.gcc" | head -10
    fi
}

layout_check kernel_like tools/cbind/cases/kernel_like.h
layout_check zlib /usr/include/zlib.h
[ -f /usr/include/stdio.h ] && layout_check linux_input /usr/include/linux/input.h --system -I /usr/include

"$BG" tools/cbind/cases/kernel_like.h --emit firn --lib kl -o "$W/kl.fi" 2> "$W/kl.warn"
grep -q "^const KL_FLAG_B: i32 = 24$" "$W/kl.fi" && ok "constants: macro expressions evaluated" || bad "constants: KL_FLAG_B"
grep -q "^const KL_ERR: i32 = 0 - 1$" "$W/kl.fi" && ok "constants: negative enum value" || bad "constants: KL_ERR"
grep -q "^const KL_PCI: i32 = 0$" "$W/kl.fi" && ok "preprocessor: #ifdef else branch" || bad "preprocessor: KL_PCI"
grep -q "extern fn kl_insert(root: \*mut \*mut kl_node, key: i32) -> i32;" "$W/kl.fi" && ok "functions: pointer to pointer" || bad "functions: kl_insert"
grep -q "visit: u64" "$W/kl.fi" && ok "functions: function pointer is u64" || bad "functions: visit"
grep -q "bit-field" "$W/kl.warn" && ok "bit-fields: warning issued" || bad "bit-fields: no warning"
grep -q "kl_logf" "$W/kl.warn" && ok "variadic: warning issued" || bad "variadic: no warning"
"$BG" tools/cbind/cases/kernel_like.h --emit firn -o "$W/kl_nolib.fi" 2> /dev/null
printf 'fn main() -> i32 {\n    return 0\n}\n' >> "$W/kl_nolib.fi"
"$FIRNC" -c "$W/kl_nolib.fi" -o "$W/kl.o" > "$W/kl.log" 2>&1 && ok "generated bindings compile (with extern fns)" || { bad "generated bindings compile"; head -8 "$W/kl.log"; }

G=tools/cbind/cases/glue.h
"$BG" "$G" --wrap twice --wrap dot --wrap "ADD3:int(int,int,int)" --wrap "MAXI:int(int,int)" --emit glue -o "$W/glue.c" 2> /dev/null
"$BG" "$G" --wrap twice --wrap dot --wrap "ADD3:int(int,int,int)" --wrap "MAXI:int(int,int)" --emit firn -o "$W/glue_decl.fi" 2> /dev/null
cat "$W/glue_decl.fi" tools/cbind/cases/glue_prog.fi > "$W/glue_full.fi"
gcc -c -I "$ROOT" -o "$W/glue.o" "$W/glue.c" 2> "$W/glue.gcc.log" || { bad "glue: C glue compiles"; head -5 "$W/glue.gcc.log"; }
"$FIRNC" -c "$W/glue_full.fi" -o "$W/prog.o" > "$W/glue.firn.log" 2>&1 || { bad "glue: Firn object"; head -8 "$W/glue.firn.log"; }
if [ -f "$W/prog.o" ] && [ -f "$W/glue.o" ]; then
    objcopy --localize-symbol=_start --localize-symbol=main "$W/prog.o"
    gcc -o "$W/glue.bin" tools/cbind/cases/glue_host.c "$W/prog.o" "$W/glue.o" 2> "$W/glue.link.log" && "$W/glue.bin" && ok "glue: static inline + macros callable from Firn" || { bad "glue: link/run"; head -5 "$W/glue.link.log"; }
fi

FD=$W/driver
mkdir -p "$FD"
DRV=(env FIRNC="$FIRNC" FIRN_ROOT="$ROOT" FIRN_CACHE="$W/cache" bash tools/firn/firn)
"${DRV[@]}" bindgen "$G" --wrap twice --wrap "ADD3:int(int,int,int)" -o "$FD/glue_decl.fi" 2> "$FD/bindgen.log"
printf 'fn main() -> i32 {\n    return twice(21) + ADD3(1, 2, 3) - 48\n}\n' > "$FD/main_tail.fi"
cat "$FD/glue_decl.fi" "$FD/main_tail.fi" > "$FD/app.fi"
[ -f "$FD/glue_decl.glue.c" ] && ok "driver: firn bindgen writes the glue file" || bad "driver: firn bindgen writes the glue file"
"${DRV[@]}" build "$FD/app.fi" -o "$FD/app" > "$FD/build.log" 2>&1 && "$FD/app"
[ $? -eq 0 ] && ok "driver: firn build links the glue automatically (exit 0)" || { bad "driver: firn build links the glue automatically"; head -5 "$FD/build.log"; }
"${DRV[@]}" bindgen tools/cbind/cases/kernel_like.h --lib kl > "$FD/kl_stdout.fi" 2> /dev/null
grep -q "^extern fn kl_insert" "$FD/kl_stdout.fi" && ok "driver: firn bindgen prints bindings" || bad "driver: firn bindgen prints bindings"

F=tools/cbind/cases/ffi_features.h
FW=(--wrap "vsum3=vsum:int(int,int,int,int)" --wrap "vsum1=vsum:int(int,int)" --wrap "dmix_ii=dmix:double(const char*,int,int,int,double)")
"$BG" "$F" "${FW[@]}" --emit glue -o "$W/feat_glue.c" 2> /dev/null
"$BG" "$F" "${FW[@]}" --emit firn -o "$W/feat_decl.fi" 2> "$W/feat.warn"
cat "$W/feat_decl.fi" tools/cbind/cases/ffi_features_prog.fi > "$W/feat_full.fi"
grep -q "^const NAME_STR: str = \"fire.tfly\"$" "$W/feat_decl.fi" && ok "consts: string macro" || bad "consts: string macro"
grep -q "^const VERSION_STR: str = \"1.2\"$" "$W/feat_decl.fi" && ok "consts: concatenated string macro" || bad "consts: concatenated string macro"
grep -q "^const NEG_E: f64 = 0.0 - 2.5e-3$" "$W/feat_decl.fi" && ok "consts: float macro" || bad "consts: float macro"
grep -q "^const HDR_SIZE: i32 = 12$" "$W/feat_decl.fi" && ok "consts: sizeof(typedef) in enum" || bad "consts: sizeof(typedef) in enum"
grep -q "^fn bits_get_b(" "$W/feat_decl.fi" && ok "bit-fields: accessors generated" || bad "bit-fields: accessors generated"
gcc -c -I "$ROOT" -o "$W/feat_glue.o" "$W/feat_glue.c" 2> "$W/feat.gcc.log" || { bad "features: C glue compiles"; head -5 "$W/feat.gcc.log"; }
"$FIRNC" -c "$W/feat_full.fi" -o "$W/feat_prog.o" > "$W/feat.firn.log" 2>&1 || { bad "features: Firn object"; head -8 "$W/feat.firn.log"; }
if [ -f "$W/feat_prog.o" ] && [ -f "$W/feat_glue.o" ]; then
    objcopy --localize-symbol=_start --localize-symbol=main "$W/feat_prog.o"
    gcc -I tools/cbind/cases -o "$W/feat.bin" tools/cbind/cases/ffi_features_host.c "$W/feat_prog.o" "$W/feat_glue.o" 2> "$W/feat.link.log" && "$W/feat.bin" && ok "features: bit-field accessors, sizeof consts, string/float consts, variadics == gcc" || { bad "features: link/run"; head -5 "$W/feat.link.log"; }
fi

if command -v aarch64-linux-gnu-gcc > /dev/null 2>&1 && command -v qemu-aarch64 > /dev/null 2>&1; then
    for name in kernel_like ffi_features; do
        "$BG" --emit layout "tools/cbind/cases/$name.h" > "$W/$name.a64.pred" 2> /dev/null
        "$BG" --emit check-c "tools/cbind/cases/$name.h" -o "$W/$name.a64.c" 2> /dev/null
        if aarch64-linux-gnu-gcc -static -I "$ROOT" -o "$W/$name.a64" "$W/$name.a64.c" 2> /dev/null && qemu-aarch64 "$W/$name.a64" > "$W/$name.a64.out" && diff -q "$W/$name.a64.pred" "$W/$name.a64.out" > /dev/null; then
            ok "$name: aarch64 LP64 layout (gcc + qemu) == x86-64 prediction"
        else
            bad "$name: aarch64 layout differs"
        fi
    done
fi

if command -v apt-get > /dev/null 2>&1 && command -v dpkg > /dev/null 2>&1; then
    (cd "$W" && mkdir -p sq && cd sq && timeout 60 apt-get download libsqlite3-dev > /dev/null 2>&1 && dpkg -x libsqlite3-dev*.deb x 2> /dev/null)
    SQ="$W/sq/x/usr/include/sqlite3.h"
    if [ -f "$SQ" ]; then
        layout_check sqlite3 "$SQ"
        "$BG" "$SQ" --emit firn -o "$W/sq.fi" 2> "$W/sq.warn"
        printf 'fn main() -> i32 {\n    return 0\n}\n' >> "$W/sq.fi"
        "$FIRNC" -c "$W/sq.fi" -o "$W/sq.o" > "$W/sq.log" 2>&1 && ok "sqlite3: $(grep -c '^extern fn' "$W/sq.fi") extern fns compile" || { bad "sqlite3: bindings compile"; head -8 "$W/sq.log"; }
    else
        echo "SKIP  sqlite3 (libsqlite3-dev not downloadable)"
    fi
fi

echo "cbind: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
