#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/english/check.sh — GEGENPROBE zur Englisch-Umstellung (Etappe A).
# Sucht deutsche Wortteile in ALLEN Bezeichnern (check.py), in allen
# Ausgabetexten der beiden Uebersetzer (check_texts.py), in den Laengen der
# Byte-Felder (check_lengths.py), in den Pfadnamen (check_names.py) UND
# in Kommentaren und Dokumentation (check_comments.py, Etappe B).
# 0 Treffer ueberall = fertig.
set -euo pipefail
cd "$(dirname "$0")/../.."
python3 tools/english/check.py
python3 tools/english/check_texts.py
python3 tools/english/check_lengths.py
python3 tools/english/check_names.py
python3 tools/english/check_comments.py
