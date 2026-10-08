#!/bin/bash
# tools/script_port/test_firn_run.sh -- tests of tools/script_port/firn-run
#
#   bash tools/script_port/test_firn_run.sh
#
# Needs a built compiler (FIRNC, default compiler/target/release/firnc) that
# knows `#!` on line 1 and --deps-out. Everything lives in one mktemp -d
# directory that is removed at the end. A counting wrapper around the compiler
# tells how often a build really happened.
set -u
cd "$(dirname "$0")/../.."
ROOT=$PWD
REAL_CC=${FIRNC:-$ROOT/compiler/target/release/firnc}
case $REAL_CC in
/*) ;;
*) REAL_CC=$ROOT/$REAL_CC ;;
esac
RUN=$ROOT/tools/script_port/firn-run
if [ ! -x "$REAL_CC" ]; then
    echo "SKIP: no compiler at $REAL_CC"
    exit 0
fi

D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT

pass=0
fail=0
ok() { pass=$((pass + 1)); }
bad() {
    fail=$((fail + 1))
    echo "  FAIL: $1"
}
check() { # check <description> <command...>   (command succeeds = ok)
    local what=$1
    shift
    if "$@"; then ok; else bad "$what"; fi
}
eq() { # eq <description> <expected> <actual>
    if [ "$2" = "$3" ]; then ok; else bad "$1: expected '$2', got '$3'"; fi
}

# A compiler wrapper that counts builds (one line per call that writes an executable).
cat >"$D/cc" <<EOF
#!/bin/sh
echo build >>"$D/builds.log"
exec "$REAL_CC" "\$@"
EOF
chmod +x "$D/cc"
builds() { if [ -f "$D/builds.log" ]; then wc -l <"$D/builds.log" | tr -d ' '; else echo 0; fi; }

export FIRNC=$D/cc
export FIRNLIB=$ROOT/lib
export FIRN_CACHE=$D/cache
unset FIRN_STRICT FIRN_OPT FIRN_CACHE_MAX FIRN_CACHE_DAYS

mkdir "$D/w"
cd "$D/w" || exit 1

# ---- 1. a plain script: exit code, cache hit
cat >seven.fi <<'EOF'
fn main() -> i32 {
    return 7
}
EOF
"$RUN" seven.fi
eq "exit code of the program is passed through" 7 $?
eq "first run builds once" 1 "$(builds)"
"$RUN" seven.fi
eq "second run: same exit code" 7 $?
eq "second run: cache hit, no build" 1 "$(builds)"
# the same script from another directory and by absolute path
(cd / && "$RUN" "$D/w/seven.fi")
eq "absolute path: same exit code" 7 $?
eq "absolute path: no new build" 1 "$(builds)"

# ---- 2. change of the source -> rebuild
sleep 0.05
cat >seven.fi <<'EOF'
fn main() -> i32 {
    return 9
}
EOF
"$RUN" seven.fi
eq "changed source: new behaviour" 9 $?
eq "changed source: rebuilt" 2 "$(builds)"
"$RUN" seven.fi
eq "changed source: then cached again" 2 "$(builds)"

# ---- 3. touch without a content change -> no rebuild (hash tier)
sleep 0.05
touch seven.fi
"$RUN" seven.fi
eq "touched script: still the same result" 9 $?
eq "touched script: no rebuild (same content hash)" 2 "$(builds)"
"$RUN" seven.fi
eq "after the touch: warm again" 2 "$(builds)"

# ---- 4. an imported module next to the script changes -> rebuild
cat >helper.fi <<'EOF'
fn value() -> i32 {
    return 11
}
EOF
cat >usemod.fi <<'EOF'
import helper
fn main() -> i32 {
    return helper.value()
}
EOF
b0=$(builds)
"$RUN" usemod.fi
eq "module: result" 11 $?
eq "module: one build" $((b0 + 1)) "$(builds)"
"$RUN" usemod.fi
eq "module: cached" $((b0 + 1)) "$(builds)"
sleep 0.05
cat >helper.fi <<'EOF'
fn value() -> i32 {
    return 12
}
EOF
"$RUN" usemod.fi
eq "module changed: new result" 12 $?
eq "module changed: rebuilt although the script itself is unchanged" $((b0 + 2)) "$(builds)"

# ---- 5. a module found through FIRNLIB changes -> rebuild
mkdir "$D/flib"
cat >"$D/flib/extlib.fi" <<'EOF'
fn ext() -> i32 {
    return 21
}
EOF
cat >useext.fi <<'EOF'
import extlib
fn main() -> i32 {
    return extlib.ext()
}
EOF
b0=$(builds)
FIRNLIB=$D/flib "$RUN" useext.fi
eq "FIRNLIB module: result" 21 $?
sleep 0.05
cat >"$D/flib/extlib.fi" <<'EOF'
fn ext() -> i32 {
    return 22
}
EOF
FIRNLIB=$D/flib "$RUN" useext.fi
eq "FIRNLIB module changed: new result" 22 $?
eq "FIRNLIB module changed: rebuilt" $((b0 + 2)) "$(builds)"

# ---- 6. arguments with blanks, stdin, stdout
cat >echoargs.fi <<'EOF'
import std.rt

fn main(start: u64) -> i32 {
    var out: rt.Buf = rt.buf_new()
    var i: usize = 1
    while i < rt.arg_count(start) {
        let a: u64 = rt.arg_ptr(start, i)
        rt.buf_push(&out, 91)
        rt.buf_push_bytes(&out, a, rt.c_length(a))
        rt.buf_push(&out, 93)
        rt.buf_push(&out, 10)
        i = i + 1
    }
    var inp: rt.Buf = rt.buf_new()
    rt.read_stdin(&inp)
    rt.buf_push_bytes(&out, rt.buf_ptr(&inp), rt.buf_len(&inp))
    rt.write_everything(1, rt.buf_ptr(&out), rt.buf_len(&out))
    return (rt.arg_count(start) - 1) as i32
}
EOF
got=$(printf 'from stdin\n' | "$RUN" echoargs.fi "a b" "" 'c"d' '*' -x)
rc=$?
want=$(printf '[a b]\n[]\n[c"d]\n[*]\n[-x]\nfrom stdin')
eq "arguments with blanks, empty, quote, glob and dash arrive unchanged; stdin reaches the program" "$want" "$got"
eq "exit code = number of arguments" 5 $rc
got=$("$RUN" echoargs.fi --no-cache </dev/null)
eq "an option AFTER the script name belongs to the script" "[--no-cache]" "$got"

# a script that pulls in library modules (and, through them, the embedded GC runtime)
cat >libs.fi <<'EOF2'
import std.rt
import std.str
import std.process
import std.fs
import std.text
import std.vec

fn main() -> i32 {
    return 13
}
EOF2
b0=$(builds)
"$RUN" libs.fi
eq "library imports: result" 13 $?
"$RUN" libs.fi
eq "library imports: second run cached (no build)" $((b0 + 1)) "$(builds)"
check "library imports: the index lists library files" grep -q "/lib/std/fs.fi" "$FIRN_CACHE"/idx/*libs.fi*
check "library imports: the index names no pseudo path" test -z "$(grep -h '^lib/' "$FIRN_CACHE"/idx/*libs.fi* 2>/dev/null)"

# ---- 7. compile error: exit 125, compiler text on stderr, nothing cached
cat >broken.fi <<'EOF'
fn main() -> i32 {
    let x: i32 = "text"
    return x
}
EOF
objs0=$(ls "$FIRN_CACHE/obj" | wc -l | tr -d ' ')
"$RUN" broken.fi >out.txt 2>err.txt
eq "compile error: exit code 125" 125 $?
check "compile error: compiler message on stderr" grep -q "expected type i32" err.txt
check "compile error: nothing on stdout" test ! -s out.txt
eq "compile error: nothing added to the cache" "$objs0" "$(ls "$FIRN_CACHE/obj" | wc -l | tr -d ' ')"
"$RUN" does-not-exist.fi 2>err.txt
eq "missing script: exit code 125" 125 $?
"$RUN" 2>err.txt
eq "no script: exit code 125" 125 $?
"$RUN" --bogus x.fi 2>err.txt
eq "unknown option: exit code 125" 125 $?

# ---- 8. shebang: a script started directly through env
mkdir "$D/bin"
ln -s "$RUN" "$D/bin/firn-run"
cat >direct.fi <<'EOF'
#!/usr/bin/env firn-run
fn main() -> i32 {
    return 5
}
EOF
chmod +x direct.fi
PATH="$D/bin:$PATH" ./direct.fi
eq "shebang script started as ./direct.fi" 5 $?
b0=$(builds)
PATH="$D/bin:$PATH" ./direct.fi
eq "shebang script: second start is cached" "$b0" "$(builds)"

# ---- 9. --no-cache builds every time and stores nothing
cat >plain.fi <<'EOF'
fn main() -> i32 {
    return 3
}
EOF
objs0=$(ls "$FIRN_CACHE/obj" | wc -l | tr -d ' ')
b0=$(builds)
"$RUN" --no-cache plain.fi
eq "--no-cache: exit code" 3 $?
"$RUN" --no-cache plain.fi
eq "--no-cache: builds every time" $((b0 + 2)) "$(builds)"
eq "--no-cache: nothing stored" "$objs0" "$(ls "$FIRN_CACHE/obj" | wc -l | tr -d ' ')"
check "--no-cache: no temp directory left" test -z "$(ls -d "$FIRN_CACHE"/tmp.* 2>/dev/null)"

# ---- 10. parallel starts on a cold cache
rm -rf "$FIRN_CACHE"
b0=$(builds)
pids=""
for i in 1 2 3 4 5 6 7 8; do
    ("$RUN" seven.fi; echo $? >"$D/par.$i") &
    pids="$pids $!"
done
wait
allok=1
for i in 1 2 3 4 5 6 7 8; do
    [ "$(cat "$D/par.$i")" = 9 ] || allok=0
done
eq "parallel cold starts: every process got the right exit code" 1 $allok
check "parallel cold starts: at least one and at most eight builds" test "$(builds)" -gt "$b0" -a "$(($(builds) - b0))" -le 8
check "parallel cold starts: no temp directory left" test -z "$(ls -d "$FIRN_CACHE"/tmp.* 2>/dev/null)"
check "parallel cold starts: exactly one executable in the cache" test "$(ls "$FIRN_CACHE/obj" | wc -l | tr -d ' ')" = 1
b1=$(builds)
"$RUN" seven.fi
eq "after the parallel start: warm" "$b1" "$(builds)"

# ---- 11. old entries are dropped (FIRN_CACHE_DAYS), the fresh one stays
"$RUN" plain.fi
old=$(ls "$FIRN_CACHE/obj" | head -1)
touch -a -d '90 days ago' "$FIRN_CACHE/obj/$old"
FIRN_CACHE_DAYS=30 "$RUN" usemod.fi
check "age limit: the unused executable is removed" test ! -e "$FIRN_CACHE/obj/$old"
check "age limit: the executable that just ran is kept" test "$(ls "$FIRN_CACHE/obj" | wc -l | tr -d ' ')" -ge 1

# ---- 12. size limit: FIRN_CACHE_MAX=0 keeps only the binary that is about to run
for s in seven plain usemod echoargs; do "$RUN" $s.fi </dev/null >/dev/null; done
FIRN_CACHE_MAX=0 "$RUN" direct.fi
eq "size limit: only the running executable is left" 1 "$(ls "$FIRN_CACHE/obj" | wc -l | tr -d ' ')"

# ---- 13. strict mode compares exactly (an OLDER file replaces the script)
cat >swap.fi <<'EOF'
fn main() -> i32 {
    return 1
}
EOF
touch -d '2020-01-01' swap.fi
FIRN_STRICT=1 "$RUN" swap.fi
eq "strict: first build" 1 $?
printf 'fn main() -> i32 {\n    return 2\n}\n' >swap2.fi
touch -d '2019-01-01' swap2.fi
mv swap2.fi swap.fi
FIRN_STRICT=1 "$RUN" swap.fi
eq "strict: an older replacement file is noticed" 2 $?

# ---- 14. --clean
"$RUN" --clean
check "--clean: cache directory is gone" test ! -e "$FIRN_CACHE"
"$RUN" --clean
eq "--clean on an empty cache is fine" 0 $?

echo "firn-run: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
