#!/usr/bin/env bash
# RUNDE KODIERER, TEIL 5 -- was der Kodierer SOFORT bringt.
#
# Frage a) Wie viel schneller wird das Uebersetzen ohne `as`?
#          Die Studie hat "as + ld" mit 195,1 us je Funktion gemessen. Wie
#          viel davon faellt weg?
#
# Verfahren: dieselbe Datei, dieselbe Baustufe, einmal ueber `as`, einmal
# ueber den eigenen Kodierer. BESTWERT aus mehreren Laeufen, damit die
# Fremdlast auf dem Wirt nicht mitgemessen wird.
set -u
cd "$(dirname "$0")/../.."
FIRNC="compiler/target/release/firnc"
export FIRNLIB="$(pwd)/lib"
RUNS="${RUNS:-5}"
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT

best() { # $@ = Befehl -> beste Zeit in Millisekunden
    local b=999999999
    for _ in $(seq "$RUNS"); do
        local t0 t1 d
        t0=$(date +%s%N)
        "$@" > /dev/null 2>&1
        t1=$(date +%s%N)
        d=$(( (t1 - t0) / 1000000 ))
        [ "$d" -lt "$b" ] && b=$d
    done
    echo "$b"
}

echo "======================================================================"
echo "TEIL 5a -- UEBERSETZUNGSZEIT MIT UND OHNE EXTERNEN ASSEMBLER"
echo "  Bestwert aus $RUNS Laeufen, Millisekunden fuer die GANZE Datei"
echo "======================================================================"
printf "%-34s %10s %10s %10s %8s\n" "Quelle" "mit as" "ohne as" "Ersparnis" "Faktor"

gesamt_alt=0
gesamt_neu=0
for f in "$@"; do
    [ -f "$f" ] || continue
    a=$(best "$FIRNC" -c -o "$W/o1.o" "$f")
    b=$(best "$FIRNC" --asm-intern -c -o "$W/o2.o" "$f")
    diff=$((a - b))
    fak=$(python3 -c "print('%.2fx' % ($a / max($b,1)))")
    printf "%-34s %10s %10s %10s %8s\n" "$(basename "$f")" "$a" "$b" "$diff" "$fak"
    gesamt_alt=$((gesamt_alt + a))
    gesamt_neu=$((gesamt_neu + b))
done
echo "----------------------------------------------------------------------"
printf "%-34s %10s %10s %10s\n" "SUMME" "$gesamt_alt" "$gesamt_neu" "$((gesamt_alt - gesamt_neu))"
python3 -c "
alt=$gesamt_alt; neu=$gesamt_neu
print('  Der externe Assembler kostete %.1f %% der Uebersetzungszeit.' % (100.0*(alt-neu)/max(alt,1)))
print('  Ohne ihn uebersetzt firnc %.2f mal so schnell.' % (alt/max(neu,1)))
"
