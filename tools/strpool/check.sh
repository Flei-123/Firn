#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-compiler/target/release/firnc}"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
rc=0
for target in x86_64-linux aarch64-linux; do
    "$FIRNC" --target="$target" --emit=asm -o "$W/$target.s" tools/strpool/main.fi > "$W/c.log" 2>&1 || { echo "  compile failed: $target"; tail -5 "$W/c.log"; exit 1; }
    pools=$(grep -c '^\.Lstatic_text\.[0-9]*:' "$W/$target.s" || true)
    if [ "$pools" -eq 2 ]; then
        echo "  ok  $target: $pools literal pool entries for 5 literal uses"
    else
        echo "  FAIL $target: $pools literal pool entries, expected 2"
        rc=1
    fi
done
exit "$rc"
