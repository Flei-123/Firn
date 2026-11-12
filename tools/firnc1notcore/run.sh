#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC1=${FIRNC1:-$ROOT/.firnc1}
export FIRNLIB="$ROOT/lib"
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT
pass=0
fail=0
ok() { pass=$((pass + 1)); echo "OK    $1"; }
bad() { fail=$((fail + 1)); echo "FAIL  $1"; }

[ -x "$FIRNC1" ] || { echo "firnc1 is missing: $FIRNC1"; exit 1; }

check_stop() {
    local name=$1 source=$2 want_file=$3
    printf '%s\n' "$source" > "$TMPD/$name.fi"
    "$FIRNC1" "$TMPD/$name.fi" > "$TMPD/$name.s" 2> "$TMPD/$name.err"
    local rc=$?
    [ "$rc" = 3 ] && ok "$name: exit code 3" || bad "$name: exit code $rc, expected 3"
    grep -q "^[^ ]*$want_file:[0-9]*: error: firnc1 cannot compile this file" "$TMPD/$name.err" \
        && ok "$name: message names $want_file and a line" \
        || bad "$name: no message naming $want_file: $(head -c 200 "$TMPD/$name.err")"
}

check_stop cpu_direct $'import std.cpu\nfn main() -> i32 {\n    return 0\n}' std/cpu.fi
check_stop sha256_chain $'import std.crypto.sha256\nfn main() -> i32 {\n    return 0\n}' std/crypto/accel.fi

printf 'fn main() -> i32 {\n    return 0\n}\n' > "$TMPD/plain.fi"
"$FIRNC1" "$TMPD/plain.fi" > "$TMPD/plain.s" 2> "$TMPD/plain.err" \
    && ok "plain file still compiles" || bad "plain file fails"

echo "SUMMARY pass=$pass fail=$fail"
[ "$fail" = 0 ]
