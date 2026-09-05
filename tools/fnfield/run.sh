#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# ROUND 68 -- PROOF IN THE ASSEMBLY: `c.hook(a, b)` costs exactly one
# indirect call, and nothing else gets more expensive.
#
# docs/ROUND63.md, gap 6, asked for the calling of a function value that
# sits in a STRUCT FIELD. The danger of such a convenience is that the
# resolution turns into a lookup at run time and every method call pays for
# it. That is a claim about the emitted code, so it is checked on the
# emitted code and not in prose -- exactly as tools/fnval/run.sh does it for
# round 58:
#
#   1. `direct(a, b)` -> `call <symbol>`. No `call rax`, no function record.
#      The target is statically known and stays that way.
#   2. `c.hook(a, b)` -> exactly ONE `call rax`. So the indirection arises
#      only where the value really sits in a field.
#   3. `n.twice(x)` where `Named` has BOTH a field `twice` AND a method
#      `twice`: the METHOD wins, and it wins as a DIRECT call. That is the
#      resolution order of the type checker, read off the machine code.
#   4. The counter-check has to strike: the same program without a function
#      value must contain NO indirect call at all.
#   5. Under the optimiser nothing that was direct may become indirect.
#   6. Both compilers. `lib/firnc1/codegen.fi` writes its own assembly, and
#      the claim has to hold there too.
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=compiler/target/release/firnc
FC1=${FIRNC1:-./.firnc1}
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d /tmp/firn-fnfield.XXXXXX)
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
# `add` and `bump` are deliberately long enough that the inliner does not
# swallow them; what is compared is the CALL SITE, not the body.
cat > "$W/both.fi" <<'EOF'
struct Ctx {
    hook: fn(i32, i32) -> i32,
    n: i32,
}
struct Named {
    twice: fn(i32) -> i32,
}
impl Named {
    fn twice(*mut self, x: i32) -> i32 {
        var s: i32 = 0
        var i: i32 = 0
        while i < x {
            s = s + 2
            i = i + 1
        }
        return s
    }
}
fn add(a: i32, b: i32) -> i32 {
    var s: i32 = 0
    var i: i32 = 0
    while i < b {
        s = s + 1
        i = i + 1
    }
    return a + s
}
fn bump(x: i32) -> i32 {
    var s: i32 = 0
    var i: i32 = 0
    while i < x {
        s = s + 1
        i = i + 1
    }
    return s
}
fn direct(c: *mut Ctx, a: i32, b: i32) -> i32 { return add(a, b) }
fn through(c: *mut Ctx, a: i32, b: i32) -> i32 { return c.hook(a, b) }
fn twice_wins(n: *mut Named, x: i32) -> i32 { return n.twice(x) }
fn main() -> i32 {
    var c: Ctx = Ctx{ hook: add, n: 0 }
    var m: Named = Named{ twice: bump }
    let r: i32 = direct(&c, 3, 4) - through(&c, 3, 4) + twice_wins(&m, 3) + 1
    // ROUND 58 + 68: a CLOSURE without captures fits into the field too,
    // and it is called through exactly the same single `call rax`.
    c.hook = fn(x: i32, y: i32) -> i32 { return x - y }
    if through(&c, 9, 2) != 7 { return 1 }
    return r
}
EOF
cat > "$W/only_direct.fi" <<'EOF'
struct Ctx {
    n: i32,
}
fn add(a: i32, b: i32) -> i32 {
    var s: i32 = 0
    var i: i32 = 0
    while i < b {
        s = s + 1
        i = i + 1
    }
    return a + s
}
fn direct(c: *mut Ctx, a: i32, b: i32) -> i32 { return add(a, b) }
fn main() -> i32 {
    var c: Ctx = Ctx{ n: 0 }
    return direct(&c, 3, 4)
}
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
        # the optimised build: nothing that was direct may become indirect
        "$cc" --emit=asm "$W/both.fi" -o "$W/opt.s" 2>/dev/null || { report "$who: optimised --emit=asm failed"; return; }
        for f in direct twice_wins; do
            if body "$W/opt.s" "$f" | grep -qE '^[[:space:]]*call[[:space:]]+(rax|rcx|rdx|r[0-9]+)$'; then
                report "$who: '$f' became an INDIRECT call under the optimiser"
            fi
        done
    fi

    # 1. the direct call stays direct
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

    # 2. the call through the FIELD is indirect, and exactly once
    local th n
    th=$(body "$W/asm.s" "through")
    n=$(printf '%s\n' "$th" | grep -cE '^[[:space:]]*call[[:space:]]+rax$')
    if [ "$n" -ne 1 ]; then
        report "$who: 'through' has $n 'call rax' instead of exactly 1"
        printf '%s\n' "$th" | grep -n 'call' | sed 's/^/        /'
    fi
    if printf '%s\n' "$th" | grep -qE '^[[:space:]]*call[[:space:]]+_F[0-9]+\.'; then
        report "$who: 'through' contains a direct call"
    fi

    # 3. a METHOD of the same name wins -- and stays direct
    local tw
    tw=$(body "$W/asm.s" "twice_wins")
    if ! printf '%s\n' "$tw" | grep -qE '^[[:space:]]*call[[:space:]]+_F[0-9]+\.Named__twice$'; then
        report "$who: 'twice_wins' does not call the METHOD Named__twice directly"
        printf '%s\n' "$tw" | grep -n 'call' | sed 's/^/        /'
    fi
    if printf '%s\n' "$tw" | grep -qE '^[[:space:]]*call[[:space:]]+(rax|rcx|rdx|r[0-9]+)$'; then
        report "$who: 'twice_wins' calls INDIRECTLY -- the field beat the method"
    fi

    # the records exist, in .rodata
    n=$(grep -cE '^\.L__fnv\..*:$' "$W/asm.s")
    if [ "$n" -lt 1 ]; then
        report "$who: no function record in .rodata"
    fi

    # 4. counter-check: without a function value there is no indirect call
    if grep -qE '^[[:space:]]*call[[:space:]]+rax$' "$W/counter.s"; then
        report "$who: COUNTER-CHECK failed -- a program without a function value already has 'call rax'"
    fi
    if grep -q '__fnv' "$W/counter.s"; then
        report "$who: COUNTER-CHECK failed -- a program without a function value already has a record"
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
echo "FNFIELD: passed -- the field call is exactly one 'call rax', the direct call stays direct, a method of the same name wins and stays direct, in both compilers, counter-checks strike"
exit 0
