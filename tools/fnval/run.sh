#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# ROUND 58 -- PROOF IN THE ASSEMBLY: a direct call STAYS direct.
#
# The whole point of the function record (compiler/src/fnval.rs) is that a
# function value costs nothing where none is used. That is a claim about the
# emitted code, so it is checked on the emitted code and not in prose:
#
#   1. `direct(a, b)` -> `call <symbol>`. No `call rax`, no load of a code
#      address, no `.L__fnv` record -- the target is statically known.
#      Measured WITHOUT the optimiser: with `release-fast` the inliner
#      swallows the call entirely (which is even better, but says nothing
#      about the lowering). The optimised build is checked separately, on
#      the one thing that matters there: no indirect call may appear where
#      none was asked for.
#   2. `f(a, b)` through a value -> exactly ONE `call rax`, plus a record in
#      `.rodata`. So the indirection arises only where it was asked for.
#   3. The counter-check has to strike: if the same program calls only
#      directly, the assembly must contain NO indirect call at all.
#   4. A closure WITHOUT captures allocates nothing: no `__gc_alloc_raw`
#      in the assembly.
#   5. Both compilers. `lib/firnc1/codegen.fi` writes its own assembly, and
#      the claim has to hold there too.
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=compiler/target/release/firnc
FC1=${FIRNC1:-./.firnc1}
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d /tmp/firn-fnval.XXXXXX)
trap 'rm -rf "$W"' EXIT
ERRORS=0
report() { echo "FAIL: $1"; ERRORS=1; }

if [ ! -x "$FIRNC" ]; then
    echo "firnc0 is missing: $FIRNC"
    exit 1
fi
if [ ! -x "$FC1" ] || [ -n "$(find bin lib -name '*.fi' -not -type l -newer "$FC1" -print -quit)" ]; then
    rm -f "$FC1"
    "$FIRNC" bin/firnc1.fi -o "$FC1" || { echo "stage 1 failed"; exit 1; }
fi

# --- the program under test ------------------------------------------------
# `add` is deliberately long enough that the inliner does not swallow it;
# what is compared is the CALL SITE, not the body.
cat > "$W/both.fi" <<'EOF'
fn add(a: i32, b: i32) -> i32 {
    var s: i32 = 0
    var i: i32 = 0
    while i < b {
        s = s + 1
        i = i + 1
    }
    return a + s
}
fn direct(a: i32, b: i32) -> i32 { return add(a, b) }
fn indirect(f: fn(i32, i32) -> i32, a: i32, b: i32) -> i32 { return f(a, b) }
fn main() -> i32 {
    let p: fn(i32, i32) -> i32 = add
    let lam: fn(i32) -> i32 = fn(x: i32) -> i32 { return x + x }
    return direct(3, 4) - indirect(p, 3, 4) + lam(0) + 7
}
EOF
cat > "$W/only_direct.fi" <<'EOF'
fn add(a: i32, b: i32) -> i32 {
    var s: i32 = 0
    var i: i32 = 0
    while i < b {
        s = s + 1
        i = i + 1
    }
    return a + s
}
fn direct(a: i32, b: i32) -> i32 { return add(a, b) }
fn main() -> i32 { return direct(3, 4) }
EOF

# Body of ONE function out of an assembly file (from the label up to the next
# label). `awk` instead of `sed`, so that the label may carry a prefix of its
# own (`_F0.` vs `_F1.`).
body() {                       # $1 = file, $2 = function name
    awk -v want="$2" '
        /^[A-Za-z_.][A-Za-z0-9_.$]*:$/ {
            lbl = substr($0, 1, length($0) - 1)
            sub(/^_F[0-9]+\./, "", lbl)
            if (lbl ~ /^\.L/) { next }
            inside = (lbl == want)
            next
        }
        inside { print }
    ' "$1"
}

check_stage() {                # $1 = label, $2 = compiler, $3 = mode
    local who="$1" cc="$2" mode="$3"
    if [ "$mode" = "keep" ]; then
        rm -f "$W/both.s" "$W/both.o" "$W/both"
        "$cc" "$W/both.fi" -o "$W/both" >/dev/null 2>&1
        [ -f "$W/both.s" ] || { report "$who: no assembly file"; return; }
        cp "$W/both.s" "$W/asm.s"
        rm -f "$W/only_direct.s" "$W/only_direct"
        "$cc" "$W/only_direct.fi" -o "$W/only_direct" >/dev/null 2>&1
        [ -f "$W/only_direct.s" ] || { report "$who: counter-check without assembly"; return; }
        cp "$W/only_direct.s" "$W/counter.s"
    else
        "$cc" --no-opt --emit=asm "$W/both.fi" -o "$W/asm.s" 2>/dev/null || { report "$who: --emit=asm failed"; return; }
        "$cc" --no-opt --emit=asm "$W/only_direct.fi" -o "$W/counter.s" 2>/dev/null || { report "$who: counter-check failed"; return; }
        # the optimised build: `direct` must not become indirect there either
        "$cc" --emit=asm "$W/both.fi" -o "$W/opt.s" 2>/dev/null || { report "$who: optimised --emit=asm failed"; return; }
        if body "$W/opt.s" "direct" | grep -qE '^[[:space:]]*call[[:space:]]+(rax|rcx|rdx|r[0-9]+)$'; then
            report "$who: 'direct' became an INDIRECT call under the optimiser"
        fi
    fi

    # 1. the direct call is direct
    local d
    d=$(body "$W/asm.s" "direct")
    if ! printf '%s\n' "$d" | grep -qE '^[[:space:]]*call[[:space:]]+_F[0-9]+\.add$'; then
        report "$who: 'direct' does not contain 'call <add>' (the direct call became indirect)"
        printf '%s\n' "$d" | grep -n 'call' | sed 's/^/        /'
    fi
    if printf '%s\n' "$d" | grep -qE '^[[:space:]]*call[[:space:]]+(rax|rcx|rdx|r[0-9]+)$'; then
        report "$who: 'direct' contains an INDIRECT call"
    fi
    if printf '%s\n' "$d" | grep -q '__fnv'; then
        report "$who: 'direct' touches a function record"
    fi

    # 2. the indirect call is indirect, and exactly once
    local ind n
    ind=$(body "$W/asm.s" "indirect")
    n=$(printf '%s\n' "$ind" | grep -cE '^[[:space:]]*call[[:space:]]+rax$')
    if [ "$n" -ne 1 ]; then
        report "$who: 'indirect' has $n 'call rax' instead of exactly 1"
        printf '%s\n' "$ind" | grep -n 'call' | sed 's/^/        /'
    fi
    if printf '%s\n' "$ind" | grep -qE '^[[:space:]]*call[[:space:]]+_F[0-9]+\.'; then
        report "$who: 'indirect' contains a direct call"
    fi

    # the record exists, in .rodata
    n=$(grep -cE '^\.L__fnv\..*:$' "$W/asm.s")
    if [ "$n" -lt 1 ]; then
        report "$who: no function record in .rodata"
    fi

    # 3. counter-check: without a function value there is no indirect call
    if grep -qE '^[[:space:]]*call[[:space:]]+rax$' "$W/counter.s"; then
        report "$who: COUNTER-CHECK failed -- a program without a function value already has 'call rax'"
    fi
    if grep -q '__fnv' "$W/counter.s"; then
        report "$who: COUNTER-CHECK failed -- a program without a function value already has a record"
    fi

    # 4. a closure without captures allocates nothing
    if grep -q '__gc_alloc_raw' "$W/asm.s"; then
        report "$who: a closure without captures pulls the collector in"
    fi
}

check_stage "firnc0" "$FIRNC" emit
check_stage "firnc1" "$FC1"   keep

# --- the programs really run and agree ------------------------------------
"$FIRNC" "$W/both.fi" -o "$W/b0" >/dev/null 2>&1
"$FC1"   "$W/both.fi" -o "$W/b1" >/dev/null 2>&1
"$W/b0" >/dev/null 2>&1; r0=$?
"$W/b1" >/dev/null 2>&1; r1=$?
[ "$r0" -eq 7 ] || report "firnc0: the program yields $r0 instead of 7"
[ "$r1" -eq 7 ] || report "firnc1: the program yields $r1 instead of 7"

if [ "$ERRORS" -ne 0 ]; then
    exit 1
fi
echo "FNVAL: passed -- direct call stays direct, exactly one 'call rax' per function value, records only where needed, in both compilers, counter-checks strike"
exit 0
