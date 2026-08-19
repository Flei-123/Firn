#!/usr/bin/env bash
# Proof of the SYMBOL NAMING SCHEME (DESIGNZIELE.md 4, modules.rs).
#
# It is checked on a binary that was really built:
#   1. Every symbol produced by Firn carries the reserved prefix with a
#      scheme version (`_F0.`) -- that leaves room for a later ABI version
#      (`_F0.name.v3`) without breaking symbols built today.
#   2. The entry point `main` keeps its bare name (an agreement with
#      the linker, `_start` calls it).
#   3. Two modules with functions of the same name produce DIFFERENT symbols.
set -euo pipefail
cd "$(dirname "$0")/../.."
FIRNC="compiler/target/release/firnc"
W=$(mktemp -d /tmp/firn-symbole.XXXXXX)
trap 'rm -rf "$W"' EXIT
FEHLER=0
melde() { echo "FEHLER: $1"; FEHLER=1; }

# --- setup: two modules with the same function ---
cat > "$W/a.fi" <<'EOF'
export { help }
fn help(x: i32) -> i32 { return x + 1 }
EOF
cat > "$W/b.fi" <<'EOF'
export { help }
fn help(x: i32) -> i32 { return x + 2 }
EOF
cat > "$W/main.fi" <<'EOF'
import a
import b
fn main() -> i32 { return a.help(10) + b.help(20) }
EOF

"$FIRNC" -o "$W/prog" "$W/main.fi"
set +e; "$W/prog"; RC=$?; set -e
[ "$RC" -eq 33 ] || melde "Programm liefert $RC statt 33 — Symbole falsch aufgeloest?"

SYMS=$(nm "$W/prog" | awk '$2 == "T" { print $3 }')

# 2. the entry point is bare
printf '%s\n' "$SYMS" | grep -qx 'main' || melde "Einstiegspunkt 'main' fehlt oder wurde umbenannt"

# 3. both module functions exist separately
A=$(printf '%s\n' "$SYMS" | grep -c '^_F[0-9]\+\.a__help$' || true)
B=$(printf '%s\n' "$SYMS" | grep -c '^_F[0-9]\+\.b__help$' || true)
[ "$A" -eq 1 ] || melde "Symbol fuer a.help fehlt (Praefix/Schema falsch?)"
[ "$B" -eq 1 ] || melde "Symbol fuer b.help fehlt (Praefix/Schema falsch?)"

# 1. no bare Firn symbols except main and the linker's own
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
