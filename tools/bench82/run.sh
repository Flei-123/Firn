#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/bench82/run.sh -- the speed of round 82, measured and compared
# against somebody else's implementation.
#
# WHAT IT DOES, in this order:
#
#   1. THE MACHINE. Processor, core count, date. A throughput number without
#      the machine it was measured on is worth nothing.
#   2. CORRECTNESS FIRST. `cross.fi` sends every length from 0 to 300 octets
#      through the hardware path AND the scalar path and demands the same
#      answer, plus the FIPS 197 and FIPS 180-4 known answers. A fast
#      implementation that is wrong is worth less than a slow one that is
#      right, so this runs BEFORE the stopwatch and its failure is fatal.
#   3. CRYPTO. SHA-256, AES-128-CBC in both directions and AES-128-CFB8, each
#      with the hardware path and with the scalar path forced, against
#      `openssl speed` on the same machine.
#   4. DEFLATE. Level 6 packing and unpacking against `gzip -6` -- on
#      literally the same octets: `speed dump` writes the test data out and
#      `gzip` gets that file.
#   5. THE COMPILER ON ITSELF. `firnc --timings` prints the wall time per
#      phase; the three most expensive ones are reported. That is the number
#      that decides how long the acceptance takes.
#   6. THE REGRESSION LIMITS. One `minquota_*.txt` per line of the table.
#      Falling below one is a failure, so that a later round cannot make the
#      speed quietly disappear again.
#
# BENCH82_FULL=1 measures with bigger buffers and asks `openssl speed` for
# longer. What `test.sh` runs is the fast variant.
#
# THE LIMITS ARE DELIBERATELY LOW. This is a shared virtual machine; the
# same binary measured between 429 and 894 MiB/s for SHA-256 depending on
# what else was running. The limits sit at roughly half of what was measured,
# so that they catch a REGRESSION (a factor of two or more) and not the noise.
set -uo pipefail
cd "$(dirname "$0")/../.."

ROOT="$(pwd)"
export FIRNLIB="$ROOT/lib"
FIRNC=compiler/target/release/firnc
W=.bench82-work
mkdir -p "$W"

FULL=0
[ "${BENCH82_FULL:-0}" = "1" ] && FULL=1

ERRORS=0
report() {
    echo "FAIL: $*"
    ERRORS=$((ERRORS + 1))
}

# ------------------------------------------------------------- 0. build
if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml >/dev/null 2>&1 \
        || { echo "FAIL: the compiler does not build"; exit 1; }
fi
"$FIRNC" --opt-level=release-fast -o "$W/speed" tools/bench82/speed.fi 2>"$W/build.log" \
    || { echo "FAIL: tools/bench82/speed.fi does not compile"; cat "$W/build.log"; exit 1; }
"$FIRNC" --opt-level=release-fast -o "$W/cross" tools/bench82/cross.fi 2>>"$W/build.log" \
    || { echo "FAIL: tools/bench82/cross.fi does not compile"; cat "$W/build.log"; exit 1; }

# ------------------------------------------------------------ 1. machine
echo "== the machine =="
echo "  $(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2- | sed 's/^ *//')"
echo "  cores: $(nproc)   kernel: $(uname -r)   date: $(date -u '+%Y-%m-%d %H:%M UTC')"
echo "  openssl: $(openssl version 2>/dev/null || echo 'not present')"
echo

# --------------------------------------------- 2. correctness before speed
echo "== both paths, the same answer =="
if "$W/cross"; then
    :
else
    report "the hardware path and the scalar path do not agree"
fi
echo

# ------------------------------------------------------------- the helper
#
# BEST OF N, not the average: the fastest pass is the one in which the
# machine was not doing something else. On a shared host the average measures
# the neighbours.
runs=5
[ "$FULL" = "1" ] && runs=9
measure() { # <what> <MiB>  -> MiB/s on stdout
    local what="$1" mb="$2" best=999999999 v
    local r
    for r in $(seq 1 $runs); do
        "$W/speed" "$what" "$mb" "$W/t.txt" >/dev/null 2>&1 || { echo "0"; return 1; }
        v=$(cat "$W/t.txt")
        [ "$v" -lt "$best" ] && best=$v
    done
    python3 -c "print('%.1f' % ($mb / ($best / 1e6)))"
}

MB_HASH=8;  MB_AES=8;  MB_CFB=2;  MB_DEF=4
if [ "$FULL" = "1" ]; then MB_HASH=64; MB_AES=64; MB_CFB=8; MB_DEF=16; fi
# the scalar paths are slow -- a smaller buffer, or the measurement takes
# minutes and measures nothing more
MB_SOFT_HASH=2; MB_SOFT_AES=1; MB_SOFT_CFB=1

# ------------------------------------------------------------- 3. crypto
echo "== crypto =="
SHA=$(measure sha256 $MB_HASH)
SHA_SOFT=$(measure sha256-soft $MB_SOFT_HASH)
AES=$(measure aes $MB_AES)
AES_SOFT=$(measure aes-soft $MB_SOFT_AES)
AESD=$(measure aesd $MB_AES)
AESD_SOFT=$(measure aesd-soft $MB_SOFT_AES)
CFB=$(measure cfb8 $MB_CFB)
CFB_SOFT=$(measure cfb8-soft $MB_SOFT_CFB)

# openssl as the yardstick. `-seconds 1` per block size; the largest block
# (16384) is the one that is comparable with a bulk measurement.
ossl() { # <algorithm> -> MiB/s of the largest block size
    local alg="$1" secs=1
    [ "$FULL" = "1" ] && secs=3
    local line
    line=$(openssl speed -elapsed -seconds $secs -evp "$alg" 2>/dev/null | tail -1)
    python3 - "$line" <<'PY'
import sys
f = sys.argv[1].split()
try:
    v = float(f[-1].rstrip('k'))
    print('%.1f' % (v * 1000 / 1048576.0))
except Exception:
    print('n/a')
PY
}
ossl_sha() {
    local secs=1
    [ "$FULL" = "1" ] && secs=3
    local line
    line=$(openssl speed -elapsed -seconds $secs sha256 2>/dev/null | tail -1)
    python3 - "$line" <<'PY'
import sys
f = sys.argv[1].split()
try:
    print('%.1f' % (float(f[-1].rstrip('k')) * 1000 / 1048576.0))
except Exception:
    print('n/a')
PY
}
O_SHA=$(ossl_sha)
O_AES=$(ossl aes-128-cbc)
O_CFB=$(ossl aes-128-cfb8)

row() { # <name> <firn> <soft> <reference>
    python3 - "$1" "$2" "$3" "$4" <<'PY'
import sys
name, fast, soft, ref = sys.argv[1:5]
def f(x):
    try: return float(x)
    except Exception: return None
a, s, r = f(fast), f(soft), f(ref)
gain = ('%6.1fx' % (a / s)) if a and s and s > 0 else '     -'
fac = ('%5.2fx' % (r / a)) if a and r and a > 0 else '    -'
print('  %-14s %9s %9s %8s %10s %8s'
      % (name, fast, soft, gain, ref if r else 'n/a', fac))
PY
}
echo "  workload         MiB/s   scalar     gain  OpenSSL  behind by"
row "SHA-256"      "$SHA"  "$SHA_SOFT"  "$O_SHA"
row "AES-128-CBC"  "$AES"  "$AES_SOFT"  "$O_AES"
row "AES-CBC dec"  "$AESD" "$AESD_SOFT" "$O_AES"
row "AES-128-CFB8" "$CFB"  "$CFB_SOFT"  "$O_CFB"
echo

# ------------------------------------------------------------ 4. deflate
echo "== deflate =="
DEF=$(measure deflate6 $MB_DEF)
INF=$(measure inflate $MB_DEF)
CRC=$(measure crc32 $MB_DEF)
"$W/speed" dump $MB_DEF "$W/data.bin" >/dev/null 2>&1
GZIP="n/a"
if [ -s "$W/data.bin" ] && command -v gzip >/dev/null; then
    GZIP=$(python3 - "$W/data.bin" "$MB_DEF" <<'PY'
import subprocess, sys, time
path, mb = sys.argv[1], float(sys.argv[2])
best = 1e9
data = open(path, 'rb').read()
for _ in range(5):
    t = time.monotonic()
    subprocess.run(['gzip', '-6', '-c'], input=data, stdout=subprocess.DEVNULL)
    best = min(best, time.monotonic() - t)
print('%.1f' % (mb / best))
PY
)
fi
row "DEFLATE -6"   "$DEF" "-"   "$GZIP"
echo "  inflate        $INF MiB/s     crc32 (scalar table)  $CRC MiB/s"
echo

# --------------------------------------------- 5. the compiler on itself
echo "== the compiler on itself =="
if "$FIRNC" --timings --opt-level=release-fast -o "$W/self.bin" bin/firnc1.fi \
        2>"$W/timings.txt" >/dev/null; then
    grep -E '^  ' "$W/timings.txt" | head -14
    SELF_MS=$(grep -E '^total ' "$W/timings.txt" | awk '{print $2}')
    echo "  total: ${SELF_MS} ms for bin/firnc1.fi (30,643 lines of Firn)"
else
    report "firnc --timings over bin/firnc1.fi"
    SELF_MS=999999
    cat "$W/timings.txt" | head -5
fi
echo

# ------------------------------------------------------- 6. the limits
echo "== the regression limits =="
limit() { # <file> <measured> <name>
    local f="tools/bench82/$1" got="$2" name="$3"
    [ -f "$f" ] || { report "the limit file $f is missing"; return; }
    local min
    min=$(cat "$f")
    python3 - "$got" "$min" "$name" <<'PY'
import sys
got, mn, name = sys.argv[1], sys.argv[2], sys.argv[3]
try:
    g = float(got)
except Exception:
    print("  %-16s NO MEASUREMENT" % name); sys.exit(1)
m = float(mn)
if g < m:
    print("  %-16s %8.1f  BELOW THE LIMIT %.1f" % (name, g, m)); sys.exit(1)
print("  %-16s %8.1f  >= %.1f  ok" % (name, g, m))
PY
    [ $? -eq 0 ] || report "$name below its limit"
}
limit minquota_sha256.txt   "$SHA"  "SHA-256"
limit minquota_aes_cbc.txt  "$AES"  "AES-CBC"
limit minquota_aes_dec.txt  "$AESD" "AES-CBC dec"
limit minquota_cfb8.txt     "$CFB"  "AES-CFB8"
limit minquota_deflate.txt  "$DEF"  "DEFLATE -6"

# The self compile is a CEILING, not a floor: it must not get slower.
if [ -f tools/bench82/maxquota_self_ms.txt ]; then
    MAXSELF=$(cat tools/bench82/maxquota_self_ms.txt)
    python3 - "$SELF_MS" "$MAXSELF" <<'PY'
import sys
got, mx = float(sys.argv[1]), float(sys.argv[2])
if got > mx:
    print("  %-16s %8.0f ms  ABOVE THE LIMIT %.0f ms" % ("self compile", got, mx)); sys.exit(1)
print("  %-16s %8.0f ms  <= %.0f ms  ok" % ("self compile", got, mx))
PY
    [ $? -eq 0 ] || report "the self compile got slower"
fi

echo
if [ "$ERRORS" -eq 0 ]; then
    echo "RESULT ok"
    exit 0
fi
echo "RESULT $ERRORS failed"
exit 1
