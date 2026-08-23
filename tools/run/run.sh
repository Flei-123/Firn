#!/usr/bin/env bash
# tools/run/run.sh -- `firnc run`: COMPILE AND START, AND A CACHE THAT MAY
# NEVER LIE (round 84, compiler/src/main.rs::run_subcommand and
# compiler/src/runcmd.rs).
#
# WHAT IS PROVEN HERE AND WHY EXACTLY THAT:
#
#   1. THE COMMAND IS TRANSPARENT. `firnc run tool.fi ...` has to behave
#      like the compiled binary itself: the exit code is the program's,
#      the arguments arrive unchanged -- INCLUDING `--help` and `-o`, the
#      two words a compiler would otherwise eat -- and standard input,
#      output and error pass through. A launcher that swallows an exit code
#      is useless in a script, and that is where this command belongs.
#
#   2. THE CACHE MAY NEVER HAND BACK A WRONG ANSWER. The interesting case is
#      not the one everybody tests (change the file, get a new build); it is
#      the one that quietly breaks: change a MODULE and leave the main file
#      alone. The key therefore covers the source text of every file the
#      module resolution finds, and case 8 proves it -- with the timestamp
#      of the main file untouched.
#
#   3. A COMPILE ERROR STAYS AN ERROR. It is shown, and the exit code is not
#      0 -- and nothing lands in the cache.
#
#   4. THE SHEBANG. A file that starts with `#!/usr/bin/env firnc-run` and
#      is marked executable starts as `./file.fi`. Only line 1, only `#!`:
#      case 13 shows that a `#!` in line 2 still reaches the parser as the
#      tokens it always was. Firn does not get a comment character through
#      the back door.
#
# The measurement (cold against warm, and against python3) is not here but
# in tools/run/bench.sh -- a test suite should not depend on a stopwatch.
# RUN84_BENCH=1 runs it at the end anyway.
#
# Usage:  bash tools/run/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)

export FIRNLIB="$ROOT/lib"
FIRNC=compiler/target/release/firnc
WORK=.run-work
rm -rf "$WORK"
mkdir -p "$WORK"
# A cache of this test's own: never touch the developer's ~/.cache/firn.
export FIRN_CACHE="$ROOT/$WORK/cache"

if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml || exit 1
fi

pass=0
fail=0
note() { echo "FAIL  $1"; fail=$((fail + 1)); }
good() { pass=$((pass + 1)); echo "PASS  $1"; }

# `firnc run` with the trace switched on: stdout in $1.out, stderr in $1.err,
# exit code in $RC.
call() {
    local tag=$1
    shift
    FIRN_RUN_TRACE=1 "$FIRNC" run "$@" > "$WORK/$tag.out" 2> "$WORK/$tag.err"
    RC=$?
}
compiled() { grep -q 'cache miss, compiling' "$WORK/$1.err"; }
from_cache() { grep -q 'cache hit' "$WORK/$1.err"; }

echo "== firnc run =="

# ---------------------------------------------------------------- 1 exit code
call c1 tools/run/cases/exit42.fi
if [ "$RC" -eq 42 ]; then
    good "1  exit code of the program is the exit code of 'run' (42)"
else
    note "1  exit code $RC instead of 42"
fi

# ------------------------------------------------------------ 2 exit code 0
call c2 examples/hello.fi
if [ "$RC" -eq 0 ] && [ "$(cat "$WORK/c2.out")" = "Hallo Welt aus Firn!" ]; then
    good "2  a program that succeeds: exit 0, output unchanged"
else
    note "2  exit $RC, output '$(cat "$WORK/c2.out")'"
fi

# ------------------------------------------------------- 3 arguments verbatim
call c3 tools/run/cases/args.fi one two three
{ echo "argc=3"; echo one; echo two; echo three; } > "$WORK/c3.want"
if [ "$RC" -eq 0 ] && cmp -s "$WORK/c3.out" "$WORK/c3.want"; then
    good "3  three arguments arrive unchanged"
else
    note "3  exit $RC; got: $(tr '\n' ' ' < "$WORK/c3.out")"
fi

# ------------------------------------------- 4 --help belongs to the PROGRAM
call c4 tools/run/cases/args.fi --help
{ echo "argc=1"; echo "--help"; } > "$WORK/c4.want"
if [ "$RC" -eq 0 ] && cmp -s "$WORK/c4.out" "$WORK/c4.want"; then
    good "4  '--help' after the file name goes to the program, not to firnc"
else
    note "4  exit $RC; got: $(tr '\n' ' ' < "$WORK/c4.out")"
fi

# ---------------------------------------------- 5 -o belongs to the PROGRAM
call c5 tools/run/cases/args.fi -o /tmp/not-written-by-firnc -- -x
{ echo "argc=4"; echo "-o"; echo "/tmp/not-written-by-firnc"; echo "--"; echo "-x"; } > "$WORK/c5.want"
if [ "$RC" -eq 0 ] && cmp -s "$WORK/c5.out" "$WORK/c5.want" \
   && [ ! -e /tmp/not-written-by-firnc ]; then
    good "5  '-o', '--' and '-x' after the file name reach the program; no file written"
else
    note "5  exit $RC; got: $(tr '\n' ' ' < "$WORK/c5.out")"
fi

# ------------------------------------------------------------ 6 standard input
printf 'first line\nsecond line\n' > "$WORK/c6.in"
FIRN_RUN_TRACE=1 "$FIRNC" run tools/run/cases/cat.fi < "$WORK/c6.in" \
    > "$WORK/c6.out" 2> "$WORK/c6.err"
RC=$?
if [ "$RC" -eq 0 ] && cmp -s "$WORK/c6.out" "$WORK/c6.in" \
   && grep -q '^stderr$' "$WORK/c6.err" && ! grep -q 'stderr' "$WORK/c6.out"; then
    good "6  standard input passes through; the program's stderr stays on stderr"
else
    note "6  exit $RC, stdin/stdout differ or the streams got mixed"
fi

# ---------------------------------------------------------- 7 compile error
call c7 tools/run/cases/broken.fi
if [ "$RC" -ne 0 ] \
   && grep -q "unknown function 'unknown_name_that_does_not_exist'" "$WORK/c7.err" \
   && grep -q 'broken.fi:4:12' "$WORK/c7.err" \
   && [ -z "$(ls "$FIRN_CACHE" 2>/dev/null | grep '^broken')" ]; then
    good "7  a compile error is shown with line:column, exit $RC != 0, nothing cached"
else
    note "7  exit $RC (expected != 0) or the message/the cache is wrong"
fi

# ------------------------------------------------------- 8 cold / warm cache
rm -rf "$FIRN_CACHE"
call c8a tools/run/cases/deep/main.fi
rc_a=$RC
call c8b tools/run/cases/deep/main.fi
if [ "$rc_a" -eq 10 ] && [ "$RC" -eq 10 ] && compiled c8a && from_cache c8b; then
    good "8  cold cache compiles, warm cache does not -- same answer (exit 10)"
else
    note "8  cold=$rc_a warm=$RC; trace: $(tail -1 "$WORK/c8a.err") / $(tail -1 "$WORK/c8b.err")"
fi

# ------------------------------ 9 a changed MODULE forces a new compilation
# The heart of the round. `deep/main.fi` is copied ONCE and never touched
# again -- its content and its timestamp stay as they are. Only the module
# next to it changes.
rm -rf "$WORK/deep" && mkdir -p "$WORK/deep"
cp tools/run/cases/deep/main.fi tools/run/cases/deep/helper.fi "$WORK/deep/"
touch -d '2020-01-01 00:00:00' "$WORK/deep/main.fi"
before=$(stat -c '%Y %s' "$WORK/deep/main.fi")
call c9a "$WORK/deep/main.fi"
rc_a=$RC
sed -i 's/return 10/return 33/' "$WORK/deep/helper.fi"
call c9b "$WORK/deep/main.fi"
after=$(stat -c '%Y %s' "$WORK/deep/main.fi")
if [ "$rc_a" -eq 10 ] && [ "$RC" -eq 33 ] && compiled c9b && [ "$before" = "$after" ]; then
    good "9  a changed MODULE forces a new compilation (10 -> 33), main file untouched"
else
    note "9  before=$rc_a after=$RC (expected 10 then 33); main file $before -> $after"
fi

# ---------------------------------------- 10 a changed MAIN file, same length
sed -i 's/return helper.value()/return helper.value()+1/' "$WORK/deep/main.fi"
call c10 "$WORK/deep/main.fi"
if [ "$RC" -eq 34 ] && compiled c10; then
    good "10 a changed main file forces a new compilation (33 -> 34)"
else
    note "10 exit $RC instead of 34"
fi

# ------------------------------------------------------------- 11 --no-cache
rm -rf "$FIRN_CACHE"
call c11a --no-cache tools/run/cases/exit42.fi
rc_a=$RC
call c11b --no-cache tools/run/cases/exit42.fi
left=$(ls "$FIRN_CACHE" 2>/dev/null | wc -l)
leftover=$(ls /tmp/firnc-run-exit42-* 2>/dev/null | wc -l)
if [ "$rc_a" -eq 42 ] && [ "$RC" -eq 42 ] && compiled c11a && compiled c11b \
   && [ "$left" -eq 0 ] && [ "$leftover" -eq 0 ]; then
    good "11 --no-cache compiles every time, writes nothing and leaves nothing behind"
else
    note "11 exits $rc_a/$RC, $left cache entries, $leftover temporary files left"
fi

# ----------------------------------------------------------- 12 the shebang
# `env firnc-run` has to find the helper on the PATH.
mkdir -p "$WORK/bin"
ln -sf "$ROOT/tools/run/firnc-run" "$WORK/bin/firnc-run"
ln -sf "$ROOT/$FIRNC" "$WORK/bin/firnc"
out=$(cd "$ROOT" && PATH="$ROOT/$WORK/bin:$PATH" ./demos/hello_run.fi 2>"$WORK/c12.err")
rc=$?
exe_bit=no
[ -x demos/hello_run.fi ] && exe_bit=yes
if [ "$rc" -eq 0 ] && [ "$out" = "hello from a shebang" ] && [ "$exe_bit" = yes ]; then
    good "12 ./demos/hello_run.fi starts directly through the shebang line"
else
    note "12 exit $rc, output '$out', executable bit: $exe_bit"
fi

# ------------------------------ 13 '#' is NOT a comment character elsewhere
printf 'fn main() -> i32 {\n#!/bin/sh\n    return 0\n}\n' > "$WORK/hash.fi"
"$FIRNC" run "$WORK/hash.fi" > "$WORK/c13.out" 2> "$WORK/c13.err"
rc=$?
printf '#!/bin/sh\nfn main() -> i32 {\n    return 7\n}\n' > "$WORK/sb.fi"
"$FIRNC" run "$WORK/sb.fi" > /dev/null 2>&1
rc2=$?
if [ "$rc" -ne 0 ] && grep -q "found '#'" "$WORK/c13.err" \
   && grep -q ':2:1' "$WORK/c13.err" && [ "$rc2" -eq 7 ]; then
    good "13 '#!' is skipped in line 1 only; in line 2 it is a token like any other"
else
    note "13 line 2: exit $rc / $(head -1 "$WORK/c13.err"); line 1 case: exit $rc2 (expected 7)"
fi

# ---------------------------------------------------------- 14 --opt-level
# Three things at once: the level reaches the compiler, it is part of the
# key (a different level is a different entry), and the default really is
# `dev-fast` (same file name as the explicit one).
rm -rf "$FIRN_CACHE"
call c14a examples/hello.fi
call c14b --opt-level=dev-fast examples/hello.fi
call c14c --opt-level=release-fast examples/hello.fi
n=$(ls "$FIRN_CACHE" | wc -l)
call c14d --opt-level=nonsense examples/hello.fi
if [ "$n" -eq 2 ] && from_cache c14b && compiled c14c && [ "$RC" -eq 2 ] \
   && grep -q "unknown build level 'nonsense'" "$WORK/c14d.err"; then
    good "14 --opt-level belongs to the key, the default is dev-fast, nonsense is refused"
else
    note "14 $n cache entries (expected 2), refusal: exit $RC / $(head -1 "$WORK/c14d.err")"
fi

# ------------------------------------------------------------ 15 --clear-cache
n_before=$(ls "$FIRN_CACHE" | wc -l)
"$FIRNC" run --clear-cache > "$WORK/c15.out" 2>&1
rc=$?
n_after=$(ls "$FIRN_CACHE" 2>/dev/null | wc -l)
call c15b examples/hello.fi
if [ "$rc" -eq 0 ] && [ "$n_before" -gt 0 ] && [ "$n_after" -eq 0 ] && compiled c15b; then
    good "15 --clear-cache empties the cache ($n_before -> 0), the next start compiles again"
else
    note "15 exit $rc, $n_before -> $n_after entries"
fi

# ------------------------------------------------- 16 the refusals of 'run'
bad_ok=1
for opt in "-o /tmp/x" "--emit=asm" "-c"; do
    # shellcheck disable=SC2086
    "$FIRNC" run $opt examples/hello.fi > "$WORK/c16.out" 2> "$WORK/c16.err"
    [ $? -eq 2 ] || bad_ok=0
    grep -q '^error:' "$WORK/c16.err" || bad_ok=0
done
"$FIRNC" run > "$WORK/c16b.err" 2>&1
[ $? -eq 2 ] || bad_ok=0
"$FIRNC" run /does/not/exist.fi > "$WORK/c16c.err" 2>&1
rc=$?
[ "$rc" -ne 0 ] || bad_ok=0
if [ "$bad_ok" -eq 1 ]; then
    good "16 'run' refuses -o/--emit=/-c before the file, a missing file and no file at all"
else
    note "16 one of the refusals did not hold"
fi

# --------------------------------------------------- 17 a cache that is warm
# is also a cache that STARTS -- twice in a row, the same output, and the
# second time no compiler ran at all.
rm -rf "$FIRN_CACHE"
call c17a tools/run/cases/args.fi alpha
call c17b tools/run/cases/args.fi beta
if cmp -s <(sed -n 2p "$WORK/c17a.out") <(echo alpha) \
   && cmp -s <(sed -n 2p "$WORK/c17b.out") <(echo beta) \
   && compiled c17a && from_cache c17b; then
    good "17 the cached binary takes new arguments (alpha, then beta)"
else
    note "17 the cached start with different arguments went wrong"
fi

if [ "${RUN84_BENCH:-0}" = "1" ]; then
    echo
    bash tools/run/bench.sh || fail=$((fail + 1))
fi

echo "--------------------------------------------------------------------"
echo "  cases: $((pass + fail))"
echo "PASS: $pass   FAIL: $fail"
[ "$fail" -eq 0 ]
