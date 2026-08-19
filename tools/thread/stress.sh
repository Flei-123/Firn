#!/usr/bin/env bash
# tools/thread/stress.sh -- soak run with several threads and a running collector.
#
# What is measured is the REAL memory consumption of the process (RSS from
# /proc/self/statm), not the self-report of the runtime. What is judged:
#
#   * no crash and no deadlock over the whole run time
#   * error word 0 -- NO thread found a chain from which the
#     collector had removed a link
#   * the mutex counter and the atomic counter match the number of rounds EXACTLY
#   * the counter WITHOUT a lock has lost (otherwise the threads did not
#     really run at the same time and the run proves nothing)
#   * the RSS does not drift: the last sample must not exceed the smallest one
#     by more than STRESS_DRIFT_KIB
#
# Environment:
#   STRESS_SEK     run time in seconds (default 130)
#   STRESS_THREADS number of threads (default 4)
#   STRESS_LOCAL   1 = free lists per thread (variant B), 0 = GC lock (A)
#   STRESS_DRIFT_KIB  allowed RSS increase (default 1024)
set -uo pipefail
cd "$(dirname "$0")/../.."

FIRNC=compiler/target/release/firnc
SEK=${STRESS_SEK:-130}
FAEDEN=${STRESS_THREADS:-4}
LOKAL=${STRESS_LOCAL:-0}
DRIFT=${STRESS_DRIFT_KIB:-1024}
ARBEIT=$(mktemp -d /tmp/firn-faden-stress.XXXXXX)
trap 'rm -rf "$ARBEIT"' EXIT

if [ ! -x "$FIRNC" ]; then
    echo "FEHLER: $FIRNC fehlt"
    exit 1
fi

cp lib/dom/meas.fi "$ARBEIT/"
sed -e "s|^const BUDGET_MS: i64 = .*$|const BUDGET_MS: i64 = $((SEK * 1000))  // STRESS_BUDGET_MS|" \
    -e "s|^const THREADS: u64 = .*$|const THREADS: u64 = $FAEDEN  // STRESS_THREADS|" \
    -e "s|^const LOCAL: u64 = .*$|const LOCAL: u64 = $LOKAL  // STRESS_LOCAL|" \
    tools/thread/stress.fi > "$ARBEIT/stress.fi"

export FIRNLIB="$(pwd)/lib"
if ! "$FIRNC" -o "$ARBEIT/stress" "$ARBEIT/stress.fi" 2>"$ARBEIT/err"; then
    echo "FEHLER: Bau fehlgeschlagen"
    head -10 "$ARBEIT/err"
    exit 1
fi

echo "== Dauerlauf: $FAEDEN Faeden, ${SEK}s, LOKAL=$LOKAL =="
set +e
timeout $((SEK + 120)) "$ARBEIT/stress" > "$ARBEIT/aus.tsv" 2>"$ARBEIT/err2"
rc=$?
set -e
if [ "$rc" -ne 0 ]; then
    echo "FEHLGESCHLAGEN: Rueckgabe $rc"
    echo "  1 = ein Faden fand eine zerstoerte Kette (der Sammler hat Lebendes eingesammelt)"
    echo "  2/3/4 = ein Zaehler stimmt nicht · 5 = ein Faden blieb uebrig · 124 = Zeitueberschreitung"
    tail -5 "$ARBEIT/aus.tsv"
    head -5 "$ARBEIT/err2"
    exit 1
fi

sed 's/^/  /' "$ARBEIT/aus.tsv"

hole() { awk -v c="$1" '$1=="Q" && $2==c {print $3}' "$ARBEIT/aus.tsv"; }
fehler=$(hole 0)
runden=$(hole 1)
mit=$(hole 2)
atom=$(hole 3)
ohne=$(hole 4)
rss_ende=$(hole 5)
laeufe=$(hole 6)
stw=$(hole 7)

rss_min=$(awk '$1=="S" {print $3}' "$ARBEIT/aus.tsv" | sort -n | head -1)
rss_max=$(awk '$1=="S" {print $3}' "$ARBEIT/aus.tsv" | sort -n | tail -1)
stichproben=$(grep -c '^S' "$ARBEIT/aus.tsv" || true)

echo
echo "  Runden           $runden"
echo "  Sammellaeufe     $laeufe"
echo "  Stop-the-World   $stw"
echo "  RSS min/max/ende $rss_min / $rss_max / $rss_ende KiB ($stichproben Stichproben)"

fehlt=0
[ "$fehler" = "0" ] || { echo "FEHLER: Fehlerwort $fehler — der Sammler hat Lebendes eingesammelt"; fehlt=1; }
[ "$mit" = "$runden" ] || { echo "FEHLER: Mutexzaehler $mit != Runden $runden"; fehlt=1; }
[ "$atom" = "$runden" ] || { echo "FEHLER: Atomzaehler $atom != Runden $runden"; fehlt=1; }
if [ "$ohne" -ge "$runden" ]; then
    echo "FEHLER: der Zaehler OHNE Sperre hat nichts verloren ($ohne von $runden) — die Faeden liefen nicht gleichzeitig"
    fehlt=1
fi
[ "$laeufe" -gt 0 ] || { echo "FEHLER: kein einziger Sammellauf"; fehlt=1; }
[ "$stw" -gt 0 ] || { echo "FEHLER: die Welt wurde nie angehalten"; fehlt=1; }
[ "$stichproben" -ge 3 ] || { echo "FEHLER: zu wenige Stichproben"; fehlt=1; }
if [ $((rss_ende - rss_min)) -gt "$DRIFT" ]; then
    echo "FEHLER: RSS-Drift $((rss_ende - rss_min)) KiB > $DRIFT KiB"
    fehlt=1
fi

if [ "$fehlt" -ne 0 ]; then
    echo "STRESS: FEHLGESCHLAGEN"
    exit 1
fi
echo "STRESS: bestanden — $runden Runden, $laeufe Sammellaeufe, $stw Anhalter, RSS-Drift $((rss_ende - rss_min)) KiB"
exit 0
