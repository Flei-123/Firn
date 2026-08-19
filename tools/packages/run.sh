#!/usr/bin/env bash
# tools/packages/run.sh -- the package and project system (round 48).
#
# THREE things are checked, on real projects on disk:
#
#   1. The build driver `--package` compiles a project along its
#      manifest, the result RUNS and prints what is expected.
#   2. Every error situation (private module, foreign package, cycle, broken
#      manifest, name conflict) is recognised -- exit code 2 and a
#      message that names the reason.
#   3. `firnc0` (Rust) and `firnc1` (Firn) behave THE SAME: every case
#      runs through BOTH compilers, and their messages are compared octet for
#      octet. A package system that is only right in one of the two
#      compilers would be none.
#
# An own `mktemp -d` per run: several rounds run on this machine
# at the same time, fixed /tmp names would overwrite each other.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)

FIRNC="$ROOT/compiler/target/release/firnc"
FC1="$ROOT/.firnc1"
export FIRNLIB="$ROOT/lib"

if [ ! -x "$FIRNC" ]; then
    echo "firnc0 is missing: $FIRNC"
    exit 1
fi

# LESSON from rounds 35/45/46: never reuse a binary just because
# it exists. If `firnc0` or a source is younger, `.firnc1` is rebuilt
# -- otherwise this run measures a compiler that no longer exists.
neu_bauen=0
[ -x "$FC1" ] || neu_bauen=1
if [ -x "$FC1" ]; then
    [ "$FIRNC" -nt "$FC1" ] && neu_bauen=1
    while IFS= read -r q; do
        [ "$q" -nt "$FC1" ] && { neu_bauen=1; break; }
    done < <(find bin lib -name '*.fi' -not -type l)
fi
if [ "$neu_bauen" -eq 1 ]; then
    "$FIRNC" bin/firnc1.fi -o "$FC1" || { echo "firnc1 cannot be built"; exit 1; }
fi

WORK=$(mktemp -d "${TMPDIR:-/tmp}/firn-packages.XXXXXXXX")
trap 'rm -rf "$WORK"' EXIT

OK=0
BAD=0
check() { printf "  %-58s" "$1"; }
good()  { OK=$((OK + 1)); echo "ok"; }
bad()  { BAD=$((BAD + 1)); echo "ERROR"; for z in "$@"; do printf '      %s\n' "$z"; done; }

# Let both compilers loose on the same case.
beide() {
    local tag="$1"; shift
    "$FIRNC" "$@" > "$WORK/$tag.0.out" 2> "$WORK/$tag.0.err"
    echo $? > "$WORK/$tag.0.rc"
    "$FC1"   "$@" > "$WORK/$tag.1.out" 2> "$WORK/$tag.1.err"
    echo $? > "$WORK/$tag.1.rc"
}

# Expectation for an ERROR CASE: both report exit 2, the same message,
# and the text contains the keyword looked for.
expect_error() {
    local tag="$1" keyword="$2"
    local rc0 rc1
    rc0=$(cat "$WORK/$tag.0.rc")
    rc1=$(cat "$WORK/$tag.1.rc")
    if [ "$rc0" != "2" ]; then
        bad "firnc0 gave exit $rc0, expected 2" "$(head -2 "$WORK/$tag.0.err")"
        return
    fi
    if [ "$rc1" != "2" ]; then
        bad "firnc1 gave exit $rc1, expected 2" "$(head -2 "$WORK/$tag.1.err")"
        return
    fi
    if ! grep -qF -- "$keyword" "$WORK/$tag.0.err"; then
        bad "message without '$keyword'" "$(head -2 "$WORK/$tag.0.err")"
        return
    fi
    if ! cmp -s "$WORK/$tag.0.err" "$WORK/$tag.1.err"; then
        bad "firnc0 and firnc1 report different things" \
            "0: $(head -1 "$WORK/$tag.0.err")" \
            "1: $(head -1 "$WORK/$tag.1.err")"
        return
    fi
    good
}

# A copy of its own of the example project per case.
kopie() {
    rm -rf "$WORK/$1"
    cp -r demos/packages "$WORK/$1"
    echo "$WORK/$1"
}

echo "== package and project system (round 48) =="

# --- 1/2: the example project in the repo, with BOTH compilers ------------

for c in 0 1; do
    if [ "$c" = 0 ]; then CC="$FIRNC"; NAME="firnc0"; else CC="$FC1"; NAME="firnc1"; fi
    check "$NAME builds demos/packages/app"
    if ! "$CC" --package demos/packages/app -o "$WORK/anw$c" \
            > "$WORK/bau$c.log" 2>&1; then
        bad "Uebersetzen schlug fehl" "$(head -4 "$WORK/bau$c.log")"
    else
        out=$("$WORK/anw$c"); rc=$?
        if [ "$rc" -ne 0 ]; then
            bad "Programm endete mit Exit $rc"
        elif [ "$out" != "12 14 3" ]; then
            bad "output '$out', expected '12 14 3'"
        else
            good
        fi
    fi
done

# --- 3: `--package` without `-o` puts the binary under the package name -----

check "--package without -o names after the manifest"
P=$(kopie f_name_aus)
if "$FIRNC" --package "$P/app" >/dev/null 2>&1 \
   && [ -x "$P/app/app" ] \
   && [ "$("$P/app/app")" = "12 14 3" ]; then
    good
else
    bad "kein lauffaehiges '$P/app/app'"
fi

# --- 4: --package-info, character-identical in both compilers --------------

check "--package-info is the same in both compilers"
beide info --package-info demos/packages/app
if [ "$(cat "$WORK/info.0.rc")" != 0 ] || [ "$(cat "$WORK/info.1.rc")" != 0 ]; then
    bad "Exit-Codes $(cat "$WORK/info.0.rc")/$(cat "$WORK/info.1.rc")"
elif ! cmp -s "$WORK/info.0.out" "$WORK/info.1.out"; then
    bad "Berichte unterscheiden sich" "$(diff "$WORK/info.0.out" "$WORK/info.1.out" | head -4)"
elif ! grep -q '^needs geo demos/packages/geo$' "$WORK/info.0.out"; then
    bad "Abhaengigkeit fehlt im Bericht" "$(head -8 "$WORK/info.0.out")"
else
    good
fi

# --- 5: private module of a dependency -----------------------------------

check "a private module of a dependency is rejected"
P=$(kopie f_privat)
sed -i 's/^import geo.dot$/import geo.inner/' "$P/app/src/main.fi"
beide private --package "$P/app" -o "$WORK/f_privat.bin"
expect_error private "is not public in package 'geo'"

# --- 6: a package that is not entered as a dependency --------------------
#
# `secret` lies IN the source tree of `app` and is a package of its own; `app`
# does not enter it. The import finds the file over way (1), the
# visibility check still has to bite.

check "a package without 'needs' is rejected"
mkdir -p "$WORK/f_fremd/app/src/secret" "$WORK/f_fremd/h"
cat > "$WORK/f_fremd/app/firn.package" <<'EOF'
package app
version 0.1.0
start src/main.fi
source src
needs h ../h
EOF
cat > "$WORK/f_fremd/app/src/main.fi" <<'EOF'
import secret.secret

fn main() -> i32 {
    return secret.value()
}
EOF
cat > "$WORK/f_fremd/app/src/secret/firn.package" <<'EOF'
package secret
version 0.1.0
EOF
cat > "$WORK/f_fremd/app/src/secret/secret.fi" <<'EOF'
export { value }
fn value() -> i32 { return 5 }
EOF
cat > "$WORK/f_fremd/h/firn.package" <<'EOF'
package h
version 0.1.0
needs secret ../app/src/secret
EOF
cat > "$WORK/f_fremd/h/h.fi" <<'EOF'
export { help_it }
fn help_it() -> i32 { return 1 }
EOF
beide foreign --package "$WORK/f_fremd/app" -o "$WORK/f_fremd.bin"
expect_error foreign "package 'secret' is not a dependency of package 'app'"

# --- 7: package cycle ----------------------------------------------------

check "a package cycle is reported"
P=$(kopie f_zyklus)
printf 'needs app ../app\n' >> "$P/geo/firn.package"
beide cycle --package "$P/app" -o "$WORK/f_zyklus.bin"
expect_error cycle "package cycle: app -> geo -> app"

# --- 8: dependency without a manifest ------------------------------------

check "a dependency without a manifest is reported"
P=$(kopie f_kein_manifest)
rm -f "$P/geo/firn.package"
beide nomanifest --package "$P/app" -o "$WORK/f_km.bin"
expect_error nomanifest "dependency 'geo' has no manifest"

# --- 9: a dependency points at a differently named package ---------------

check "wrong package name in the dependency"
P=$(kopie f_name)
sed -i 's/^package  *geo$/package  geometry/' "$P/geo/firn.package"
beide wrongname --package "$P/app" -o "$WORK/f_name.bin"
expect_error wrongname "dependency 'geo' points to package 'geometry'"

# --- 10: broken version entry --------------------------------------------

check "invalid version in the manifest"
P=$(kopie f_version)
sed -i 's/^version  *0.2.0$/version  0.2/' "$P/geo/firn.package"
beide version --package "$P/app" -o "$WORK/f_ver.bin"
expect_error version "invalid version '0.2' (expected number.number.number)"

# --- 11: unknown key -----------------------------------------------------

check "unknown key in the manifest"
P=$(kopie f_schluessel)
sed -i 's/^public  *geo dot$/publi   geo dot/' "$P/geo/firn.package"
beide schluessel --package "$P/app" -o "$WORK/f_sch.bin"
expect_error schluessel "unknown key 'publi'"

# --- 12: missing mandatory line ------------------------------------------

check "manifest without a 'package' line"
P=$(kopie f_ohne_paket)
sed -i 's/^package  *app$//' "$P/app/firn.package"
beide ohnepaket --package "$P/app" -o "$WORK/f_op.bin"
expect_error ohnepaket "the manifest needs a line 'package <name>'"

# --- 13: name conflict of two modules ------------------------------------

check "two modules of the same name are reported"
P=$(kopie f_konflikt)
cat > "$P/geo/src/help.fi" <<'EOF'
export { value }
fn value() -> i32 { return 7 }
EOF
sed -i 's/^public  *geo dot$/public   geo dot help/' "$P/geo/firn.package"
cat > "$P/app/src/main.fi" <<'EOF'
import help
import geo.help

fn main() -> i32 {
    help.sep()
    return 0
}
EOF
beide konflikt --package "$P/app" -o "$WORK/f_konf.bin"
expect_error konflikt "name conflict: module 'help' comes from two files"

# --- 14: `--package` on a library without an entry point -------------------

check "a library without 'start' cannot be built"
beide biblio --package demos/packages/geo -o "$WORK/f_bib.bin"
expect_error biblio "the manifest has no entry point"

# --- 15: `--package` on a directory without a manifest --------------------

check "a directory without a manifest is reported"
mkdir -p "$WORK/leer"
beide leer --package "$WORK/leer" -o "$WORK/f_leer.bin"
expect_error leer "no manifest in"

# --- 16: a private module INSIDE one's own package stays allowed ---------

check "inside a package there is no barrier"
P=$(kopie f_intern)
cat > "$P/app/src/main.fi" <<'EOF'
import geo

fn main() -> i32 {
    // geo.extent computes through geo.inner -- a module that is NOT public.
    // Inside the package 'geo' that is allowed.
    return geo.extent(geo.rect_new(0, 0, 3, 4))
}
EOF
"$FIRNC" --package "$P/app" -o "$WORK/f_intern0.bin" > "$WORK/f_intern.log" 2>&1
rc0=$?
"$WORK/f_intern0.bin" >/dev/null 2>&1; run0=$?
"$FC1" --package "$P/app" -o "$WORK/f_intern1.bin" >> "$WORK/f_intern.log" 2>&1
rc1=$?
"$WORK/f_intern1.bin" >/dev/null 2>&1; run1=$?
if [ "$rc0" -eq 0 ] && [ "$rc1" -eq 0 ] && [ "$run0" -eq 14 ] && [ "$run1" -eq 14 ]; then
    good
else
    bad "exit $rc0/$rc1, run $run0/$run1 (expected 0/0 and 14/14)" \
        "$(head -4 "$WORK/f_intern.log")"
fi

# --- 17: without a manifest NOTHING changes ------------------------------

check "without a manifest the resolution of round 47 stays"
"$FIRNC" tests/110_module.fi -o "$WORK/alt0" >/dev/null 2>&1 && "$WORK/alt0"
a0=$?
"$FC1" tests/110_module.fi -o "$WORK/alt1" >/dev/null 2>&1 && "$WORK/alt1"
a1=$?
if [ "$a0" -eq 60 ] && [ "$a1" -eq 60 ]; then
    good
else
    bad "exit $a0/$a1, expected 60/60"
fi

# --- 18: search order -- one's own source wins ---------------------------

check "the project source wins over a module of the same name in a dependency"
P=$(kopie f_vorrang)
cat > "$P/app/src/number.fi" <<'EOF'
export { value }
fn value() -> i32 { return 1 }
EOF
cat > "$P/text/src/number.fi" <<'EOF'
export { value }
fn value() -> i32 { return 2 }
EOF
cat > "$P/app/src/main.fi" <<'EOF'
import number

fn main() -> i32 {
    return number.value()
}
EOF
"$FIRNC" --package "$P/app" -o "$WORK/f_vor0.bin" >/dev/null 2>&1 && "$WORK/f_vor0.bin"
v0=$?
"$FC1" --package "$P/app" -o "$WORK/f_vor1.bin" >/dev/null 2>&1 && "$WORK/f_vor1.bin"
v1=$?
if [ "$v0" -eq 1 ] && [ "$v1" -eq 1 ]; then
    good
else
    bad "exit $v0/$v1, expected 1/1 (the own source)"
fi

# --- 19: the manifest is found UPWARDS from the source file --------------

check "the manifest is found upwards even without --package"
P=$(kopie f_aufwaerts)
"$FIRNC" "$P/app/src/main.fi" -o "$WORK/f_auf0.bin" >/dev/null 2>&1
r0=$?
"$FC1" "$P/app/src/main.fi" -o "$WORK/f_auf1.bin" >/dev/null 2>&1
r1=$?
if [ "$r0" -eq 0 ] && [ "$r1" -eq 0 ] \
   && [ "$("$WORK/f_auf0.bin")" = "12 14 3" ] \
   && [ "$("$WORK/f_auf1.bin")" = "12 14 3" ]; then
    good
else
    bad "exit $r0/$r1 or wrong output"
fi

# --- 20: several source directories in one package -----------------------

check "a second 'source' directory is searched"
P=$(kopie f_zweitquelle)
mkdir -p "$P/app/extra"
printf 'source   extra\n' >> "$P/app/firn.package"
cat > "$P/app/extra/extra_mod.fi" <<'EOF'
export { three }
fn three() -> i32 { return 3 }
EOF
cat > "$P/app/src/main.fi" <<'EOF'
import extra_mod

fn main() -> i32 {
    return extra_mod.three()
}
EOF
"$FIRNC" --package "$P/app" -o "$WORK/f_zq0.bin" >/dev/null 2>&1 && "$WORK/f_zq0.bin"
z0=$?
"$FC1" --package "$P/app" -o "$WORK/f_zq1.bin" >/dev/null 2>&1 && "$WORK/f_zq1.bin"
z1=$?
if [ "$z0" -eq 3 ] && [ "$z1" -eq 3 ]; then
    good
else
    bad "exit $z0/$z1, expected 3/3"
fi

# --- 21: `--package` and a source file exclude each other -----------------

check "--package together with a source file is rejected"
beide beides --package demos/packages/app tests/110_module.fi -o "$WORK/f_beides.bin"
expect_error beides "--package and an input file are mutually exclusive"

echo
echo "PACKAGES: $OK passed, $BAD failed"
[ "$BAD" -eq 0 ] || exit 1
exit 0
