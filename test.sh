#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# The complete test suite for firnc0.
#
# Sequence:
#   1. Build the compiler (cargo build --release) -- warnings are reported.
#   2. Module tests of the compiler (cargo test --release).
#   3. Every program in tests/, tests/opt/ and examples/ is compiled in ALL
#      FOUR build levels plus --no-opt (--opt-level=release-fast,
#      --opt-level=release-safe, --opt-level=dev-fast, --no-opt),
#      assembled, linked,
#      RUN, and the exit code resp. the standard output is checked against the
#      expectation in line 1 (// expect_exit: N  resp.  // expect_out: TEXT).
#   4. Every program in tests/neg/ has to stop with an exit code != 0 and print
#      the expected message with a line:column (// expect_error: L:C TEXT).
#      A Rust panic counts as a failure.
#   5. Proof of the optimiser (test_opt.sh: FIR before/after).
#   6. Proof of the result-location guarantee (tools/result_location/run.sh:
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
#      tools/tokenizer/minquota.txt (tools/tokenizer/run.sh).
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
#  63. THE PAGE COMES ALIVE (tools/liveb4/run.sh, round B4): the DOM
#      bound into the JavaScript engine, `<script>` really executed with
#      the ordering rules of `async` and `defer`, a per-node dirty mark
#      that narrows the recomputation after a change -- checked against a
#      FULL layout box for box -- and an HTTP/1.1 client in Firn measured
#      against Python's own `http.server` over a real socket. Measured
#      with the OFFICIAL Web Platform Tests of the dom/ area through the
#      unmodified `testharness.js`; files whose harness never finished are
#      counted separately and never as passes. TLS is refused by name.
#      Numbers and limits in docs/ROUNDB4.md.
#  55. A TCP/IP STACK OF ITS OWN (tools/k3net/run.sh, round K3): Ethernet,
#      ARP, IPv4 with checksum and fragment refusal, ICMP, UDP and TCP with
#      all eleven states -- written in Firn, without an allocator, and
#      measured against the LINUX kernel over a veth pair: `ping` answered,
#      `nc` pushing a megaoctet in and back with the same md5, `curl`
#      fetching an HTTP answer, the stack connecting ACTIVELY to a python
#      server, and `tc netem` dropping 5 % in both directions. With the
#      counter-checks: a wrong checksum has to be dropped, an
#      acknowledgement for octets never sent has to be refused, and
#      WITHOUT retransmission the transfer under loss has to stay
#      incomplete.
#  36. Sockets against the OUTSIDE (tools/net/run.sh, round 76): `nc`
#      pushes 1 MiB through an echo server written in Firn and the checksums
#      are compared, `curl` fetches an HTTP answer, sixteen connections run
#      at the same time -- plus the counter-checks (a closed port refuses, a
#      killed server does not leave the client hanging).
#  37. NBT against Notch's reference file (tools/nbt/run.sh, round 76):
#      `bigtest.nbt` is rebuilt out of lib/std/nbt.fi and compared OCTET FOR
#      OCTET, and a second parser in Python reads the same files field for
#      field, in both directions.
#  38. A Minecraft client really gets into the world (tools/mcserver/run.sh,
#      round 76): server list ping, the whole login through to Join Game,
#      the same login dribbled out ONE OCTET PER WRITE, sixteen logins at
#      the same time -- and, if node is there, node-minecraft-protocol as a
#      third implementation nobody here wrote.
#  54. ACCEPTANCE items 6 and 2 (round 95): the build step that reads the
#      Unicode Character Database and produces generated/unicode_tables.fi
#      out of it -- reproducible octet for octet, and checked against an
#      independent parser over all 1,114,112 code points -- and the
#      endurance run of the collector with CHANGING object sizes, with the
#      counter-check that has to grow (tools/ucd/build.sh,
#      tools/gc_soak/run.sh).
#  41. The standard library of round 81 (tools/stdlib81/run.sh): the hash
#      and the octet keys of the map (a million entries, the longest probe
#      chain MEASURED, an endurance run with a counter-check that must
#      grow), DEFLATE/zlib/gzip in BOTH directions against python3 zlib,
#      gzip and the gunzip binary, JSON against JSONTestSuite and
#      python3 -m json.tool, and lib/std/crypto against 1,919 NIST CAVP
#      vectors, the openssl binary and python3 hashlib. Three build stages.
#  18. Package and project system (tools/packages/run.sh): manifest, search
#      order, visibility, build driver -- in BOTH compilers.
#  19. Freestanding compilation (tools/freestanding/run.sh, round 52):
#      `profile kernel`, inline assembly, MMIO, `#[interrupt]` -- the
#      kernel example becomes an ELF object file WITHOUT undefined
#      symbols, in BOTH compilers, and is linked against a linker script.
#  31. `str` does not leak (tools/strsoak/run.sh, round 70): an endurance
#      run with many short lived concatenations, the RSS of the process
#      measured, plus the counter-check with the collector switched off --
#      in BOTH compilers.
#  30. std.core in a kernel (tools/core/run.sh, round 73): the half of the
#      library that needs neither an allocator nor a system call became a
#      module of its own; `demos/freestanding/kcore.fi` imports it, compiles in
#      the kernel profile to a freestanding ELF object and boots in QEMU.
#      With counter-checks: what still allocates stays forbidden, and a
#      module that only CLAIMS the kernel profile is caught.
#  27. Function values in a STRUCT FIELD (tools/fnfield/run.sh, round 68):
#      `c.hook(a, b)` is exactly ONE `call rax`, a direct call stays a
#      direct `call`, and a METHOD of the same name still wins and stays
#      direct -- in both compilers and with counter-checks.
#  23. The formatter firnfmt (tools/fmt/run.sh, round 64): the whole tree
#      gets formatted, the token stream and the syntax tree stay unchanged,
#      a second run changes nothing, and the shape does not depend on
#      blanks (random test).
#  49. Phi nodes (tools/phi/run.sh, ROUND 92): a loop counter is written on
#      every pass, and until round 92 that meant it stayed in MEMORY -- FIR
#      had no phi nodes, so `mem2reg` could only resolve cells written once.
#      Checked here: the counter really is a phi and its function has no
#      alloca/load/store left; the x86 code did not get LONGER (the copy per
#      back edge has to be folded into the instruction that computes the
#      value, or the round is a regression); aarch64, which has no register
#      allocation to rescue the counter, loses two thirds of the memory
#      accesses in that loop; and three variables rotating in a circle come
#      out right, which is the parallel copy of `phi.rs` doing its job.
#  25. The language server (tools/lsp/run.sh, round 64): `firnc --lsp`
#      speaks the Language Server Protocol; a real client checks
#      diagnostics, definition, hover, completion, rename and formatting.
#  24. Debug information (tools/dwarf/run.sh, round 64): `.debug_info`
#      written by the compiler itself, `gdb` driven in batch mode over two
#      translated Firn programs -- breakpoints, backtrace, `print` of
#      variables, structs, pointers and arrays, with counter-checks.
#  28. The number reader (tools/lexnum/run.sh, round 65): several thousand
#      floating point literals -- halfway cases, subnormals, eight hundred
#      digits -- read by firnc0, firnc1, C strtod and Python. Compared is
#      the BIT PATTERN, and it has to be the same four times.
#  29. The comfort layer of the standard library (tools/strlib/comfort/run.sh,
#      round 69): demos/number_check.fi really runs -- four inputs, the
#      whole output compared -- and the new input layer does not leak:
#      hundreds of thousands of lines through `io.read_text()` with a flat
#      RSS, plus the deliberately leaking counter-check that MUST strike.
#  34. The features of round 66 (tools/js/round66.sh): generators,
#      async/await with the job queue, the private class elements and the
#      endurance run for the new objects -- per feature against test262,
#      with the limits in tools/js/minquota_r66.txt.
#  35. The features of round 74 (tools/js/round74.sh): the long tail of the
#      built in objects, the PATTERN ENGINE (compared against node
#      character for character) and the endurance run for the objects the
#      round adds -- per group against test262, with the limits in
#      tools/js/minquota_r74.txt.
#  40. A pointer into a local cannot leave its frame (tools/escape/run.sh,
#      round 79): the escape analysis of `compiler/src/escape.rs` and
#      `lib/firnc1/escape.fi` against 22 programs that have to be REFUSED and
#      14 counter-checks that have to keep building -- in both compilers, with
#      the whole message compared character for character.
#  43. THE SECOND MACHINE (tools/aarch64/, round 80): the same Firn program
#      compiled for x86-64 AND for aarch64, both RUN, and the standard
#      output compared character for character (tools/aarch64/run.sh, in
#      both build stages). On top of that the object file itself:
#      EM_AARCH64 with an x86 counter-check, the four promised relocation
#      types, the disassembly, and the calling convention against
#      aarch64-linux-gnu-gcc in both directions past the end of the
#      register file (tools/aarch64/machine.sh). What aarch64 cannot do is
#      counted and named, not filtered out.
#  65. FREESTANDING TARGETS (tools/freestanding/none.sh, round
#      ARM-FREESTANDING): `--target=x86_64-none` and `--target=aarch64-none`
#      -- no operating system underneath. The x86 build has to stay OCTET
#      IDENTICAL to the plain `profile kernel` build, and both images have
#      to BOOT in QEMU and say something over the serial line.
#  66. The two system call tables (tools/aarch64/syscall_table.sh, round
#      ARM-FREESTANDING): `compiler/src/syscalls.rs` against
#      `lib/firnc1/syscalls.fi`, the latter read out of a running program
#      built by both compilers.
#  44. Checked integer arithmetic (tools/checked/run.sh, round 72): a
#      program that goes out of range ABORTS in dev/dev-fast/release-safe
#      and WRAPS in release-fast -- in BOTH compilers, with the same
#      message octet for octet (file, line, column, operator). Plus the
#      explicit `+% -% *%` / `+| -| *|`, which are never checked in any
#      level, and two counter-checks: what stays in range behaves exactly
#      as it always did, and a program without a checked operation carries
#      neither the message table nor the trampoline.
#  46. THE FOUR BUILD LEVELS AGREE (tools/optlevels/run.sh, round 90):
#      fourteen programs through BOTH compilers in all four levels, and
#      every level has to produce the same exit code and the same output --
#      the question that would have caught round 90's wrong-code bug
#      (`mul` writes rdx, the allocator did not know) on the day it was
#      merged. With three counter-checks: an overflowing program MUST
#      differ between release-fast and the checked levels, and the exact
#      crossing analysis has to agree with the coarse one
#      (`FIRN_RA_ROUGH=1`).
#  45. THE FIRST FIVE MINUTES (tools/firstrun/run.sh, round 88): eight
#      programs of the kind a stranger writes before he has read anything --
#      join text, compare, print, take pieces out, put a number into a
#      sentence, read a file, forty thousand joins as a real load on the
#      collector, and `string`/`str` as ONE type. Not one of them says a
#      word about a collector; each has to COMPILE, RUN and print exactly
#      its `.out` file, with the optimizer, without it, and through firnc1.
#      Four counter-checks: a program without text gets NO setup in
#      `_start`, the joining one gets it EXACTLY once, `profile kernel` gets
#      neither an entry point nor a collector, and both spellings of the
#      text type pull in the same runtime and name the same canonical type
#      in an error message.
#  53. THE TEST RUNNER AND THE LINE TABLE (round 94,
#      tools/testrunner/run.sh): `#[test]` functions found by the compiler,
#      run one process per case, reported as JSON and as TAP with name,
#      outcome, duration and file:line:column. Counter-checks: the case
#      after a crashing one is still in the report (without isolation it
#      would not be), a file without a test case is refused, a file with a
#      `main` of its own is refused, a wrong signature is refused, and the
#      same file is core language for BOTH compilers. Section 25
#      (tools/dwarf/run.sh) grew the second half of the round: the line
#      table is held against the panic message the program prints itself.
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

# ROUND 90: `safe` is the fourth mode, and it is not decoration. The bug of
# that round (regalloc.rs::inst_clobbers) made 117 of these programs fail
# under `--opt-level=release-safe` and 25 under `--opt-level=dev-fast` --
# the DEFAULT level -- and this suite did not notice, because it never ran
# `release-safe` at all and had been green the last time somebody ran it
# before the round that broke it. A build level nobody runs is a build level
# nobody can ship.
run_case() {          # $1 = file, $2 = "opt" | "noopt" | "devfast" | "safe"
    local file="$1" mode="$2"
    local base ext bin flags hdr exp out rc
    base=$(basename "$file" .fi)
    # ROUND 72: a handful of programs exist SPECIFICALLY to prove that
    # `--opt-level=release-fast` wraps/truncates instead of checking (SPEC
    # section 13, `L9`) -- by construction they cast a value that DOES go
    # out of range, which is exactly what dev/dev-fast/release-safe now
    # correctly panic on. `// only_mode: opt` on line 2 opts a program out
    # of the other two modes instead of hiding the truncation it exists
    # to demonstrate behind some other operator.
    if [ "$mode" != "opt" ] && head -5 "$file" | grep -q '^// only_mode: opt$'; then
        return
    fi
    bin="$WORK/${base}.${mode}"
    # ROUND 72: `mode=opt` used to mean "no flag at all", which quietly
    # meant `release-fast` ONLY as long as `OptConfig::default()` itself
    # happened to be `release-fast` -- the very default this round found
    # wrong and fixed to `dev-fast` (DESIGN_GOALS.md line 554, `opt.rs`).
    # After that fix "no flag" stopped exercising `release-fast` at all,
    # silently leaving this suite's own header comment (line 8: "--opt-
    # level=release-fast, --no-opt and --opt-level=dev-fast") untrue and
    # `release-fast`'s unchecked-arithmetic promise (SPEC section 13, `L9`)
    # completely untested by `mode=opt` -- made explicit here instead of
    # relying on whatever the CLI default happens to be today.
    flags="--opt-level=release-fast"
    [ "$mode" = "noopt" ]   && flags="--no-opt"
    [ "$mode" = "devfast" ] && flags="--opt-level=dev-fast"
    [ "$mode" = "safe" ]    && flags="--opt-level=release-safe"

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

echo "== 3. positive tests (in every build level) =="
PROGS=$(ls tests/*.fi tests/opt/*.fi examples/*.fi)
NPROG=0
for f in $PROGS; do
    NPROG=$((NPROG + 1))
    printf '  %-40s' "$f"
    run_case "$f" opt
    run_case "$f" noopt
    run_case "$f" devfast
    run_case "$f" safe
    echo "  [opt+noopt+devfast+safe]"
done
echo "   $NPROG programs x 4 runs (release-fast / no-opt / dev-fast / release-safe)"

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
bash tools/result_location/run.sh > "$WORK/result_location.log" 2>&1 && EORC=0 || EORC=$?
if [ "$EORC" -eq 0 ]; then
    ok
    tail -1 "$WORK/result_location.log" | sed 's/^/   /'
else
    bad "tools/result_location/run.sh failed (see .test-work/result_location.log)"
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

echo "== 28. the number reader: four readers, one bit pattern (tools/lexnum/run.sh, ROUND 65/71) =="
# ROUND 71: the same exercise for `f32`, with two references of its own --
# C `strtof` and an exact reference in Decimal/Fraction. It was this
# measurement that found the real error of the round: reading a decimal as a
# correctly rounded binary64 and narrowing it afterwards is NOT correctly
# rounded (63568 of 239064 middle cases came out one ulp wrong).
# The corpus of the project contains a few dozen floating point literals, all
# of them harmless. That is why the divergence at `9007199254740991.0` only
# came to light in round 63 -- BY ACCIDENT. Here several thousand literals
# that are MEANT to hurt are read by four readers: the lexer in Rust, the
# lexer in Firn, C `strtod` and Python. Compared is the bit pattern.
bash tools/lexnum/run.sh > "$WORK/lexnum.log" 2>&1 && LNRC=0 || LNRC=$?
if [ "$LNRC" -eq 0 ]; then
    ok
    grep -E '^   (float literals|integer literals|firnc0 vs|refused literals)|^OK:' "$WORK/lexnum.log" | sed 's/^/   /'
else
    bad "tools/lexnum/run.sh failed (see .test-work/lexnum.log)"
    tail -20 "$WORK/lexnum.log" | sed 's/^/   /'
fi

echo "== 31. str does not leak: endurance run with a counter-check (tools/strsoak/run.sh, ROUND 70) =="
# `a + b` on `str` allocates in the GC heap. The endurance run builds many
# short lived strings and measures the REAL memory of the process (RSS out of
# /proc/self/statm). The counter-check with the collection threshold set to
# infinity runs EVERY TIME -- if that one stays flat too, the measuring method
# is broken and the green result is worthless.
bash tools/strsoak/run.sh > "$WORK/strsoak.log" 2>&1 && SSRC=0 || SSRC=$?
if [ "$SSRC" -eq 0 ]; then
    ok
    grep -E '^  (firnc0|firnc1)|^STRSOAK' "$WORK/strsoak.log" | sed 's/^/   /'
else
    bad "tools/strsoak/run.sh failed (see .test-work/strsoak.log)"
    tail -20 "$WORK/strsoak.log" | sed 's/^/   /'
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

echo "== 27. function values in a struct field (tools/fnfield/run.sh, ROUND 68) =="
# Round 68 (docs/ROUND68.md). `c.hook(a, b)` may now be written directly.
# What that costs is measured on the emitted code: exactly one `call rax`
# per field call, a direct call stays direct, a method of the same name
# wins and stays direct -- in both compilers and with counter-checks.
bash tools/fnfield/run.sh > "$WORK/fnfield.log" 2>&1 && FFRC=0 || FFRC=$?
if [ "$FFRC" -eq 0 ]; then
    ok
    tail -1 "$WORK/fnfield.log" | sed 's/^/   /'
else
    bad "tools/fnfield/run.sh failed (see .test-work/fnfield.log)"
    grep FAIL "$WORK/fnfield.log" | head -10 | sed 's/^/   /'
fi
echo "== 23. layout: from the computed style to the box with coordinates (tools/layout/run.sh) =="
# Rounds 61 and 67. The box model with margin collapsing, the block flow,
# the inline flow with line boxes, floats and `clear`, position
# relative/absolute/fixed/sticky, the paint order with `z-index`, and the
# full flexbox of css-flexbox-1. THREE proofs, and none of them replaces
# another: the box tree against the frozen expectation (text against
# text), the SAME cases box against box out of getBoundingClientRect(),
# and the PAINT ORDER against document.elementFromPoint. Plus a soak run
# with a counter check.
#
# ROUND 78: the two browser comparisons run against the FROZEN measurement
# in tools/layout/reference/*.json -- Chromium was asked once, its answer
# is in the repository, and this section starts no foreign program and
# opens no socket. `bash tools/layout/run.sh --live-chromium` still asks a
# live browser, and `--refresh-reference` rewrites the frozen files; both
# are for a person at a keyboard and must never be called from here.
bash tools/layout/run.sh --fast > "$WORK/layout.log" 2>&1 && LYRC=0 || LYRC=$?
if [ "$LYRC" -eq 0 ]; then
    ok
    tail -1 "$WORK/layout.log" | sed 's/^/   /'
else
    bad "tools/layout/run.sh failed (see .test-work/layout.log)"
    grep -E 'FAILED|ERROR' "$WORK/layout.log" | head -10 | sed 's/^/   /'
    tail -5 "$WORK/layout.log" | sed 's/^/   /'
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

echo "== 24. the formatter: canonical shape (tools/fmt/run.sh, ROUND 64) =="
# firnfmt, written in Firn. Proven is: the token stream and the syntax tree
# stay unchanged over the WHOLE tree, a second run changes nothing, the
# shape does not depend on blanks (random test), and the tree in the
# repository IS in canonical shape. The short version; the full run is in
# docs/ROUND64.md.
bash tools/fmt/run.sh --fast > "$WORK/fmt.log" 2>&1 && FMRC=0 || FMRC=$?
if [ "$FMRC" -eq 0 ]; then
    ok
    grep -E '^   (files formatted|token stream|syntax tree|second run)' "$WORK/fmt.log" | sed 's/^/   /'
else
    bad "tools/fmt/run.sh failed (see .test-work/fmt.log)"
    grep FAIL "$WORK/fmt.log" | head -10 | sed 's/^/   /'
fi

echo "== 25. debug information: gdb in a Firn program (tools/dwarf/run.sh, ROUND 64) =="
# `.debug_info` written by the compiler itself: functions, parameters, local
# variables with types. `gdb` is driven in batch mode and its output held
# against expectations -- breakpoint, backtrace, `info args`, `print` of a
# struct, of a pointer and of an array. With counter-checks: WITH the
# optimizer there must be no variable information.
bash tools/dwarf/run.sh > "$WORK/dwarf.log" 2>&1 && DWRC=0 || DWRC=$?
if [ "$DWRC" -eq 0 ]; then
    ok
    tail -1 "$WORK/dwarf.log" | sed 's/^/   /'
else
    bad "tools/dwarf/run.sh failed (see .test-work/dwarf.log)"
    grep FAIL "$WORK/dwarf.log" | head -10 | sed 's/^/   /'
fi

echo "== 26. the language server: firnc --lsp (tools/lsp/run.sh, ROUND 64) =="
# The Language Server Protocol over standard input/output, on the same
# lexer, parser and type checker the compiler uses. tools/lsp/client.py is
# a real client and holds the answers against expectations: diagnostics
# with the suggestions, definition, hover, completion, rename, formatting --
# with counter-checks.
bash tools/lsp/run.sh > "$WORK/lsp.log" 2>&1 && LSRC=0 || LSRC=$?
if [ "$LSRC" -eq 0 ]; then
    ok
    tail -1 "$WORK/lsp.log" | sed 's/^/   /'
else
    bad "tools/lsp/run.sh failed (see .test-work/lsp.log)"
    grep FAIL "$WORK/lsp.log" | head -10 | sed 's/^/   /'
fi

echo "== 29. the comfort layer: demo + input soak (tools/strlib/comfort/run.sh, ROUND 69) =="
# Two things, both with a counter-check: demos/number_check.fi really runs
# (four inputs, the whole output compared -- it reads from standard input
# and can therefore not lie in tests/), and the new input layer does not
# leak. The soak run reads lines with `io.read_line()` and releases every
# `Text`; RSS has to stay flat. The counter-check is the same program with
# the `free` left out -- its RSS HAS to climb, otherwise the measurement is
# broken. The short version; the long run is in docs/ROUND69.md.
COMFORT_LINES=${COMFORT_LINES:-60000} COMFORT_LEAK_LINES=${COMFORT_LEAK_LINES:-20000} \
  bash tools/strlib/comfort/run.sh > "$WORK/comfort.log" 2>&1 && CFRC=0 || CFRC=$?
if [ "$CFRC" -eq 0 ]; then
    ok
    grep -E '^   (demo|soak|counter-check)' "$WORK/comfort.log" | sed 's/^/   /'
else
    bad "tools/strlib/comfort/run.sh failed (see .test-work/comfort.log)"
    grep FAIL "$WORK/comfort.log" | head -10 | sed 's/^/   /'
fi

echo "== 30. std.core in a kernel: the library without an allocator (tools/core/run.sh, ROUND 73) =="
# Round 52 forbade `import std.*` under `profile kernel` wholesale. Round 73
# makes the ban precise: the half of the library that needs neither an
# allocator nor a system call moved into lib/std/core.fi, and THAT module is
# admitted -- because it declares `profile kernel` itself and lands in the
# same compilation unit, where the claim gets checked.
# Proven here, with both compilers: demos/freestanding/kcore.fi says
# `import std.core`, becomes an ELF object WITHOUT an undefined name and
# WITHOUT a syscall instruction, boots in QEMU and reports over the serial
# line what it searched, split, read and allocated. With counter-checks:
# std.io/str/vec/rc stay refused, a module that CLAIMS the kernel profile
# and allocates all the same is refused, and the arena keeps RSS flat over
# 40 000 rounds while the leaking counter-check HAS to climb.
CORE_ROUNDS=${CORE_ROUNDS:-40000} CORE_LEAK_ROUNDS=${CORE_LEAK_ROUNDS:-20000} \
  bash tools/core/run.sh > "$WORK/core.log" 2>&1 && CORERC=0 || CORERC=$?
if [ "$CORERC" -eq 0 ]; then
    ok
    tail -1 "$WORK/core.log" | sed 's/^/   /'
    grep -E '^  OK    (soak|counter-check|firnc0: .core\.ok)' "$WORK/core.log" | sed 's/^/   /'
else
    bad "tools/core/run.sh failed (see .test-work/core.log)"
    grep FAIL "$WORK/core.log" | head -10 | sed 's/^/   /'
fi

echo "== 32. the calling convention against GCC (tools/abi/run.sh, ROUND 71) =="
# Up to round 70 an `f64` travelled as a bit pattern in an INTEGER register.
# Within Firn that was consistent and correct; SPEC 14.1.f64 named it as
# deviation F2. With `f32` the debt came due: whoever reads a WAV or a glTF
# talks to code that somebody else translated, and that code follows System
# V AMD64.
# "We follow System V now" is a claim; this is the measurement. Firn objects
# are linked into a C program translated by GCC and called in BOTH
# directions -- GCC calls Firn, and Firn calls GCC (the stubs are weakened
# with `objcopy`, so the strong definitions of the C side win).
bash tools/abi/run.sh > "$WORK/abi.log" 2>&1 && ABIRC=0 || ABIRC=$?
if [ "$ABIRC" -eq 0 ]; then
    ok
    grep -E '^abi:|^RESULT' "$WORK/abi.log" | sed 's/^/   /'
else
    bad "tools/abi/run.sh failed (see .test-work/abi.log)"
    grep -E 'ERROR|FAIL' "$WORK/abi.log" | head -10 | sed 's/^/   /'
fi

echo "== 33. f32 against real data: WAV and glTF (tools/f32data/run.sh, ROUND 71) =="
# A test in which a program writes a number and reads it back proves nothing
# about a file format -- it proves that the program agrees with itself.
# Here the octets come from outside: a WAV with 32-bit float PCM and a
# binary glTF, produced by tools/f32data/gen.py, read by BOTH compilers, and
# every value held against what Python reads out of the very same octets.
bash tools/f32data/run.sh > "$WORK/f32data.log" 2>&1 && F32RC=0 || F32RC=$?
if [ "$F32RC" -eq 0 ]; then
    ok
    grep -E 'identical|^OK' "$WORK/f32data.log" | sed 's/^/   /'
else
    bad "tools/f32data/run.sh failed (see .test-work/f32data.log)"
    grep -E 'DIFFERENT|FAIL' "$WORK/f32data.log" | head -10 | sed 's/^/   /'
fi

echo "== 34. the features of round 66: generators, async, classes (tools/js/round66.sh) =="
# Section 9d measures the JavaScript path as a whole; this one measures the
# four groups of round 66 SEPARATELY, so that a regression in one of them
# cannot hide behind the total -- plus the endurance run that shows that a
# generator abandoned in the middle of its body, a promise and a BigInt are
# ordinary objects of the collector. Nothing is filtered.
bash tools/js/round66.sh --fast > "$WORK/round66.log" 2>&1 && R66RC=0 || R66RC=$?
if [ "$R66RC" -eq 0 ]; then
    ok
    grep -E '^   (generators|async|classes|gen |genleak|jobs )' "$WORK/round66.log" | sed 's/^/   /'
else
    bad "tools/js/round66.sh failed (see .test-work/round66.log)"
    grep -E 'FAILED|BELOW' "$WORK/round66.log" | head -10 | sed 's/^/   /'
fi

echo "== 35. the features of round 74: built ins, regular expressions, dates (tools/js/round74.sh) =="
# Section 9d measures the JavaScript path as a whole, section 34 measures
# round 66; this one measures the groups of round 74 SEPARATELY. It holds
# the PATTERN ENGINE against node character for character -- test262 says
# whether a case passes, it does not say whether two engines agree on the
# captures a pattern produces, and that is exactly where a backtracking
# matcher goes wrong quietly. Plus the endurance run that shows that a
# compiled pattern, an iterator in mid-flight, a Date and the weak
# collections are ordinary objects of the collector. Nothing is filtered.
bash tools/js/round74.sh --fast > "$WORK/round74.log" 2>&1 && R74RC=0 || R74RC=$?
if [ "$R74RC" -eq 0 ]; then
    ok
    grep -E '^   (builtins|text|re_0|clean |leak )' "$WORK/round74.log" | sed 's/^/   /'
else
    bad "tools/js/round74.sh failed (see .test-work/round74.log)"
    grep -E 'FAILED|BELOW|DIFFERENT' "$WORK/round74.log" | head -10 | sed 's/^/   /'
fi

echo "== 36. sockets against the outside: nc, curl, sixteen at once (tools/net/run.sh, ROUND 76) =="
# `tests/1600_net_echo.fi` in section 3 pushes 1 MiB between a server thread
# and a client IN THE SAME PROCESS. That is necessary and not enough: both
# ends are this repository, and two ends that misunderstand the same thing
# agree perfectly. Here the other end is somebody else's -- netcat is from
# 1996 and curl checks a status line, headers and Content-Length and says so
# when they are wrong. Plus the throughput as a NUMBER and the
# counter-checks: a port on which nothing listens has to refuse, and a
# server killed mid transfer must not leave its client hanging.
NET_MB=${NET_MB:-1} bash tools/net/run.sh > "$WORK/net.log" 2>&1 && NETRC=0 || NETRC=$?
if [ "$NETRC" -eq 0 ]; then
    ok
    grep -E '^  (release-fast|no-opt|dev-fast):' "$WORK/net.log" | sed 's/^/ /'
else
    bad "tools/net/run.sh failed (see .test-work/net.log)"
    grep -E 'FAIL|RESULT' "$WORK/net.log" | head -10 | sed 's/^/   /'
fi

echo "== 37. NBT against Notch's reference file (tools/nbt/run.sh, ROUND 76) =="
# `bigtest.nbt` is the example of the NBT specification. `tools/nbt/bigtest.fi`
# rebuilds it out of `lib/std/nbt.fi`, and the first 1,543 octets have to be
# IDENTICAL to the published file -- not "parses the same", identical. That
# cannot be satisfied by a reader and a writer that are wrong in the same
# way. On top of it a second parser in Python turns the same files into the
# same canonical text, in BOTH directions, and the counter-checks (truncated,
# tag 13, negative length, 256 levels of nesting) all have to be refused.
bash tools/nbt/run.sh > "$WORK/nbt.log" 2>&1 && NBTRC=0 || NBTRC=$?
if [ "$NBTRC" -eq 0 ]; then
    ok
    grep -E '^  (reference|release-fast|no-opt|dev-fast)' "$WORK/nbt.log" | sed 's/^/ /'
else
    bad "tools/nbt/run.sh failed (see .test-work/nbt.log)"
    grep -E 'FAIL|RESULT' "$WORK/nbt.log" | head -10 | sed 's/^/   /'
fi

echo "== 38. a Minecraft client gets into the world (tools/mcserver/run.sh, ROUND 76) =="
# `demos/mcserver` speaks protocol 765 (1.20.4) in offline mode. Three
# clients check it and none of them takes its word for anything:
# `tools/mcserver/harness.py` (own VarInt reader, own framing, own NBT
# parser -- and it logs into the VANILLA server too, so a failure here is
# this server's fault), the same harness with ONE OCTET PER WRITE, and
# node-minecraft-protocol, which validates every field against
# `minecraft-data` and throws when something does not fit. The UUID for
# 'Notch' has to be the one the real vanilla server derives.
# MC_FAST=1 runs only the optimised build stage.
MC_FAST=${MC_FAST:-0} bash tools/mcserver/run.sh > "$WORK/mcserver.log" 2>&1 && MCRC=0 || MCRC=$?
if [ "$MCRC" -eq 0 ]; then
    ok
    grep -E '^  [a-z-]+: +(ping: version|OK |the UUID|dribbled|nmp: login|OK nmp|flood:|play: chunk verified|config: Registry|SKIPPED|ping :|login:  *[0-9]|soak:|counter-check )' \
        "$WORK/mcserver.log" | sed 's/^/ /'
else
    bad "tools/mcserver/run.sh failed (see .test-work/mcserver.log)"
    grep -E 'FAIL|RESULT' "$WORK/mcserver.log" | head -10 | sed 's/^/   /'
fi

echo "== 39. foreign functions in both directions (tools/extfn/run.sh, ROUND 75) =="
# Round 75 built `extern fn` and a proof script for it, and then did not
# hang the proof into this suite -- so a later round could have broken the
# C ABI without anything going red. It is hung in here now. Six cases,
# both compilers (firnc0 and firnc1): Firn calls a C function
# (`callout`), Firn calls a symbol under a different name
# (`#[link_name]`), and C calls back into Firn through a function address
# it was handed (`#[export_c]` plus a callback).
bash tools/extfn/run.sh > "$WORK/extfn.log" 2>&1 && EXTRC=0 || EXTRC=$?
if [ "$EXTRC" -eq 0 ]; then
    ok
    grep -E '^(PASS|FAIL):? ' "$WORK/extfn.log" | sed 's/^/   /'
else
    bad "tools/extfn/run.sh failed (see .test-work/extfn.log)"
    grep -E 'FAIL' "$WORK/extfn.log" | head -10 | sed 's/^/   /'
fi

echo "== 40. a pointer into a local cannot leave its frame (tools/escape/run.sh, ROUND 79) =="
# Round 66 found the gap while writing a JavaScript engine: a raw pointer into
# a LOCAL survived its frame and the compiler said nothing. Round 79 makes it
# an error at compile time -- in BOTH compilers, with the same text.
# 36 cases: 22 programs in which the address really gets out (return, an out
# parameter, a struct that is returned, a callee that keeps it, a thread) and
# 14 COUNTER-CHECKS -- correct programs with pointers that have to keep
# building. The counter-checks are the half that matters more: a checker that
# refuses everything catches every error. Every false alarm counts as a
# failure. The whole message block of firnc0 and firnc1 is compared with `cmp`.
bash tools/escape/run.sh > "$WORK/escape.log" 2>&1 && ESCRC=0 || ESCRC=$?
if [ "$ESCRC" -eq 0 ]; then
    ok
    grep -E '^  (cases|messages identical):|^PASS:' "$WORK/escape.log" | sed 's/^/ /'
else
    bad "tools/escape/run.sh failed (see .test-work/escape.log)"
    grep -E '^FAIL' "$WORK/escape.log" | head -10 | sed 's/^/   /'
fi

echo "== 41. the standard library of round 81 (tools/stdlib81/run.sh) =="
# FOUR AREAS, none of them judged by this repository:
#   * HASH AND MAP: xxHash64 against the author's own implementation
#     (python-xxhash) and FNV-1a against its published vectors; then a
#     MILLION entries with string keys -- time, memory and the LONGEST
#     PROBE CHAIN, because open addressing degenerates silently. Plus an
#     endurance run (1.2 M insert+delete, RSS flat) WITH the counter-check
#     that leaves the deletions out and MUST grow.
#   * DEFLATE: everything Firn packs is unpacked by python3 zlib, by gzip
#     and by the gunzip binary, everything they pack is unpacked by Firn,
#     over empty input, one octet, incompressible data, one repeated octet
#     and real files -- plus four broken streams that have to be REFUSED.
#   * JSON: JSONTestSuite (testdata/json/) -- every y_ accepted, every n_
#     refused -- and the output against python3 -m json.tool.
#   * CRYPTO: 1,919 NIST CAVP vectors (testdata/crypto/), the FIPS 197
#     known answer test, multi block CBC/CFB8 against openssl (the KAT
#     files are single block and do not test chaining at all) and python3
#     hashlib/hmac over random data.
# All of it in THREE build stages. STDLIB81_FAST=1 runs the optimised one.
bash tools/stdlib81/run.sh > "$WORK/stdlib81.log" 2>&1 && STDRC=0 || STDRC=$?
if [ "$STDRC" -eq 0 ]; then
    ok
    grep -E '^  (FNV-1a|hash vectors|release-fast [0-9]|probe chain|soak |counter-check |level [0-9]|y_ |n_ |i_ |json.tool|json.load|error position|python/openssl|getrandom|testdata/|sha1 |sha256 |aes |cfb8 )' \
        "$WORK/stdlib81.log" | sed 's/^/ /'
    grep -E '^NIST TOTAL' "$WORK/stdlib81.log" | head -1 | sed 's/^/   /'
    grep -E '^  RESULT ok \(' "$WORK/stdlib81.log" | head -1 | sed 's/^/   deflate /'
else
    bad "tools/stdlib81/run.sh failed (see .test-work/stdlib81.log)"
    grep -E 'FAIL|RESULT' "$WORK/stdlib81.log" | head -12 | sed 's/^/   /'
fi

echo "== 42. the speed of round 82 (tools/bench82/run.sh) =="
# THREE THINGS IN ONE SECTION, and the first one is not a measurement:
#
#   * BOTH PATHS, THE SAME ANSWER. `lib/std/crypto/accel.fi` is a SECOND
#     implementation of SHA-256 and AES-128, on the processor's own
#     instructions (`sha256rnds2`, `aesenc`). Every length from 0 to 300
#     octets goes through it AND through the scalar path, and the two have to
#     agree octet for octet -- plus the FIPS 197 and FIPS 180-4 known answers,
#     which come from outside this repository. That check runs BEFORE the
#     stopwatch, because a fast cipher that is wrong is worth less than a
#     slow one that is right.
#   * THE THROUGHPUT, against `openssl speed` and `gzip -6` ON THE SAME
#     MACHINE and, for DEFLATE, on literally the same octets. A number
#     without a yardstick next to it says nothing.
#   * THE REGRESSION LIMITS (`tools/bench82/minquota_*.txt`, and one CEILING
#     for the self compile). They sit at roughly half of what was measured:
#     this is a shared virtual machine and the same binary varies by a factor
#     of two depending on the neighbours. That catches a real regression and
#     not the noise.
#
# BENCH82_FULL=1 measures with bigger buffers; what runs here is the fast
# variant.
bash tools/bench82/run.sh > "$WORK/bench82.log" 2>&1 && B82RC=0 || B82RC=$?
if [ "$B82RC" -eq 0 ]; then
    ok
    grep -E '^  (SHA-256|AES-128-CBC|AES-CBC dec|AES-128-CFB8|DEFLATE|inflate|FIPS|processor|total:)' \
        "$WORK/bench82.log" | sed 's/^/ /'
else
    bad "tools/bench82/run.sh failed (see .test-work/bench82.log)"
    grep -E 'FAIL|MISMATCH|BELOW|ABOVE|RESULT' "$WORK/bench82.log" | head -12 | sed 's/^/   /'
fi
echo
echo "== 43. the second machine: aarch64 (tools/aarch64/, ROUND 80) =="
# Round 80 gave the compiler a second target. The proof is not that the
# code generator exists -- it is that the SAME source does the same thing on
# both machines. `run.sh` compiles every case of tests/*.fi twice, runs the
# x86-64 build natively and the aarch64 build under qemu-aarch64, and
# compares the standard output character for character and the exit code.
# Four buckets, all of them counted: SAME, DIFFERENT (the script fails if
# this is not 0), NOT SUPPORTED (the code generator REFUSED the program and
# said why) and ENVIRONMENT (the difference is the runner's -- and a C
# probe has to prove that in the same run, or the case counts as
# DIFFERENT again).
#
# `machine.sh` looks at the object file rather than at the behaviour:
# EM_AARCH64 with the x86 counter-check, R_AARCH64_CALL26 /
# ADR_PREL_PG_HI21 / ADD_ABS_LO12_NC / ABS64, the disassembly, and AAPCS64
# against aarch64-linux-gnu-gcc -- ten integer words and nine floating
# point words, so the stack part of the convention is exercised too.
#
# Without the cross toolchain both scripts say SKIP and the suite stays
# green. A64_FAST=1 runs only the optimised build stage.
bash tools/aarch64/machine.sh > "$WORK/a64_machine.log" 2>&1 && A64MRC=0 || A64MRC=$?
if [ "$A64MRC" -eq 0 ]; then
    ok
    grep -E '^(  ok |  aapcs64|SKIP)' "$WORK/a64_machine.log" | sed 's/^/ /'
else
    bad "tools/aarch64/machine.sh failed (see .test-work/a64_machine.log)"
    grep -E '^  FAIL' "$WORK/a64_machine.log" | head -10 | sed 's/^/   /'
fi

A64_RC=0
for stage in "" "--no-opt"; do
    [ -n "$stage" ] && [ "${A64_FAST:-0}" = "1" ] && continue
    tag=${stage:-opt}
    bash tools/aarch64/run.sh $stage > "$WORK/a64_run.$tag.log" 2>&1 && rc=0 || rc=$?
    if [ "$rc" -eq 0 ]; then
        ok
        grep -E '^(  (build stage|SAME|DIFFERENT|NOT SUPPORTED|ENVIRONMENT|x86 already|RESULT)|SKIP)' \
            "$WORK/a64_run.$tag.log" | sed 's/^/ /'
    else
        A64_RC=1
        bad "tools/aarch64/run.sh $stage failed (see .test-work/a64_run.$tag.log)"
        grep -E '^  DIFF |^FAIL' "$WORK/a64_run.$tag.log" | head -10 | sed 's/^/   /'
    fi
done

echo "== 65. FREESTANDING TARGETS: no operating system underneath (ROUND ARM-FREESTANDING) =="
# `--target=x86_64-none` and `--target=aarch64-none`. The two sharp claims
# are checked and not asserted:
#   * the x86 path does not change -- `--target=x86_64-none` and the plain
#     build of a `profile kernel` source produce the SAME OCTETS;
#   * both images BOOT. qemu-system-x86_64 -kernel and
#     qemu-system-aarch64 -M virt, and the serial output is compared.
# In between: the object file is ET_REL with no undefined name except the
# two the kernel author owes it, `syscall` and `profile app` are refused
# with a message that names the target, and `eret`/`mrs`/`msr` plus the
# interrupt register save are really in the machine code (round 80 could
# produce none of them).
bash tools/freestanding/none.sh > "$WORK/none.log" 2>&1 && NRC=0 || NRC=$?
if [ "$NRC" -eq 0 ]; then
    ok
    grep -E '^(FREESTANDING TARGETS|SKIP)' "$WORK/none.log" | sed 's/^/ /'
else
    bad "tools/freestanding/none.sh failed (see .test-work/none.log)"
    grep -E '^  FAIL' "$WORK/none.log" | head -10 | sed 's/^/   /'
fi

echo "== 66. the two system call tables agree (ROUND ARM-FREESTANDING) =="
# `compiler/src/syscalls.rs` (in use since round 80) and
# `lib/firnc1/syscalls.fi` (new, and without a caller until firnc1 can
# generate A64). The Firn one is read out of a RUNNING program built by both
# compilers, so a table that drifts is caught the day it drifts.
bash tools/aarch64/syscall_table.sh > "$WORK/systab.log" 2>&1 && SRC0=0 || SRC0=$?
if [ "$SRC0" -eq 0 ]; then
    ok
    grep -E '^(SYSCALL TABLES|  \(firnc1)' "$WORK/systab.log" | sed 's/^/ /'
else
    bad "tools/aarch64/syscall_table.sh failed (see .test-work/systab.log)"
    grep -E '^  FAIL' "$WORK/systab.log" | head -10 | sed 's/^/   /'
fi

echo "== 44. checked integer arithmetic (tools/checked/run.sh, ROUND 72) =="
# SPEC section 13, item L9. The number 44 and not 40: round 72 took 40 while
# it stood on an older `main`, and 40 to 43 were handed out in the meantime
# (40 = the escape analysis of round 79, 41 = the library of round 81,
# 42 = the speed of round 82, 43 = the second machine of round 80). Section
# numbers have collided in this project often enough that the next free one
# is worth checking with `grep -n 'echo "== ' test.sh` before it is used.
#
# What this measures cannot be seen anywhere else in the suite: section 3
# runs every test program in three build levels, but a program that PANICS
# is a failure there, never a result. Here the panic IS the result -- and
# the message it prints has to come out of BOTH compilers identically,
# which is the part that made the self-hosting fixpoint hold again.
bash tools/checked/run.sh > "$WORK/checked.log" 2>&1 && CHKRC=0 || CHKRC=$?
if [ "$CHKRC" -eq 0 ]; then
    ok
    grep -E '^(CHECKS|  compiler)' "$WORK/checked.log" | sed 's/^/   /'
else
    bad "tools/checked/run.sh failed (see .test-work/checked.log)"
    grep -E '  FAIL|CHECKS' "$WORK/checked.log" | head -12 | sed 's/^/   /'
fi

echo "== 45. the first five minutes with the language (tools/firstrun/run.sh, ROUND 88) =="
# Every other section here proves that something DIFFICULT works. This one
# proves that the EASY thing works: the programs anybody writes first. Round
# 87 failed four of the seven, for four different reasons -- the collector
# had to be started by hand, `str` reached only half of the methods of
# `Span`, `io.print_line("x")` took no argument, and the message that came
# out of it was German (docs/ROUND88.md).
bash tools/firstrun/run.sh > "$WORK/firstrun.log" 2>&1 && FRRC=0 || FRRC=$?
if [ "$FRRC" -eq 0 ]; then
    ok
    grep -E '^(PASS|  SKIP)' "$WORK/firstrun.log" | sed 's/^/   /'
else
    bad "tools/firstrun/run.sh failed (see .test-work/firstrun.log)"
    grep -E '^  FAIL|^FAIL' "$WORK/firstrun.log" | head -10 | sed 's/^/   /'
fi

echo "== 46. global variables: 'static' and 'static mut' (tools/state/run.sh, ROUND 89) =="
# SPEC 14.1.statics. The one thing the kernel migration hung on: without a
# place that survives the function that wrote it there is no scheduler, no
# page allocator, no interrupt table and no device driver. What is measured
# here is not that a `static` compiles but WHERE it lands -- `readelf` says
# which section the linker really put it in -- that it is ONE place across
# module boundaries, that the kernel profile carries no undefined name of
# ours, and that both machines and both compilers agree.
bash tools/state/run.sh > "$WORK/state.log" 2>&1 && STRC=0 || STRC=$?
if [ "$STRC" -eq 0 ]; then
    ok
    grep -E '^state:|^   SKIPPED' "$WORK/state.log" | sed 's/^/   /'
else
    bad "tools/state/run.sh failed (see .test-work/state.log)"
    grep -E '^  FAIL|^state:' "$WORK/state.log" | head -12 | sed 's/^/   /'
fi

echo "== 47. the checked index, the checked division, the panic handler (tools/checkidx/run.sh, ROUND 89) =="
# SPEC section 13, item L9 -- the same shape as section 44 and for the
# same reason: the ABORT is the result, and a program that panics counts as
# a failure in section 3, never as a measurement. On top of the message
# (which has to come out of both compilers octet for octet), this section
# measures what the promise COSTS: `release-fast` carries no comparison at
# all, and a loop the optimiser can prove is inside loses its check again.
bash tools/checkidx/run.sh > "$WORK/checkidx.log" 2>&1 && CIRC=0 || CIRC=$?
if [ "$CIRC" -eq 0 ]; then
    ok
    grep -E '^checkidx:' "$WORK/checkidx.log" | sed 's/^/   /'
else
    bad "tools/checkidx/run.sh failed (see .test-work/checkidx.log)"
    grep -E '^  FAIL|^checkidx:' "$WORK/checkidx.log" | head -12 | sed 's/^/   /'
fi

echo "== 48. the four build levels agree (tools/optlevels/run.sh, ROUND 90) =="
bash tools/optlevels/run.sh > "$WORK/optlevels.log" 2>&1 && OLRC=0 || OLRC=$?
grep -E '^  (firnc0|firnc1|overflow|exact)' "$WORK/optlevels.log" | sed 's/^/ /'
if [ "$OLRC" -eq 0 ]; then
    ok
else
    bad "tools/optlevels/run.sh failed (see .test-work/optlevels.log)"
    sed 's/^/        /' "$WORK/optlevels.log" | grep FAIL | head -12
fi
echo "== 51. the same package on two machines (tools/repro/two_machines.sh, ROUND 93) =="
# ACCEPTANCE item 5. Not "two directories" like `tools/repro/run.sh` of round
# 48, but a second run that differs in everything a second machine differs
# in: working directory, $HOME, $TMPDIR, $TZ, $LANG, $PATH, umask, the
# clock, the file time stamps, the ORDER the sources were written in, the
# path of the compiler binary -- and, if qemu-x86_64 is installed, the CPU
# implementation. Measured on BOTH compilers, plus the lock file.
bash tools/repro/two_machines.sh > "$WORK/twomachines.log" 2>&1 && TMRC=0 || TMRC=$?
grep -E '^   (sources|binary|assembly|firn\.lock|the program)' "$WORK/twomachines.log" \
    | cut -c1-110 | sed 's/^/ /'
if [ "$TMRC" -eq 0 ]; then
    ok
else
    bad "tools/repro/two_machines.sh failed (see .test-work/twomachines.log)"
    grep -E 'DIFFERENT|FAILED|MISSING|failed' "$WORK/twomachines.log" | head -12 | sed 's/^/   /'
fi

echo "== 52. phi nodes: the loop counter leaves the frame (tools/phi/run.sh, ROUND 92) =="
bash tools/phi/run.sh > "$WORK/phi.log" 2>&1 && PHRC=0 || PHRC=$?
grep -E '^  (@sum_to|without the pass|sum_to|rotate|swap_n|frame|aarch64|four levels|firnc1)' "$WORK/phi.log" | sed 's/^/ /'
if [ "$PHRC" -eq 0 ]; then
    ok
else
    bad "tools/phi/run.sh failed (see .test-work/phi.log)"
    grep -E '^  FAIL|^phi:' "$WORK/phi.log" | head -12 | sed 's/^/   /'
fi

echo "== 53. the test runner and the line table (ROUND 94) =="
# Two things of item 4 of ACCEPTANCE.md, both measured and not claimed:
#   * `firnc --test` finds `#[test]` functions, runs each of them in a
#     process of its own and reports JSON/TAP with name, outcome, duration
#     and file:line:column. A case that crashes does not take the runner
#     with it -- the case after it is in the report.
#   * the LINE TABLE agrees with the program's own panic message, at every
#     build level, even for a function the optimizer embedded in its caller.
#     Before this round an optimized build attributed everything to the line
#     of the `fn` (tools/dwarf/run.sh sections 7 and 8).
bash tools/testrunner/run.sh > "$WORK/testrunner.log" 2>&1 && TRRC=0 || TRRC=$?
if [ "$TRRC" -eq 0 ]; then
    ok
    grep -E '^   (case |total|the case after|firnc1|tools/testrunner)' "$WORK/testrunner.log" | head -8 | sed 's/^/   /'
    tail -1 "$WORK/testrunner.log" | sed 's/^/   /'
else
    bad "tools/testrunner/run.sh failed (see .test-work/testrunner.log)"
    grep FAIL "$WORK/testrunner.log" | head -10 | sed 's/^/   /'
fi

# ROUND K3. Sections 53 and 54 belong to the rounds running in parallel;
# this one was assigned 55 and takes it whether or not those land.
echo "== 55. a TCP/IP stack of its own, measured against Linux (tools/k3net/run.sh, ROUND K3) =="
bash tools/k3net/run.sh > "$WORK/k3net.log" 2>&1 && K3RC=0 || K3RC=$?
grep -E '^        (throughput|rtt|md5|retransmissions|out-of-order|sent |symbols|udp_back|body:)' "$WORK/k3net.log" | sed 's/^ */ /'
grep -E '^K3NET: ' "$WORK/k3net.log" | sed 's/^/ /'
if [ "$K3RC" -eq 0 ]; then
    ok
else
    bad "tools/k3net/run.sh failed (see .test-work/k3net.log)"
    grep -E '^  FAIL' "$WORK/k3net.log" | head -12 | sed 's/^/   /'
fi

echo "== 54. the Unicode table out of the UCD and the endurance run of the collector (ROUND 95) =="
# ACCEPTANCE items 6 and 2.
#
# (a) tools/ucd/build.sh --verify: the build step reads UnicodeData.txt and
#     DerivedCoreProperties.txt (sha256 pinned), the COMPILER parses them in
#     `comptime` blocks, tools/ucd/pack.fi packs the result into the three
#     stage table -- and the outcome has to be octet for octet the file that
#     lies in the repository. That is what keeps round 93's reproducibility
#     intact: a generated file that is not reproducible would break it.
# (b) tools/ucd/probe_tables asks the table about every one of the 1,114,112
#     code points and tools/ucd/verify_tables.py holds the answers against a
#     parser of its own over both UCD files.
# (c) tools/gc_soak/run.sh: the endurance run with CHANGING object sizes,
#     with the counter-check that has to grow, plus the evaluation of the
#     long series and of the rescan A/B in tools/gc_soak/longrun/.
UCDRC=0
bash tools/ucd/build.sh --verify > "$WORK/ucd_build.log" 2>&1 || UCDRC=$?
grep -E '^   (the table at run time|level [123]|case mappings|identical|Unicode version|the generated source)' \
    "$WORK/ucd_build.log" | cut -c1-110 | sed 's/^/ /'
if [ "$UCDRC" -eq 0 ]; then
    FIRNLIB="$(pwd)/lib" compiler/target/release/firnc tools/ucd/probe_tables.fi \
        -o "$WORK/probe_tables" 2>> "$WORK/ucd_build.log" \
        && "$WORK/probe_tables" > "$WORK/ucd_answers.txt" \
        && python3 tools/ucd/verify_tables.py "$WORK/ucd_answers.txt" > "$WORK/ucd_verify.log" 2>&1 \
        || UCDRC=9
    grep -E 'IDENTICAL|DIFFERENT|compared' "$WORK/ucd_verify.log" | cut -c1-110 | sed 's/^/ /'
fi
SOAKRC=0
SOAK_SEC=${TEST_SOAK_SEC:-90} SOAK_LEAK_SEC=${TEST_SOAK_LEAK_SEC:-30} \
    SOAK_SAMPLE_MS=1000 SOAK_MIN_MS=1000000000 \
    bash tools/gc_soak/run.sh > "$WORK/gc_soak.log" 2>&1 || SOAKRC=$?
grep -E '^   (PASSED|FAILED|Counter-check|Overhead)|^     (duration|rounds|heap/live 2nd|pause per window|PASSED|FAILED)|^     throughput' \
    "$WORK/gc_soak.log" | cut -c1-110 | sed 's/^/ /'
if [ "$UCDRC" -eq 0 ] && [ "$SOAKRC" -eq 0 ]; then
    ok
else
    bad "round 95 failed (ucd $UCDRC, soak $SOAKRC -- see .test-work/ucd_build.log, .test-work/gc_soak.log)"
    grep -E 'FAILED|ERROR|error' "$WORK/ucd_build.log" "$WORK/ucd_verify.log" "$WORK/gc_soak.log" 2>/dev/null | head -10 | sed 's/^/   /'
fi
echo "== 59. the official html5lib tests, the DOM and the style tree (tools/domb1/run.sh, ROUND B1) =="
# The number 59 is fixed for this round; 53 to 58 belong to the rounds that
# were running next to it.
bash tools/domb1/run.sh > "$WORK/domb1.log" 2>&1 && B1RC=0 || B1RC=$?
grep -E '^   (tree construction|DOM and style)|^B1: ' "$WORK/domb1.log" | sed 's/^/ /'
if [ "$B1RC" -eq 0 ]; then
    ok
else
    bad "tools/domb1/run.sh failed (see .test-work/domb1.log)"
    grep -E '^   (ERROR|FAILED)|^ *>>' "$WORK/domb1.log" | head -12 | sed 's/^/   /'
fi
echo "== 61. LAYOUT against the official Web Platform Tests (tools/layoutb2/run.sh, ROUND B2) =="
# The number 61 is fixed for this round; 60 belongs to round R96, which was
# running next to it.
#
# Round B1 made a tree with a computed style on every element. This round
# turns that into a tree of RECTANGLES -- and measures it against a suite
# nobody here wrote. The css/ area of the Web Platform Tests is mostly
# reftests, which need a rasteriser this round does not have; but a large
# part of it is SELF-DESCRIBING through `resources/check-layout-th.js`:
# the expected `offsetWidth`, `offsetLeft`, `clientHeight` and their kin
# stand in the markup as `data-expected-*` attributes. That is position
# and size instead of pixels, which is exactly what a layout engine
# produces. 471 such tests lie in tests/data/wpt-css (PROVENANCE.md there);
# the corpus, the three groups counted separately and the method are in
# docs/ROUNDB2.md.
#
# The section also proves the SPLIT that a browser lives on: laying the
# same tree out at 800, then at 400, then at 800 again has to give the
# first layout back, box for box, for every one of the 471 documents.
# Only the intrinsic widths survive a reflow -- they are the one thing the
# window width cannot change.
bash tools/layoutb2/run.sh > "$WORK/layoutb2.log" 2>&1 && B2RC=0 || B2RC=$?
grep -E '^   (b2|vertical|grid|script|all but script|corpus B2|reflow) ' \
    "$WORK/layoutb2.log" | sed 's/^/ /'
grep -E '^REFLOW: |^B2 OK: ' "$WORK/layoutb2.log" | sed 's/^/ /'
if [ "$B2RC" -eq 0 ]; then
    ok
else
    bad "tools/layoutb2/run.sh failed (see .test-work/layoutb2.log)"
    grep -E 'FAILED|Traceback|Error' "$WORK/layoutb2.log" | head -12 | sed 's/^/   /'
    tail -5 "$WORK/layoutb2.log" | sed 's/^/   /'
fi

echo "== 62. PAINTING against the official reference tests (tools/paintb3/run.sh, ROUND B3) =="
# The number 62 is fixed for this round; 60 belongs to round R96 and 61 to
# round B2, both of which were running next to it.
#
# Round B2 turned the tree of styles into a tree of RECTANGLES and could
# say where every box stands. It still had no picture, and it said so:
# most of the css/ area of the Web Platform Tests are REFERENCE TESTS,
# two documents that are rendered and whose PIXELS are compared, and
# nothing but a rasteriser can run them. This round is that rasteriser.
#
# What runs here: a display list in the order of CSS 2.1, Appendix E; a
# scanline rasteriser with exact, analytic anti-aliasing; round corners,
# eight border styles, linear and radial gradients, box and text shadows
# with a real blur, transparency, the separable blend modes and clipping;
# a TrueType reader with composite glyphs and kerning, whose advance
# widths flow BACK into the layout so that the line breaks where the
# letters end; and PNG in both directions. 541 reference pairs lie in
# tests/data/wpt-ref (PROVENANCE.md there), the method and the numbers are
# in docs/ROUNDB3.md.
#
# THE GUARD, and it is the point of the section: an engine that draws
# NOTHING passes every reference test in the world -- both sides come out
# white and white equals white. So a pair counts only if the picture is
# also not empty, the empty matches are printed separately as `vacuous`,
# and every own case carries the number of pixels its glyphs really set.
# That is the lesson of round K7B in the kernel, where a screen was 87 per
# cent correct and every single letter was missing.
bash tools/paintb3/run.sh > "$WORK/paintb3.log" 2>&1 && B3RC=0 || B3RC=$?
grep -E '^   (font |metrics |glyphs drawn|encoder |decoder |corpus |passed |vacuous |failed |reference tests|glyphs: |time/page|1\. ink|2\. shrink|the same with)' \
    "$WORK/paintb3.log" | sed 's/^/ /'
grep -E '^(B3 OK|CASES|PNG|TEXTFIT): ' "$WORK/paintb3.log" | sed 's/^/ /'
if [ "$B3RC" -eq 0 ]; then
    ok
else
    bad "tools/paintb3/run.sh failed (see .test-work/paintb3.log)"
    grep -E 'FAILED|Traceback|Error' "$WORK/paintb3.log" | head -12 | sed 's/^/   /'
    tail -5 "$WORK/paintb3.log" | sed 's/^/   /'
fi

echo "== 63. THE PAGE COMES ALIVE: scripts, invalidation and HTTP (ROUND B4) =="
# The number 63 is fixed for this round. 62 belongs to round B3.
#
# What rounds B1 to B3 built was a pipeline that ran ONCE: markup in,
# picture out. Two things were missing, and they are the two things that
# make a page a page: a script could not touch the tree, and nothing could
# fetch a document out of the network. This round is both.
#
# What runs here: the DOM as JavaScript sees it (`document`,
# `getElementById`, `querySelector`, `createElement`, `appendChild`,
# `textContent`, `innerHTML` through the FRAGMENT PARSING ALGORITHM,
# `setAttribute`, `classList`, `style`, `addEventListener` with the three
# phases of the event flow, `window`, `location`, `setTimeout`); the
# ordering of `<script>`, `<script async>` and `<script defer>`, with the
# external ones really fetched over a socket; a per-node DIRTY MARK that
# narrows the recomputation after a change; and an HTTP/1.1 client in Firn
# with chunked transfer, gzip, redirects, a cache and cookies.
#
# THE GUARDS, and they are the point of the section:
#   * the official `testharness.js` is used UNMODIFIED, and a file counts
#     only if its harness really finished AND produced at least one
#     subtest. A test that reports nothing passes nothing -- that is the
#     lesson of the 32 empty reference pictures of round B3, and the files
#     that could not run are printed as their own number.
#   * the narrowed recomputation is checked against a FULL layout after
#     EVERY single mutation, box for box, x, y, w and h as bit patterns.
#     A narrowing that is fast and wrong is worth less than none. It found
#     two real bugs while it was being built (docs/ROUNDB4.md 4.3).
#   * the HTTP client talks to Python's own `http.server`, started and
#     killed by the runner. Both ends being this repository is exactly the
#     situation in which a shared misunderstanding stays invisible.
#   * `https://` is REFUSED, by name, twice -- typed in and reached
#     through a redirect. TLS is not in this round and is not faked.
bash tools/liveb4/run.sh > "$WORK/liveb4.log" 2>&1 && B4RC=0 || B4RC=$?
grep -E '^   (url |http dates|Set-Cookie|Cookie header|http rules|persistent|jar / cache|keep-alive|script order|counter-check|own cases|document:|elements styled|nodes visited|boxes laid|microseconds|layout walls|boxes whose|files |subtests|wpt_|own_|http_|narrow_|layout after)' \
    "$WORK/liveb4.log" | sed 's/^/ /'
grep -E '^(B4 OK|B4-WPT|URL OK|COOKIE OK|HTTP OK|SCRIPTS OK|CASES OK|INVALIDATE OK): ' \
    "$WORK/liveb4.log" | sed 's/^/ /'
if [ "$B4RC" -eq 0 ]; then
    ok
else
    bad "tools/liveb4/run.sh failed (see .test-work/liveb4.log)"
    grep -E 'FAIL|Traceback|Error' "$WORK/liveb4.log" | head -12 | sed 's/^/   /'
    tail -5 "$WORK/liveb4.log" | sed 's/^/   /'
fi

echo "== 64. CHAPTER Z: the defence against fingerprinting (ROUND B6) =="
# The number 64 is fixed for this round. 63 belongs to round B4.
#
# The occasion was a page on AliExpress on 24 August 2026: a WebAudio graph
# with a sawtooth oscillator and an `AnalyserNode` AT VOLUME ZERO, out of
# which a device fingerprint was read. Muting the tab did not help, because
# there was no media element to mute -- the measurement was never meant to
# be heard.
#
# What runs here is the part of chapter Z whose building blocks exist: the
# canvas readback (`toDataURL`, `getImageData`) and the `navigator` fields,
# noised PER ORIGIN AND PER SESSION after Brave's method, on by default and
# with no switch (Z3, Z4, Z6). WebAudio is not built yet; Z1 stands in
# REQUIREMENTS.md as a condition on ITS construction, not as a repair
# afterwards.
#
# THE GUARDS, and they are the point of the section:
#   * the same session and the same origin give BYTE-IDENTICAL answers over
#     twenty reads. A reading that differs from itself can be averaged away
#     and tells the script it is being lied to.
#   * THE COUNTER-CHECK: the same path with the farbling taken out has to
#     give exactly ONE answer over 500 origins. "500 origins, 500 different
#     canvases" is also true of a program that returns pure noise.
#   * the largest deviation of any colour channel is reported, not a mean,
#     and the alpha channel is checked separately -- a flipped alpha bit is
#     visible where a pixel is fully transparent.
#   * all three build stages have to give the SAME numbers. A key stream
#     that depends on the optimiser depends on the machine.
bash tools/fpz/run.sh > "$WORK/fpz.log" 2>&1 && ZRC=0 || ZRC=$?
grep -E '^   (stable|[0-9]+ origins|the same path|[0-9]+ sessions|largest|alpha|share|16 x 16|navigator|clock|checks)' \
    "$WORK/fpz.log" | sed 's/^/ /'
grep -E '^FPZ OK: ' "$WORK/fpz.log" | tail -1 | sed 's/^/ /'
if [ "$ZRC" -eq 0 ]; then
    ok
else
    bad "tools/fpz/run.sh failed (see .test-work/fpz.log)"
    grep -E 'FAIL|Traceback|Error' "$WORK/fpz.log" | head -12 | sed 's/^/   /'
    tail -5 "$WORK/fpz.log" | sed 's/^/   /'
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
