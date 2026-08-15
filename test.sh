#!/usr/bin/env bash
# Vollstaendige Testsuite fuer firnc0.
#
# Ablauf:
#   1. Compiler bauen (cargo build --release) — Warnungen werden gemeldet.
#   2. Modul-Tests des Compilers (cargo test --release).
#   3. Jedes Programm in tests/, tests/opt/ und examples/ wird ZWEIMAL
#      uebersetzt (--opt-level=release-fast, --no-opt und --opt-level=dev-fast),
#      assembliert, gelinkt,
#      AUSGEFUEHRT und Exit-Code bzw. Standardausgabe gegen die Erwartung
#      in Zeile 1 geprueft (// expect_exit: N  bzw.  // expect_out: TEXT).
#   4. Jedes Programm in tests/neg/ muss mit Exit-Code != 0 abbrechen und die
#      erwartete Meldung mit Zeile:Spalte ausgeben (// expect_error: Z:S TEXT).
#      Ein Rust-Panic gilt als Fehlschlag.
#   5. Nachweis des Optimierers (test_opt.sh: FIR vorher/nachher).
#   6. Nachweis der Ergebnisort-Garantie (tools/ergebnisort/run.sh:
#      Rahmengroessen im erzeugten Assembler).
#   7. Architekturpruefung: Feldzugriff ist vom Speicherort getrennt
#      (tools/schichten/run.sh, Vorbedingung fuer SoA).
#   8. Symbol-Namensschema: reservierter Praefix, Platz fuer die
#      ABI-Version, Module kollisionsfrei (tools/symbole/run.sh).
#   9. HTML5-Tokenizer (lib/html/, in Firn) gegen die offizielle
#      html5lib-Testsuite: exakte Quote aus 6.810 Faellen, Schranke in
#      tools/tokenizer/mindestquote.txt (tools/tokenizer/run.sh).
#  10. DOM-Dauerlauf (tools/dom_soak/run.sh): der DOM-Prototyp in Firn baut
#      fortlaufend echte Zyklen (Eltern/Kind, Listener, JS-Wrapper) und darf
#      dabei nicht wachsen; die absichtlich leckende Gegenprobe mit
#      Zaehlverweisen MUSS anschlagen, sonst gilt die Messung als kaputt.
#
# Kein '|| true', kein Verschlucken von Exit-Codes: set -euo pipefail.
set -euo pipefail

cd "$(dirname "$0")"
ROOT=$(pwd)
FIRNC="$ROOT/compiler/target/release/firnc"
WORK="$ROOT/.test-work"

PASS=0
FAIL=0
FAILED=""

ok()  { PASS=$((PASS + 1)); }
bad() { FAIL=$((FAIL + 1)); FAILED="$FAILED\n  $1"; echo "  FAIL  $1"; }

echo "== 1. Compiler bauen =="
cargo build --release --manifest-path compiler/Cargo.toml

echo "== 2. Modul-Tests des Compilers =="
cargo test --release --manifest-path compiler/Cargo.toml -- --test-threads=4 >/dev/null
echo "   cargo test: ok"

rm -rf "$WORK"
mkdir -p "$WORK"

run_case() {          # $1 = Datei, $2 = "opt" | "noopt" | "devfast"
    local file="$1" mode="$2"
    local base ext bin flags hdr exp out rc
    base=$(basename "$file" .fi)
    bin="$WORK/${base}.${mode}"
    flags=""
    [ "$mode" = "noopt" ]   && flags="--no-opt"
    [ "$mode" = "devfast" ] && flags="--opt-level=dev-fast"

    if ! "$FIRNC" $flags -o "$bin" "$file" >"$WORK/$base.$mode.cerr" 2>&1; then
        bad "$file [$mode]: uebersetzen fehlgeschlagen"
        sed 's/^/        /' "$WORK/$base.$mode.cerr" | head -8
        return
    fi
    hdr=$(head -1 "$file")
    case "$hdr" in
        *expect_out:*)
            exp=${hdr#*expect_out: }
            set +e
            out=$("$bin")
            rc=$?
            set -e
            if [ "$rc" -ne 0 ]; then
                bad "$file [$mode]: Programm endete mit Exit-Code $rc (erwartet 0)"
            elif [ "$out" = "$exp" ]; then
                ok
            else
                bad "$file [$mode]: Ausgabe '$out', erwartet '$exp'"
            fi
            ;;
        *expect_exit:*)
            exp=${hdr#*expect_exit: }
            set +e
            "$bin" >/dev/null
            rc=$?
            set -e
            if [ "$rc" = "$exp" ]; then
                ok
            else
                bad "$file [$mode]: Exit-Code $rc, erwartet $exp"
            fi
            ;;
        *)
            bad "$file: keine Erwartung in Zeile 1 (// expect_exit: / // expect_out:)"
            ;;
    esac
}

echo "== 3. Positivtests (jeweils mit und ohne Optimierer) =="
PROGS=$(ls tests/*.fi tests/opt/*.fi examples/*.fi)
NPROG=0
for f in $PROGS; do
    NPROG=$((NPROG + 1))
    printf '  %-40s' "$f"
    run_case "$f" opt
    run_case "$f" noopt
    run_case "$f" devfast
    echo "  [opt+noopt+devfast]"
done
echo "   $NPROG Programme x 3 Durchlaeufe (opt / noopt / dev-fast)"

echo "== 4. Negativtests (Fehlermeldungen) =="
for f in tests/neg/*.fi; do
    hdr=$(head -1 "$f")
    exp=${hdr#*expect_error: }
    pos=${exp%% *}
    msg=${exp#* }
    set +e
    "$FIRNC" -o "$WORK/neg.bin" "$f" >"$WORK/neg.out" 2>&1
    rc=$?
    set -e
    printf '  %-40s' "$f"
    if [ "$rc" -eq 0 ]; then
        echo
        bad "$f: Compiler meldete KEINEN Fehler (Exit 0)"
        continue
    fi
    if grep -qE "panicked at|RUST_BACKTRACE" "$WORK/neg.out"; then
        echo
        bad "$f: Rust-Panic statt sauberer Fehlermeldung"
        continue
    fi
    if ! grep -qF ":$pos" "$WORK/neg.out"; then
        echo
        bad "$f: Position '$pos' fehlt in der Meldung"
        sed 's/^/        /' "$WORK/neg.out" | head -6
        continue
    fi
    if ! grep -qF "$msg" "$WORK/neg.out"; then
        echo
        bad "$f: Text '$msg' fehlt in der Meldung"
        sed 's/^/        /' "$WORK/neg.out" | head -6
        continue
    fi
    # Quelltextzeile und Markierung muessen dabei sein
    if ! grep -q '\^' "$WORK/neg.out"; then
        echo
        bad "$f: keine Markierung (^) in der Meldung"
        continue
    fi
    cnt_hdr=$(sed -n '2p' "$f")
    case "$cnt_hdr" in
        *expect_error_count:*)
            want=${cnt_hdr#*expect_error_count: }
            got=$(grep -c '^error:' "$WORK/neg.out")
            if [ "$got" -ne "$want" ]; then
                echo
                bad "$f: $got Fehler gemeldet, erwartet $want"
                continue
            fi
            ;;
    esac
    ok
    echo "  [Fehler wie erwartet]"
done

echo "== 5. Nachweis des Optimierers =="
bash test_opt.sh > "$WORK/opt.log" 2>&1 && OPTRC=0 || OPTRC=$?
if [ "$OPTRC" -eq 0 ]; then
    ok
    tail -1 "$WORK/opt.log" | sed 's/^/   /'
else
    bad "test_opt.sh schlug fehl (siehe .test-work/opt.log)"
    tail -20 "$WORK/opt.log" | sed 's/^/   /'
fi

echo "== 6. Nachweis der Ergebnisort-Garantie (SPEC.md 13.1) =="
bash tools/ergebnisort/run.sh > "$WORK/ergebnisort.log" 2>&1 && EORC=0 || EORC=$?
if [ "$EORC" -eq 0 ]; then
    ok
    tail -1 "$WORK/ergebnisort.log" | sed 's/^/   /'
else
    bad "tools/ergebnisort/run.sh schlug fehl (siehe .test-work/ergebnisort.log)"
    tail -20 "$WORK/ergebnisort.log" | sed 's/^/   /'
fi

echo "== 7. Architektur: Feldzugriff <-> Speicherort getrennt =="
bash tools/schichten/run.sh > "$WORK/schichten.log" 2>&1 && SCRC=0 || SCRC=$?
if [ "$SCRC" -eq 0 ]; then
    ok
    tail -1 "$WORK/schichten.log" | sed 's/^/   /'
else
    bad "tools/schichten/run.sh schlug fehl (siehe .test-work/schichten.log)"
    tail -20 "$WORK/schichten.log" | sed 's/^/   /'
fi

echo "== 8. Symbol-Namensschema (DESIGNZIELE 4) =="
bash tools/symbole/run.sh > "$WORK/symbole.log" 2>&1 && SYRC=0 || SYRC=$?
if [ "$SYRC" -eq 0 ]; then
    ok
    tail -1 "$WORK/symbole.log" | sed 's/^/   /'
else
    bad "tools/symbole/run.sh schlug fehl (siehe .test-work/symbole.log)"
    tail -20 "$WORK/symbole.log" | sed 's/^/   /'
fi

echo "== 9. HTML5-Tokenizer gegen html5lib (tools/tokenizer/run.sh) =="
bash tools/tokenizer/run.sh --schnell > "$WORK/tokenizer.log" 2>&1 && TKRC=0 || TKRC=$?
if [ "$TKRC" -eq 0 ]; then
    ok
    grep -E '^GESAMT' "$WORK/tokenizer.log" | sed 's/^/   /'
else
    bad "tools/tokenizer/run.sh schlug fehl (siehe .test-work/tokenizer.log)"
    tail -20 "$WORK/tokenizer.log" | sed 's/^/   /'
fi

echo "== 10. DOM-Dauerlauf: Zyklen ohne Leck (tools/dom_soak/run.sh) =="
# Kurzfassung: 12 s je Variante. Der lange Lauf steht in ABNAHME.md Punkt 2;
# hier geht es darum, dass die Zusage bei JEDER Aenderung nachgeprueft wird.
SOAK_SEK=${SOAK_SEK:-12} SOAK_ZYKLEN=${SOAK_ZYKLEN:-400000} \
  SOAK_STICHPROBE=${SOAK_STICHPROBE:-10000} SOAK_MIN_ZYKLEN=${SOAK_MIN_ZYKLEN:-100000} \
  bash tools/dom_soak/run.sh > "$WORK/dom_soak.log" 2>&1 && DSRC=0 || DSRC=$?
if [ "$DSRC" -eq 0 ]; then
    ok
    grep -E 'BESTANDEN|Gegenprobe schlaegt an' "$WORK/dom_soak.log" | sed 's/^/   /'
else
    bad "tools/dom_soak/run.sh schlug fehl (siehe .test-work/dom_soak.log)"
    tail -20 "$WORK/dom_soak.log" | sed 's/^/   /'
fi

echo "== 11. Lexer in Firn gegen Lexer in Rust (tools/lex_vergleich.sh) =="
# Der erste Teil von Stufe 1: `lib/firnc1/lexer.fi` erzeugt denselben
# Tokenstrom wie `firnc0 --emit=tokens`, ueber das ganze Quellkorpus.
bash tools/lex_vergleich.sh > "$WORK/lex_vergleich.log" 2>&1 && LXRC=0 || LXRC=$?
if [ "$LXRC" -eq 0 ]; then
    ok
    grep -E '^(GLEICH|UNGLEICH|TOKEN|GLEITKOMMA)' "$WORK/lex_vergleich.log" | sed 's/^/   /'
else
    bad "tools/lex_vergleich.sh schlug fehl (siehe .test-work/lex_vergleich.log)"
    tail -20 "$WORK/lex_vergleich.log" | sed 's/^/   /'
fi

echo "== 12. Parser in Firn gegen Parser in Rust (tools/parser_vergleich.sh) =="
bash tools/parser_vergleich.sh > "$WORK/parser_vergleich.log" 2>&1 && PVRC=0 || PVRC=$?
if [ "$PVRC" -eq 0 ]; then
    ok
    grep -E '^(GLEICH|UNGLEICH|NICHT KERN)' "$WORK/parser_vergleich.log" | sed 's/^/   /'
else
    bad "tools/parser_vergleich.sh schlug fehl (siehe .test-work/parser_vergleich.log)"
    tail -20 "$WORK/parser_vergleich.log" | sed 's/^/   /'
fi

echo "== 13. Layout und ABI in Firn gegen Rust (tools/typen_vergleich.sh) =="
bash tools/typen_vergleich.sh > "$WORK/typen_vergleich.log" 2>&1 && TVRC=0 || TVRC=$?
if [ "$TVRC" -eq 0 ]; then
    ok
    grep -E '^(GLEICH|UNGLEICH|MIT STRUCTS)' "$WORK/typen_vergleich.log" | sed 's/^/   /'
else
    bad "tools/typen_vergleich.sh schlug fehl (siehe .test-work/typen_vergleich.log)"
    tail -20 "$WORK/typen_vergleich.log" | sed 's/^/   /'
fi

TOTAL=$((PASS + FAIL))
echo
if [ "$FAIL" -eq 0 ]; then
    echo "PASS $PASS/$TOTAL"
    exit 0
else
    echo "FAIL $FAIL/$TOTAL fehlgeschlagen:"
    printf "%b\n" "$FAILED"
    exit 1
fi
