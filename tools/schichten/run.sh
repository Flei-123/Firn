#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# ARCHITECTURE CHECK: field access is separated from the memory location.
#
# Background: DESIGN_GOALS.md 8. As long as `a.b` is written out as
# "base + offset" everywhere in the tree, the SoA arrangement (`SoaVec[T]`)
# cannot be added later. That is why an offset may only become an
# address in `layout.rs`.
#
# What is checked:
#   1. `Op::PtrAdd` is built outside of layout.rs only in the one
#      helper function `ptradd_const`.
#   2. `ptradd_const` is called outside of layout.rs only at places
#      marked as `// ABI-Wortkopie` (aggregate passing in
#      registers -- that is no field access).
#   3. No field offset (`.offset`, `offsets.get`) is put directly into
#      an address computation in the lowering.
set -euo pipefail
cd "$(dirname "$0")/../.."
SRC=compiler/src
ERRORS=0

report() { echo "ERROR: $1"; ERRORS=1; }

# --- 1. Op::PtrAdd ---
HITS=$(grep -rn 'Op::PtrAdd' "$SRC"/lower.rs "$SRC"/lower_match.rs 2>/dev/null || true)
# Exactly one place is allowed: the body of `ptradd_const` (base + off).
LEFT=$(printf '%s\n' "$HITS" | grep -v 'Op::PtrAdd { base, off: o }' || true)
if [ -n "$LEFT" ]; then
    report "Op::PtrAdd is built in the lowering outside of ptradd_const/layout.rs:"
    printf '%s\n' "$LEFT" | sed 's/^/        /'
fi

# --- 2. calls of ptradd_const ---
while IFS= read -r line; do
    [ -z "$line" ] && continue
    case "$line" in
        *"fn ptradd_const"*) continue ;;      # the definition itself
        *"ABI-Wortkopie"*)   continue ;;      # explicitly allowed
        *"layout.rs"*)       continue ;;
        *"///"*)             continue ;;      # a doc comment
        *"//"*"ptradd_const"*) continue ;;
    esac
    report "an unmarked ptradd_const call (field access belongs in layout.rs):"
    echo "        $line"
done < <(grep -rn 'ptradd_const' "$SRC"/lower.rs "$SRC"/lower_match.rs 2>/dev/null || true)

# --- 3. an offset directly into an address ---
for pattern in '\.offset' 'offsets\.get'; do
    T=$(grep -rn "$pattern" "$SRC"/lower.rs 2>/dev/null || true)
    if [ -n "$T" ]; then
        report "a field offset is computed in lower.rs instead of in layout.rs:"
        printf '%s\n' "$T" | sed 's/^/        /'
    fi
done

# --- 4. layout.rs exists and is used ---
[ -f "$SRC/layout.rs" ] || report "compiler/src/layout.rs is missing"
grep -q 'mod layout;' "$SRC/main.rs" || report "layout is not declared in main.rs"

if [ "$ERRORS" -ne 0 ]; then
    echo
    echo "The separation of field access and memory location is violated (DESIGN_GOALS.md 8)."
    exit 1
fi

CNT=$(grep -c 'pub(crate) fn ' "$SRC/layout.rs")
echo "OK: field access and memory location separated ($CNT entry points in layout.rs, no bypass)."
