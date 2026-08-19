#!/usr/bin/env bash
# tools/gc_meas/run.sh -- GC measuring tools of round 38:
#   1. pause histogram   (pause.fi, DOM workload, all pauses in classes)
#   2. fragmentation     (frag.fi, changing object sizes under continuous load)
#   3. pauses with a LARGE live set (pause_big.fi, round 40): only
#      this run keeps enough alive for the heap to rise above INKR_AB (8 MiB)
#      -- only there does the incremental cycle run at all. Run 2
#      measures the NON-incremental path.
#
# Both are built in all three build stages; a short run compares the
# counters (collections/live objects have to match -- otherwise
# the measurement depends on the build stage and is worthless). The actual measuring run
# uses the release build stage.
#
# Environment:
#   GCM_PAUSE_SEK   time budget of the pause run (default 20)
#   GCM_ROUNDS      rounds of the fragmentation test (default 600)
#   GCM_BATCH       objects per round and class (default 200)
#   GCM_GROSS_SEK   time budget of the large run (default 20)
#   GCM_CHILDREN    live text nodes in the large run (default 120000, ~10 MiB)
set -euo pipefail
cd "$(dirname "$0")/../.."

FIRNC=compiler/target/release/firnc
ARBEIT=.gc-meas-work
AUS=tools/gc_meas
PAUSE_SEK=${GCM_PAUSE_SEK:-20}
RUNDEN=${GCM_ROUNDS:-600}
BATCH=${GCM_BATCH:-200}
GROSS_SEK=${GCM_GROSS_SEK:-20}
KINDER=${GCM_CHILDREN:-120000}

if [ ! -x "$FIRNC" ]; then
    echo "FEHLER: $FIRNC fehlt — zuerst 'cargo build --release' im Ordner compiler/."
    exit 1
fi

rm -rf "$ARBEIT"
mkdir -p "$ARBEIT"
cp lib/dom/dom.fi lib/dom/meas.fi "$ARBEIT/"
cp tools/gc_meas/pause.fi tools/gc_meas/frag.fi tools/gc_meas/frag2.fi "$ARBEIT/"
cp tools/gc_meas/pause_big.fi "$ARBEIT/"

echo "== GC-Messung (Runde 38) =="

# ---------------------------------------------------------- 1. build stage probe
echo
echo "-- 1. Kurzlauf in drei Baustufen (Vergleich der Zaehler) --"
STUFEN=("release-fast:" "no-opt:--no-opt" "dev-fast:--opt-level=dev-fast")
fehler=0
for prog in pause frag; do
    erwartet=""
    for st in "${STUFEN[@]}"; do
        name=${st%%:*}
        opt=${st#*:}
        if ! "$FIRNC" "$ARBEIT/$prog.fi" -o "$ARBEIT/kurz_${prog}_$name" $opt 2>"$ARBEIT/bau.err"; then
            echo "   FEHLER: Bau $prog/$name:"
            head -5 "$ARBEIT/bau.err"
            fehler=1
            continue
        fi
        if [ "$prog" = pause ]; then
            # Short run: 2 s
            sed "s|^const BUDGET_MS: i64 = .*$|const BUDGET_MS: i64 = 2000  // GCM_BUDGET_MS|" \
                "$ARBEIT/pause.fi" > "$ARBEIT/kurz_pause.fi"
            "$FIRNC" "$ARBEIT/kurz_pause.fi" -o "$ARBEIT/kurz_pause_$name" $opt 2>/dev/null
            out=$("$ARBEIT/kurz_pause_$name")
            z=$(echo "$out" | grep '^# zyklen=' | cut -d= -f2)
            l=$(echo "$out" | grep '^# sammellaeufe=' | cut -d= -f2)
            m=$(echo "$out" | grep '^# uebersehen_mehrfach=' | cut -d= -f2)
            printf '   pause %-12s zyklen=%s laeufe=%s mehrfach=%s\n' "$name" "$z" "$l" "$m"
            if [ "$m" != "0" ]; then
                echo "   FEHLER: pause/$name hat Sammellaeufe uebersehen ($m) — Messung ungenau"
                fehler=1
            fi
            # Time budget: cycles/runs depend on the build stage -- no comparison.
            wert="ok"
        else
            sed -e "s|^const ROUNDS: u64 = .*$|const ROUNDS: u64 = 120  // GCM_ROUNDS|" \
                -e "s|^const BATCH: u32 = .*$|const BATCH: u32 = 50   // GCM_BATCH|" \
                "$ARBEIT/frag.fi" > "$ARBEIT/kurz_frag.fi"
            "$FIRNC" "$ARBEIT/kurz_frag.fi" -o "$ARBEIT/kurz_frag_$name" $opt 2>/dev/null
            out=$("$ARBEIT/kurz_frag_$name")
            l=$(echo "$out" | grep '^# lebende=' | cut -d= -f2)
            printf '   frag  %-12s lebende=%s\n' "$name" "$l"
            wert="$l"
        fi
        if [ -z "$erwartet" ]; then
            erwartet="$wert"
        elif [ "$wert" != "$erwartet" ]; then
            # Conservative scan: in slower build stages more pointers stick
            # in unscrubbed frames -- MORE live objects is allowed
            # (retention), FEWER would be a real collector bug.
            if [ "$wert" -lt "$erwartet" ]; then
                echo "   FEHLER: $prog/$name liefert '$wert' < '$erwartet' — lebende Objekte eingesammelt!"
                fehler=1
            else
                echo "   HINWEIS: $prog/$name liefert '$wert' > '$erwartet' (konservative Retention, zulaessig)"
            fi
        fi
    done
done
if [ $fehler -ne 0 ]; then
    echo "ABBRUCH: Baustufen liefern unterschiedliche Ergebnisse."
    exit 1
fi

# ------------------------------------------------------------ 2. pause run
echo
echo "-- 2. Pausen-Histogramm (${PAUSE_SEK}s) --"
sed -e "s|^const BUDGET_MS: i64 = .*$|const BUDGET_MS: i64 = $((PAUSE_SEK * 1000))  // GCM_BUDGET_MS|" \
    "$ARBEIT/pause.fi" > "$ARBEIT/pause_lauf.fi"
"$FIRNC" "$ARBEIT/pause_lauf.fi" -o "$ARBEIT/pause_lauf" 2>"$ARBEIT/bau2.err" || {
    echo "FEHLER: Bau des Pausenlaufs"; head -5 "$ARBEIT/bau2.err"; exit 1; }
"$ARBEIT/pause_lauf" > "$AUS/pause.tsv"
grep '^#' "$AUS/pause.tsv"

# ------------------------------------ 2b. pauses with a large live set
echo
echo "-- 2b. Pausen bei grosser lebender Menge (${GROSS_SEK}s, $KINDER lebende Knoten) --"
sed -e "s|^const BUDGET_MS: i64 = .*$|const BUDGET_MS: i64 = $((GROSS_SEK * 1000))  // GCM_BUDGET_MS|" \
    -e "s|^const CHILDREN: u32 = .*$|const CHILDREN: u32 = $KINDER    // GCM_CHILDREN|" \
    "$ARBEIT/pause_big.fi" > "$ARBEIT/gross_lauf.fi"
"$FIRNC" "$ARBEIT/gross_lauf.fi" -o "$ARBEIT/gross_lauf" 2>"$ARBEIT/bau2b.err" || {
    echo "FEHLER: Bau des Grosslaufs"; head -5 "$ARBEIT/bau2b.err"; exit 1; }
"$ARBEIT/gross_lauf" > "$AUS/pause_gross.tsv"
grep '^#' "$AUS/pause_gross.tsv"

# ---------------------------------------------------- 3. fragmentation run
echo
echo "-- 3. Fragmentierungstest ($RUNDEN Runden x $BATCH Objekte) --"
sed -e "s|^const ROUNDS: u64 = .*$|const ROUNDS: u64 = $RUNDEN  // GCM_ROUNDS|" \
    -e "s|^const BATCH: u32 = .*$|const BATCH: u32 = $BATCH   // GCM_BATCH|" \
    "$ARBEIT/frag.fi" > "$ARBEIT/frag_lauf.fi"
"$FIRNC" "$ARBEIT/frag_lauf.fi" -o "$ARBEIT/frag_lauf" 2>"$ARBEIT/bau3.err" || {
    echo "FEHLER: Bau des Fragmentierungstests"; head -5 "$ARBEIT/bau3.err"; exit 1; }
"$ARBEIT/frag_lauf" > "$AUS/frag.tsv"
grep '^#' "$AUS/frag.tsv"

# ------------------------------------------------ 3b. phase fragmentation
echo
echo "-- 3b. Phasen-Fragmentierung (gross -> klein) --"
"$FIRNC" "$ARBEIT/frag2.fi" -o "$ARBEIT/frag2_lauf" 2>"$ARBEIT/bau4.err" || {
    echo "FEHLER: Bau des Phasen-Tests"; head -5 "$ARBEIT/bau4.err"; exit 1; }
"$ARBEIT/frag2_lauf" > "$AUS/frag2.tsv"
grep '^#' "$AUS/frag2.tsv"
python3 - "$AUS/frag2.tsv" <<'PYEOF2'
import sys
a_max = b_max = ende = None
for z in open(sys.argv[1]):
    z = z.strip()
    if z.startswith('# rss_phase_a_max_kib='):
        a_max = int(z.split('=')[1])
    if z.startswith('# rss_phase_b_max_kib='):
        b_max = int(z.split('=')[1])
    if z.startswith('# rss_ende_kib='):
        ende = int(z.split('=')[1])
print(f'   Phase A max {a_max} KiB, Phase B max {b_max} KiB, Ende {ende} KiB')
if ende is not None and a_max and ende > a_max * 0.5:
    print('   HINWEIS: RSS-Ende ueber 50 % des Phase-A-Maximums — Rueckgabe pruefen')
PYEOF2

# ------------------------------------------------------------ 4. evaluation
echo
echo "-- 4. Auswertung --"
python3 - "$AUS/pause.tsv" "$AUS/frag.tsv" "$AUS/pause_gross.tsv" <<'PYEOF'
import sys

def lies(pfad):
    kopf, zeilen = {}, []
    with open(pfad, encoding='utf-8') as f:
        for z in f:
            z = z.strip()
            if not z:
                continue
            if z.startswith('#'):
                if '=' in z:
                    k, v = z[1:].strip().split('=', 1)
                    kopf[k.strip()] = v.strip()
                continue
            if not z[0].isdigit():
                continue
            feld = z.split('\t')
            try:
                zeilen.append([int(x) for x in feld if x != ''])
            except ValueError:
                continue
    return kopf, zeilen

pk, pz = lies(sys.argv[1])
print('   Pausen:')
print(f'     Zyklen {pk.get("zyklen","?")}, Sammellaeufe {pk.get("sammellaeufe","?")}, '
      f'RSS {pk.get("rss_kib","?")} KiB')
pmax = int(pk.get('pause_max_ns', 0))
ptot = int(pk.get('pause_total_ns', 0))
print(f'     laengste Pause {pmax} ns ({pmax/1e6:.2f} ms), Summe {ptot/1e6:.1f} ms')
# Histogram: the last 9 data lines are (limit, count)
hist = pz[-9:]
gesamt = sum(a for _, a in hist)
if gesamt:
    kum = 0
    for grenze, anzahl in hist:
        kum += anzahl
        if anzahl:
            print(f'     <= {grenze:>9} ns : {anzahl:>6}  (kumuliert {kum*100.0/gesamt:5.1f} %)')
    print(f'     >=16000000 ns : {hist[-1][1]:>6}')
else:
    print('     (keine Sammellaeufe beobachtet)')

gk, gz = lies(sys.argv[3]) if len(sys.argv) > 3 else ({}, [])
if gk:
    print('   Pausen bei grosser lebender Menge:')
    print(f'     lebende Knoten {gk.get("lebende_knoten","?")}, Heap {int(gk.get("heap_bytes",0))/1048576:.1f} MiB, '
          f'Sammellaeufe {gk.get("sammellaeufe","?")}')
    gmax = int(gk.get('pause_max_ns', 0))
    print(f'     laengste Pause {gmax} ns ({gmax/1e6:.2f} ms), Summe {int(gk.get("pause_total_ns",0))/1e6:.1f} ms')
    ghist = gz[-9:]
    ggesamt = sum(a for _, a in ghist)
    if ggesamt:
        kum = 0
        for grenze, anzahl in ghist:
            kum += anzahl
            if anzahl:
                print(f'     <= {grenze:>9} ns : {anzahl:>6}  (kumuliert {kum*100.0/ggesamt:5.1f} %)')
    # IMPORTANT: pause_max_ns is the maximum SINCE THE START OF THE PROCESS and
    # therefore contains the collections of the BUILD-UP (the heap grows, no
    # incremental cycle yet). The classes above only count the runs of the
    # measuring loop -- a deviation is to be expected and is no contradiction.

fk, fz = lies(sys.argv[2])
print('   Fragmentierung:')
rss = [z[1] for z in fz if len(z) >= 2]
n = len(rss)
# Fragmentation = GROWTH WITH THE ROUNDS. A one-time rise
# (a new size class is created for the first time) is no growth.
# What is measured is therefore the drift within the last third:
# the median of the first half of the third vs. the median of the second half.
d3 = rss[2*n//3:] or rss
h1 = d3[:len(d3)//2] or d3[:1]
h2 = d3[len(d3)//2:] or d3[-1:]
m1 = sorted(h1)[len(h1)//2]
m2 = sorted(h2)[len(h2)//2]
drift_prozent = (m2 - m1) * 100.0 / m1 if m1 else 0.0
print(f'     RSS Start {fk.get("rss_start_kib","?")} KiB, Maximum {fk.get("rss_max_kib","?")} KiB, '
      f'Ende {fk.get("rss_ende_kib","?")} KiB')
print(f'     letztes Drittel: Median 1. Haelfte {m1} KiB, Median 2. Haelfte {m2} KiB, '
      f'Drift {drift_prozent:+.1f} %')
wuchs = drift_prozent > 5.0
print(f'     Urteil: {"WAECHST (Fragmentierung)" if wuchs else "stabil (kein Wachstum ueber die Runden)"}')
sys.exit(0)
PYEOF

echo
echo "OK: Messung abgeschlossen ($AUS/pause.tsv, $AUS/frag.tsv)."
