#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/liveb4/run.sh -- ROUND B4: the page comes ALIVE.
#
#   1. compile the round's five root files in THREE build stages
#      (opt / --no-opt / dev-fast)
#   2. URL resolution against `urllib.parse.urljoin` -- the 33 reference
#      examples of RFC 3986, 5.4 (tools/liveb4/url_check.py)
#   3. HTTP dates against `email.utils.parsedate_to_datetime`, and the
#      cookie jar against the rules of RFC 6265 with four counter-checks
#      (tools/liveb4/cookie_check.py)
#   4. the HTTP client against a REAL SERVER -- Python's own `http.server`,
#      started here, driven over a real socket: Content-Length, chunked
#      with extensions and a trailer, gzip, deflate, redirects with the
#      method rules, cookies, the cache, persistent connections, and the
#      counter-checks including the TLS refusal (tools/liveb4/http_check.py,
#      tools/liveb4/server.py)
#   5. `<script src>` over that same socket: the ordering of
#      parser-blocking, `async` and `defer` (tools/liveb4/scripts_check.py)
#   6. the OWN cases: script order, the event flow in three phases,
#      timers, `innerHTML` in a fragment context, wrapper identity
#      (tools/liveb4/cases.py)
#   7. THE NARROWED RECOMPUTATION, measured with the counter-check that
#      matters: the same mutations with the narrowing on and off, and
#      after every single one a full layout for comparison, box for box
#      (tools/liveb4/invalidate.py)
#   8. the OFFICIAL Web Platform Tests for the DOM and for events, run
#      through the ORIGINAL `testharness.js` (tools/liveb4/wpt.py,
#      tests/data/wpt-dom/PROVENANCE.md)
#   9. the regression limits from tools/liveb4/minquota.txt
#
# ONE SOCKET IS OPENED, and it is a loopback socket to a server this script
# starts and kills itself. That is the point: an HTTP client whose only
# counterpart is this repository proves nothing, because two ends that
# misunderstand the same thing agree perfectly. Nothing is fetched from the
# internet -- the WPT corpus lies in tests/data/wpt-dom, harvested once by
# tools/liveb4/harvest.py, which is never called from here.
#
# WHY THE QUOTA HAS A GUARD. A DOM test that reports nothing passes
# nothing, and a harness that never reaches its completion callback looks
# exactly like a page with no failures. `wpt.py` therefore counts a file
# only if its harness really finished AND produced at least one subtest,
# and prints the files that could not run as their own number. That is the
# same lesson as the 32 empty reference pictures of round B3.
set -uo pipefail
cd "$(dirname "$0")/../.."

FIRNC="compiler/target/release/firnc"
export FIRNLIB="$(pwd)/lib"
WORK=".b4-work"
mkdir -p "$WORK"
ERRORS=0
fail() { echo "  FAIL  $1"; ERRORS=$((ERRORS + 1)); }

if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml
fi

echo "== 1. three build stages of the round's five root files =="
ROOTS="lib/browser/b4_main.fi lib/browser/live_probe.fi lib/net/http_main.fi
       lib/net/url_main.fi lib/net/httpstate_main.fi"
for SRC in $ROOTS; do
    BASE=$(basename "$SRC" .fi)
    for STAGE in "opt:" "noopt:--no-opt" "dev:--opt-level=dev-fast"; do
        NAME=${STAGE%%:*}
        OPT=${STAGE#*:}
        if ! $FIRNC $OPT -o "$WORK/${BASE}_${NAME}" "$SRC" \
            2>"$WORK/build_${BASE}_${NAME}.log"; then
            fail "$SRC does not compile ($NAME)"
            head -6 "$WORK/build_${BASE}_${NAME}.log" | sed 's/^/        /'
        fi
    done
done
echo "   opt, --no-opt and dev-fast built for browser, live, http, url, jar"

B4="$WORK/b4_main_opt"
PROBE="$WORK/live_probe_opt"
HTTPC="$WORK/http_main_opt"
URLM="$WORK/url_main_opt"
JARM="$WORK/httpstate_main_opt"

echo "== 2. URL resolution against urllib.parse.urljoin =="
python3 tools/liveb4/url_check.py "$URLM" | tee "$WORK/url.txt" || \
    fail "url_check"

echo "== 3. HTTP dates and the cookie jar =="
python3 tools/liveb4/cookie_check.py "$JARM" | tee "$WORK/cookie.txt" || \
    fail "cookie_check"

echo "== 4. the HTTP client against a real server =="
python3 tools/liveb4/http_check.py "$HTTPC" | tee "$WORK/http.txt" || \
    fail "http_check"

echo "== 5. <script src> over a real socket: blocking, async, defer =="
python3 tools/liveb4/scripts_check.py "$B4" | tee "$WORK/scripts.txt" || \
    fail "scripts_check"

echo "== 6. the own cases: DOM, events, timers =="
python3 tools/liveb4/cases.py "$B4" | tee "$WORK/cases.txt" || \
    fail "cases"

echo "== 7. the narrowed recomputation, with the counter-check =="
python3 tools/liveb4/invalidate.py "$PROBE" | tee "$WORK/inval.txt" || \
    fail "invalidate"

echo "== 8. the official Web Platform Tests for the DOM =="
python3 tools/liveb4/wpt.py "$B4" --json "$WORK/wpt.json" \
    | tee "$WORK/wpt.txt" || fail "wpt"

echo "== 9. the same corpus in the other two build stages =="
# Not the whole corpus three times -- that is twenty minutes for a number
# that cannot differ unless the optimiser is broken. A slice of it, and
# the slice has to give the SAME quota.
for NAME in noopt dev; do
    python3 tools/liveb4/wpt.py "$WORK/b4_main_${NAME}" --limit 60 \
        > "$WORK/wpt_$NAME.txt" 2>&1
    A=$(grep -o 'B4-WPT: [0-9]* / [0-9]*' "$WORK/wpt_$NAME.txt" | head -1)
    echo "   $NAME  $A"
done
python3 tools/liveb4/wpt.py "$B4" --limit 60 > "$WORK/wpt_opt60.txt" 2>&1
AOPT=$(grep -o 'B4-WPT: [0-9]* / [0-9]*' "$WORK/wpt_opt60.txt" | head -1)
echo "   opt    $AOPT"
for NAME in noopt dev; do
    A=$(grep -o 'B4-WPT: [0-9]* / [0-9]*' "$WORK/wpt_$NAME.txt" | head -1)
    if [ "$A" != "$AOPT" ]; then
        fail "build stage $NAME gives $A instead of $AOPT"
    fi
done

echo "== 10. the regression limits =="
QUOTA=$(grep -o 'B4-WPT: [0-9]*' "$WORK/wpt.txt" | awk '{print $2}')
TOTAL=$(grep -oE 'B4-WPT: [0-9]+ / [0-9]+' "$WORK/wpt.txt" | awk '{print $4}')
WHOLE=$(grep -oE '[0-9]+ / [0-9]+ files whole' "$WORK/wpt.txt" | awk '{print $1}')
CASES=$(grep -oE 'CASES OK: [0-9]+' "$WORK/cases.txt" | awk '{print $3}')
RULES=$(grep -oE 'HTTP OK: [0-9]+' "$WORK/http.txt" | awk '{print $3}')
FACTOR=$(grep -oE 'elements styled   narrowed +[0-9]+   full +[0-9]+' \
    "$WORK/inval.txt" | awk '{print int($6/$4)}')
BAD=$(grep -oE 'differs from a full layout: [0-9]+' "$WORK/inval.txt" \
    | awk '{print $6}')
while read -r KEY VALUE _; do
    case "$KEY" in
        \#*|"") continue ;;
    esac
    GOT=""
    case "$KEY" in
        wpt_subtests) GOT="$QUOTA" ;;
        wpt_whole) GOT="$WHOLE" ;;
        own_cases) GOT="$CASES" ;;
        http_rules) GOT="$RULES" ;;
        narrow_factor) GOT="$FACTOR" ;;
    esac
    if [ -z "$GOT" ]; then
        continue
    fi
    if [ "$GOT" -lt "$VALUE" ]; then
        fail "$KEY fell to $GOT, the limit is $VALUE"
    else
        echo "   $KEY $GOT (limit $VALUE)"
    fi
done < tools/liveb4/minquota.txt
if [ "${BAD:-1}" != "0" ]; then
    fail "the narrowed recomputation changes the picture in $BAD boxes"
else
    echo "   layout after a narrowed update: 0 boxes differ from a full one"
fi

echo
if [ "$ERRORS" -eq 0 ]; then
    echo "B4 OK: $QUOTA / $TOTAL WPT subtests, $WHOLE files whole, "\
"$CASES own cases, $RULES HTTP rules, ${FACTOR}x less style work, 0 boxes wrong"
    exit 0
fi
echo "B4 FAILED: $ERRORS"
exit 1
