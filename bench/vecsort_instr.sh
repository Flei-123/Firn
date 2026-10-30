#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/.."
FIRNC="${1:?firnc missing}"
W="$(mktemp -d)"
trap 'rm -rf "$W"' EXIT
count() {
    valgrind --tool=callgrind --callgrind-out-file="$W/cg.out" "$1" 2>&1 | grep 'refs:' | tr -d ', ' | sed 's/.*refs://'
}
grep -v 'vec_sort\[i32\]' bench/firn/vecsort.fi | sed 's/if bad != 0 { return 2 }//' > "$W/base.fi"
FIRNLIB="$PWD/lib" "$FIRNC" --opt-level=release-fast bench/firn/vecsort.fi -o "$W/firn" || exit 1
FIRNLIB="$PWD/lib" "$FIRNC" --opt-level=release-fast "$W/base.fi" -o "$W/firn_base" || exit 1
"$W/firn" || { echo "firn sort check failed"; exit 1; }
grep -v 'sort_it(&mut v);' bench/rust/vecsort.rs | sed 's/if black_box(bad) != 0 {/if false {/' > "$W/base.rs"
rustc -O bench/rust/vecsort.rs -o "$W/rust" 2>/dev/null || exit 1
rustc -O "$W/base.rs" -o "$W/rust_base" 2>/dev/null || exit 1
F=$(count "$W/firn"); FB=$(count "$W/firn_base"); R=$(count "$W/rust"); RB=$(count "$W/rust_base")
echo "firn sort instr: $((F - FB))"
echo "rust sort instr: $((R - RB))"
