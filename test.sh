#!/usr/bin/env bash
# Vollstaendige Testsuite fuer firnc0.
#
# Ablauf:
#   1. Compiler bauen (cargo build --release) — Warnungen werden gemeldet.
#   2. Modul-Tests des Compilers (cargo test --release).
#   3. Jedes Programm in tests/, tests/opt/ und examples/ wird ZWEIMAL
#      uebersetzt (mit Optimierer und mit --no-opt), assembliert, gelinkt,
#      AUSGEFUEHRT und Exit-Code bzw. Standardausgabe gegen die Erwartung
#      in Zeile 1 geprueft (// expect_exit: N  bzw.  // expect_out: TEXT).
#   4. Jedes Programm in tests/neg/ muss mit Exit-Code != 0 abbrechen und die
#      erwartete Meldung mit Zeile:Spalte ausgeben (// expect_error: Z:S TEXT).
#      Ein Rust-Panic gilt als Fehlschlag.
#   5. Nachweis des Optimierers (test_opt.sh: FIR vorher/nachher).
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

run_case() {          # $1 = Datei, $2 = "opt" | "noopt"
    local file="$1" mode="$2"
    local base ext bin flags hdr exp out rc
    base=$(basename "$file" .fi)
    bin="$WORK/${base}.${mode}"
    flags=""
    [ "$mode" = "noopt" ] && flags="--no-opt"

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
    echo "  [opt+noopt]"
done
echo "   $NPROG Programme x 2 Durchlaeufe"

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
