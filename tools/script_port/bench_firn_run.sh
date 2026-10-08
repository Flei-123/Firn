#!/bin/bash
# tools/script_port/bench_firn_run.sh -- start-up cost of firn-run.
#
#   bash tools/script_port/bench_firn_run.sh            (N=100 runs per block, ROUNDS=3)
#
# Warm: mean wall time of N starts, ROUNDS blocks interleaved (so a load change
# hits every variant alike); the table shows the median block. Cold: mean of
# COLD builds (empty cache each time). Times in microseconds on one line each.
set -u
cd "$(dirname "$0")/../.."
ROOT=$PWD
N=${N:-100}
ROUNDS=${ROUNDS:-3}
COLD=${COLD:-5}
export FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
export FIRNLIB=$ROOT/lib
RUN=$ROOT/tools/script_port/firn-run
[ -x "$FIRNC" ] || { echo "no compiler at $FIRNC"; exit 1; }

D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
export FIRN_CACHE=$D/cache
mkdir "$D/bin"
ln -s "$RUN" "$D/bin/firn-run"

cat >"$D/small.fi" <<'EOF'
fn main() -> i32 {
    return 0
}
EOF
cat >"$D/direct.fi" <<'EOF'
#!/usr/bin/env firn-run
fn main() -> i32 {
    return 0
}
EOF
chmod +x "$D/direct.fi"
cat >"$D/imports.fi" <<'EOF'
import std.rt
import std.str
import std.process
import std.fs
import std.text
import std.vec

fn main() -> i32 {
    return 0
}
EOF
printf 'import sys\nsys.exit(0)\n' >"$D/small.py"

"$FIRNC" -o "$D/small.bin" "$D/small.fi" || exit 1
"$RUN" "$D/small.fi" || exit 1 # warms the cache

mean_us() { # mean_us <command...>: mean wall microseconds over N runs
    local s e i
    s=$(date +%s%N)
    for ((i = 0; i < N; i++)); do "$@" >/dev/null 2>&1 </dev/null; done
    e=$(date +%s%N)
    echo $(((e - s) / N / 1000))
}

names=("true (binary)" "sh -c : (shell start)" "direct Firn binary" "firn-run small.fi" "firn-run FIRN_STRICT=1" "./direct.fi via env firn-run" "python3 -c pass" "python3 small.py")
declare -A res
for r in $(seq "$ROUNDS"); do
    res["0,$r"]=$(mean_us /bin/true)
    res["1,$r"]=$(mean_us sh -c :)
    res["2,$r"]=$(mean_us "$D/small.bin")
    res["3,$r"]=$(mean_us "$RUN" "$D/small.fi")
    res["4,$r"]=$(FIRN_STRICT=1 mean_us "$RUN" "$D/small.fi")
    res["5,$r"]=$(PATH="$D/bin:$PATH" mean_us "$D/direct.fi")
    res["6,$r"]=$(mean_us python3 -c pass)
    res["7,$r"]=$(mean_us python3 "$D/small.py")
done
median() { printf '%s\n' "$@" | sort -n | sed -n "$((($# + 1) / 2))p"; }
echo "load: $(cut -d' ' -f1-3 /proc/loadavg), N=$N runs per block, $ROUNDS blocks (median block shown)"
echo "| start-up of | mean us |"
echo "|---|---:|"
declare -A med
for k in 0 1 2 3 4 5 6 7; do
    vals=()
    for r in $(seq "$ROUNDS"); do vals+=("${res["$k,$r"]}"); done
    med[$k]=$(median "${vals[@]}")
    echo "| ${names[$k]} | ${med[$k]} |"
done
echo
echo "overhead firn-run (warm) over the direct binary: $((med[3] - med[2])) us"
echo "overhead with FIRN_STRICT=1:                     $((med[4] - med[2])) us"
echo "overhead through #!/usr/bin/env firn-run:        $((med[5] - med[2])) us"
echo "python3 -c pass minus firn-run small.fi:         $((med[6] - med[3])) us"

echo
echo "cold builds (empty cache, mean of $COLD):"
for f in small imports; do
    for lvl in dev-fast release-safe release-fast; do
        tot=0
        for i in $(seq "$COLD"); do
            rm -rf "$FIRN_CACHE"
            s=$(date +%s%N)
            FIRN_OPT=$lvl "$RUN" "$D/$f.fi" >/dev/null 2>&1
            e=$(date +%s%N)
            tot=$((tot + (e - s) / 1000))
        done
        echo "  $f.fi  $lvl: $((tot / COLD / 1000)) ms"
    done
done
