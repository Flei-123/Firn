#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/firn/test_firn.sh -- checks for the `firn` driver (tools/firn/firn).
#   bash tools/firn/test_firn.sh        (needs firnc, as, ld; ~10 s on a quiet machine)
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FI=$ROOT/tools/firn/firn
W=$(mktemp -d "${TMPDIR:-/tmp}/firn-driver-test.XXXXXX")
trap 'rm -rf "$W"' EXIT
export FIRN_CACHE=$W/cache
ok()  { echo "ok" >> "$W/n.ok"; printf '  %-52s ok\n' "$1"; }
bad() { echo "x" >> "$W/n.bad"; printf '  %-52s ERROR %s\n' "$1" "${2:-}"; }
t() { local name=$1; shift; if "$@" > "$W/out" 2> "$W/err"; then ok "$name"; else bad "$name" "$(head -2 "$W/err" | tr '\n' ' ')"; fi; }

echo "== firn driver =="
t "help lists sub-commands"        bash -c "$FI help | grep -q 'firn repl'"
t "doctor names compiler and lib"  bash -c "$FI doctor | grep -q '^lib'"
cd "$W"
t "new makes a program"            "$FI" new demo
t "new refuses an existing dir"    bash -c "! $FI new demo"
t "new --lib"                      "$FI" new tool --lib
( cd demo
  t "build DIR (--package)"        "$FI" build . 
  t "built program runs"           bash -c "./demo | grep -q 'hello from demo'"
  t "test runs the TAP suite"      bash -c "$FI test | grep -q 'passed 1, failed 0'"
  t "fmt -c accepts canonical"     "$FI" fmt -c src/main.fi tests/basic.fi
  t "check accepts a good file"    "$FI" check src/main.fi
  printf 'fn main() -> i32 { let x: i32 = "a"; return x }\n' > bad.fi
  t "check rejects a type error"   bash -c "! $FI check bad.fi 2> err.txt && grep -q 'expected type i32' err.txt"
  printf 'fn  main()->i32{return 0}\n' > ugly.fi
  t "fmt -c flags a non-canonical file" bash -c "! $FI fmt -c ugly.fi >/dev/null 2>&1"
  printf 'import std.io\nfn main() -> i32 {\n    io.print_line("hi " + "there")\n    return 3\n}\n' > run.fi
  t "run: output"                  bash -c "$FI run run.fi | grep -q 'hi there'"
  "$FI" run run.fi > /dev/null; rc=$?
  if [ $rc -eq 3 ]; then ok "run: exit code passes through"; else bad "run: exit code passes through" "rc=$rc"; fi
)
t "eval: expression"               bash -c "[ \"\$($FI eval '6 * 7')\" = 42 ]"
t "eval: error is reported"        bash -c "! $FI eval '1 +' >/dev/null 2>&1"
t "repl: state across lines"       bash -c "printf '%s\n' 'var a = 20' 'a + 22' ':q' | $FI repl 2>/dev/null | grep -qx 42"
t "repl: fn item and call"         bash -c "printf '%s\n' 'fn sq(x: i32) -> i32 { return x * x }' 'sq(9)' ':q' | $FI repl 2>/dev/null | grep -qx 81"
t "repl: a failing line is rolled back" bash -c "printf '%s\n' 'var a = 1' 'a +' 'a + 1' ':q' | $FI repl 2>/dev/null | grep -qx 2"
t "repl: :undo"                    bash -c "printf '%s\n' 'var a = 1' 'a = 5' ':undo' 'a' ':q' | $FI repl 2>/dev/null | grep -qx 1"
t "unknown command exits 125"      bash -c "$FI nope 2>/dev/null; [ \$? -eq 125 ]"
OK=$(cat "$W/n.ok" 2>/dev/null | wc -l); BAD=$(cat "$W/n.bad" 2>/dev/null | wc -l)
echo "firn driver: $OK ok, $BAD errors"
[ "$BAD" -eq 0 ]
