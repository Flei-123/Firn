#!/usr/bin/env bash
# tools/englisch/check.sh — GEGENPROBE zur Englisch-Umstellung (Etappe A).
# Sucht deutsche Wortteile in ALLEN Bezeichnern (pruefe.py), in allen
# Ausgabetexten der beiden Uebersetzer (pruefe_texte.py), in den Laengen der
# Byte-Felder (pruefe_laengen.py), in den Pfadnamen (pruefe_namen.py) UND
# in Kommentaren und Dokumentation (pruefe_kommentare.py, Etappe B).
# 0 Treffer ueberall = fertig.
set -euo pipefail
cd "$(dirname "$0")/../.."
python3 tools/englisch/pruefe.py
python3 tools/englisch/pruefe_texte.py
python3 tools/englisch/pruefe_laengen.py
python3 tools/englisch/pruefe_namen.py
python3 tools/englisch/pruefe_kommentare.py
