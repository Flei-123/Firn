#!/usr/bin/env bash
# tools/dom_soak/run.sh — Dauerlauf des DOM-Prototyps (Abnahmepunkt 2).
#
# SKELETTFASSUNG des Lead-Architekten: Modul `mess` baut diese Datei aus
# (Vertrag in PLAN.md, Runde 4, Abschnitt 4). Sie scheitert absichtlich
# sichtbar, solange die Messung nicht gebaut ist — eine leere Erfolgsmeldung
# waere schlimmer als ein Fehlschlag.
set -euo pipefail
cd "$(dirname "$0")/../.."

echo "== DOM-Dauerlauf (Abnahmepunkt 2) =="
if [ ! -f lib/dom/soak_gc.fi ]; then
    echo "FEHLER: lib/dom/soak_gc.fi fehlt — der DOM-Prototyp ist nicht gebaut."
    echo "        Stand siehe ABNAHME.md Punkt 2 und docs/berichte/."
    exit 1
fi
echo "FEHLER: tools/dom_soak/run.sh ist noch nicht ausgebaut (Modul 'mess')."
exit 1
