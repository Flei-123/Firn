#!/usr/bin/env bash
# tools/xml_cross/run.sh -- lib/std/xml.fi against Python's expat / xml.etree,
# and against hostile input (r319).
#
# FOUR THINGS, and each is judged by somebody who is not this library:
#
#   1. VALID DOCUMENTS. gen.py makes XML_VALID (default 3000) random
#      well-formed documents (nesting, attributes in both quote styles, all
#      five entities and numeric references, CDATA, comments, PIs, CR/CRLF,
#      non-ASCII text and names, prefixed names, declarations, a BOM) and the
#      tree Python's expat builds (through xml.etree.ElementTree.TreeBuilder).
#      The Firn program prints its tree in the same text form and compares;
#      every document must come out equal, and parse(write(parse(x))) too.
#   2. MUTATED DOCUMENTS. XML_MUTATED (default 6000) documents damaged at byte
#      level (flips, cuts, inserted `<`, `&`, `]]>`, `--`, bad UTF-8 ...).
#      Python decides accepted/refused; Firn must agree and, if both accept,
#      the trees must be equal. The only allowed differences are the
#      DOCUMENTED ones (std.xml HONEST list): DOCTYPE, an encoding other
#      than UTF-8, an XML declaration whose version is not `1.<digits>` (all
#      stricter in Firn), and non-ASCII name characters that XML 1.0 5th
#      edition allows and expat does not (laxer in Firn). They are counted.
#   3. FUZZ. xml_cli mutates a corpus itself, parses every result with an
#      UNMAPPED PAGE right behind the input (an over-read is a SIGSEGV), in a
#      release-safe build too (an overflow is a trap), round-trips what is
#      accepted, and compares the resident set at 10 % and at the end (a
#      leak shows as growth). No crash, no hang (timeout), no leak.
#   4. COUNTER-CHECKS: the guard page really kills an over-read; a damaged
#      expectation file really makes the comparison fail.
#
# ALL IN FOUR BUILD STAGES (release-fast, release-safe, dev-fast, no-opt);
# the fuzz runs are long in the two release stages and short in the others.
# XML_FAST=1 runs release-safe only.
set -uo pipefail
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
W=$(mktemp -d /tmp/firn-xmlx.XXXXXX)
trap 'rm -rf "$W"' EXIT
NVALID=${XML_VALID:-3000}
NMUT=${XML_MUTATED:-6000}
FUZZ=${XML_FUZZ:-40000}
ERRORS=0
report() { echo "  FAIL  $1"; ERRORS=$((ERRORS + 1)); }

mkdir -p "$W/valid" "$W/mut"
python3 -I tools/xml_cross/gen.py "$W/valid" "$NVALID" 1 valid "$W/seeds.txt" | sed 's/^/  /'
python3 -I tools/xml_cross/gen.py "$W/mut" "$NMUT" 2 mutated | sed 's/^/  /'

STAGES="release-fast:--opt-level=release-fast release-safe:--opt-level=release-safe dev-fast:--opt-level=dev-fast no-opt:--no-opt"
[ "${XML_FAST:-0}" = "1" ] && STAGES="release-safe:--opt-level=release-safe"

first=1
for stage in $STAGES; do
    name=${stage%%:*}
    opt=${stage#*:}
    cli="$W/xml_cli.$name"
    if ! "$FIRNC" $opt -o "$cli" tools/xml_cross/xml_cli.fi 2> "$W/err"; then
        report "$name: tools/xml_cross/xml_cli.fi does not compile"
        sed 's/^/        /' "$W/err" | head -8
        continue
    fi
    # --- 1. valid documents
    if "$cli" cross "$W/valid" "$NVALID" > "$W/valid.$name.out" 2>&1; then
        bo=$(awk '$1=="both_ok"{print $2}' "$W/valid.$name.out")
        [ "$bo" = "$NVALID" ] || report "$name: valid: both_ok=$bo, expected $NVALID"
    else
        report "$name: valid corpus differs from Python"
        head -12 "$W/valid.$name.out" | sed 's/^/        /'
    fi
    # --- 2. mutated documents
    if ! "$cli" cross "$W/mut" "$NMUT" > "$W/mut.$name.out" 2>&1; then
        report "$name: mutated corpus differs from Python beyond the documented deviations"
        head -14 "$W/mut.$name.out" | sed 's/^/        /'
    fi
    if [ $first = 1 ]; then
        first=0
        echo "  xml: valid   $(awk '$1=="cases"{c=$2} $1=="both_ok"{o=$2} $1=="tree_diff"{t=$2} $1=="roundtrip_bad"{r=$2} END{printf "%d documents, %d trees equal to Python, %d differ, %d round-trip failures", c, o, t, r}' "$W/valid.$name.out")"
        echo "  xml: mutated $(awk '$1=="cases"{c=$2} $1=="both_ok"{o=$2} $1=="both_err"{e=$2} $1=="line_agree"{l=$2} $1=="tree_diff"{t=$2} $1=="firn_laxer"{x=$2} $1=="firn_stricter"{s=$2} $1=="stricter_documented"{sd=$2} $1=="laxer_nonascii_name_x5"{lx=$2} END{printf "%d documents: %d accepted by both (trees equal), %d refused by both (same line %d), undocumented laxer %d / stricter %d, documented: Firn stricter %d, Firn laxer (non-ASCII names) %d, tree differences %d", c, o, e, l, x, s, sd, lx, t}' "$W/mut.$name.out")"
        refstage=$name
    else
        # the answer must not depend on the build stage
        cmp -s "$W/mut.$refstage.out" "$W/mut.$name.out" || report "$name: the mutated-corpus result differs from $refstage"
        cmp -s "$W/valid.$refstage.out" "$W/valid.$name.out" || report "$name: the valid-corpus result differs from $refstage"
    fi
    # --- 3. fuzz
    n=$FUZZ
    case "$name" in dev-fast|no-opt) n=$((FUZZ / 10)) ;; esac
    if timeout 600 "$cli" fuzz "$W/seeds.txt" "$n" "${XML_FUZZ_SEED:-1}" > "$W/fuzz.$name.out" 2>&1; then
        echo "  xml: fuzz    $name: $(awk '$1=="fuzz_iterations"{n=$2} $1=="accepted"{a=$2} $1=="refused"{r=$2} $1=="bad"{b=$2} $1=="rss_pages_after_10pct"{r0=$2} $1=="rss_pages_end"{r1=$2} END{printf "%d mutants (%d accepted+round-tripped, %d refused), bad %d, RSS pages %d -> %d", n, a, r, b, r0, r1}' "$W/fuzz.$name.out")"
    else
        report "$name: fuzz failed (crash, hang, round trip or leak)"
        tail -6 "$W/fuzz.$name.out" | sed 's/^/        /'
    fi
done

# --- 4. counter-checks (with the first stage that was built)
cli=$(ls "$W"/xml_cli.* 2>/dev/null | head -1)
if [ -n "$cli" ]; then
    "$cli" overread x > /dev/null 2>&1
    rc=$?
    if [ $rc -eq 139 ]; then
        echo "  xml: counter-check: reading one octet past the input kills the process (SIGSEGV), so the guard page works"
    else
        report "counter-check: the guard page did not stop an over-read (exit $rc)"
    fi
    # a fuzz run that never frees its documents must be reported as a leak (exit 2)
    "$cli" fuzz "$W/seeds.txt" 4000 3 leak > "$W/leak.out" 2>&1
    rc=$?
    if [ $rc -eq 2 ] && grep -q 'RSS grew' "$W/leak.out"; then
        echo "  xml: counter-check: a fuzz run that leaks every document is reported (RSS $(awk '$1=="rss_pages_after_10pct"{a=$2} $1=="rss_pages_end"{b=$2} END{printf "%d -> %d pages", a, b}' "$W/leak.out")), so the leak test works"
    else
        report "counter-check: a leaking fuzz run was not noticed (exit $rc)"
    fi
    mkdir -p "$W/bad"
    for i in 0 1 2; do cp "$W/valid/$i.xml" "$W/bad/$i.xml"; cp "$W/valid/$i.exp" "$W/bad/$i.exp"; done
    sed -i '2s/^/x/' "$W/bad/1.exp"
    if "$cli" cross "$W/bad" 3 > "$W/bad.out" 2>&1; then
        report "counter-check: a damaged expectation was not noticed"
    elif grep -q '^tree_diff 1$' "$W/bad.out"; then
        echo "  xml: counter-check: one damaged expectation is reported as tree_diff 1"
    else
        report "counter-check: unexpected result for a damaged expectation"
    fi
fi

# --- speed (information, not a verdict): a 24 MB document against Python's expat
python3 -I - "$W" <<'PY'
import sys, random
w = sys.argv[1]
rng = random.Random(3)
parts = ['<?xml version="1.0" encoding="UTF-8"?>\n<catalog>\n']
n = 0
size = 0
while size < 24_000_000:
    line = '  <item id="%d" kind="k%d"><name>Item %d &amp; co</name><note><![CDATA[x<y]]> caf\u00e9</note></item>\n' % (n, rng.randrange(9), n)
    parts.append(line)
    size += len(line)
    n += 1
parts.append('</catalog>\n')
open(w + "/big.xml", "w", encoding="utf-8").write("".join(parts))
PY
if [ -n "$cli" ]; then
    FASTCLI="$W/xml_cli.release-fast"
    [ -x "$FASTCLI" ] || FASTCLI="$cli"
    python3 -I - "$FASTCLI" "$W/big.xml" <<'PY'
import subprocess, sys, time
import xml.etree.ElementTree as ET
cli, f = sys.argv[1], sys.argv[2]
data = open(f, "rb").read()
t = time.perf_counter(); ET.fromstring(data); tp = time.perf_counter() - t
t = time.perf_counter(); r = subprocess.run([cli, "check", f]); tf = time.perf_counter() - t
mb = len(data) / 1e6
print("  xml: speed   %.0f MB: Firn %.2f s (%.0f MB/s, incl. reading the file and starting), Python ET.fromstring %.2f s (%.0f MB/s)" % (mb, tf, mb / tf, tp, mb / tp))
sys.exit(r.returncode)
PY
    [ $? -eq 0 ] || report "speed run: the 24 MB document was refused"
fi

if [ $ERRORS -eq 0 ]; then
    echo "XML CROSS OK"
    exit 0
fi
echo "XML CROSS FAILED ($ERRORS)"
exit 1
