#!/usr/bin/env bash
# tools/englisch/pruefe.sh — GEGENPROBE zur Englisch-Umstellung (Etappe A).
# Sucht deutsche Wortteile in ALLEN Bezeichnern. 0 Treffer = fertig.
set -euo pipefail
cd "$(dirname "$0")/../.."
python3 tools/englisch/pruefe.py
