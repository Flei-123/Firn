#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# Proof of the SYMBOL NAMING SCHEME (DESIGN_GOALS.md 4, modules.rs).
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
ERRORS=0
report() { echo "ERROR: $1"; ERRORS=1; }

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
[ "$RC" -eq 33 ] || report "the program yields $RC instead of 33 -- symbols resolved wrongly?"

SYMS=$(nm "$W/prog" | awk '$2 == "T" { print $3 }')

# 2. the entry point is bare
printf '%s\n' "$SYMS" | grep -qx 'main' || report "the entry point 'main' is missing or was renamed"

# 3. both module functions exist separately
A=$(printf '%s\n' "$SYMS" | grep -c '^_F[0-9]\+\.a__help$' || true)
B=$(printf '%s\n' "$SYMS" | grep -c '^_F[0-9]\+\.b__help$' || true)
[ "$A" -eq 1 ] || report "the symbol for a.help is missing (wrong prefix/scheme?)"
[ "$B" -eq 1 ] || report "the symbol for b.help is missing (wrong prefix/scheme?)"

# 1. no bare Firn symbols except main and the linker's own
FOREIGN=$(printf '%s\n' "$SYMS" \
    | grep -v '^_F[0-9]\+\.' \
    | grep -vx 'main' \
    | grep -v '^_start$' \
    | grep -v '^__bss_start$' | grep -v '^_edata$' | grep -v '^_end$' || true)
if [ -n "$FOREIGN" ]; then
    report "symbols without the scheme prefix (a later ABI version would break):"
    printf '%s\n' "$FOREIGN" | sed 's/^/        /'
fi

[ "$ERRORS" -eq 0 ] || exit 1
echo "OK: symbol scheme kept (_F0. prefix, 'main' bare, modules free of collisions)."
