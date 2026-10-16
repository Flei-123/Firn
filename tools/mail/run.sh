#!/usr/bin/env bash
# tools/mail/run.sh -- lib/mail/rfc822.fi against Python's `email`, plus a fuzz run.
#   bash tools/mail/run.sh            (MAIL_CASES=3000 MAIL_FUZZ=30 MAIL_SEEDS="1 2")
# Cases: tools/mail/gen.py (messages, address lists, encoded words, dates, QP, base64,
# builder output read back by Python). All four build levels. Exit 0 = 0 differences, 0 crashes.
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=compiler/target/release/firnc; [ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$PWD/lib"
W=$(mktemp -d "${TMPDIR:-/tmp}/firn-mail.XXXXXX"); trap 'rm -rf "$W"' EXIT
ERR=0
for stage in "no-opt:--no-opt" "dev-fast:--opt-level=dev-fast" "release-fast:--opt-level=release-fast" "release-safe:--opt-level=release-safe"; do
  n=${stage%%:*}; o=${stage#*:}
  $FIRNC $o -o "$W/check.$n" tools/mail/check.fi 2> "$W/err" || { echo "FAIL compile $n"; head -5 "$W/err"; ERR=1; continue; }
  for s in ${MAIL_SEEDS:-1 2}; do
    [ -f "$W/cases.$s" ] || python3 tools/mail/gen.py gen $s ${MAIL_CASES:-3000} "$W/cases.$s" "$W/exp.$s"
    "$W/check.$n" "$W/cases.$s" "$W/res" && out=$(python3 tools/mail/gen.py cmp "$W/cases.$s" "$W/exp.$s" "$W/res" | tail -1) || ERR=1
    echo "  $n seed $s: $out"; case "$out" in *" 0 differences"*) ;; *) ERR=1;; esac
  done
done
$FIRNC --opt-level=release-safe -o "$W/fuzz" tools/mail/fuzz.fi 2> "$W/err" || { echo "FAIL compile fuzz"; head -5 "$W/err"; exit 1; }
for s in ${MAIL_SEEDS:-1 2}; do echo "  fuzz seed $s: $(timeout 600 "$W/fuzz" "$W/cases.$s" ${MAIL_FUZZ:-30} 2>&1 | tail -1)"; [ ${PIPESTATUS[0]} -eq 0 ] || ERR=1; done
exit $ERR
