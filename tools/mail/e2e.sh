#!/usr/bin/env bash
# tools/mail/e2e.sh -- end to end: lib/mail/smtp.fi + imap.fi (driver tools/mail/e2e_main.fi)
# against the fake servers (tools/mail/fake_mail.py), smtplib / imaplib as the reference.
#   bash tools/mail/e2e.sh                 (E2E_SEEDS="1 2" E2E_MSGS=20 E2E_STAGES="no-opt dev-fast release-fast release-safe")
# Exit 0 = 0 differences in every stage. Heavy: run it through /root/jarvis/bin/heavy.
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=compiler/target/release/firnc; [ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$PWD/lib"
W=$(mktemp -d "${TMPDIR:-/tmp}/firn-e2e.XXXXXX"); trap 'rm -rf "$W"' EXIT
ERR=0
for n in ${E2E_STAGES:-no-opt dev-fast release-fast release-safe}; do
  case "$n" in no-opt) o=--no-opt;; *) o=--opt-level=$n;; esac
  $FIRNC $o -o "$W/drv.$n" tools/mail/e2e_main.fi 2> "$W/err" || { echo "FAIL compile $n"; head -5 "$W/err"; ERR=1; continue; }
  mkdir -p "$W/w.$n"
  out=$(python3 -I tools/mail/e2e.py "$W/drv.$n" "$W/w.$n" ${E2E_SEEDS:-1 2} 2>&1); rc=$?
  echo "$out" | grep -A2 '^DIFF' | head -30
  echo "  $n: $(echo "$out" | tail -1)"
  [ $rc -eq 0 ] || ERR=1
done
exit $ERR
