#!/usr/bin/env bash
# ARCHITECTURE CHECK: field access is separated from the memory location.
#
# Background: DESIGNZIELE.md 8. As long as `a.b` is written out as
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
FEHLER=0

melde() { echo "FEHLER: $1"; FEHLER=1; }

# --- 1. Op::PtrAdd ---
TREFFER=$(grep -rn 'Op::PtrAdd' "$SRC"/lower.rs "$SRC"/lower_match.rs 2>/dev/null || true)
# Exactly one place is allowed: the body of `ptradd_const` (base + off).
UEBRIG=$(printf '%s\n' "$TREFFER" | grep -v 'Op::PtrAdd { base, off: o }' || true)
if [ -n "$UEBRIG" ]; then
    melde "Op::PtrAdd wird im Lowering ausserhalb von ptradd_const/layout.rs gebaut:"
    printf '%s\n' "$UEBRIG" | sed 's/^/        /'
fi

# --- 2. calls of ptradd_const ---
while IFS= read -r zeile; do
    [ -z "$zeile" ] && continue
    case "$zeile" in
        *"fn ptradd_const"*) continue ;;      # die Definition selbst
        *"ABI-Wortkopie"*)   continue ;;      # ausdruecklich erlaubt
        *"layout.rs"*)       continue ;;
        *"///"*)             continue ;;      # Doku-Kommentar
        *"//"*"ptradd_const"*) continue ;;
    esac
    melde "ungekennzeichneter ptradd_const-Aufruf (Feldzugriff gehoert nach layout.rs):"
    echo "        $zeile"
done < <(grep -rn 'ptradd_const' "$SRC"/lower.rs "$SRC"/lower_match.rs 2>/dev/null || true)

# --- 3. an offset directly into an address ---
for muster in '\.offset' 'offsets\.get'; do
    T=$(grep -rn "$muster" "$SRC"/lower.rs 2>/dev/null || true)
    if [ -n "$T" ]; then
        melde "Feld-Versatz wird in lower.rs berechnet statt in layout.rs:"
        printf '%s\n' "$T" | sed 's/^/        /'
    fi
done

# --- 4. layout.rs exists and is used ---
[ -f "$SRC/layout.rs" ] || melde "compiler/src/layout.rs fehlt"
grep -q 'mod layout;' "$SRC/main.rs" || melde "layout ist in main.rs nicht angemeldet"

if [ "$FEHLER" -ne 0 ]; then
    echo
    echo "Die Trennung Feldzugriff <-> Speicherort ist verletzt (DESIGNZIELE.md 8)."
    exit 1
fi

ANZ=$(grep -c 'pub(crate) fn ' "$SRC/layout.rs")
echo "OK: Feldzugriff und Speicherort getrennt ($ANZ Zugaenge in layout.rs, keine Umgehung)."
