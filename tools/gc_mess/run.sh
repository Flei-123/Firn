#!/usr/bin/env bash
# tools/gc_mess/run.sh — GC-Messwerkzeuge der Runde 38:
#   1. Pausen-Histogramm  (pause.fi, DOM-Workload, alle Pausen in Klassen)
#   2. Fragmentierung     (frag.fi, wechselnde Objektgroessen unter Dauerlast)
#
# Beide werden in allen drei Baustufen gebaut; ein Kurzlauf vergleicht die
# Zaehler (Sammellaeufe/lebende Objekte muessen uebereinstimmen — sonst ist
# die Messung baustufenabhaengig und damit wertlos). Der eigentliche Messlauf
# nutzt die Release-Baustufe.
#
# Umgebung:
#   GCM_PAUSE_SEK   Laufzeitbudget des Pausenlaufs (Standard 20)
#   GCM_RUNDEN      Runden des Fragmentierungstests (Standard 600)
#   GCM_BATCH       Objekte je Runde und Klasse (Standard 200)
set -euo pipefail
cd "$(dirname "$0")/../.."

FIRNC=compiler/target/release/firnc
ARBEIT=.gc-mess-work
AUS=tools/gc_mess
PAUSE_SEK=${GCM_PAUSE_SEK:-20}
RUNDEN=${GCM_RUNDEN:-600}
BATCH=${GCM_BATCH:-200}

if [ ! -x "$FIRNC" ]; then
    echo "FEHLER: $FIRNC fehlt — zuerst 'cargo build --release' im Ordner compiler/."
    exit 1
fi

rm -rf "$ARBEIT"
mkdir -p "$ARBEIT"
cp lib/dom/dom.fi lib/dom/mess.fi "$ARBEIT/"
cp tools/gc_mess/pause.fi tools/gc_mess/frag.fi "$ARBEIT/"

echo "== GC-Messung (Runde 38) =="

# ---------------------------------------------------------- 1. Baustufen-Probe
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
            # Kurzlauf: 2 s
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
            # Zeitbudget: Zyklen/Laeufe haengen von der Baustufe ab — kein Vergleich.
            wert="ok"
        else
            sed -e "s|^const RUNDEN: u64 = .*$|const RUNDEN: u64 = 120  // GCM_RUNDEN|" \
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
            echo "   FEHLER: $prog/$name liefert '$wert', erwartet '$erwartet'"
            fehler=1
        fi
    done
done
if [ $fehler -ne 0 ]; then
    echo "ABBRUCH: Baustufen liefern unterschiedliche Ergebnisse."
    exit 1
fi

# ------------------------------------------------------------ 2. Pausenlauf
echo
echo "-- 2. Pausen-Histogramm (${PAUSE_SEK}s) --"
sed -e "s|^const BUDGET_MS: i64 = .*$|const BUDGET_MS: i64 = $((PAUSE_SEK * 1000))  // GCM_BUDGET_MS|" \
    "$ARBEIT/pause.fi" > "$ARBEIT/pause_lauf.fi"
"$FIRNC" "$ARBEIT/pause_lauf.fi" -o "$ARBEIT/pause_lauf" 2>"$ARBEIT/bau2.err" || {
    echo "FEHLER: Bau des Pausenlaufs"; head -5 "$ARBEIT/bau2.err"; exit 1; }
"$ARBEIT/pause_lauf" > "$AUS/pause.tsv"
grep '^#' "$AUS/pause.tsv"

# ---------------------------------------------------- 3. Fragmentierungslauf
echo
echo "-- 3. Fragmentierungstest ($RUNDEN Runden x $BATCH Objekte) --"
sed -e "s|^const RUNDEN: u64 = .*$|const RUNDEN: u64 = $RUNDEN  // GCM_RUNDEN|" \
    -e "s|^const BATCH: u32 = .*$|const BATCH: u32 = $BATCH   // GCM_BATCH|" \
    "$ARBEIT/frag.fi" > "$ARBEIT/frag_lauf.fi"
"$FIRNC" "$ARBEIT/frag_lauf.fi" -o "$ARBEIT/frag_lauf" 2>"$ARBEIT/bau3.err" || {
    echo "FEHLER: Bau des Fragmentierungstests"; head -5 "$ARBEIT/bau3.err"; exit 1; }
"$ARBEIT/frag_lauf" > "$AUS/frag.tsv"
grep '^#' "$AUS/frag.tsv"

# ------------------------------------------------------------ 4. Auswertung
echo
echo "-- 4. Auswertung --"
python3 - "$AUS/pause.tsv" "$AUS/frag.tsv" <<'PYEOF'
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
# Histogramm: die letzten 9 Datenzeilen sind (grenze, anzahl)
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

fk, fz = lies(sys.argv[2])
print('   Fragmentierung:')
rss = [z[1] for z in fz if len(z) >= 2]
n = len(rss)
# Fragmentierung = WACHSTUM MIT DEN RUNDEN. Ein einmaliger Hochlauf
# (neue Groessenklasse wird erstmalig angelegt) ist kein Wachstum.
# Gemessen wird daher die Drift innerhalb des letzten Drittels:
# Median der ersten Haelfte des Drittels vs. Median der zweiten Haelfte.
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
