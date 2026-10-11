#!/usr/bin/env bash
# tools/mc/testmc.sh [pattern] -- compile and run the Minecraft library tests in
# tests/ (files named tests/NNNN_mc_*.fi) in three build stages and check the
# expectation in line 1 (`// expect_exit: N` or `// expect_out: TEXT`), exactly
# like test.sh does for the whole suite. Quick enough to run after every change.
#
#   bash tools/mc/testmc.sh            all tests/*_mc_*.fi
#   bash tools/mc/testmc.sh packet     only tests/*_mc_packet*.fi
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
PAT=${1:-}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
pass=0; fail=0
for f in tests/*_mc_*${PAT}*.fi; do
    [ -f "$f" ] || continue
    first=$(head -1 "$f")
    for stage in "" "--no-opt" "--opt-level=dev-fast"; do
        name=${stage:-release-fast}
        if ! $FIRNC $stage -o "$W/t" "$f" 2>"$W/err"; then
            echo "  FAIL  $f [$name]: does not compile"; head -5 "$W/err" | sed 's/^/        /'
            fail=$((fail+1)); continue
        fi
        out=$(timeout 120 "$W/t" 2>"$W/stderr"); rc=$?
        case "$first" in
            "// expect_exit: "*)
                want=${first#// expect_exit: }
                if [ "$rc" = "$want" ]; then pass=$((pass+1)); else
                    echo "  FAIL  $f [$name]: exit $rc, wanted $want"; head -5 "$W/stderr" | sed 's/^/        /'; fail=$((fail+1)); fi ;;
            "// expect_out: "*)
                want=${first#// expect_out: }
                if [ "$out" = "$want" ]; then pass=$((pass+1)); else
                    echo "  FAIL  $f [$name]: output differs"; fail=$((fail+1)); fi ;;
            *) echo "  FAIL  $f: no expect_exit/expect_out in line 1"; fail=$((fail+1)) ;;
        esac
    done
done
echo "mc tests: $pass passed, $fail failed"
[ $fail -eq 0 ]
