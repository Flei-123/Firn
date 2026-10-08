#!/usr/bin/env bash
# tools/glob_cross/run.sh -- std.glob against Python's glob and fnmatch modules.
#
#   bash tools/glob_cross/run.sh            (GLOB_TREES=300 by default)
#   GLOB_TREES=1000 GLOB_SEEDS="1 2 3" bash tools/glob_cross/run.sh
#
# tools/glob_cross/gen.py builds GLOB_TREES random directory trees (files,
# directories, symbolic links to files, to directories and broken ones; names
# with dots, accented letters, euro signs, emoji, spaces and the glob characters
# themselves) and lets Python answer 5 to 9 random patterns on each tree with
# `glob.glob(root_dir=..., recursive=..., include_hidden=...)` -- patterns made
# from names that exist, mutated with `*`, `?`, classes, `**`, a trailing `/`, a
# leading `./`, and some made of nothing but pattern characters. Next to that
# 12 random `fnmatch.fnmatchcase` cases per tree (patterns and names built from
# `[ ] ! - ^ \ & | ~ * ?` and non-ASCII characters) and `glob.escape`.
# tools/glob_cross/check.fi asks lib/std/glob.fi the same and compares the sorted
# list byte for byte, in all four build stages of test.sh.
#
# Python 3.11's `a/**` for a directory that does not exist answers "a/"; 3.12
# fixed that and so did std.glob, so the reference replaces glob._glob2 by the
# fixed one (see gen.py).
#
# COUNTER-CHECKS. A library with a deliberate bug has to be CAUGHT, otherwise
# the comparison would pass with a harness that checks nothing: (1) hidden names
# matched by `*`, (2) a class range written backwards that is not empty, (3) `?`
# that eats a byte and not a character.
#
# Needs python3. Exit 0 = no difference.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC="$ROOT/compiler/target/release/firnc"
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
TREES=${GLOB_TREES:-300}
SEEDS=${GLOB_SEEDS:-"1 2"}
W=$(mktemp -d "${TMPDIR:-/tmp}/firn-glob-cross.XXXXXX")
trap 'rm -rf "$W"' EXIT
ERRORS=0
report() { echo "  FAIL  $1"; ERRORS=$((ERRORS + 1)); }
export FIRNLIB="$ROOT/lib"
mkdir -p "$W/trees"

for seed in $SEEDS; do
    mkdir -p "$W/trees/$seed"   # one forest per seed: a pattern such as `../**` looks at the neighbour trees
    python3 tools/glob_cross/gen.py "$seed" "$TREES" "$W/trees/$seed" "$W/cases.$seed" > "$W/gen.log" || { report "gen.py failed"; cat "$W/gen.log"; exit 1; }
    sed 's/^/  seed '"$seed"': /' "$W/gen.log"
done

TOTAL=0
DIFFS=0
for stage in "release-fast:--opt-level=release-fast" "no-opt:--no-opt" "dev-fast:--opt-level=dev-fast" "release-safe:--opt-level=release-safe"; do
    name=${stage%%:*}
    opt=${stage#*:}
    if ! $FIRNC $opt -o "$W/check.$name" tools/glob_cross/check.fi 2> "$W/err"; then
        report "$name: tools/glob_cross/check.fi does not compile"
        sed 's/^/        /' "$W/err" | head -6
        continue
    fi
    for seed in $SEEDS; do
        out=$("$W/check.$name" "$W/cases.$seed" 2>&1); rc=$?
        last=$(echo "$out" | tail -1)
        echo "  $name seed $seed: $last"
        if [ "$rc" -ne 0 ]; then
            report "$name seed $seed: cross-check differs"
            echo "$out" | cut -c1-300 | head -30 | sed 's/^/        /'
        fi
        d=$(echo "$last" | sed -n 's/.* \([0-9]*\) differences$/\1/p')
        DIFFS=$((DIFFS + ${d:-0}))
    done
done
DISTINCT=0
GLOBS=0
for seed in $SEEDS; do
    DISTINCT=$((DISTINCT + $(wc -l < "$W/cases.$seed")))
    GLOBS=$((GLOBS + $(grep -c '^G ' "$W/cases.$seed")))
done

# --- counter-checks
mkdir -p "$W/badlib/std"
for f in "$ROOT"/lib/*; do [ "$(basename "$f")" = std ] || ln -s "$f" "$W/badlib/$(basename "$f")"; done
for f in "$ROOT"/lib/std/*; do [ "$(basename "$f")" = glob.fi ] || ln -s "$f" "$W/badlib/std/$(basename "$f")"; done
seed1=${SEEDS%% *}
counter() {      # $1 = label, $2 = python replacement script text (old, new) as two args
    local label=$1 old=$2 new=$3
    python3 - "$ROOT/lib/std/glob.fi" "$W/badlib/std/glob.fi" "$old" "$new" <<'EOF' || { report "counter-check '$label': the text to break is not in glob.fi"; return; }
import sys
src = open(sys.argv[1]).read()
old, new = sys.argv[3], sys.argv[4]
if old not in src:
    sys.exit(1)
open(sys.argv[2], "w").write(src.replace(old, new, 1))
EOF
    rm -f "$W/check.bad"
    FIRNLIB="$W/badlib" $FIRNC --opt-level=release-safe -o "$W/check.bad" tools/glob_cross/check.fi 2> "$W/err" || { report "counter-check '$label': the broken library does not compile"; head -4 "$W/err"; return; }
    if "$W/check.bad" "$W/cases.$seed1" > "$W/bad.out" 2>&1; then
        report "counter-check '$label': the cross-check did NOT catch it"
    else
        echo "  counter-check '$label': caught ($(tail -1 "$W/bad.out" | sed 's/.*), //'))"
    fi
}
counter "hidden names matched by *" "    let sees_hidden: bool = (*c).hidden || (pattern.n > 0 && *pattern.p == G_DOT)" "    let sees_hidden: bool = true"
counter "backwards range is not empty" "if c1 <= c2 && cp >= c1 && cp <= c2 {" "if cp >= c1 && cp <= c2 || cp == c1 {"
counter "? eats one byte" "    if c == G_QUEST {
        *used = nl
        return pi + 1" "    if c == G_QUEST {
        *used = 1
        return pi + 1"

echo
echo "glob_cross: $DISTINCT cases ($GLOBS glob over random trees, the rest fnmatch/escape; x4 build stages), $DIFFS differences"
if [ "$ERRORS" -ne 0 ]; then
    echo "FAILED: $ERRORS"
    exit 1
fi
echo "ALL OK"
