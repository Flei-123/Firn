#!/usr/bin/env bash
# tools/dom_soak/run.sh -- soak run of the DOM prototype (acceptance item 2).
#
# Checks the promise from FIRN-ANFORDERUNGEN.md 13: "DOM prototype with
# parent/child back references and listener cycles that does not leak in a soak run."
#
# What is measured is the REAL memory consumption of the process (RSS from
# /proc/self/statm), not the self-report of the runtime. In addition the
# deliberately leaking counter-check runs EVERY TIME (lib/dom/soak_leak.fi, the same
# set of cycles with reference counts). If it stays green, the measuring method
# is broken and this script aborts -- a measurement that cannot show anything
# would be worse than none.
#
# Environment:
#   SOAK_SEC         time budget per version in seconds (default 600)
#   SOAK_CYCLES      maximum number of cycle sets (default 100000000)
#   SOAK_SAMPLE      cycles per data line (default 1000)
#   SOAK_MIN_CYCLES  minimum number of cycles for a valid verdict (default 100000)
#   SOAK_LEAK_CYCLES upper limit for the LEAKING counter-check (default 600000;
#                    since round 53 one set leaks 13 objects of 128 bytes,
#                    that is about 1.0 GiB -- before it was 6 of 64 bytes)
#   SOAK_LEAK_MB     hard memory brake for the counter-check in MiB (default 3072)
#
# WHY THE COUNTER-CHECK IS CAPPED: by construction it leaks about 384 bytes per
# cycle (6 of 7 objects at 64 bytes). Without a cap it eats double-digit
# gigabytes at the full number of cycles and takes the machine down with it -- measured
# on the first long run: 7.0 GB after 18.8 million cycles. 2 million cycles (~770 MiB)
# show the same picture and are harmless. In addition `ulimit -v` limits
# the address space as a hard brake.
set -uo pipefail
cd "$(dirname "$0")/../.."

FIRNC=compiler/target/release/firnc
ARBEIT=.dom-soak-work
AUS=tools/dom_soak
SEK=${SOAK_SEC:-600}
ZYKLEN=${SOAK_CYCLES:-100000000}
STICH=${SOAK_SAMPLE:-1000}
MINZ=${SOAK_MIN_CYCLES:-100000}
LECK_ZYKLEN=${SOAK_LEAK_CYCLES:-600000}
LECK_MB=${SOAK_LEAK_MB:-3072}
BUDGET_MS=$((SEK * 1000))

if [ ! -x "$FIRNC" ]; then
    echo "FEHLER: $FIRNC fehlt — zuerst 'cargo build --release' im Ordner compiler/."
    exit 1
fi
for f in lib/dom/dom.fi lib/dom/meas.fi lib/dom/soak_gc.fi lib/dom/soak_leak.fi; do
    if [ ! -f "$f" ]; then
        echo "FEHLER: $f fehlt — der DOM-Prototyp ist nicht gebaut."
        exit 1
    fi
done

rm -rf "$ARBEIT"
mkdir -p "$ARBEIT" "$AUS"
cp lib/dom/dom.fi lib/dom/meas.fi "$ARBEIT/"

# Create a working copy with the constants changed over.
# $1 source  $2 target  $3 budget ms  $4 cycles  $5 sample
stelle_um() {
    sed -e "s|^const BUDGET_MS: i64 = .*$|const BUDGET_MS: i64 = $3  // SOAK_BUDGET_MS|" \
        -e "s|^const CYCLES_MAX: i64 = .*$|const CYCLES_MAX: i64 = $4  // SOAK_CYCLES_MAX|" \
        -e "s|^const SAMPLE: i64 = .*$|const SAMPLE: i64 = $5  // SOAK_SAMPLE|" \
        "$1" > "$2"
    # The three lines really have to have been replaced.
    if ! grep -q "const BUDGET_MS: i64 = $3 " "$2"; then
        echo "FEHLER: BUDGET_MS in $1 nicht ersetzbar (Zeile veraendert?)."
        exit 1
    fi
    if ! grep -q "const CYCLES_MAX: i64 = $4 " "$2"; then
        echo "FEHLER: CYCLES_MAX in $1 nicht ersetzbar."
        exit 1
    fi
    if ! grep -q "const SAMPLE: i64 = $5 " "$2"; then
        echo "FEHLER: SAMPLE in $1 nicht ersetzbar."
        exit 1
    fi
}

echo "== DOM-Dauerlauf (Abnahmepunkt 2) =="
echo "   Budget je Fassung: ${SEK}s, hoechstens $ZYKLEN Zyklen, Stichprobe alle $STICH"

# ---------------------------------------------------------- 1. build stages
# Build both programs in ALL THREE build stages and compare a short run.
# A memory model that only holds with the optimiser switched on is worthless.
echo
echo "-- 1. Bau in drei Baustufen und Kurzlauf-Vergleich --"
STUFEN=("release-fast:" "no-opt:--no-opt" "dev-fast:--opt-level=dev-fast")
fehler=0
for variante in gc leak; do
    stelle_um "lib/dom/soak_$variante.fi" "$ARBEIT/kurz_$variante.fi" 3000 5000 1000
    erwartet=""
    for st in "${STUFEN[@]}"; do
        name=${st%%:*}
        opt=${st#*:}
        # shellcheck disable=SC2086
        if ! "$FIRNC" "$ARBEIT/kurz_$variante.fi" -o "$ARBEIT/kurz_${variante}_$name" $opt 2>"$ARBEIT/bau_${variante}_$name.err"; then
            echo "   FEHLER: Bau $variante/$name gescheitert:"
            head -5 "$ARBEIT/bau_${variante}_$name.err"
            fehler=1
            continue
        fi
        "$ARBEIT/kurz_${variante}_$name" > "$ARBEIT/kurz_${variante}_$name.tsv" 2>&1
        rc=$?
        zl=$(grep -c '^[0-9]' "$ARBEIT/kurz_${variante}_$name.tsv")
        fertig=$(grep -o 'zyklen=[0-9]*' "$ARBEIT/kurz_${variante}_$name.tsv" | head -1)
        if [ $rc -ne 0 ] || grep -q '^# fehler' "$ARBEIT/kurz_${variante}_$name.tsv"; then
            echo "   FEHLER: Kurzlauf $variante/$name endete mit $rc:"
            grep '^# fehler' "$ARBEIT/kurz_${variante}_$name.tsv" | head -2
            fehler=1
            continue
        fi
        if [ -z "$erwartet" ]; then
            erwartet="$fertig"
        elif [ "$fertig" != "$erwartet" ]; then
            echo "   FEHLER: $variante/$name liefert $fertig, erwartet $erwartet"
            fehler=1
        fi
        printf '   %-6s %-12s %s, %s Datenzeilen\n' "$variante" "$name" "$fertig" "$zl"
    done
done
if [ $fehler -ne 0 ]; then
    echo "ABBRUCH: die Baustufen liefern nicht dasselbe Ergebnis."
    exit 1
fi

# ------------------------------------------------------------ 2. measuring run
echo
echo "-- 2. Dauerlauf --"
for variante in gc leak; do
    grenze=$ZYKLEN
    if [ "$variante" = leak ]; then
        # Capped: see the head of the file. This version leaks on purpose.
        grenze=$LECK_ZYKLEN
    fi
    stelle_um "lib/dom/soak_$variante.fi" "$ARBEIT/soak_$variante.fi" "$BUDGET_MS" "$grenze" "$STICH"
    if ! "$FIRNC" "$ARBEIT/soak_$variante.fi" -o "$ARBEIT/soak_$variante" 2>"$ARBEIT/bau_$variante.err"; then
        echo "   FEHLER: Bau der Messfassung $variante gescheitert:"
        head -5 "$ARBEIT/bau_$variante.err"
        exit 1
    fi
    start=$(date +%s)
    if [ "$variante" = leak ]; then
        # Hard brake: the address space is limited so that a bug in the
        # counter-check never takes the machine with it.
        ( ulimit -v $((LECK_MB * 1024)); exec "$ARBEIT/soak_$variante" ) > "$AUS/measurement-$variante.tsv"
    else
        "$ARBEIT/soak_$variante" > "$AUS/measurement-$variante.tsv"
    fi
    rc=$?
    dauer=$(( $(date +%s) - start ))
    if [ $rc -ne 0 ]; then
        echo "   FEHLER: Lauf $variante endete mit $rc:"
        grep '^# fehler' "$AUS/measurement-$variante.tsv" | head -2
        exit 1
    fi
    echo "   $variante: $(grep '^# fertig' "$AUS/measurement-$variante.tsv") (${dauer}s Wanduhr)"
done

# ---------------------------------------------------------------- 3. evaluation
echo
echo "-- 3. Auswertung --"
LECK_MINZ=$((LECK_ZYKLEN / 4))
if [ "$LECK_MINZ" -gt "$MINZ" ]; then LECK_MINZ=$MINZ; fi
python3 - "$AUS/measurement-gc.tsv" "$AUS/measurement-leak.tsv" "$MINZ" "$LECK_MINZ" <<'PYEOF'
import sys

def lies(pfad):
    zeilen = []
    fertig = None
    with open(pfad, encoding='utf-8') as f:
        for z in f:
            z = z.strip()
            if not z:
                continue
            if z.startswith('#'):
                if z.startswith('# fertig'):
                    fertig = z
                continue
            t = z.split('\t')
            if len(t) != 7:
                raise SystemExit(f'FEHLER: {pfad}: Zeile mit {len(t)} Feldern statt 7: {z}')
            zeilen.append([int(x) for x in t])
    return zeilen, fertig

def median(xs):
    s = sorted(xs)
    n = len(s)
    if n == 0:
        return 0
    return s[n // 2] if n % 2 else (s[n // 2 - 1] + s[n // 2]) / 2

def urteil(pfad, minz):
    z, fertig = lies(pfad)
    if len(z) < 8:
        raise SystemExit(f'FEHLER: {pfad}: nur {len(z)} Stichproben — zu wenig fuer ein Urteil.')
    zyklen = z[-1][1]
    if zyklen < minz:
        raise SystemExit(f'FEHLER: {pfad}: nur {zyklen} Zyklen, verlangt sind {minz}.')
    n = len(z)
    v = n // 4                      # Aufwaermphase: erstes Viertel
    zweites = z[v:2 * v] or z[v:v + 1]
    letztes = z[3 * v:] or z[-1:]
    rss2, rssl = median([r[2] for r in zweites]), median([r[2] for r in letztes])
    leb2, lebl = median([r[3] for r in zweites]), median([r[3] for r in letztes])
    # Monotonicity: does the RSS rise continuously after the warm-up phase?
    nach = [r[2] for r in z[v:]]
    monoton = all(b >= a for a, b in zip(nach, nach[1:])) and nach[-1] > nach[0]
    # Threshold: a 5 % increase of the median counts as a leak.
    wuchs = rssl > rss2 * 1.05
    return {
        'pfad': pfad, 'zyklen': zyklen, 'stichproben': n, 'fertig': fertig,
        'rss2': rss2, 'rssl': rssl, 'leb2': leb2, 'lebl': lebl,
        'monoton': monoton, 'wuchs': wuchs,
        'leck': wuchs or monoton,
        'rss_max': max(r[2] for r in z), 'pause_max': max(r[6] for r in z),
        'laeufe': z[-1][4], 'heap': z[-1][5],
    }

gc = urteil(sys.argv[1], int(sys.argv[3]))
leck = urteil(sys.argv[2], int(sys.argv[4]))

def zeig(u, name):
    print(f'   {name}:')
    print(f'     Zyklen                {u["zyklen"]}  ({u["stichproben"]} Stichproben)')
    print(f'     RSS Median 2. Viertel {u["rss2"]} KiB')
    print(f'     RSS Median letztes V. {u["rssl"]} KiB')
    print(f'     RSS Hoechstwert       {u["rss_max"]} KiB')
    print(f'     lebende Objekte       {u["leb2"]} -> {u["lebl"]}')
    print(f'     Sammellaeufe          {u["laeufe"]}, Haldengroesse {u["heap"]} B')
    print(f'     laengste Pause        {u["pause_max"]} ns')
    print(f'     Urteil                {"LECK" if u["leck"] else "kein Leck"}'
          f' (Zuwachs {"ja" if u["wuchs"] else "nein"}, monoton {"ja" if u["monoton"] else "nein"})')

zeig(gc, 'GC-Fassung  (lib/dom/soak_gc.fi)')
zeig(leck, 'Zaehlverweis (lib/dom/soak_leak.fi, MUSS lecken)')

print()
fehler = 0
if gc['leck']:
    print('   FEHLGESCHLAGEN: die GC-Fassung leckt.')
    fehler = 1
else:
    faktor = leck['rssl'] / max(gc['rssl'], 1)
    print(f'   BESTANDEN: die GC-Fassung haelt den Verbrauch flach '
          f'({gc["rss2"]} -> {gc["rssl"]} KiB).')
    print(f'   Die Gegenprobe braucht am Ende das {faktor:.1f}-fache.')
if not leck['leck']:
    print('   FEHLGESCHLAGEN: die Gegenprobe leckt NICHT — die Messung taugt nichts.')
    fehler = 1
else:
    print(f'   Gegenprobe schlaegt an: {leck["rss2"]} -> {leck["rssl"]} KiB, '
          f'{leck["lebl"]} lebende Objekte.')
sys.exit(fehler)
PYEOF
rc=$?

echo
if [ $rc -eq 0 ]; then
    echo "OK: DOM-Dauerlauf bestanden (Messreihen in $AUS/measurement-gc.tsv und $AUS/measurement-leak.tsv)."
else
    echo "FEHLER: DOM-Dauerlauf NICHT bestanden."
fi
exit $rc
