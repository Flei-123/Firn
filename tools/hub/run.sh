#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/hub/run.sh -- remote sources, the cache and the fetcher (round FIRNHUB).
#
# What is checked here, on real projects on disk and with a real git
# repository:
#
#   1. THE TWO LIBRARIES of this round (`demos/hub/json`, `demos/hub/date`)
#      build and their OWN TESTS pass -- in both compilers.
#   2. `firnpkg fetch` resolves a git source with a fixed tag into the
#      content addressed cache and writes `firn.have`.
#   3. The project builds with the fetched dependency, in both compilers,
#      and both write a CHARACTER IDENTICAL `firn.lock` with `cache:<hash>`
#      and an `origin` line.
#   4. REPRODUCIBLE: fetching twice and building twice yields the same
#      `firn.have` and the same `firn.lock`, octet for octet.
#   5. OFFLINE: with the cache filled, `--offline` changes nothing; with an
#      EMPTY cache it refuses with a sentence instead of reaching out.
#   6. THE ERROR CASE: a file edited inside the cache is caught -- by
#      `firnpkg verify` and, independently of it, by `firnc --locked`.
#   7. The content hash of `firnpkg` agrees with `sha256sum` out of
#      coreutils over the same stream -- a second implementation of the
#      format, in shell.
#   8. Every error message of the new manifest forms comes out CHARACTER
#      IDENTICAL from both compilers.
#
# An own `mktemp -d` per run: several rounds run on this machine at once.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)

FIRNC="$ROOT/compiler/target/release/firnc"
FC1="$ROOT/.firnc1"
PKG="$ROOT/.firnpkg"
export FIRNLIB="$ROOT/lib"

if [ ! -x "$FIRNC" ]; then
    echo "firnc0 is missing: $FIRNC"
    exit 1
fi

# The same lesson as in tools/packages/run.sh: never reuse a binary just
# because it is there.
rebuild=0
[ -x "$FC1" ] || rebuild=1
if [ -x "$FC1" ]; then
    [ "$FIRNC" -nt "$FC1" ] && rebuild=1
    while IFS= read -r q; do
        [ "$q" -nt "$FC1" ] && { rebuild=1; break; }
    done < <(find bin lib -name '*.fi' -not -type l)
fi
if [ "$rebuild" -eq 1 ]; then
    "$FIRNC" bin/firnc1.fi -o "$FC1" || { echo "firnc1 cannot be built"; exit 1; }
fi
"$FIRNC" bin/firnpkg.fi -o "$PKG" || { echo "firnpkg cannot be built"; exit 1; }

WORK=$(mktemp -d "${TMPDIR:-/tmp}/firn-hub.XXXXXXXX")
trap 'rm -rf "$WORK"' EXIT

OK=0
BAD=0
check() { printf "  %-58s" "$1"; }
good()  { OK=$((OK + 1)); echo "ok"; }
bad()  { BAD=$((BAD + 1)); echo "ERROR"; for z in "$@"; do printf '      %s\n' "$z"; done; }

# The content hash of a tree, computed with coreutils and nothing else --
# the same stream firnpkg builds: per file `path \n length \n content \n`,
# sorted by path, over EVERY regular file.
tree_sum() {
    local d=$1 list f
    list=$( (cd "$d" && find . -type f | sed 's|^\./||') | LC_ALL=C sort )
    ( cd "$d" || exit 1
      for f in $list; do
          printf '%s\n%s\n' "$f" "$(stat -c%s "$f")"
          cat "$f"
          printf '\n'
      done ) | sha256sum | cut -d' ' -f1
}

echo "== the two libraries of round FIRNHUB =="

for lib in json date; do
    for c in 0 1; do
        if [ "$c" = 0 ]; then CC="$FIRNC"; NAME=firnc0; else CC="$FC1"; NAME=firnc1; fi
        check "demos/hub/$lib passes its own tests ($NAME)"
        if "$CC" --package "demos/hub/$lib" -o "$WORK/$lib.$c" > "$WORK/$lib.$c.log" 2>&1 \
           && "$WORK/$lib.$c" > "$WORK/$lib.$c.out" 2>&1 \
           && grep -q "^$lib .* checks passed$" "$WORK/$lib.$c.out"; then
            good
        else
            bad "$(tail -3 "$WORK/$lib.$c.log")" "$(head -3 "$WORK/$lib.$c.out" 2>/dev/null)"
        fi
    done
done

echo "== the content hash: firnpkg against coreutils =="

for d in demos/hub/json demos/hub/date lib/std demos/packages; do
    check "firnpkg hash $d == sha256sum over the same stream"
    a=$("$PKG" hash "$d")
    b=$(tree_sum "$d")
    if [ "$a" = "$b" ]; then good; else bad "firnpkg $a" "coreutils $b"; fi
done

echo "== a git source, fetched and built =="

# A real git repository with a real tag. `file://` because this machine has
# no reason to be online for a proof -- the transport is the same `git`.
GITDIR="$WORK/git/firn-date"
mkdir -p "$WORK/git"
cp -r demos/hub/date "$GITDIR"
( cd "$GITDIR" && git init -q . && git add -A \
  && git -c user.email=hub@firn -c user.name=firnhub commit -qm "date 1.1.0" \
  && git tag v1.1.0 ) > "$WORK/git.log" 2>&1
COMMIT=$(cd "$GITDIR" && git rev-parse HEAD)

PROJ="$WORK/proj"
mkdir -p "$PROJ"
cp -r demos/hub/app "$PROJ/app"
cp -r demos/hub/json "$PROJ/json"
sed -i "s|^needs    date  ../date  1.1.0\$|needs    date  git+file://$GITDIR#v1.1.0  1.1.0|" \
    "$PROJ/app/firn.pkg"

export FIRN_CACHE="$WORK/cache"

check "the build refuses before anything was fetched"
"$FIRNC" --package "$PROJ/app" -o "$WORK/x" > /dev/null 2> "$WORK/nf.0"
r0=$?
"$FC1" --package "$PROJ/app" -o "$WORK/x" > /dev/null 2> "$WORK/nf.1"
r1=$?
if [ "$r0" != 2 ] || [ "$r1" != 2 ]; then
    bad "exit $r0/$r1, expected 2/2" "$(head -2 "$WORK/nf.0")"
elif ! cmp -s "$WORK/nf.0" "$WORK/nf.1"; then
    bad "the two compilers say different things" "$(diff "$WORK/nf.0" "$WORK/nf.1" | head -4)"
elif ! grep -q "has not been fetched" "$WORK/nf.0"; then
    bad "$(head -2 "$WORK/nf.0")"
else
    good
fi

check "--offline with an empty cache refuses instead of reaching out"
"$PKG" fetch --offline --quiet "$PROJ/app" > /dev/null 2> "$WORK/off.err"
if [ $? = 2 ] && grep -q -- "--offline" "$WORK/off.err"; then good; else bad "$(head -2 "$WORK/off.err")"; fi

check "firnpkg fetch resolves the git source"
if "$PKG" fetch --quiet "$PROJ/app" > /dev/null 2> "$WORK/fetch.err" \
   && [ -f "$PROJ/app/firn.have" ]; then good; else bad "$(head -3 "$WORK/fetch.err")"; fi

check "firn.have names the commit and the content hash"
HAVE=$(cat "$PROJ/app/firn.have" 2>/dev/null)
CONTENT=$(awk '/^need date /{print $5}' "$PROJ/app/firn.have" 2>/dev/null)
if echo "$HAVE" | grep -q "^need date git+file://$GITDIR#v1.1.0 $COMMIT [0-9a-f]\{64\}$"; then
    good
else
    bad "$HAVE" "commit expected: $COMMIT"
fi

check "the tree in the cache carries its own hash as its name"
if [ -n "$CONTENT" ] && [ "$(tree_sum "$FIRN_CACHE/pkg/$CONTENT")" = "$CONTENT" ]; then
    good
else
    bad "content=$CONTENT" "measured=$(tree_sum "$FIRN_CACHE/pkg/$CONTENT" 2>/dev/null)"
fi

check "the fetched package is NOT a checkout (.git is gone)"
if [ -n "$CONTENT" ] && [ ! -e "$FIRN_CACHE/pkg/$CONTENT/.git" ]; then good; else bad "there is a .git in the cache"; fi

echo "== the build with the fetched dependency =="

check "firnc0 builds it and the program prints what it should"
if "$FIRNC" --package "$PROJ/app" --lock -o "$WORK/app0" > "$WORK/b0.log" 2>&1 \
   && [ "$("$WORK/app0")" = "firn 2026-08-30 Sunday" ]; then good; else bad "$(tail -3 "$WORK/b0.log")"; fi
cp "$PROJ/app/firn.lock" "$WORK/lock.0" 2>/dev/null

check "firnc1 builds it and writes the SAME firn.lock"
if "$FC1" --package "$PROJ/app" --lock -o "$WORK/app1" > "$WORK/b1.log" 2>&1 \
   && [ "$("$WORK/app1")" = "firn 2026-08-30 Sunday" ] \
   && cmp -s "$WORK/lock.0" "$PROJ/app/firn.lock"; then
    good
else
    bad "$(tail -3 "$WORK/b1.log")" "$(diff "$WORK/lock.0" "$PROJ/app/firn.lock" 2>/dev/null | head -4)"
fi

check "the lock file addresses the fetched package by its CONTENT"
if grep -q "^package date 1.1.0 cache:$CONTENT " "$PROJ/app/firn.lock" \
   && grep -q "^origin date git+file://$GITDIR#v1.1.0 $COMMIT $CONTENT\$" "$PROJ/app/firn.lock" \
   && grep -q "^lock 2\$" "$PROJ/app/firn.lock"; then
    good
else
    bad "$(cat "$PROJ/app/firn.lock")"
fi

check "the local dependency keeps its relative path"
if grep -q "^package json 1.0.0 ../json " "$PROJ/app/firn.lock"; then good; else bad "$(grep '^package json' "$PROJ/app/firn.lock")"; fi

echo "== reproducible =="

check "fetching a second time changes nothing"
cp "$PROJ/app/firn.have" "$WORK/have.1"
if "$PKG" fetch --quiet "$PROJ/app" > /dev/null 2>&1 \
   && cmp -s "$WORK/have.1" "$PROJ/app/firn.have"; then good; else bad "firn.have moved"; fi

check "a build without a network (--offline) works out of the cache"
if "$PKG" fetch --offline --quiet "$PROJ/app" > /dev/null 2>&1 \
   && cmp -s "$WORK/have.1" "$PROJ/app/firn.have"; then good; else bad "offline failed"; fi

check "building twice yields the same firn.lock, octet for octet"
"$FIRNC" --package "$PROJ/app" --lock -o "$WORK/app0b" > /dev/null 2>&1
if cmp -s "$WORK/lock.0" "$PROJ/app/firn.lock"; then good; else bad "$(diff "$WORK/lock.0" "$PROJ/app/firn.lock" | head -4)"; fi

check "--locked accepts the state it wrote"
if "$FIRNC" --package "$PROJ/app" --locked -o "$WORK/app0c" > "$WORK/lk.log" 2>&1 \
   && "$FC1" --package "$PROJ/app" --locked -o "$WORK/app1c" >> "$WORK/lk.log" 2>&1; then
    good
else
    bad "$(tail -3 "$WORK/lk.log")"
fi

echo "== the error case: the cache was tampered with =="

check "firnpkg verify finds a file edited inside the cache"
printf '\n// changed\n' >> "$FIRN_CACHE/pkg/$CONTENT/src/date.fi"
"$PKG" verify "$PROJ/app" > /dev/null 2> "$WORK/ver.err"
if [ $? = 2 ] && grep -q "is not what it says" "$WORK/ver.err"; then good; else bad "$(head -3 "$WORK/ver.err")"; fi

check "and --locked stops the build, in both compilers"
"$FIRNC" --package "$PROJ/app" --locked -o "$WORK/x" > /dev/null 2> "$WORK/lk.0"
r0=$?
"$FC1" --package "$PROJ/app" --locked -o "$WORK/x" > /dev/null 2> "$WORK/lk.1"
r1=$?
if [ "$r0" != 2 ] || [ "$r1" != 2 ]; then
    bad "exit $r0/$r1, expected 2/2"
elif ! cmp -s "$WORK/lk.0" "$WORK/lk.1"; then
    bad "the two compilers say different things" "$(diff "$WORK/lk.0" "$WORK/lk.1" | head -4)"
elif ! grep -q "does not match the sources" "$WORK/lk.0"; then
    bad "$(head -2 "$WORK/lk.0")"
else
    good
fi

check "a fetch after the tampering puts the true tree back"
rm -rf "$FIRN_CACHE/pkg/$CONTENT"
if "$PKG" fetch --quiet "$PROJ/app" > /dev/null 2>&1 \
   && [ "$(tree_sum "$FIRN_CACHE/pkg/$CONTENT")" = "$CONTENT" ] \
   && "$FIRNC" --package "$PROJ/app" --locked -o "$WORK/x" > /dev/null 2>&1; then
    good
else
    bad "the cache was not restored"
fi

echo "== trust on first use =="

check "a tag that was moved is refused"
( cd "$GITDIR" && git tag -d v1.1.0 > /dev/null 2>&1
  printf '\n// another release under the same tag\n' >> src/date.fi
  git add -A && git -c user.email=hub@firn -c user.name=firnhub commit -qm "moved" \
  && git tag v1.1.0 ) > /dev/null 2>&1
rm -rf "$FIRN_CACHE/pkg/$CONTENT"
"$PKG" fetch --quiet "$PROJ/app" > /dev/null 2> "$WORK/tofu.err"
if [ $? = 2 ] && grep -q "changed under a fixed reference" "$WORK/tofu.err"; then
    good
else
    bad "$(head -4 "$WORK/tofu.err")"
fi

echo "== the new manifest forms, in both compilers =="

mkdir -p "$WORK/bad/src"
printf 'fn main() -> i32 {\n    return 0\n}\n' > "$WORK/bad/src/main.fi"
form_case() {
    local tag="$1" line="$2" keyword="$3"
    check "$4"
    { printf 'package bad\nversion 0.1.0\nmain src/main.fi\nsource src\n'
      printf '%s\n' "$line"; } > "$WORK/bad/firn.pkg"
    "$FIRNC" --package-info "$WORK/bad" > /dev/null 2> "$WORK/$tag.0"
    local a=$?
    "$FC1" --package-info "$WORK/bad" > /dev/null 2> "$WORK/$tag.1"
    local b=$?
    if [ "$a" != 2 ] || [ "$b" != 2 ]; then
        bad "exit $a/$b, expected 2/2" "$(head -2 "$WORK/$tag.0")"
    elif ! cmp -s "$WORK/$tag.0" "$WORK/$tag.1"; then
        bad "the two compilers say different things" "$(diff "$WORK/$tag.0" "$WORK/$tag.1" | head -4)"
    elif ! grep -q "$keyword" "$WORK/$tag.0"; then
        bad "$(head -2 "$WORK/$tag.0")"
    else
        good
    fi
}

form_case gitnoref 'needs j git+https://h/r' 'has no fixed reference' \
    "a git source without a reference is an error"
form_case gitnourl 'needs j git+#v1' 'has no address' \
    "a git source without an address is an error"
form_case arcnosum 'needs j https://h/x.tar' 'has no checksum' \
    "an archive without a checksum is an error"
form_case arcbadsum 'needs j https://h/x.tar#sha256=abc' 'has no checksum' \
    "a checksum that is not 64 hex digits is an error"

echo "== the registry short form over an index =="

mkdir -p "$WORK/reg"
cat > "$WORK/reg/main.idx" <<EOF
# the whole registry: one line per release
pkg date 1.1.0 git+file://$GITDIR#v1.1.0
EOF
export FIRN_INDEX="$WORK/reg/main.idx"
PROJ2="$WORK/proj2"
mkdir -p "$PROJ2"
cp -r demos/hub/app "$PROJ2/app"
cp -r demos/hub/json "$PROJ2/json"
sed -i "s|^needs    date  ../date  1.1.0\$|needs    date  1.1.0|" "$PROJ2/app/firn.pkg"

check "needs date 1.1.0 is resolved through \$FIRN_INDEX"
if "$PKG" fetch --quiet "$PROJ2/app" > /dev/null 2> "$WORK/reg.err" \
   && grep -q '^need date 1.1.0 ' "$PROJ2/app/firn.have"; then
    good
else
    bad "$(head -3 "$WORK/reg.err")" "$(cat "$PROJ2/app/firn.have" 2>/dev/null)"
fi

check "and the project builds out of it, in both compilers"
if "$FIRNC" --package "$PROJ2/app" -o "$WORK/r0" > "$WORK/r0.log" 2>&1 \
   && [ "$("$WORK/r0")" = "firn 2026-08-30 Sunday" ] \
   && "$FC1" --package "$PROJ2/app" -o "$WORK/r1" >> "$WORK/r0.log" 2>&1 \
   && [ "$("$WORK/r1")" = "firn 2026-08-30 Sunday" ]; then
    good
else
    bad "$(tail -3 "$WORK/r0.log")"
fi

check "an unknown version in the index is an error with a sentence"
sed -i "s|^needs    date  1.1.0\$|needs    date  9.9.9|" "$PROJ2/app/firn.pkg"
rm -f "$PROJ2/app/firn.have"
"$PKG" fetch --quiet "$PROJ2/app" > /dev/null 2> "$WORK/reg2.err"
if [ $? = 2 ] && grep -q "is not in the index" "$WORK/reg2.err"; then good; else bad "$(head -2 "$WORK/reg2.err")"; fi

echo
echo "HUB: $OK passed, $BAD failed"
[ "$BAD" = 0 ]
