#!/usr/bin/env bash
# Nachweis, dass der Optimierer (compiler/src/opt.rs) wirklich wirkt.
#
# Fuer jedes Programm in tests/opt/ wird die FIR VOR (--emit=fir-raw) und NACH
# (--emit=fir-opt) der Optimierung erzeugt und geprueft:
#   (a) die Instruktionszahl sinkt,
#   (b) die erwartete gefaltete Konstante steht im Opt-Dump,
#   (c) ein Block, der im Raw-Dump noch da ist (erkennbar an einer eindeutigen
#       Marke), ist im Opt-Dump verschwunden und die Blockzahl sinkt.
# Zusaetzlich laufen die Modul-Tests aus opt.rs (cargo test opt::).
#
# Kein '|| true', keine geschluckten Exit-Codes: set -euo pipefail.
# Die Gleichheit der LAUFZEITergebnisse mit und ohne Optimierung prueft die
# grosse Testsuite (test.sh), die jedes Programm zweimal uebersetzt und ausfuehrt.
set -euo pipefail

cd "$(dirname "$0")"
ROOT=$(pwd)
# FIRNC_BIN kann auf einen anderen Compiler zeigen (Selbstpruefung des Skripts).
FIRNC="${FIRNC_BIN:-$ROOT/compiler/target/release/firnc}"
WORK="$ROOT/.opt-work"

echo "== baue Compiler =="
cargo build --release --manifest-path compiler/Cargo.toml

echo "== Modul-Tests des Optimierers =="
cargo test --manifest-path compiler/Cargo.toml opt:: -- --nocapture

rm -rf "$WORK"
mkdir -p "$WORK"

PASS=0
FAIL=0
FAILED=""

count_insts() {
    awk '/^  /{ if ($0 !~ /^  (br |brcond |ret)/) n++ } END { print n+0 }' "$1"
}
count_blocks() {
    awk '/^bb[0-9]+:/{ n++ } END { print n+0 }' "$1"
}

ok() {
    PASS=$((PASS + 1))
    echo "  ok   $1"
}
bad() {
    FAIL=$((FAIL + 1))
    FAILED="$FAILED\n  $1"
    echo "  FAIL $1"
}

# Faelle: datei | erwartete gefaltete Konstante | Marke des toten Blocks ('-' = keine)
CASES="
tests/opt/fold_arith.fi|const.i32 42|-
tests/opt/fold_cast_shift.fi|const.i32 100|-
tests/opt/fold_bits.fi|const.i32 90|-
tests/opt/dead_branch.fi|const.i32 42|777
tests/opt/dead_loop.fi|const.i32 3|999
tests/opt/dead_after_if.fi|const.i32 8|555
"

echo "== Vorher/Nachher-Vergleich =="
while IFS='|' read -r FILE WANT MARK; do
    [ -n "$FILE" ] || continue
    base=$(basename "$FILE" .fi)
    raw="$WORK/$base.raw.fir"
    opt="$WORK/$base.opt.fir"
    echo "-- $FILE"
    if ! "$FIRNC" --emit=fir-raw "$FILE" > "$raw"; then
        bad "$FILE: --emit=fir-raw schlug fehl"
        continue
    fi
    if ! "$FIRNC" --emit=fir-opt "$FILE" > "$opt"; then
        bad "$FILE: --emit=fir-opt schlug fehl"
        continue
    fi
    n_raw=$(count_insts "$raw")
    n_opt=$(count_insts "$opt")
    if [ "$n_opt" -lt "$n_raw" ]; then
        ok "$FILE: Instruktionen $n_raw -> $n_opt"
    else
        bad "$FILE: Instruktionszahl sinkt nicht ($n_raw -> $n_opt)"
    fi

    if grep -qF "$WANT" "$opt"; then
        ok "$FILE: gefaltete Konstante '$WANT' im Opt-Dump"
    else
        bad "$FILE: '$WANT' fehlt im Opt-Dump"
    fi

    if [ "$MARK" != "-" ]; then
        b_raw=$(count_blocks "$raw")
        b_opt=$(count_blocks "$opt")
        if grep -qF "$MARK" "$raw" && ! grep -qF "$MARK" "$opt" && [ "$b_opt" -lt "$b_raw" ]; then
            ok "$FILE: toter Block (Marke $MARK) entfernt, Bloecke $b_raw -> $b_opt"
        else
            bad "$FILE: toter Block mit Marke $MARK nicht entfernt (Bloecke $b_raw -> $b_opt)"
        fi
        # Terminatoren duerfen nur auf existierende Bloecke zeigen
        maxbb=$((b_opt - 1))
        if awk -v max="$maxbb" '
            /^  br bb/      { if (substr($2,3)+0 > max) bad=1 }
            /^  brcond /    { n=split($0, p, "bb"); for (k=2; k<=n; k++) if (p[k]+0 > max) bad=1 }
            END { exit bad?1:0 }' "$opt"; then
            ok "$FILE: Block-Ids in allen Terminatoren konsistent"
        else
            bad "$FILE: Terminator zeigt auf einen entfernten Block"
        fi
    fi
done <<< "$CASES"

TOTAL=$((PASS + FAIL))
echo
if [ "$FAIL" -eq 0 ]; then
    echo "PASS $PASS/$TOTAL (Optimierer-Nachweis)"
    exit 0
else
    echo "FAIL $FAIL/$TOTAL fehlgeschlagen:"
    printf "%b\n" "$FAILED"
    exit 1
fi
