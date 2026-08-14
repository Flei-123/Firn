#!/usr/bin/env bash
# Nachweis des SYMBOL-NAMENSSCHEMAS (DESIGNZIELE.md §4, modules.rs).
#
# Geprueft wird an einem wirklich gebauten Binary:
#   1. Jedes von Firn erzeugte Symbol traegt den reservierten Praefix mit
#      Schemaversion (`_F0.`) — damit ist Platz fuer eine spaetere ABI-Version
#      (`_F0.name.v3`), ohne dass heute gebaute Symbole brechen.
#   2. Der Einstiegspunkt `main` behaelt seinen nackten Namen (Verabredung mit
#      dem Linker, `_start` ruft ihn).
#   3. Zwei Module mit gleichnamigen Funktionen erzeugen VERSCHIEDENE Symbole.
set -euo pipefail
cd "$(dirname "$0")/../.."
FIRNC="compiler/target/release/firnc"
W=$(mktemp -d /tmp/firn-symbole.XXXXXX)
trap 'rm -rf "$W"' EXIT
FEHLER=0
melde() { echo "FEHLER: $1"; FEHLER=1; }

# --- Aufbau: zwei Module mit derselben Funktion ---
cat > "$W/a.fi" <<'EOF'
export { hilf }
fn hilf(x: i32) -> i32 { return x + 1 }
EOF
cat > "$W/b.fi" <<'EOF'
export { hilf }
fn hilf(x: i32) -> i32 { return x + 2 }
EOF
cat > "$W/main.fi" <<'EOF'
import a
import b
fn main() -> i32 { return a.hilf(10) + b.hilf(20) }
EOF

"$FIRNC" -o "$W/prog" "$W/main.fi"
set +e; "$W/prog"; RC=$?; set -e
[ "$RC" -eq 33 ] || melde "Programm liefert $RC statt 33 — Symbole falsch aufgeloest?"

SYMS=$(nm "$W/prog" | awk '$2 == "T" { print $3 }')

# 2. Einstiegspunkt nackt
printf '%s\n' "$SYMS" | grep -qx 'main' || melde "Einstiegspunkt 'main' fehlt oder wurde umbenannt"

# 3. beide Modulfunktionen getrennt vorhanden
A=$(printf '%s\n' "$SYMS" | grep -c '^_F[0-9]\+\.a__hilf$' || true)
B=$(printf '%s\n' "$SYMS" | grep -c '^_F[0-9]\+\.b__hilf$' || true)
[ "$A" -eq 1 ] || melde "Symbol fuer a.hilf fehlt (Praefix/Schema falsch?)"
[ "$B" -eq 1 ] || melde "Symbol fuer b.hilf fehlt (Praefix/Schema falsch?)"

# 1. keine nackten Firn-Symbole ausser main und den Linker-eigenen
FREMD=$(printf '%s\n' "$SYMS" \
    | grep -v '^_F[0-9]\+\.' \
    | grep -vx 'main' \
    | grep -v '^_start$' \
    | grep -v '^__bss_start$' | grep -v '^_edata$' | grep -v '^_end$' || true)
if [ -n "$FREMD" ]; then
    melde "Symbole ohne Schema-Praefix (spaetere ABI-Version waere ein Bruch):"
    printf '%s\n' "$FREMD" | sed 's/^/        /'
fi

[ "$FEHLER" -eq 0 ] || exit 1
echo "OK: Symbolschema gehalten (_F0.-Praefix, 'main' nackt, Module kollisionsfrei)."
