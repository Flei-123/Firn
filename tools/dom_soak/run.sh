#!/usr/bin/env bash
# tools/dom_soak/run.sh — Dauerlauf des DOM-Prototyps (Abnahmepunkt 2).
#
# Prueft die Zusage aus FIRN-ANFORDERUNGEN.md §13: "DOM-Prototyp mit Eltern-/
# Kind-Rueckverweisen und Listener-Zyklen, der im Dauerlauf nicht leckt."
#
# Gemessen wird der ECHTE Speicherverbrauch des Prozesses (RSS aus
# /proc/self/statm), nicht die Selbstauskunft der Laufzeit. Zusaetzlich laeuft
# JEDES MAL die absichtlich leckende Gegenprobe (lib/dom/soak_leak.fi, gleicher
# Zyklensatz mit Zaehlverweisen). Bleibt die gruen, ist das Messverfahren
# kaputt und dieses Skript bricht ab — eine Messung, die nichts anzeigen kann,
# waere schlimmer als keine.
#
# Umgebung:
#   SOAK_SEK         Laufzeitbudget je Fassung in Sekunden (Standard 600)
#   SOAK_ZYKLEN      Hoechstzahl Zyklensaetze (Standard 100000000)
#   SOAK_STICHPROBE  Zyklen je Datenzeile (Standard 1000)
#   SOAK_MIN_ZYKLEN  Mindestzahl Zyklen fuer ein gueltiges Urteil (Standard 100000)
#   SOAK_LECK_ZYKLEN Obergrenze fuer die LECKENDE Gegenprobe (Standard 600000;
#                    seit Runde 53 leckt ein Satz 13 Objekte zu 128 Byte,
#                    das sind rund 1,0 GiB — vorher 6 zu 64 Byte)
#   SOAK_LECK_MB     harte Speicherbremse fuer die Gegenprobe in MiB (Standard 3072)
#
# WARUM DIE GEGENPROBE GEDECKELT IST: sie leckt bauartbedingt rund 384 Byte je
# Zyklus (6 von 7 Objekten a 64 Byte). Ohne Deckel frisst sie bei voller
# Zyklenzahl zweistellige Gigabyte und reisst die Maschine mit — beim ersten
# Langlauf gemessen: 7,0 GB nach 18,8 Mio. Zyklen. 2 Mio. Zyklen (~770 MiB)
# zeigen dasselbe Bild und sind ungefaehrlich. Zusaetzlich begrenzt `ulimit -v`
# den Adressraum als harte Bremse.
set -uo pipefail
cd "$(dirname "$0")/../.."

FIRNC=compiler/target/release/firnc
ARBEIT=.dom-soak-work
AUS=tools/dom_soak
SEK=${SOAK_SEK:-600}
ZYKLEN=${SOAK_ZYKLEN:-100000000}
STICH=${SOAK_STICHPROBE:-1000}
MINZ=${SOAK_MIN_ZYKLEN:-100000}
LECK_ZYKLEN=${SOAK_LECK_ZYKLEN:-600000}
LECK_MB=${SOAK_LECK_MB:-3072}
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

# Arbeitskopie mit umgestellten Konstanten anlegen.
# $1 Quelle  $2 Ziel  $3 Budget ms  $4 Zyklen  $5 Stichprobe
stelle_um() {
    sed -e "s|^const BUDGET_MS: i64 = .*$|const BUDGET_MS: i64 = $3  // SOAK_BUDGET_MS|" \
        -e "s|^const ZYKLEN_MAX: i64 = .*$|const ZYKLEN_MAX: i64 = $4  // SOAK_ZYKLEN_MAX|" \
        -e "s|^const STICHPROBE: i64 = .*$|const STICHPROBE: i64 = $5  // SOAK_STICHPROBE|" \
        "$1" > "$2"
    # Die drei Zeilen muessen wirklich ersetzt worden sein.
    if ! grep -q "const BUDGET_MS: i64 = $3 " "$2"; then
        echo "FEHLER: BUDGET_MS in $1 nicht ersetzbar (Zeile veraendert?)."
        exit 1
    fi
    if ! grep -q "const ZYKLEN_MAX: i64 = $4 " "$2"; then
        echo "FEHLER: ZYKLEN_MAX in $1 nicht ersetzbar."
        exit 1
    fi
    if ! grep -q "const STICHPROBE: i64 = $5 " "$2"; then
        echo "FEHLER: STICHPROBE in $1 nicht ersetzbar."
        exit 1
    fi
}

echo "== DOM-Dauerlauf (Abnahmepunkt 2) =="
echo "   Budget je Fassung: ${SEK}s, hoechstens $ZYKLEN Zyklen, Stichprobe alle $STICH"

# ---------------------------------------------------------------- 1. Baustufen
# Beide Programme in ALLEN DREI Baustufen bauen und einen Kurzlauf vergleichen.
# Ein Speichermodell, das nur bei eingeschaltetem Optimierer haelt, taugt nichts.
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

# ------------------------------------------------------------------ 2. Messlauf
echo
echo "-- 2. Dauerlauf --"
for variante in gc leak; do
    grenze=$ZYKLEN
    if [ "$variante" = leak ]; then
        # Gedeckelt: siehe Kopf der Datei. Diese Fassung leckt absichtlich.
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
        # Harte Bremse: der Adressraum ist begrenzt, damit ein Fehler in der
        # Gegenprobe niemals die Maschine mitnimmt.
        ( ulimit -v $((LECK_MB * 1024)); exec "$ARBEIT/soak_$variante" ) > "$AUS/messung-$variante.tsv"
    else
        "$ARBEIT/soak_$variante" > "$AUS/messung-$variante.tsv"
    fi
    rc=$?
    dauer=$(( $(date +%s) - start ))
    if [ $rc -ne 0 ]; then
        echo "   FEHLER: Lauf $variante endete mit $rc:"
        grep '^# fehler' "$AUS/messung-$variante.tsv" | head -2
        exit 1
    fi
    echo "   $variante: $(grep '^# fertig' "$AUS/messung-$variante.tsv") (${dauer}s Wanduhr)"
done

# ---------------------------------------------------------------- 3. Auswertung
echo
echo "-- 3. Auswertung --"
LECK_MINZ=$((LECK_ZYKLEN / 4))
if [ "$LECK_MINZ" -gt "$MINZ" ]; then LECK_MINZ=$MINZ; fi
python3 - "$AUS/messung-gc.tsv" "$AUS/messung-leak.tsv" "$MINZ" "$LECK_MINZ" <<'PYEOF'
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
    # Monotonie: steigt der RSS nach der Aufwaermphase durchgehend?
    nach = [r[2] for r in z[v:]]
    monoton = all(b >= a for a, b in zip(nach, nach[1:])) and nach[-1] > nach[0]
    # Schwelle: 5 % Zuwachs des Medians gilt als Leck.
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
    echo "OK: DOM-Dauerlauf bestanden (Messreihen in $AUS/messung-gc.tsv und $AUS/messung-leak.tsv)."
else
    echo "FEHLER: DOM-Dauerlauf NICHT bestanden."
fi
exit $rc
