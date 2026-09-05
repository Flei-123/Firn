#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# spdx-umstellen.sh -- Firn: GPL-2.0-only und MIT  ->  MPL-2.0
#
# NICHT AUTOMATISCH AUSFUEHREN. Standard ist Trockenlauf.
#
#   bash spdx-umstellen.sh            # zeigt nur, was passieren wuerde
#   bash spdx-umstellen.sh --echt     # schreibt wirklich
#   bash spdx-umstellen.sh --pruefen  # Dateien ohne SPDX-Zeile
#
# ACHTUNG, EINE ECHTE ENTSCHEIDUNG STECKT HIER DRIN:
# `main` traegt heute NOCH den Certus-Code (lib/browser, lib/css, lib/dom,
# lib/font, lib/html, lib/js, lib/layout, lib/net, lib/paint, lib/tls).
# Certus ist am 30.08.2026 in ein eigenes Repo herausgeloest worden und
# soll GPL-3.0-or-later werden -- NICHT MPL. Deshalb laesst dieses Skript
# diese zehn Verzeichnisse in Ruhe. Zwei Wege, danach:
#   (a) sie aus dem Firn-Repo entfernen (sie leben jetzt im Certus-Repo), oder
#   (b) sie hier auf GPL-3.0-or-later setzen (CERTUS_MIT_UMSTELLEN=1).
set -u
WURZEL="$(git rev-parse --show-toplevel)"   # das Skript liegt unter lizenz/, arbeitet aber an der Wurzel
cd "$WURZEL"
NEU='SPDX-License-Identifier: MPL-2.0'
MODUS="${1:---trocken}"
CERTUS_DIRS=(lib/browser lib/css lib/dom lib/font lib/html lib/js lib/layout lib/net lib/paint lib/tls)

aus=()
for d in "${CERTUS_DIRS[@]}"; do aus+=(":!$d/**"); done

dateien() {
  git grep -Il -e 'SPDX-License-Identifier: GPL-2.0-only' -e 'SPDX-License-Identifier: MIT' -- \
    ':!vendor/**' ':!testdata/**' ':!tests/data/**' ':!tools/ucd/*.txt' \
    ':!bench/tokenizer/**' "${aus[@]}" | sort -u
}

if [ "$MODUS" = "--pruefen" ]; then
  echo "== Dateien OHNE SPDX-Zeile (haengen an .reuse/dep5):"
  comm -23 <(git ls-files | sort) <(git grep -Il 'SPDX-License-Identifier' | sort)
  echo
  echo "== Certus-Verzeichnisse, die dieses Skript AUSLAESST:"
  for d in "${CERTUS_DIRS[@]}"; do printf '   %-14s %s Dateien\n' "$d" "$(git ls-files -- "$d" | wc -l)"; done
  exit 0
fi

n=0
while IFS= read -r f; do
  [ -z "$f" ] && continue
  alt="$(grep -m1 -o 'SPDX-License-Identifier: \(GPL-2.0-only\|MIT\)' "$f")"
  [ -z "$alt" ] && continue
  n=$((n+1))
  if [ "$MODUS" = "--echt" ]; then
    python3 - "$f" "$alt" "$NEU" <<'PY'
import sys, io
pfad, alt, neu = sys.argv[1:4]
t = io.open(pfad, encoding='utf-8', errors='surrogateescape').read()
io.open(pfad, 'w', encoding='utf-8', errors='surrogateescape').write(t.replace(alt, neu, 1))
PY
  else
    printf '%s\n    %s  ->  %s\n' "$f" "$alt" "$NEU"
  fi
done < <(dateien)

echo
echo "$n Dateien betroffen."
[ "$MODUS" = "--echt" ] || echo "TROCKENLAUF -- nichts geschrieben. Mit --echt wirklich umstellen."
cat <<'REST'

DANACH VON HAND:
  1. LICENSE ersetzen (Vorschlag: lizenz/firn/LICENSE), LICENSE.MIT und
     LICENSE.MIT.old BEHALTEN -- sie belegen die Historie.
  2. NOTICE hinzufuegen, LICENSING.md neu schreiben (der heutige Text
     begruendet noch GPL-2.0-only + MIT und stimmt danach nicht mehr).
  3. .reuse/dep5 nachziehen: "Files: *" -> "License: MPL-2.0";
     die MIT-Blockliste faellt weg; testdata/vendor bleiben wie sie sind.
  4. In JEDE umgestellte Datei gehoert nach MPL Exhibit A ein Hinweis auf
     die Lizenz. Die SPDX-Zeile allein genuegt Mozilla ausdruecklich
     ("include the notice in a location such as a LICENSE file in a
     relevant directory"); wer sichergehen will, setzt Exhibit A wortwoertlich.
  5. NICHT Exhibit B setzen -- das wuerde die GPL-Vertraeglichkeit
     (MPL 1.5, 3.3) zerstoeren, die Certus und Osum brauchen.
  6. ./test.sh -- besonders die Tests, deren Zeile 1 eine Erwartung ist.
REST
