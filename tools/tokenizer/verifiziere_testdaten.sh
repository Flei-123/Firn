#!/usr/bin/env bash
# Proves that NOTHING was changed in the html5lib test data.
#
# Two things are checked:
#   1. Exactly the 14 expected .test files lie in
#      testdata/html5lib-tokenizer/ -- no more, no fewer.
#   2. Every file has exactly the sha256 sum from
#      tools/tokenizer/testdaten.sha256. These sums were checked byte for
#      byte against the upstream commit
#      224991ec10db04f056a89eed8b0bd8695fd2950e of
#      https://github.com/html5lib/html5lib-tests (path tokenizer/).
#
# In addition the number of cases is counted (expectation: 6810), so that
# a change that happened to have the same sum would show up as well.
#
# With --gegen-upstream the script loads the files of the fixed
# commit from GitHub again and compares directly (needs the network; without it
# the switch is a clean error, not a silent success).
#
# Usage:  bash tools/tokenizer/verifiziere_testdaten.sh [--gegen-upstream]
# Return: 0 = everything unchanged, 1 = a deviation was found.
set -euo pipefail
cd "$(dirname "$0")/../.."

COMMIT="224991ec10db04f056a89eed8b0bd8695fd2950e"
DATEN="testdata/html5lib-tokenizer"
SUMMEN="tools/tokenizer/testdaten.sha256"
ERWARTETE_DATEIEN=14
ERWARTETE_FAELLE=6810
GEGEN_UPSTREAM=0
[ "${1:-}" = "--gegen-upstream" ] && GEGEN_UPSTREAM=1

fehler=0

echo "== Testdaten pruefen: $DATEN =="
echo "   Referenz: html5lib-tests @ $COMMIT (Pfad tokenizer/)"

# --- 1. the set of files ---------------------------------------------------
vorhanden=$(cd "$DATEN" && ls -1 *.test 2>/dev/null | sort)
erwartet=$(awk '!/^#/ && NF==2 {print $2}' "$SUMMEN" | sort)
anzahl=$(printf '%s\n' "$vorhanden" | grep -c . || true)

if [ "$vorhanden" != "$erwartet" ]; then
    echo "   FEHLER: der Dateibestand weicht ab."
    diff <(printf '%s\n' "$erwartet") <(printf '%s\n' "$vorhanden") \
        | sed 's/^/          /' || true
    fehler=1
fi
if [ "$anzahl" -ne "$ERWARTETE_DATEIEN" ]; then
    echo "   FEHLER: $anzahl .test-Dateien statt $ERWARTETE_DATEIEN"
    fehler=1
else
    echo "   Dateien : $anzahl (erwartet $ERWARTETE_DATEIEN)"
fi

# --- 2. sha256 against the fixed set ----------------------------------------
# sha256sum reads the names relative to the data directory.
if (cd "$DATEN" && grep -v '^#' "../../$SUMMEN" | grep . | sha256sum -c --status -); then
    echo "   sha256  : alle $ERWARTETE_DATEIEN Summen stimmen"
else
    echo "   FEHLER: mindestens eine Summe weicht ab:"
    (cd "$DATEN" && grep -v '^#' "../../$SUMMEN" | grep . | sha256sum -c - 2>&1 \
        | grep -v ': OK$' | sed 's/^/          /') || true
    fehler=1
fi

# --- 3. count the cases -----------------------------------------------------
faelle=$(python3 - "$DATEN" <<'PY'
import glob, json, os, sys
n = 0
for pfad in sorted(glob.glob(os.path.join(sys.argv[1], "*.test"))):
    d = json.load(open(pfad, encoding="utf-8"))
    n += len(d.get("tests", d.get("xmlViolationTests", [])))
print(n)
PY
)
if [ "$faelle" -ne "$ERWARTETE_FAELLE" ]; then
    echo "   FEHLER: $faelle Testfaelle statt $ERWARTETE_FAELLE"
    fehler=1
else
    echo "   Faelle  : $faelle (erwartet $ERWARTETE_FAELLE)"
fi

# --- 4. optional: directly against upstream ---------------------------------
if [ "$GEGEN_UPSTREAM" -eq 1 ]; then
    echo
    echo "== Direktvergleich mit GitHub (Commit $COMMIT) =="
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    for f in $(printf '%s\n' "$erwartet"); do
        url="https://raw.githubusercontent.com/html5lib/html5lib-tests/$COMMIT/tokenizer/$f"
        if ! curl -sSfL --max-time 60 -o "$tmp/$f" "$url"; then
            echo "   FEHLER: $f liess sich nicht laden ($url)"
            fehler=1
            continue
        fi
        if cmp -s "$tmp/$f" "$DATEN/$f"; then
            echo "   $f: identisch"
        else
            echo "   $f: WEICHT AB vom Upstream"
            fehler=1
        fi
    done
fi

echo
if [ "$fehler" -eq 0 ]; then
    echo "OK: Testdaten unveraendert (14 Dateien, 6810 Faelle, sha256 wie Upstream)."
    exit 0
fi
echo "FEHLGESCHLAGEN: die Testdaten weichen vom festgeschriebenen Stand ab."
exit 1
