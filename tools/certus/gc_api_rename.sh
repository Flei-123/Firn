#!/bin/bash
# tools/certus/gc_api_rename.sh -- renames the collector diagnostics API that the
# Certus fork (/root/firnc-gc) called by German names to the English names of
# lib/gc/gc.fi. Two Certus files use them (lib/browser/certus_app.fi and
# lib/browser/web1_main.fi); the script edits whatever it is pointed at.
#
#   bash tools/certus/gc_api_rename.sh <file-or-dir>...        (edits in place)
#
# Whole words only, longest names first, *.fi / *.py / *.sh files only.
set -eu
# english: ok -- the left column is the OLD (German) name, that is the point of the script
MAP='gc_zaehle_klassen=gc_count_classes
gc_klasse_anzahl=gc_class_count
gc_klasse_oktette=gc_class_bytes
gc_klassen_n=gc_class_total
gc_klasse_name=gc_class_name
gc_klasse_groesse=gc_class_size
gc_laengen=gc_len_histogram
gc_laenge_fach=gc_len_bucket
gc_feld_zaehle64=gc_field_count64
gc_feld_zaehle=gc_field_count32
gc_slots_zeigt_auf=gc_slot_refs_to
gc_zeigt_auf=gc_refs_to
gc_zeit_ty=gc_time_by_phase
gc_anzahl_ty=gc_slices_by_phase
gc_scan_worte=gc_scan_words'
# english: ok -- the same old names as a pattern
OLD='gc_(zaehle_klassen|klasse_anzahl|klasse_oktette|klassen_n|klasse_name|klasse_groesse|laengen|laenge_fach|feld_zaehle64|feld_zaehle|zeigt_auf|slots_zeigt_auf|zeit_ty|anzahl_ty|scan_worte)\b'
files=$(grep -rlE "$OLD" "$@" --include='*.fi' --include='*.py' --include='*.sh' 2>/dev/null || true)
for f in $files; do
    echo "$MAP" | while IFS='=' read -r old new; do
        sed -i "s/\b$old\b/$new/g" "$f"
    done
    echo "renamed in $f"
done
