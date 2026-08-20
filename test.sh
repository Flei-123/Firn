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
#   8d. Functions as values (round 58): a DIRECT call stays a direct
#      `call`, a call through a function value is exactly one `call rax`,
#      a closure without captures allocates nothing -- in both compilers
#      and with counter-checks (tools/fnval/run.sh).
#   9. HTML5 tokenizer (lib/html/, in Firn) against the official
#      html5lib test suite: the exact quota out of 6,810 cases, the limit in
#      tools/tokenizer/mindestquote.txt (tools/tokenizer/run.sh).
#   9b. HTML tree construction and the DOM core (lib/browser/, in Firn) against
#      the own cases from the WHATWG standard, against real pages and
#      in a soak run with a counter-check (tools/html/run.sh, docs/ROUND54.md).
#   9c. CSS: syntax, selectors and cascade (lib/css/, in Firn) against the
#      official suite css-parsing-tests, against own cases and against
#      cssselect2 on real pages (tools/css/run.sh, docs/ROUND60.md).
#   9d. JavaScript: lexer, parser, interpreter and the built in objects
#      (lib/js/, in Firn) against the official suite test262, against node
#      as a second engine, and in an endurance run with a counter check
#      (tools/js/run.sh, docs/ROUND63.md).
#  18. Package and project system (tools/packages/run.sh): manifest, search
#      order, visibility, build driver -- in BOTH compilers.
#  19. Freestanding compilation (tools/freestanding/run.sh, round 52):
#      `profile kernel`, inline assembly, MMIO, `#[interrupt]` -- the
#      kernel example becomes an ELF object file WITHOUT undefined
#      symbols, in BOTH compilers, and is linked against a linker script.
#  22. The kernel (tools/kernel/run.sh, round 59): `demos/kernel/kmain.fi`
#      boots in QEMU and is checked over its serial output -- IDT and
#      exception reports (#DE, #PF, #GP, #DF), PIC/PIT with a tick counter
#      that runs up, memory map, frame allocator and heap, keyboard over
#      IRQ1, ring 3 with `syscall`/`sysret`. With counter-checks.
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

echo "== 1. build the compiler =="
cargo build --release --manifest-path compiler/Cargo.toml

echo "== 2. module tests of the compiler =="
cargo test --release --manifest-path compiler/Cargo.toml -- --test-threads=4 >/dev/null
echo "   cargo test: ok"

rm -rf "$WORK"
mkdir -p "$WORK"

run_case() {          # $1 = file, $2 = "opt" | "noopt" | "devfast"
    local file="$1" mode="$2"
    local base ext bin flags hdr exp out rc
    base=$(basename "$file" .fi)
    bin="$WORK/${base}.${mode}"
    flags=""
    [ "$mode" = "noopt" ]   && flags="--no-opt"
    [ "$mode" = "devfast" ] && flags="--opt-level=dev-fast"

    if ! "$FIRNC" $flags -o "$bin" "$file" >"$WORK/$base.$mode.cerr" 2>&1; then
        bad "$file [$mode]: compilation failed"
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
                bad "$file [$mode]: program ended with exit code $rc (expected 0)"
            elif [ "$out" = "$exp" ]; then
                ok
            else
                bad "$file [$mode]: output '$out', expected '$exp'"
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
                bad "$file [$mode]: exit code $rc, expected $exp"
            fi
            ;;
        *)
            bad "$file: no expectation in line 1 (// expect_exit: / // expect_out:)"
            ;;
    esac
}

echo "== 3. positive tests (each with and without the optimiser) =="
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
echo "   $NPROG programs x 3 runs (opt / noopt / dev-fast)"

echo "== 4. negative tests (error messages) =="
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
        bad "$f: the compiler reported NO error (exit 0)"
        continue
    fi
    if grep -qE "panicked at|RUST_BACKTRACE" "$WORK/neg.out"; then
        echo
        bad "$f: a Rust panic instead of a clean error message"
        continue
    fi
    if ! grep -qF ":$pos" "$WORK/neg.out"; then
        echo
        bad "$f: position '$pos' is missing from the message"
        sed 's/^/        /' "$WORK/neg.out" | head -6
        continue
    fi
    if ! grep -qF "$msg" "$WORK/neg.out"; then
        echo
        bad "$f: text '$msg' is missing from the message"
        sed 's/^/        /' "$WORK/neg.out" | head -6
        continue
    fi
    # The source line and the marker have to be there
    if ! grep -q '\^' "$WORK/neg.out"; then
        echo
        bad "$f: no marker (^) in the message"
        continue
    fi
    cnt_hdr=$(sed -n '2p' "$f")
    case "$cnt_hdr" in
        *expect_error_count:*)
            want=${cnt_hdr#*expect_error_count: }
            got=$(grep -c '^error:' "$WORK/neg.out")
            if [ "$got" -ne "$want" ]; then
                echo
                bad "$f: $got errors reported, expected $want"
                continue
            fi
            ;;
    esac
    ok
    echo "  [error as expected]"
done

echo "== 5. proof of the optimiser =="
bash test_opt.sh > "$WORK/opt.log" 2>&1 && OPTRC=0 || OPTRC=$?
if [ "$OPTRC" -eq 0 ]; then
    ok
    tail -1 "$WORK/opt.log" | sed 's/^/   /'
else
    bad "test_opt.sh failed (see .test-work/opt.log)"
    tail -20 "$WORK/opt.log" | sed 's/^/   /'
fi

echo "== 6. proof of the result-location guarantee (SPEC.md 13.1) =="
bash tools/ergebnisort/run.sh > "$WORK/result_location.log" 2>&1 && EORC=0 || EORC=$?
if [ "$EORC" -eq 0 ]; then
    ok
    tail -1 "$WORK/result_location.log" | sed 's/^/   /'
else
    bad "tools/ergebnisort/run.sh failed (see .test-work/result_location.log)"
    tail -20 "$WORK/result_location.log" | sed 's/^/   /'
fi

echo "== 7. architecture: field access <-> memory location separated =="
bash tools/schichten/run.sh > "$WORK/layers.log" 2>&1 && SCRC=0 || SCRC=$?
if [ "$SCRC" -eq 0 ]; then
    ok
    tail -1 "$WORK/layers.log" | sed 's/^/   /'
else
    bad "tools/schichten/run.sh failed (see .test-work/layers.log)"
    tail -20 "$WORK/layers.log" | sed 's/^/   /'
fi

echo "== 8. symbol naming scheme (DESIGN_GOALS 4) =="
bash tools/symbole/run.sh > "$WORK/symbols.log" 2>&1 && SYRC=0 || SYRC=$?
if [ "$SYRC" -eq 0 ]; then
    ok
    tail -1 "$WORK/symbols.log" | sed 's/^/   /'
else
    bad "tools/symbole/run.sh failed (see .test-work/symbols.log)"
    tail -20 "$WORK/symbols.log" | sed 's/^/   /'
fi

echo "== 8b. atomic primitive: 'lock xadd' (tools/atomic/run.sh, ROUND 47) =="
bash tools/atomic/run.sh > "$WORK/atomic.log" 2>&1 && ATRC=0 || ATRC=$?
if [ "$ATRC" -eq 0 ]; then
    ok
    tail -1 "$WORK/atomic.log" | sed 's/^/   /'
else
    bad "tools/atomic/run.sh failed (see .test-work/atomic.log)"
    tail -20 "$WORK/atomic.log" | sed 's/^/   /'
fi

echo "== 8c. bounds: static dispatch without an indirect call (ROUND 50) =="
# `fn f[T: I]` calls the interface method DIRECTLY -- proven on the emitted
# assembly and on the FIR, with `dyn I` as a counter-check, in both compilers.
BOUNDS_MEASURE=${BOUNDS_MEASURE:-0} bash tools/bounds/run.sh > "$WORK/bounds.log" 2>&1 && SKRC=0 || SKRC=$?
if [ "$SKRC" -eq 0 ]; then
    ok
    tail -1 "$WORK/bounds.log" | sed 's/^/   /'
else
    bad "tools/bounds/run.sh failed (see .test-work/bounds.log)"
    tail -20 "$WORK/bounds.log" | sed 's/^/   /'
fi

echo "== 9. HTML5 tokenizer against html5lib (tools/tokenizer/run.sh) =="
bash tools/tokenizer/run.sh --fast > "$WORK/tokenizer.log" 2>&1 && TKRC=0 || TKRC=$?
if [ "$TKRC" -eq 0 ]; then
    ok
    grep -E '^TOTAL' "$WORK/tokenizer.log" | sed 's/^/   /'
else
    bad "tools/tokenizer/run.sh failed (see .test-work/tokenizer.log)"
    tail -20 "$WORK/tokenizer.log" | sed 's/^/   /'
fi

echo "== 9b. HTML tree construction + DOM core (tools/html/run.sh) =="
# The tree building in Firn (lib/browser/) against the own cases from the
# WHATWG standard, plus the real pages from testdata/realweb/ and the
# soak run with a counter-check. The short version; the full run is in
# docs/ROUND54.md.
bash tools/html/run.sh --fast > "$WORK/tree.log" 2>&1 && BMRC=0 || BMRC=$?
if [ "$BMRC" -eq 0 ]; then
    ok
    grep -E '^TOTAL|^OK:' "$WORK/tree.log" | sed 's/^/   /'
else
    bad "tools/html/run.sh failed (see .test-work/tree.log)"
    tail -20 "$WORK/tree.log" | sed 's/^/   /'
fi

echo "== 9c. CSS: syntax, selectors, cascade (tools/css/run.sh) =="
# The CSS path in Firn (lib/css/) against the foreign suite
# css-parsing-tests, against the own cases for cascade and error tolerance,
# against cssselect2 on the real pages, plus the soak run with a counter
# check. The short version; the full run is in docs/ROUND60.md.
CSS_SOAK_MS=${CSS_SOAK_MS:-6000} bash tools/css/run.sh --fast > "$WORK/css.log" 2>&1 && CSRC=0 || CSRC=$?
if [ "$CSRC" -eq 0 ]; then
    ok
    grep -E '^TOTAL|^OK:|^match comparisons' "$WORK/css.log" | sed 's/^/   /'
else
    bad "tools/css/run.sh failed (see .test-work/css.log)"
    tail -20 "$WORK/css.log" | sed 's/^/   /'
fi

echo "== 10. DOM soak run: cycles without a leak (tools/dom_soak/run.sh) =="
# The short version: 12 s per variant. The long run is in ACCEPTANCE.md item 2;
# the point here is that the promise is re-checked at EVERY change.
SOAK_SEC=${SOAK_SEC:-12} SOAK_CYCLES=${SOAK_CYCLES:-400000} \
  SOAK_SAMPLE=${SOAK_SAMPLE:-10000} SOAK_MIN_CYCLES=${SOAK_MIN_CYCLES:-100000} \
  bash tools/dom_soak/run.sh > "$WORK/dom_soak.log" 2>&1 && DSRC=0 || DSRC=$?
if [ "$DSRC" -eq 0 ]; then
    ok
    grep -E 'PASSED|counter-check strikes' "$WORK/dom_soak.log" | sed 's/^/   /'
else
    bad "tools/dom_soak/run.sh failed (see .test-work/dom_soak.log)"
    tail -20 "$WORK/dom_soak.log" | sed 's/^/   /'
fi

echo "== 11. lexer in Firn against the lexer in Rust (tools/lex_compare.sh) =="
# The first part of stage 1: `lib/firnc1/lexer.fi` produces the same
# token stream as `firnc0 --emit=tokens`, over the whole source corpus.
bash tools/lex_compare.sh > "$WORK/lex_compare.log" 2>&1 && LXRC=0 || LXRC=$?
if [ "$LXRC" -eq 0 ]; then
    ok
    grep -E '^(SAME|DIFFERENT|TOKENS|FLOATING)' "$WORK/lex_compare.log" | sed 's/^/   /'
else
    bad "tools/lex_compare.sh failed (see .test-work/lex_compare.log)"
    tail -20 "$WORK/lex_compare.log" | sed 's/^/   /'
fi

echo "== 12. parser in Firn against the parser in Rust (tools/parser_compare.sh) =="
bash tools/parser_compare.sh > "$WORK/parser_compare.log" 2>&1 && PVRC=0 || PVRC=$?
if [ "$PVRC" -eq 0 ]; then
    ok
    grep -E '^(SAME|DIFFERENT|NOT CORE)' "$WORK/parser_compare.log" | sed 's/^/   /'
else
    bad "tools/parser_compare.sh failed (see .test-work/parser_compare.log)"
    tail -20 "$WORK/parser_compare.log" | sed 's/^/   /'
fi

echo "== 13. layout and ABI in Firn against Rust (tools/types_compare.sh) =="
bash tools/types_compare.sh > "$WORK/types_compare.log" 2>&1 && TVRC=0 || TVRC=$?
if [ "$TVRC" -eq 0 ]; then
    ok
    grep -E '^(SAME|DIFFERENT|WITH STRUCTS)' "$WORK/types_compare.log" | sed 's/^/   /'
else
    bad "tools/types_compare.sh failed (see .test-work/types_compare.log)"
    tail -20 "$WORK/types_compare.log" | sed 's/^/   /'
fi

echo "== 14. type checker in Firn against Rust (tools/sema_compare.sh) =="
bash tools/sema_compare.sh > "$WORK/sema_compare.log" 2>&1 && SVRC=0 || SVRC=$?
if [ "$SVRC" -eq 0 ]; then
    ok
    grep -E '^(SAME|DIFFERENT|EXPRESSIONS|NOT CORE)' "$WORK/sema_compare.log" | sed 's/^/   /'
else
    bad "tools/sema_compare.sh failed (see .test-work/sema_compare.log)"
    tail -20 "$WORK/sema_compare.log" | sed 's/^/   /'
fi

echo "== 15. lowering in Firn against Rust (tools/fir_compare.sh) =="
bash tools/fir_compare.sh > "$WORK/fir_compare.log" 2>&1 && FVRC=0 || FVRC=$?
if [ "$FVRC" -eq 0 ]; then
    ok
    grep -E '^(SAME|DIFFERENT|INSTRUCTIONS|DEFER)' "$WORK/fir_compare.log" | sed 's/^/   /'
else
    bad "tools/fir_compare.sh failed (see .test-work/fir_compare.log)"
    tail -20 "$WORK/fir_compare.log" | sed 's/^/   /'
fi

echo "== 16. the compiler in Firn compiles, the result runs (tools/self_compare.sh) =="
bash tools/self_compare.sh > "$WORK/self_compare.log" 2>&1 && SBRC=0 || SBRC=$?
if [ "$SBRC" -eq 0 ]; then
    ok
    grep -E '^(SAME|DIFFERING|FAULTY|CODEGEN)' "$WORK/self_compare.log" | sed 's/^/   /'
else
    bad "tools/self_compare.sh failed (see .test-work/self_compare.log)"
    tail -20 "$WORK/self_compare.log" | sed 's/^/   /'
fi

echo "== 17. the fixpoint: Firn compiles itself (tools/fixpoint.sh) =="
bash tools/fixpoint.sh > "$WORK/fixpoint.log" 2>&1 && FPRC=0 || FPRC=$?
if [ "$FPRC" -eq 0 ]; then
    ok
    grep -E '^(STAGE|FIXPOINT|CORPUS)' "$WORK/fixpoint.log" | sed 's/^/   /'
else
    bad "tools/fixpoint.sh failed (see .test-work/fixpoint.log)"
    tail -20 "$WORK/fixpoint.log" | sed 's/^/   /'
fi

echo "== 20. concurrency: threads, mutex, atomic primitives (tools/thread/run.sh) =="
# Round 49. clone(2)/exit(2), `lock cmpxchg`, thread storage over `fs:0` --
# in three build stages and BOTH compilers, with counter-checks that have to
# strike. The soak run (tools/thread/stress.sh) does not run here but
# separately: it needs minutes.
bash tools/thread/run.sh > "$WORK/thread.log" 2>&1 && FDRC=0 || FDRC=$?
if [ "$FDRC" -eq 0 ]; then
    ok
    tail -1 "$WORK/thread.log" | sed 's/^/   /'
else
    bad "tools/thread/run.sh failed (see .test-work/thread.log)"
    grep FAIL "$WORK/thread.log" | head -10 | sed 's/^/   /'
fi

echo "== 19. freestanding: profile kernel, inline asm, MMIO, iretq (tools/freestanding/run.sh) =="
# Round 52. The kernel example is compiled by BOTH compilers into an
# ELF object file that has NO undefined name, contains no
# syscall and can be linked against a linker script.
bash tools/freestanding/run.sh > "$WORK/freestanding.log" 2>&1 && FSRC=0 || FSRC=$?
if [ "$FSRC" -eq 0 ]; then
    ok
    tail -1 "$WORK/freestanding.log" | sed 's/^/   /'
else
    bad "tools/freestanding/run.sh failed (see .test-work/freestanding.log)"
    grep FAIL "$WORK/freestanding.log" | head -10 | sed 's/^/   /'
fi

echo "== 9d. JavaScript: lexer, parser, interpreter (tools/js/run.sh) =="
# The JavaScript path in Firn (lib/js/) against the foreign suite test262
# (parser AND engine), against node on the same small programs, plus the
# endurance run with the deliberately leaking counter check.
JS_SOAK_ROUNDS=${JS_SOAK_ROUNDS:-20000} bash tools/js/run.sh --fast > "$WORK/js.log" 2>&1 && JSRC=0 || JSRC=$?
if [ "$JSRC" -eq 0 ]; then
    ok
    grep -E '^TOTAL|^OK:|^cross check' "$WORK/js.log" | sed 's/^/   /'
else
    bad "tools/js/run.sh failed (see .test-work/js.log)"
    tail -20 "$WORK/js.log" | sed 's/^/   /'
fi

echo "== 18. package and project system (tools/packages/run.sh) =="
# The manifest `firn.package`, the search order, visibility at module level and the
# build driver `--package` -- every case through BOTH compilers, messages
# compared octet for octet.
bash tools/packages/run.sh > "$WORK/packages.log" 2>&1 && PKRC=0 || PKRC=$?
if [ "$PKRC" -eq 0 ]; then
    ok
    grep -E '^PACKAGES' "$WORK/packages.log" | sed 's/^/   /'
else
    bad "tools/packages/run.sh failed (see .test-work/packages.log)"
    tail -20 "$WORK/packages.log" | sed 's/^/   /'
fi

echo "== 8d. functions as values: direct stays direct (tools/fnval/run.sh) =="
# Round 58. The function record costs nothing where no function value is
# used -- that is a claim about the emitted code, so it is measured on the
# emitted code, in both compilers and with counter-checks.
bash tools/fnval/run.sh > "$WORK/fnval.log" 2>&1 && FVRC=0 || FVRC=$?
if [ "$FVRC" -eq 0 ]; then
    ok
    tail -1 "$WORK/fnval.log" | sed 's/^/   /'
else
    bad "tools/fnval/run.sh failed (see .test-work/fnval.log)"
    grep FAIL "$WORK/fnval.log" | head -10 | sed 's/^/   /'
fi

echo "== 22. the kernel really runs: tasks, address spaces, system calls, files (tools/kernel/run.sh) =="
# Round 59 and 62. `demos/kernel/kmain.fi` is booted in QEMU -- once per
# case, each with a time limit. Checked is the SERIAL OUTPUT and the exit
# code: exceptions with error code and register set, a tick counter that
# runs up, frame allocator and heap, keys over IRQ1, ring 3 (round 59) --
# and on top of that three tasks interleaved on one processor, two
# processes with an address space of their own, system calls with real
# error codes, a file system on a RAM disk AND on a real ATA disk, and a
# command line in ring 3 (round 62). Every point with a counter-check:
# masked IRQ0 counts zero ticks, `nopreempt` lets nothing interleave, a
# process that touches kernel memory dies while the kernel lives, `mount`
# refuses an unformatted disk, and `hlt` in ring 3 yields #GP with
# cs=0x2b.
bash tools/kernel/run.sh > "$WORK/kernel.log" 2>&1 && KRRC=0 || KRRC=$?
if [ "$KRRC" -eq 0 ]; then
    ok
    tail -1 "$WORK/kernel.log" | sed 's/^/   /'
else
    bad "tools/kernel/run.sh failed (see .test-work/kernel.log)"
    grep FAIL "$WORK/kernel.log" | head -10 | sed 's/^/   /'
fi

echo "== 21. english migration: no German identifiers left (tools/english/check.sh) =="
# Stage A (round 55): every identifier in compiler/src, lib, bin, tools,
# tests and demos is held against the morpheme table. A hit means
# that a German name was overlooked.
bash tools/english/check.sh > "$WORK/english.log" 2>&1 && ENRC=0 || ENRC=$?
if [ "$ENRC" -eq 0 ]; then
    ok
    tail -1 "$WORK/english.log" | sed 's/^/   /'
else
    bad "tools/english/check.sh reports German identifiers (see .test-work/english.log)"
    tail -20 "$WORK/english.log" | sed 's/^/   /'
fi

TOTAL=$((PASS + FAIL))
echo
if [ "$FAIL" -eq 0 ]; then
    echo "PASS $PASS/$TOTAL"
    exit 0
else
    echo "FAIL $FAIL/$TOTAL failed:"
    printf "%b\n" "$FAILED"
    exit 1
fi
