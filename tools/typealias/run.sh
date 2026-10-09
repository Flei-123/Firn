#!/usr/bin/env bash
# tools/typealias/run.sh -- type aliases (`type Idx = u32`, docs/GAPS.md B14)
# in BOTH compilers.
#
# `firnc0` (Rust) replaces an alias in the parser and, across modules, in the
# renamer (compiler/src/alias.rs, modules.rs); `firnc1` (Firn) replaces it
# while parsing and parses a module before the files that import it as soon as
# any file declares an alias (lib/firnc1/parser.fi, bin/firnc1.fi). A feature
# that only one of the two has breaks the fixpoint the moment anybody uses it,
# so this is checked here, with the negative cases as well:
#
#   1. the positive programs (tests/2400, 2401) give the same result in both
#   2. the same program with the IMPORTS IN THE OTHER ORDER -- the importing
#      module first -- gives the same result in both (the module order of firnc1)
#   3. every tests/neg alias program is REFUSED by both, with a real error
#      (exit 1/5), never "not core" (3) and never a crash
#   4. a cycle of aliases across two modules is refused by both
#   5. the syntax tree of a core program with aliases is the same in both
#      (`--emit=ast-canon` against `bin/astdump.fi`): the alias is gone
set -uo pipefail
cd "$(dirname "$0")/../.."

export FIRNLIB="$(pwd)/lib"
FIRNC=compiler/target/release/firnc
FC1=${FIRNC1:-./.firnc1}
DUMP=${ASTDUMP:-./.astdump}
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

if [ ! -x "$FIRNC" ]; then
    echo "firnc0 is missing: $FIRNC"
    exit 1
fi
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

if [ ! -x "$DUMP" ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer "$DUMP" -print -quit)" ]; then
    rm -f "$DUMP"
    "$FIRNC" bin/astdump.fi -o "$DUMP" || exit 1
fi

fail=0
n=0
say() { printf '  %-62s %s\n' "$1" "$2"; }

# build + run with both compilers, compare exit code and output
both() { # name  source
    local name="$1" src="$2"
    "$FIRNC" -o "$WORK/a0" "$src" > "$WORK/a0.err" 2>&1 || { say "$name" "firnc0 FAILED"; head -5 "$WORK/a0.err"; fail=$((fail+1)); return; }
    "$FC1" "$src" -o "$WORK/a1" > "$WORK/a1.err" 2>&1 || { say "$name" "firnc1 FAILED"; head -5 "$WORK/a1.err"; fail=$((fail+1)); return; }
    "$WORK/a0" > "$WORK/a0.out" 2>&1; r0=$?
    "$WORK/a1" > "$WORK/a1.out" 2>&1; r1=$?
    n=$((n+1))
    if [ "$r0" = "0" ] && [ "$r1" = "0" ] && cmp -s "$WORK/a0.out" "$WORK/a1.out"; then
        say "$name" "OK (firnc0 and firnc1 agree)"
    else
        say "$name" "DIFFERENT (firnc0 exit $r0, firnc1 exit $r1)"
        fail=$((fail+1))
    fi
    rm -f "$WORK/a0" "$WORK/a1"
}

for src in tests/2400_type_alias.fi tests/2401_type_alias_module.fi; do
    both "positive: $src" "$src"
done

# the importing module FIRST: the order of the queue is not the order of the imports
mkdir -p "$WORK/ord"
cp tests/modules/alias_base.fi tests/modules/alias_user.fi "$WORK/ord/"
sed -e 's/^import modules.alias_base$/import alias_user/' \
    -e 's/^import modules.alias_user$/import alias_base/' \
    tests/2401_type_alias_module.fi > "$WORK/ord/main.fi"
both "importing module first" "$WORK/ord/main.fi"

# an alias of another module as the argument of a generic: one instantiation
mkdir -p "$WORK/gm"
cp tests/modules/alias_base.fi "$WORK/gm/"
cat > "$WORK/gm/main.fi" <<'EOT'
import alias_base

struct Box[T] { v: T }

fn get(b: Box[u64]) -> u64 { return b.v }

fn main() -> i32 {
    let b: Box[alias_base.Count] = Box[u64] { v: 7 }
    let c: Box[alias_base.Total] = b
    return get(c) as i32 - 7
}
EOT
both "alias of a module as a generic argument" "$WORK/gm/main.fi"

# a chain over three modules, the topmost imported first
both "chain over three modules" tools/typealias/chain_main.fi

# negative programs: both must say no, with a real error
for f in tests/neg/24*alias*.fi tools/typealias/cyc_main.fi; do
    "$FIRNC" -o "$WORK/n0" "$f" > "$WORK/n0.err" 2>&1; r0=$?
    "$FC1" "$f" -o "$WORK/n1" > "$WORK/n1.err" 2>&1; r1=$?
    n=$((n+1))
    ok0=0; ok1=0
    [ "$r0" = "1" ] && ok0=1
    case "$r1" in 1|5) ok1=1 ;; esac
    if grep -qE "panicked at|RUST_BACKTRACE" "$WORK/n0.err"; then ok0=0; fi
    if [ "$ok0" = "1" ] && [ "$ok1" = "1" ]; then
        say "refused: $f" "OK (firnc0 $r0, firnc1 $r1)"
    else
        say "refused: $f" "WRONG (firnc0 exit $r0, firnc1 exit $r1)"
        fail=$((fail+1))
    fi
done

# the syntax tree of a core program: the alias is gone, the same in both
for src in tools/typealias/core_alias.fi; do
    "$FIRNC" --emit=ast-canon "$src" > "$WORK/t0.txt" 2>/dev/null
    "$DUMP" "$src" > "$WORK/t1.txt" 2>/dev/null; rd=$?
    n=$((n+1))
    if [ "$rd" = "0" ] && cmp -s "$WORK/t0.txt" "$WORK/t1.txt" && ! grep -q "Idx\|Octet\|Chain" "$WORK/t0.txt"; then
        say "same syntax tree: $src" "OK ($(wc -l < "$WORK/t0.txt") lines, no alias name left)"
    else
        say "same syntax tree: $src" "DIFFERENT (astdump exit $rd)"
        diff "$WORK/t0.txt" "$WORK/t1.txt" | head -6
        fail=$((fail+1))
    fi
done
both "core program with aliases" tools/typealias/core_alias.fi

echo "  typealias: $n programs through both compilers, $fail deviations"
[ "$fail" -eq 0 ]
