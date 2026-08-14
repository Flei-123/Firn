#!/usr/bin/env bash
# Durchsatz des Firn-Tokenizers auf ZWEI Eingabekorpora, danach — wenn
# vorhanden — html5ever (cargo --release) auf DENSELBEN Korpora.
#
#   Korpus A "html5lib": die Eingaben der html5lib-Faelle, vielfach
#       aneinandergehaengt. ABSICHTLICH PATHOLOGISCH (fast nur Grenzfaelle,
#       sehr viele Zustandswechsel je Byte, kaum lange Textlaeufe) — ein Wert
#       fuer den schlechtesten Fall. Begruendung in tools/tokenizer/korpus.py.
#   Korpus B "realweb": acht gespeicherte echte Seiten aus testdata/realweb/
#       (Wikipedia, WHATWG-Standard, W3C, rustdoc, Hacker News), ~4,6 MB,
#       unveraendert wie ausgeliefert. Das ist der Alltagsfall.
#
# Beide Korpora erzeugt tools/tokenizer/korpus.py; beide Seiten (Firn und
# html5ever) bekommen exakt dieselben Bytes.
#
# Gemessen wird DREIMAL, ausgewiesen wird der beste Lauf je Seite (die
# Schwankung zwischen Laeufen liegt bei ~30 %). Der Faktor wird ausgerechnet
# und ausgegeben, auch wenn er das Abnahmeziel (<= 2x) verfehlt.
#
# FAIRER VERGLEICH (seit 14.08.2026): gemessen wird `tokenize_bench`, das nur
# Token ZAEHLT — genau wie html5ever. Der urspruengliche Treiber schrieb
# zusaetzlich html5lib-JSON; gemessen mit callgrind waren das **14,7 % aller
# Instruktionen** (`out_json_cp` 7,93 %, `out_json_cpbuf` 4,22 %, `out_wort`
# 2,55 %). Ein Faktor, der solche Arbeit einrechnet, misst nicht den Tokenizer.
# Steht `tokenize_bench` nicht bereit, faellt das Skript auf den JSON-Treiber
# zurueck und sagt das an.
#
# NICHT herausgerechnet wird die Dekodierung nach UTF-32 (`dekodiere`, 28 % der
# Instruktionen), obwohl html5ever sie nicht braucht: das ist ein echter
# Nachteil von Firns Aufbau und keine Unfairness des Messaufbaus.
#
# Aufruf:  bash tools/tokenizer/durchsatz.sh [tokenizer-binary] [laeufe]
set -euo pipefail
cd "$(dirname "$0")/../.."
BIN="${1:-}"
if [ -z "$BIN" ]; then
    if [ -x .tokenizer-work/tokenize_bench ]; then
        BIN=.tokenizer-work/tokenize_bench
    else
        BIN=.tokenizer-work/tokenize
        echo "   HINWEIS: tokenize_bench fehlt — gemessen wird der JSON-Treiber."
        echo "            Bauen mit: firnc -o .tokenizer-work/tokenize_bench lib/html/tokenize_bench.fi"
    fi
fi
LAEUFE="${2:-3}"
WORK=".tokenizer-work"
mkdir -p "$WORK"

# beste (kleinste) Zeit aus $LAEUFE Laeufen
beste_zeit() {
    local best=""
    local i a b t
    for ((i = 0; i < LAEUFE; i++)); do
        a=$(date +%s.%N)
        "$@" >/dev/null
        b=$(date +%s.%N)
        t=$(awk -v a="$a" -v b="$b" 'BEGIN{printf "%.6f", b-a}')
        best=$(awk -v x="$t" -v y="$best" 'BEGIN{if(y==""||x+0<y+0)print x;else print y}')
    done
    echo "$best"
}

messe_korpus() {
    local quelle="$1" beschreibung="$2"
    local html="$WORK/korpus.$quelle.html"
    local auftrag="$WORK/korpus.$quelle.auftrag"
    local aus="$WORK/korpus.$quelle.out"

    echo "   -- Korpus '$quelle' ($beschreibung)"
    if [ ! -f "$html" ] || [ ! -f "$auftrag" ]; then
        python3 tools/tokenizer/korpus.py "$html" "$auftrag" --quelle "$quelle"
    fi
    local groesse
    groesse=$(stat -c%s "$html")

    local tf
    tf=$(beste_zeit sh -c "\"$BIN\" < \"$auftrag\" > \"$aus\"")
    if grep -q 'NICHT-UNTERSTUETZT' "$aus"; then
        echo "      ACHTUNG: der Tokenizer hat den Korpus NICHT vollstaendig verarbeitet"
        echo "               (Zustand nicht umgesetzt) — die MB/s sind daher kein"
        echo "               vergleichbarer Wert und werden nur nachrichtlich gezeigt."
    fi
    awk -v t="$tf" -v n="$groesse" -v l="$LAEUFE" \
        'BEGIN{printf "      Firn      : %8.2f MB/s  (%.3f s fuer %.2f MB, bester von %d)\n", n/t/1048576, t, n/1048576, l}'

    if [ -x bench/tokenizer/target/release/html5ever_bench ]; then
        local tr
        tr=$(beste_zeit bench/tokenizer/target/release/html5ever_bench "$html")
        awk -v t="$tr" -v n="$groesse" -v l="$LAEUFE" \
            'BEGIN{printf "      html5ever : %8.2f MB/s  (%.3f s, bester von %d)\n", n/t/1048576, t, l}'
        awk -v a="$tf" -v b="$tr" \
            'BEGIN{printf "      Faktor    : %.2fx langsamer als html5ever (Abnahmeziel <= 2.00x)\n", a/b}'
    else
        echo "      html5ever : nicht gebaut — bauen mit"
        echo "                  cargo build --release --manifest-path bench/tokenizer/Cargo.toml"
    fi
}

messe_korpus html5lib "Grenzfaelle der Testsuite, absichtlich pathologisch"
echo
messe_korpus realweb  "acht echte Seiten aus testdata/realweb/"
