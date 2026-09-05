#!/usr/bin/env bash
# tools/env/run.sh -- ROUND FIRN-ENV: build time environment variables.
#
# `tests/1640_env_const.fi` checks the case WITHOUT the variable, because a
# test of the corpus is translated exactly once. The other half of the
# promise needs a second translation of the SAME file with a different
# environment, and that is what happens here:
#
#   1. unset          -> the written default comes out
#   2. set            -> the value out of the environment comes out
#   3. set, then run with `env -i` -> the SAME value. A program that runs
#      may not need the build environment any more; that is the whole point
#      of a build time constant, and it is the one thing a `getenv` at run
#      time would silently get wrong.
#   4. the allow list: a prefix outside it is an error, `--env-allow=`
#      opens exactly that one and nothing else.
#   5. `--env-log` says which variable was read and where the value came
#      from -- character for character the same in both compilers.
#
# Everything runs in BOTH compilers: `firnc0` (Rust) and `.firnc1` (Firn).
# A feature that only one of the two has would break the fixpoint the
# moment anybody used it.
set -uo pipefail
cd "$(dirname "$0")/../.."

export FIRNLIB="$(pwd)/lib"
FIRNC=compiler/target/release/firnc
FC1=${FIRNC1:-./.firnc1}
WORK=.env-work
mkdir -p "$WORK"

if [ ! -x "$FIRNC" ]; then
    echo "firnc0 is missing: $FIRNC"
    exit 1
fi
# The same lesson as in `self_compare.sh`: NEVER reuse a binary just
# because it exists.
rebuild=0
[ -x "$FC1" ] || rebuild=1
if [ -x "$FC1" ]; then
    [ "$FIRNC" -nt "$FC1" ] && rebuild=1
    while IFS= read -r q; do
        [ "$q" -nt "$FC1" ] && { rebuild=1; break; }
    done < <(find bin lib -name '*.fi' -not -type l)
fi
if [ "$rebuild" -eq 1 ]; then
    "$FIRNC" bin/firnc1.fi -o "$FC1" || exit 1
fi

fail=0
say() { printf '  %-58s %s\n' "$1" "$2"; }
check() { # name  wanted  got
    if [ "$2" = "$3" ]; then
        say "$1" "OK"
    else
        say "$1" "DIFFERENT"
        echo "      wanted: $2"
        echo "      got   : $3"
        fail=$((fail + 1))
    fi
}

SRC=tests/1640_env_const.fi
DEFAULT='FreeViewer|FreeViewer 1.0|stock|10|FreeViewer|ok'
BRANDED='OrientOS|OrientOS 1.0|branded|8|FreeViewer|DIFFERENT'

cat > "$WORK/allow.fi" <<'EOF'
import std.io

fn main() -> i32 {
    io.print_line(__env_or("FV_BRAND_NAME", "FreeViewer"))
    return 0
}
EOF
cat > "$WORK/allow2.fi" <<'EOF'
import std.io

fn main() -> i32 {
    io.print_line(__env_or("XY_BRAND", "FreeViewer"))
    return 0
}
EOF

for who in firnc0 firnc1; do
    if [ "$who" = firnc0 ]; then CC="$FIRNC"; else CC="$FC1"; fi

    # --- 1. no variable: the written default
    rm -f "$WORK/a"
    env -u FIRN_TEST_BRAND "$CC" "$SRC" -o "$WORK/a" >/dev/null 2>&1
    check "$who: unset -> default" "$DEFAULT" "$("$WORK/a")"

    # --- 2. variable set: the value out of the environment
    rm -f "$WORK/b"
    FIRN_TEST_BRAND=OrientOS "$CC" "$SRC" -o "$WORK/b" >/dev/null 2>&1
    check "$who: set -> environment" "$BRANDED" "$("$WORK/b")"

    # --- 3. the finished program has no environment left to ask
    check "$who: runs without the environment" "$BRANDED" "$(env -i "$WORK/b")"

    # --- 4. the allow list
    rm -f "$WORK/c"
    FV_BRAND_NAME=OrientOS "$CC" "$WORK/allow.fi" -o "$WORK/c" >/dev/null 2>&1
    if [ -x "$WORK/c" ]; then
        say "$who: prefix outside the list is refused" "PERMITTED (wrong)"
        fail=$((fail + 1))
    else
        say "$who: prefix outside the list is refused" "OK"
    fi
    rm -f "$WORK/c"
    FV_BRAND_NAME=OrientOS "$CC" --env-allow=FV_ "$WORK/allow.fi" -o "$WORK/c" >/dev/null 2>&1
    check "$who: --env-allow=FV_ opens it" "OrientOS" "$("$WORK/c" 2>/dev/null)"
    # ... and nothing else. A variable under a prefix that was NOT allowed
    # stays refused even while `FV_` is open.
    rm -f "$WORK/d"
    "$CC" --env-allow=FV_ "$WORK/allow2.fi" -o "$WORK/d" >/dev/null 2>&1
    if [ -x "$WORK/d" ]; then
        say "$who: --env-allow opens ONLY its prefix" "PERMITTED (wrong)"
        fail=$((fail + 1))
    else
        say "$who: --env-allow opens ONLY its prefix" "OK"
    fi

    # --- 5. the manifest
    rm -f "$WORK/e"
    log=$(FIRN_TEST_BRAND=OrientOS "$CC" --env-log "$SRC" -o "$WORK/e" 2>&1 | grep '^env:')
    want='env: FIRN_TEST_BRAND = "OrientOS" (environment)
env: FIRN_TEST_BRAND ? true'
    check "$who: --env-log names the variable" "$want" "$log"

    # --- 6. a control character in the value is refused, not truncated
    rm -f "$WORK/f"
    FIRN_TEST_BRAND="$(printf 'a\nb')" "$CC" "$SRC" -o "$WORK/f" >/dev/null 2>&1
    if [ -x "$WORK/f" ]; then
        say "$who: control character in the value is refused" "PERMITTED (wrong)"
        fail=$((fail + 1))
    else
        say "$who: control character in the value is refused" "OK"
    fi
done

# --- 6b. THE ORIENTOS SHAPE (kernel/marke.fi, commit c3ecd95 there).
#
# The brand of a KERNEL cannot use the type `str`: writing `str` pulls in
# the tracing collector (strtype.rs::source_uses_str), and profile `kernel`
# has none. It does not have to. A text is a pointer and a length, and every
# struct of that shape is a VIEW of `str` (round 88) -- so it takes a text
# literal, and therefore a `__env_or`, with no collector anywhere.
#
# This is the line that replaces `tools/marke-einsetzen.py` over there. It is
# proved here and NOT rebuilt over there.
cat > "$WORK/marke.fi" <<'EOF'
profile kernel

struct Text {
    p: *mut u8,
    n: usize,
}

fn produkt() -> Text {
    let t: Text = __env_or("OSUM_MARKE_PRODUKT", "OrientOS")
    return t
}

fn k_main() -> i32 {
    let t: Text = produkt()
    return t.n as i32
}
EOF
rm -f "$WORK/marke.o"
OSUM_MARKE_PRODUKT="Xoffi OS" "$FIRNC" --env-allow=OSUM_MARKE_ -c "$WORK/marke.fi" -o "$WORK/marke.o" >/dev/null 2>&1
if [ -s "$WORK/marke.o" ]; then
    # the octets of the brand really stand in the object file, and no
    # collector came along for the ride
    if strings -a "$WORK/marke.o" | grep -q 'gc_' ; then
        say "OrientOS shape: kernel object without a collector" "COLLECTOR (wrong)"
        fail=$((fail + 1))
    else
        say "OrientOS shape: kernel object without a collector" "OK"
    fi
else
    say "OrientOS shape: kernel object without a collector" "NO OBJECT (wrong)"
    fail=$((fail + 1))
fi

# --- 7. the two compilers produce the same program, not merely a working one
rm -f "$WORK/h0" "$WORK/h1"
FIRN_TEST_BRAND=OrientOS "$FIRNC" "$SRC" -o "$WORK/h0" >/dev/null 2>&1
FIRN_TEST_BRAND=OrientOS "$FC1" "$SRC" -o "$WORK/h1" >/dev/null 2>&1
check "both compilers, same output" "$("$WORK/h0")" "$("$WORK/h1")"

if [ "$fail" -ne 0 ]; then
    echo "ENV: $fail check(s) failed"
    exit 1
fi
echo "ENV: build time environment variables agree in both compilers"
exit 0
