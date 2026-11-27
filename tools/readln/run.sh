#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC=${FIRNC0:-compiler/target/release/firnc}
FC1=${FIRNC1:-./.firnc1}
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
fail=0
cat > "$WORK/echo.fi" <<'F'
fn main() -> i32 {
    let a: str = readln()
    let b: str = readln()
    let c: str = readln()
    println(f"[{a}][{b}][{c}]")
    return 0
}
F
for name in firnc0 firnc1; do
    comp=$FIRNC
    [ "$name" = firnc1 ] && comp=$FC1
    if [ ! -x "$comp" ]; then echo "skip $name: $comp missing"; continue; fi
    if ! "$comp" -o "$WORK/echo.$name" "$WORK/echo.fi" 2>"$WORK/$name.err"; then
        echo "FAIL $name: compile"; head -5 "$WORK/$name.err"; fail=1; continue
    fi
    got=$(printf 'hello\n\nlast' | "$WORK/echo.$name")
    if [ "$got" = "[hello][][last]" ]; then echo "ok $name"; else echo "FAIL $name: $got"; fail=1; fi
done
exit $fail
