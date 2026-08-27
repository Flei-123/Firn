#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# Nachweis, dass der Optimierer (compiler/src/opt.rs) wirklich wirkt.
#
# Fuer jedes Programm in tests/opt/ wird die FIR VOR (--emit=fir-raw) und NACH
# (--emit=fir-opt) der Optimierung erzeugt und geprueft:
#   (a) die Instruktionszahl sinkt,
#   (b) die erwartete gefaltete Konstante steht im Opt-Dump,
#   (c) ein Block, der im Raw-Dump noch da ist (erkennbar an einer eindeutigen
#       Marke), ist im Opt-Dump verschwunden und die Blockzahl sinkt.
# Runde 2 zusaetzlich:
#   (d) Mustertabelle: eine FIR-Zeile, die im Raw-Dump mindestens n-mal
#       vorkommt, darf im Opt-Dump hoechstens m-mal vorkommen (mem2reg,
#       tote Speicherung, Inlining, CSE, Bereichspruefung, Blockverschmelzung).
#   (e) Assembler-Nachweis der Registerzuteilung: der heisse Schleifenblock
#       enthaelt KEINEN Stackzugriff, benutzt callee-saved Register und
#       sichert/restauriert sie ordnungsgemaess.
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
# ROUND 72: the build level this script proves, made explicit.
#
# Every check below is about a pass that runs at the FULL level only
# (`opt::Level::allows_all`) -- constant folding, dead code, inlining, CSE,
# LICM, register allocation. Without a flag that used to be `release-fast`,
# because `OptConfig::default()` happened to say so; round 72 corrected the
# default to `dev-fast` (which runs only the debug preserving passes) and
# this script silently started measuring a compiler that was not supposed
# to fold anything at all -- ten of its checks failed for that reason and
# not one of them was about the round. Exactly the same trap `test.sh`'s
# `mode=opt` fell into, in the file next door.
LVL="--opt-level=release-fast"
WORK="$ROOT/.opt-work"

echo "== baue Compiler =="
cargo build --release --manifest-path compiler/Cargo.toml

echo "== Modul-Tests des Optimierers =="
cargo test --manifest-path compiler/Cargo.toml --release -- opt:: mem2reg:: inline:: regalloc::

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
    if ! "$FIRNC" $LVL --emit=fir-raw "$FILE" > "$raw"; then
        bad "$FILE: --emit=fir-raw schlug fehl"
        continue
    fi
    if ! "$FIRNC" $LVL --emit=fir-opt "$FILE" > "$opt"; then
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


# ------------------------------------------------------------------ (d) ---
# Muster | raw_min | opt_max
PATTERNS="
tests/opt/mem2reg_single_store.fi|load.i32|1|0
tests/opt/dead_store.fi|store.i32|1|0
tests/opt/dead_store.fi|alloca|1|0
tests/opt/inline_call.fi|call.i32 @square|1|0
tests/opt/cse_common.fi|mul.i32|2|1
tests/opt/redundant_check.fi|brcond|3|2
tests/opt/block_merge.fi|bb|8|1
"

echo "== Mustertabelle (mem2reg, tote Speicherung, Inlining, CSE, Pruefungen) =="
while IFS='|' read -r FILE PAT RMIN OMAX; do
    [ -n "$FILE" ] || continue
    base=$(basename "$FILE" .fi)
    raw="$WORK/$base.raw.fir"
    opt="$WORK/$base.opt.fir"
    "$FIRNC" $LVL --emit=fir-raw "$FILE" > "$raw"
    "$FIRNC" $LVL --emit=fir-opt "$FILE" > "$opt"
    n_raw=$(grep -cF "$PAT" "$raw" || true)
    n_opt=$(grep -cF "$PAT" "$opt" || true)
    if [ "$n_raw" -ge "$RMIN" ] && [ "$n_opt" -le "$OMAX" ]; then
        ok "$FILE: '$PAT' $n_raw -> $n_opt (erlaubt: >= $RMIN -> <= $OMAX)"
    else
        bad "$FILE: '$PAT' $n_raw -> $n_opt (erwartet >= $RMIN -> <= $OMAX)"
    fi
done <<< "$PATTERNS"

# ------------------------------------------------------------------ (e) ---
echo "== Registerzuteilung im Assembler =="
ASM="$WORK/regalloc_loop.s"
"$FIRNC" $LVL --emit=asm -o "$ASM" tests/opt/regalloc_loop.fi
BODY=$(awk '/^\.Lsum__bb2:/{f=1;next} /^\.Lsum__bb3:/{f=0} f' "$ASM")
if [ -z "$BODY" ]; then
    bad "regalloc: Schleifenblock .Lsum__bb2 nicht gefunden"
else
    if echo "$BODY" | grep -q '\[rbp-'; then
        bad "regalloc: Schleifenrumpf greift noch auf den Stack zu"
        echo "$BODY" | sed 's/^/        /'
    else
        ok "regalloc: Schleifenrumpf ohne einen einzigen Stackzugriff"
    fi
    # ROUND SPEED: the 32 bit spellings count too. Until the layout round
    # the loop body was emitted BEHIND .Lsum__bb3, so the awk above ran to
    # the end of the file and this pattern was matching `mov rax, r8` out of
    # `main` -- not the loop body at all. Now the body really is the three
    # lines between head and exit, and an `i32` counter is `add r9d, r10d`:
    # the right register, spelled 32 bits wide.
    if echo "$BODY" | grep -qE '\b(rbx|ebx|bx|bl|r(8|9|1[0-5])[dwb]?)\b'; then
        ok "regalloc: Schleifenrumpf rechnet in Registern"
    else
        bad "regalloc: keine zugeteilten Register im Schleifenrumpf"
    fi
fi
# callee-saved Register muessen gesichert UND zurueckgeholt werden
for R in rbx r12 r13 r14 r15; do
    SAVE=$(grep -cE "mov qword ptr \[rbp-[0-9]+\], $R\$" "$ASM" || true)
    LOAD=$(grep -cE "mov $R, qword ptr \[rbp-[0-9]+\]\$" "$ASM" || true)
    if [ "$SAVE" -ne 0 ] || [ "$LOAD" -ne 0 ]; then
        if [ "$SAVE" -ge 1 ] && [ "$LOAD" -ge 1 ]; then
            ok "regalloc: $R wird gesichert ($SAVE) und zurueckgeholt ($LOAD)"
        else
            bad "regalloc: $R unausgeglichen gesichert/zurueckgeholt ($SAVE/$LOAD)"
        fi
    fi
done

# Optimierung darf das Ergebnis nie aendern: jedes Optimierertestprogramm
# einmal mit und einmal ohne Optimierer ausfuehren und vergleichen.
echo "== gleiches Ergebnis mit und ohne Optimierer =="
for FILE in tests/opt/*.fi; do
    base=$(basename "$FILE" .fi)
    "$FIRNC" -o "$WORK/$base.opt.bin" "$FILE"
    "$FIRNC" --no-opt -o "$WORK/$base.noopt.bin" "$FILE"
    set +e
    "$WORK/$base.opt.bin" > "$WORK/$base.opt.out"; A=$?
    "$WORK/$base.noopt.bin" > "$WORK/$base.noopt.out"; B=$?
    set -e
    if [ "$A" = "$B" ] && cmp -s "$WORK/$base.opt.out" "$WORK/$base.noopt.out"; then
        ok "$FILE: identisches Ergebnis (Exit $A)"
    else
        bad "$FILE: Optimierer aendert das Ergebnis ($A vs $B)"
    fi
done

# ------------------------------------------------------------------ (g) ---
# LICM: schleifeninvariante Rechnung wandert in den Vorkopf. Der Nachweis
# vergleicht ZWEI Bloecke desselben FIR-Dumps — die Instruktion verschwindet
# nicht, sie zieht um. Eine reine Zaehlung ueber die ganze Funktion wuerde das
# gar nicht bemerken.
echo "== LICM: schleifeninvariante Rechnung im Vorkopf =="
LIC="$WORK/licm_hoist.opt.fir"
"$FIRNC" $LVL --emit=fir-opt tests/opt/licm_hoist.fi > "$LIC"
RAW="$WORK/licm_hoist.raw.fir"
"$FIRNC" $LVL --emit=fir-raw tests/opt/licm_hoist.fi > "$RAW"
rumpf_roh=$(awk '/^bb2:/{f=1;next} /^bb3:/{f=0} f' "$RAW" | grep -c 'mul\.u64' || true)
rumpf_opt=$(awk '/^bb2:/{f=1;next} /^bb3:/{f=0} f' "$LIC" | grep -c 'mul\.u64' || true)
vorkopf_opt=$(awk '/^bb0:/{f=1;next} /^bb1:/{f=0} f' "$LIC" | grep -c 'mul\.u64' || true)
if [ "$rumpf_roh" -ge 1 ]; then
    ok "licm: unoptimiert steht die Multiplikation im Schleifenrumpf ($rumpf_roh)"
else
    bad "licm: der Testfall hat unoptimiert gar keine Multiplikation im Rumpf"
fi
if [ "$rumpf_opt" -eq 0 ]; then
    ok "licm: nach der Optimierung steht keine mehr im Rumpf"
else
    bad "licm: es steht noch eine Multiplikation im Schleifenrumpf ($rumpf_opt)"
    awk '/^bb2:/{f=1;next} /^bb3:/{f=0} f' "$LIC" | sed 's/^/        /'
fi
if [ "$vorkopf_opt" -ge 1 ]; then
    ok "licm: sie steht jetzt im Vorkopf ($vorkopf_opt)"
else
    bad "licm: im Vorkopf ist keine Multiplikation angekommen"
fi

TOTAL=$((PASS + FAIL))
echo
if [ "$FAIL" -eq 0 ]; then
    echo "PASS $PASS/$TOTAL (proof of the optimiser)"
    exit 0
else
    echo "FAIL $FAIL/$TOTAL failed:"
    printf "%b\n" "$FAILED"
    exit 1
fi
