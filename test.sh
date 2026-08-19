#!/usr/bin/env bash
# The complete test suite for firnc0.
#
# Sequence:
#   1. Build the compiler (cargo build --release) -- warnings are reported.
#   2. Module tests of the compiler (cargo test --release).
#   3. Every program in tests/, tests/opt/ and examples/ is compiled TWICE
#      (--opt-level=release-fast, --no-opt and --opt-level=dev-fast),
#      assembled, linked,
#      RUN, and the exit code resp. the standard output is checked against the
#      expectation in line 1 (// expect_exit: N  resp.  // expect_out: TEXT).
#   4. Every program in tests/neg/ has to stop with an exit code != 0 and print
#      the expected message with a line:column (// expect_error: L:C TEXT).
#      A Rust panic counts as a failure.
#   5. Proof of the optimiser (test_opt.sh: FIR before/after).
#   6. Proof of the result-location guarantee (tools/ergebnisort/run.sh:
#      frame sizes in the emitted assembly).
#   7. Architecture check: field access is separated from the memory location
#      (tools/schichten/run.sh, a precondition for SoA).
#   8. Symbol naming scheme: reserved prefix, room for the
#      ABI version, modules free of collisions (tools/symbole/run.sh).
#   8b. The atomic primitive `__atomic_add` really produces a
#      `lock xadd` -- in three build stages and in both compilers, with a
#      counter-check (tools/atomic/run.sh, round 47).
#   8c. Interface bounds dispatch STATICALLY: no indirect call,
#      no method table -- counter-check with `dyn I`, both compilers
#      (tools/bounds/run.sh, round 50).
#   9. HTML5 tokenizer (lib/html/, in Firn) against the official
#      html5lib test suite: the exact quota out of 6,810 cases, the limit in
#      tools/tokenizer/mindestquote.txt (tools/tokenizer/run.sh).
#   9b. HTML tree construction and the DOM core (lib/browser/, in Firn) against
#      the own cases from the WHATWG standard, against real pages and
#      in a soak run with a counter-check (tools/html/run.sh, docs/RUNDE54.md).
#  18. Package and project system (tools/packages/run.sh): manifest, search
#      order, visibility, build driver -- in BOTH compilers.
#  19. Freestanding compilation (tools/freestanding/run.sh, round 52):
#      `profile kernel`, inline assembly, MMIO, `#[interrupt]` -- the
#      kernel example becomes an ELF object file WITHOUT undefined
#      symbols, in BOTH compilers, and is linked against a linker script.
#  10. DOM soak run (tools/dom_soak/run.sh): the DOM prototype in Firn builds
#      real cycles continuously (parent/child, listener, JS wrapper) and must
#      not grow while doing so; the deliberately leaking counter-check with
#      reference counts MUST strike, otherwise the measurement counts as broken.
#
# No '|| true', no swallowing of exit codes: set -euo pipefail.
set -euo pipefail

cd "$(dirname "$0")"
ROOT=$(pwd)
FIRNC="$ROOT/compiler/target/release/firnc"
WORK="$ROOT/.test-work"

# Module search path (round 39): `import std.*` resolves over $FIRNLIB to
# <repo>/lib -- no matter which directory a test project calls from.
export FIRNLIB="$ROOT/lib"

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
    # The source line and the marker have to be there
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

echo "== 8b. Atomares Primitiv: 'lock xadd' (tools/atomic/run.sh, RUNDE 47) =="
bash tools/atomic/run.sh > "$WORK/atomar.log" 2>&1 && ATRC=0 || ATRC=$?
if [ "$ATRC" -eq 0 ]; then
    ok
    tail -1 "$WORK/atomar.log" | sed 's/^/   /'
else
    bad "tools/atomic/run.sh schlug fehl (siehe .test-work/atomar.log)"
    tail -20 "$WORK/atomar.log" | sed 's/^/   /'
fi

echo "== 8c. Schranken: statischer Versand ohne indirekten Aufruf (RUNDE 50) =="
# `fn f[T: I]` calls the interface method DIRECTLY -- proven on the emitted
# assembly and on the FIR, with `dyn I` as a counter-check, in both compilers.
SCHRANKEN_MESSEN=${SCHRANKEN_MESSEN:-0} bash tools/bounds/run.sh > "$WORK/schranken.log" 2>&1 && SKRC=0 || SKRC=$?
if [ "$SKRC" -eq 0 ]; then
    ok
    tail -1 "$WORK/schranken.log" | sed 's/^/   /'
else
    bad "tools/bounds/run.sh schlug fehl (siehe .test-work/schranken.log)"
    tail -20 "$WORK/schranken.log" | sed 's/^/   /'
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

echo "== 9b. HTML-Baumkonstruktion + DOM-Kern (tools/html/run.sh) =="
# The tree building in Firn (lib/browser/) against the own cases from the
# WHATWG standard, plus the real pages from testdata/realweb/ and the
# soak run with a counter-check. The short version; the full run is in
# docs/RUNDE54.md.
bash tools/html/run.sh --schnell > "$WORK/baum.log" 2>&1 && BMRC=0 || BMRC=$?
if [ "$BMRC" -eq 0 ]; then
    ok
    grep -E '^GESAMT|^OK:' "$WORK/baum.log" | sed 's/^/   /'
else
    bad "tools/html/run.sh schlug fehl (siehe .test-work/baum.log)"
    tail -20 "$WORK/baum.log" | sed 's/^/   /'
fi

echo "== 10. DOM-Dauerlauf: Zyklen ohne Leck (tools/dom_soak/run.sh) =="
# The short version: 12 s per variant. The long run is in ABNAHME.md item 2;
# the point here is that the promise is re-checked at EVERY change.
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

echo "== 11. Lexer in Firn gegen Lexer in Rust (tools/lex_compare.sh) =="
# The first part of stage 1: `lib/firnc1/lexer.fi` produces the same
# token stream as `firnc0 --emit=tokens`, over the whole source corpus.
bash tools/lex_compare.sh > "$WORK/lex_vergleich.log" 2>&1 && LXRC=0 || LXRC=$?
if [ "$LXRC" -eq 0 ]; then
    ok
    grep -E '^(GLEICH|UNGLEICH|TOKEN|GLEITKOMMA)' "$WORK/lex_vergleich.log" | sed 's/^/   /'
else
    bad "tools/lex_compare.sh schlug fehl (siehe .test-work/lex_vergleich.log)"
    tail -20 "$WORK/lex_vergleich.log" | sed 's/^/   /'
fi

echo "== 12. Parser in Firn gegen Parser in Rust (tools/parser_compare.sh) =="
bash tools/parser_compare.sh > "$WORK/parser_vergleich.log" 2>&1 && PVRC=0 || PVRC=$?
if [ "$PVRC" -eq 0 ]; then
    ok
    grep -E '^(GLEICH|UNGLEICH|NICHT KERN)' "$WORK/parser_vergleich.log" | sed 's/^/   /'
else
    bad "tools/parser_compare.sh schlug fehl (siehe .test-work/parser_vergleich.log)"
    tail -20 "$WORK/parser_vergleich.log" | sed 's/^/   /'
fi

echo "== 13. Layout und ABI in Firn gegen Rust (tools/types_compare.sh) =="
bash tools/types_compare.sh > "$WORK/typen_vergleich.log" 2>&1 && TVRC=0 || TVRC=$?
if [ "$TVRC" -eq 0 ]; then
    ok
    grep -E '^(GLEICH|UNGLEICH|MIT STRUCTS)' "$WORK/typen_vergleich.log" | sed 's/^/   /'
else
    bad "tools/types_compare.sh schlug fehl (siehe .test-work/typen_vergleich.log)"
    tail -20 "$WORK/typen_vergleich.log" | sed 's/^/   /'
fi

echo "== 14. Typpruefer in Firn gegen Rust (tools/sema_compare.sh) =="
bash tools/sema_compare.sh > "$WORK/sema_vergleich.log" 2>&1 && SVRC=0 || SVRC=$?
if [ "$SVRC" -eq 0 ]; then
    ok
    grep -E '^(GLEICH|UNGLEICH|AUSDRUECKE|NICHT KERN)' "$WORK/sema_vergleich.log" | sed 's/^/   /'
else
    bad "tools/sema_compare.sh schlug fehl (siehe .test-work/sema_vergleich.log)"
    tail -20 "$WORK/sema_vergleich.log" | sed 's/^/   /'
fi

echo "== 15. Lowering in Firn gegen Rust (tools/fir_compare.sh) =="
bash tools/fir_compare.sh > "$WORK/fir_vergleich.log" 2>&1 && FVRC=0 || FVRC=$?
if [ "$FVRC" -eq 0 ]; then
    ok
    grep -E '^(GLEICH|UNGLEICH|INSTRUKTIONEN|DEFER)' "$WORK/fir_vergleich.log" | sed 's/^/   /'
else
    bad "tools/fir_compare.sh schlug fehl (siehe .test-work/fir_vergleich.log)"
    tail -20 "$WORK/fir_vergleich.log" | sed 's/^/   /'
fi

echo "== 16. Der Compiler in Firn uebersetzt, das Ergebnis laeuft (tools/self_compare.sh) =="
bash tools/self_compare.sh > "$WORK/selbst_vergleich.log" 2>&1 && SBRC=0 || SBRC=$?
if [ "$SBRC" -eq 0 ]; then
    ok
    grep -E '^(GLEICHES|ABWEICHEND|FEHLERHAFT|CODEGEN)' "$WORK/selbst_vergleich.log" | sed 's/^/   /'
else
    bad "tools/self_compare.sh schlug fehl (siehe .test-work/selbst_vergleich.log)"
    tail -20 "$WORK/selbst_vergleich.log" | sed 's/^/   /'
fi

echo "== 17. Der Fixpunkt: Firn uebersetzt sich selbst (tools/fixpoint.sh) =="
bash tools/fixpoint.sh > "$WORK/fixpunkt.log" 2>&1 && FPRC=0 || FPRC=$?
if [ "$FPRC" -eq 0 ]; then
    ok
    grep -E '^(STUFE|FIXPUNKT|KORPUS)' "$WORK/fixpunkt.log" | sed 's/^/   /'
else
    bad "tools/fixpoint.sh schlug fehl (siehe .test-work/fixpunkt.log)"
    tail -20 "$WORK/fixpunkt.log" | sed 's/^/   /'
fi

echo "== 20. Nebenlaeufigkeit: Faeden, Mutex, atomare Primitive (tools/thread/run.sh) =="
# Round 49. clone(2)/exit(2), `lock cmpxchg`, thread storage over `fs:0` --
# in three build stages and BOTH compilers, with counter-checks that have to
# strike. The soak run (tools/thread/stress.sh) does not run here but
# separately: it needs minutes.
bash tools/thread/run.sh > "$WORK/faden.log" 2>&1 && FDRC=0 || FDRC=$?
if [ "$FDRC" -eq 0 ]; then
    ok
    tail -1 "$WORK/faden.log" | sed 's/^/   /'
else
    bad "tools/thread/run.sh schlug fehl (siehe .test-work/faden.log)"
    grep FAIL "$WORK/faden.log" | head -10 | sed 's/^/   /'
fi

echo "== 19. Freistehend: profile kernel, Inline-Asm, MMIO, iretq (tools/freestanding/run.sh) =="
# Round 52. The kernel example is compiled by BOTH compilers into an
# ELF object file that has NO undefined name, contains no
# syscall and can be linked against a linker script.
bash tools/freestanding/run.sh > "$WORK/freistehend.log" 2>&1 && FSRC=0 || FSRC=$?
if [ "$FSRC" -eq 0 ]; then
    ok
    tail -1 "$WORK/freistehend.log" | sed 's/^/   /'
else
    bad "tools/freestanding/run.sh schlug fehl (siehe .test-work/freistehend.log)"
    grep FAIL "$WORK/freistehend.log" | head -10 | sed 's/^/   /'
fi

echo "== 18. Paket- und Projektsystem (tools/packages/run.sh) =="
# The manifest `firn.package`, the search order, visibility at module level and the
# build driver `--package` -- every case through BOTH compilers, messages
# compared octet for octet.
bash tools/packages/run.sh > "$WORK/pakete.log" 2>&1 && PKRC=0 || PKRC=$?
if [ "$PKRC" -eq 0 ]; then
    ok
    grep -E '^PAKETE' "$WORK/pakete.log" | sed 's/^/   /'
else
    bad "tools/packages/run.sh schlug fehl (siehe .test-work/pakete.log)"
    tail -20 "$WORK/pakete.log" | sed 's/^/   /'
fi

echo "== 21. Englisch-Umstellung: keine deutschen Bezeichner mehr (tools/englisch/pruefe.sh) =="
# Stage A (round 55): every identifier in compiler/src, lib, bin, tools,
# tests and demos is held against the morpheme table. A hit means
# that a German name was overlooked.
bash tools/englisch/pruefe.sh > "$WORK/englisch.log" 2>&1 && ENRC=0 || ENRC=$?
if [ "$ENRC" -eq 0 ]; then
    ok
    tail -1 "$WORK/englisch.log" | sed 's/^/   /'
else
    bad "tools/englisch/pruefe.sh meldet deutsche Bezeichner (siehe .test-work/englisch.log)"
    tail -20 "$WORK/englisch.log" | sed 's/^/   /'
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
