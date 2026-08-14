#!/usr/bin/env bash
# ARCHITEKTURPRUEFUNG: Feldzugriff ist vom Speicherort getrennt.
#
# Hintergrund: DESIGNZIELE.md §8. Solange `a.b` im ganzen Baum fest als
# "Basis + Versatz" ausgeschrieben wird, ist die SoA-Anordnung (`SoaVec[T]`)
# nicht nachruestbar. Deshalb darf aus einem Versatz NUR in `layout.rs` eine
# Adresse werden.
#
# Geprueft wird:
#   1. `Op::PtrAdd` wird ausserhalb von layout.rs nur in der einen
#      Hilfsfunktion `ptradd_const` gebaut.
#   2. `ptradd_const` wird ausserhalb von layout.rs nur an Stellen gerufen,
#      die als `// ABI-Wortkopie` gekennzeichnet sind (Aggregatuebergabe in
#      Registern — das ist kein Feldzugriff).
#   3. Kein Feld-Versatz (`.offset`, `offsets.get`) wird im Lowering direkt in
#      eine Adressrechnung gesteckt.
set -euo pipefail
cd "$(dirname "$0")/../.."
SRC=compiler/src
FEHLER=0

melde() { echo "FEHLER: $1"; FEHLER=1; }

# --- 1. Op::PtrAdd ---
TREFFER=$(grep -rn 'Op::PtrAdd' "$SRC"/lower.rs "$SRC"/lower_match.rs 2>/dev/null || true)
# Erlaubt ist genau eine Stelle: der Rumpf von `ptradd_const` (base + off).
UEBRIG=$(printf '%s\n' "$TREFFER" | grep -v 'Op::PtrAdd { base, off: o }' || true)
if [ -n "$UEBRIG" ]; then
    melde "Op::PtrAdd wird im Lowering ausserhalb von ptradd_const/layout.rs gebaut:"
    printf '%s\n' "$UEBRIG" | sed 's/^/        /'
fi

# --- 2. ptradd_const-Aufrufe ---
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

# --- 3. Versatz direkt in eine Adresse ---
for muster in '\.offset' 'offsets\.get'; do
    T=$(grep -rn "$muster" "$SRC"/lower.rs 2>/dev/null || true)
    if [ -n "$T" ]; then
        melde "Feld-Versatz wird in lower.rs berechnet statt in layout.rs:"
        printf '%s\n' "$T" | sed 's/^/        /'
    fi
done

# --- 4. layout.rs existiert und wird benutzt ---
[ -f "$SRC/layout.rs" ] || melde "compiler/src/layout.rs fehlt"
grep -q 'mod layout;' "$SRC/main.rs" || melde "layout ist in main.rs nicht angemeldet"

if [ "$FEHLER" -ne 0 ]; then
    echo
    echo "Die Trennung Feldzugriff <-> Speicherort ist verletzt (DESIGNZIELE.md 8)."
    exit 1
fi

ANZ=$(grep -c 'pub(crate) fn ' "$SRC/layout.rs")
echo "OK: Feldzugriff und Speicherort getrennt ($ANZ Zugaenge in layout.rs, keine Umgehung)."
