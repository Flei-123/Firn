# tools/testkit/testkit.sh -- the shared test helpers for shell test scripts (source it, do not run it).
#
#   . "${FIRN:-/root/firn}/tools/testkit/testkit.sh"
#
#   ok "label"               count a pass and print "  OK    label"
#   bad "label"              count a fail and print "  FAIL  label"
#   check "label" cmd args   run cmd; ok when it exits 0, bad otherwise (its output is hidden)
#   check_out "label" want cmd args   ok when the trimmed stdout of cmd equals want
#   tk_tmpdir [prefix]       make a scratch directory in $D, removed when the script exits
#   tk_summary [name]        print "name: N passed, M failed", return 1 when something failed
#
# The counters are `pass` and `fail` (the names the older scripts of Osum, Certus and Firn
# already use) and the lines are byte for byte the ones of the copies this file replaces,
# so a script that only swaps its own ok()/bad() lines for `. testkit.sh` prints the same.
pass=${pass:-0}
fail=${fail:-0}
ok()  { pass=$((pass+1)); printf '  OK    %s\n' "$1"; }
bad() { fail=$((fail+1)); printf '  FAIL  %s\n' "$1"; }
check() { # check "label" cmd args...
    local label=$1; shift
    if "$@" > /dev/null 2>&1; then ok "$label"; else bad "$label"; fi
}
check_out() { # check_out "label" "want" cmd args...
    local label=$1 want=$2 got; shift 2
    got=$("$@" 2> /dev/null) || true
    got=${got%$'\n'}
    if [ "$got" = "$want" ]; then ok "$label"; else bad "$label (got '$got', want '$want')"; fi
}
tk_tmpdir() { # sets D; one trap for the whole script
    D=$(mktemp -d "${TMPDIR:-/tmp}/${1:-tk}.XXXXXX")
    trap 'rm -rf "$D"' EXIT
}
tk_summary() {
    printf '%s: %d passed, %d failed\n' "${1:-tests}" "$pass" "$fail"
    [ "$fail" -eq 0 ]
}
