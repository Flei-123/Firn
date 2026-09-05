#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/stdlib81/run.sh -- the proof for round 81: hash/map, DEFLATE, JSON,
# crypto.
#
# FOUR AREAS, and every single one of them is judged by somebody who is not
# this repository:
#
#   1. HASH AND MAP. xxHash64 against the reference implementation
#      (python-xxhash, if it is installed) and against its published
#      vectors, FNV-1a against its own. Then the load test: a MILLION
#      entries with string keys -- time, memory, the LONGEST PROBE CHAIN and
#      the average. Open addressing degenerates silently, so it is measured
#      instead of assumed. Plus the endurance run (insert/delete for ever,
#      RSS flat) WITH the counter-check that leaves out the deletions and
#      must climb.
#   2. DEFLATE. Everything Firn packs is unpacked by `python3 zlib`, by
#      `gzip` and by the `gunzip` binary; everything those pack is unpacked
#      by Firn; and the edge cases (empty, one octet, incompressible,
#      one repeated octet) are in the corpus. Plus four counter-checks that
#      have to be REFUSED.
#   3. JSON. JSONTestSuite: every `y_` accepted, every `n_` refused, and the
#      output held against `python3 -m json.tool` octet for octet.
#   4. CRYPTO. 1,919 NIST CAVP vectors (SHA-1, SHA-256, HMAC, AES-128 CBC
#      and CFB8) out of `testdata/crypto/`, the FIPS 197 known answer test,
#      multi block CBC/CFB8 against the `openssl` binary (the KAT files are
#      single block and do not test chaining at all), and `python3 hashlib`
#      over random data.
#
# ALL OF IT IN THREE BUILD STAGES, so that the optimiser cannot be the
# difference between right and wrong. The big MEASUREMENTS run in the
# optimised stage only -- they are numbers, not verdicts.
#
# STDLIB81_FAST=1 runs the optimised stage only.
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=compiler/target/release/firnc
REPO=$(pwd)
W=$(mktemp -d /tmp/firn-std81.XXXXXX)
trap 'rm -rf "$W"' EXIT
ERRORS=0
report() { echo "  FAIL  $1"; ERRORS=$((ERRORS + 1)); }
export FIRNLIB="$(pwd)/lib"
FAST=${STDLIB81_FAST:-0}

# ---------------------------------------------------------------- test data
mkdir -p "$W/data" "$W/work"
: > "$W/data/empty.bin"
printf 'A' > "$W/data/one.bin"
head -c 200000 /dev/urandom > "$W/data/random.bin"
python3 -c "open('$W/data/onechar.bin','wb').write(b'a'*300000)"
cat lib/std/*.fi > "$W/data/libstd.txt"
cp testdata/realweb/wikipedia_en_rust.html "$W/data/" 2>/dev/null
cp testdata/realweb/hackernews.html "$W/data/" 2>/dev/null

tar xzf testdata/json/JSONTestSuite.tar.gz -C "$W" || {
    echo "FAIL: testdata/json/JSONTestSuite.tar.gz cannot be unpacked"; exit 1; }
mkdir -p "$W/vectors"
tar xzf testdata/crypto/nist-vectors.tar.gz -C "$W/vectors" || {
    echo "FAIL: testdata/crypto/nist-vectors.tar.gz cannot be unpacked"; exit 1; }

# THE PINNING, and it is per FILE, not per archive: `files.sha256` names
# every single one, so a repacked archive changes nothing and a changed
# test case is caught.
if ( cd "$W" && sha256sum --quiet -c "$REPO/testdata/json/files.sha256" ) \
        > "$W/json.sums" 2>&1; then
    echo "  testdata/json: all $(wc -l < "$REPO/testdata/json/files.sha256") file sums ok"
else
    report "testdata/json: a file sum does not match"
    head -3 "$W/json.sums" | sed 's/^/        /'
fi
if ( cd "$W/vectors" && sha256sum --quiet -c "$REPO/testdata/crypto/files.sha256" ) \
        > "$W/crypto.sums" 2>&1; then
    echo "  testdata/crypto: all $(wc -l < "$REPO/testdata/crypto/files.sha256") file sums ok"
else
    report "testdata/crypto: a file sum does not match"
    head -3 "$W/crypto.sums" | sed 's/^/        /'
fi

STAGES="release-fast: no-opt:--no-opt dev-fast:--opt-level=dev-fast"
if [ "$FAST" = "1" ]; then
    STAGES="release-fast:"
fi

for stage in $STAGES; do
    name=${stage%%:*}
    opt=${stage#*:}
    echo "== stage $name =="

    ok=1
    for prog in deflate_cli json_cli crypto_cli mapbench; do
        if ! $FIRNC $opt -o "$W/$prog.$name" "tools/stdlib81/$prog.fi" 2>"$W/err"; then
            report "$name: tools/stdlib81/$prog.fi does not compile"
            sed 's/^/        /' "$W/err" | head -8
            ok=0
        fi
    done
    [ $ok = 1 ] || continue

    DCLI="$W/deflate_cli.$name"
    JCLI="$W/json_cli.$name"
    CCLI="$W/crypto_cli.$name"
    MB="$W/mapbench.$name"

    # ------------------------------------------------------------- 1. hash
    echo "  -- hash and map"
    if [ "$name" = "release-fast" ]; then
        HB="$W/hash.txt"
        "$MB" hash 16 "$HB" || report "$name: mapbench hash failed"
        python3 - "$HB" <<'PY'
import sys
v = dict(l.split() for l in open(sys.argv[1]) if l.strip())
mb = 16.0
print("  FNV-1a   %8.1f MiB/s   xxHash64 %8.1f MiB/s"
      % (mb / (int(v["fnv_us"]) / 1e6), mb / (int(v["xx_us"]) / 1e6)))
PY
    fi
    if ! $FIRNC $opt -o "$W/hashprobe.$name" tools/stdlib81/hashprobe.fi 2>"$W/err"; then
        report "$name: tools/stdlib81/hashprobe.fi does not compile"
        sed 's/^/        /' "$W/err" | head -6
    elif ! python3 tools/stdlib81/hashvectors.py "$W/hashprobe.$name" | sed 's/^/  /'; then
        report "$name: the hash vectors do not match"
    fi

    N=1000000
    [ "$name" = "release-fast" ] || N=200000
    "$MB" load $N "$W/load.$name" || report "$name: mapbench load failed"
    python3 - "$W/load.$name" "$N" "$name" <<'PY'
import sys
v = dict(l.split() for l in open(sys.argv[1]) if l.strip())
n = int(sys.argv[2])
bad = []
if int(v["entries"]) != n: bad.append("entries")
if int(v["missing"]): bad.append("a key that was inserted is missing")
if int(v["wrong"]): bad.append("a value came back wrong")
if int(v["ghost"]): bad.append("a key that was never inserted was found")
if int(v["iterated"]) != n: bad.append("iteration did not see every entry")
if int(v["after_delete"]) != n // 2: bad.append("after the deletions the count is wrong")
if int(v["survived"]) != n // 2: bad.append("the wrong half survived")
print("  %-12s %d entries: insert %.2f s, lookup %.2f s, iterate %.3f s, delete %.2f s"
      % (sys.argv[3], n, int(v["insert_us"]) / 1e6, int(v["lookup_us"]) / 1e6,
         int(v["iter_us"]) / 1e6, int(v["delete_us"]) / 1e6))
print("  probe chain: longest %s, average %.3f, load %.3f, table %.1f MiB for %.1f MiB of keys, RSS %.1f MiB"
      % (v["probe_max"], int(v["probe_total"]) / n, n / int(v["cap"]),
         int(v["map_bytes"]) / 1048576, int(v["key_bytes"]) / 1048576,
         int(v["rss_kib"]) / 1024))
if bad:
    print("  FAIL " + "; ".join(bad))
    sys.exit(1)
PY
    [ $? -eq 0 ] || report "$name: the map load test found something"

    if [ "$name" = "release-fast" ]; then
        "$MB" soak 60 "$W/soak.txt" || report "soak failed"
        "$MB" leak 60 "$W/leak.txt" || report "leak counter-check failed"
        python3 - "$W/soak.txt" "$W/leak.txt" <<'PY'
import sys
s = dict(l.split() for l in open(sys.argv[1]) if l.strip())
k = dict(l.split() for l in open(sys.argv[2]) if l.strip())
grow_s = int(s["rssn_kib"]) - int(s["rss0_kib"])
grow_l = int(k["rssn_kib"]) - int(k["rss0_kib"])
print("  soak 1.2 M insert+delete: RSS %s -> %s KiB (+%d), entries left %s"
      % (s["rss0_kib"], s["rssn_kib"], grow_s, s["entries"]))
print("  counter-check without the deletions: RSS %s -> %s KiB (+%d), entries %s"
      % (k["rss0_kib"], k["rssn_kib"], grow_l, k["entries"]))
if grow_s > 4096:
    print("  FAIL the soak run grew by %d KiB" % grow_s); sys.exit(1)
if grow_l < 4096:
    print("  FAIL the counter-check did NOT grow -- the measurement is blind")
    sys.exit(1)
PY
        [ $? -eq 0 ] || report "the endurance measurement"
    fi

    # ---------------------------------------------------------- 2. deflate
    echo "  -- deflate"
    CORPUS="$W/data/empty.bin $W/data/one.bin $W/data/random.bin $W/data/onechar.bin $W/data/libstd.txt"
    if [ "$name" = "release-fast" ]; then
        CORPUS="$CORPUS $W/data/wikipedia_en_rust.html"
        [ -f "$W/data/hackernews.html" ] && CORPUS="$CORPUS $W/data/hackernews.html"
    fi
    python3 tools/stdlib81/deflate_check.py "$DCLI" "$W/work" $CORPUS \
        || report "$name: the DEFLATE cross-check failed"

    if [ "$name" = "release-fast" ]; then
        python3 - "$DCLI" "$W/data/wikipedia_en_rust.html" "$W/work" <<'PY'
import os, subprocess, sys, time, zlib
cli, src, work = sys.argv[1], sys.argv[2], sys.argv[3]
data = open(src, "rb").read()
for level in (1, 6, 9):
    p = os.path.join(work, "s.z")
    t = time.time(); subprocess.run([cli, "zlibc", str(level), src, p], check=True)
    dt = time.time() - t
    size = os.path.getsize(p)
    t = time.time(); subprocess.run([cli, "zlibd", "0", p, os.path.join(work, "s.out")], check=True)
    dt2 = time.time() - t
    ref = len(zlib.compress(data, level))
    print("  level %d: pack %6.1f MiB/s -> %7d octets (zlib %7d, %.1f%%), unpack %6.1f MiB/s"
          % (level, len(data) / dt / 1048576, size, ref, 100.0 * size / ref,
             len(data) / dt2 / 1048576))
PY
    fi

    # ------------------------------------------------------------- 3. JSON
    echo "  -- json"
    python3 tools/stdlib81/jsoncheck.py "$JCLI" "$W" "$W/work" \
        || report "$name: JSONTestSuite"
    "$JCLI" build "$W/work/built.json" || report "$name: the JSON writer failed"
    python3 - "$W/work/built.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
want = {"name": "firn", "version": 81, "pi": 3.14159, "ok": True,
        "none": None, "tags": ['a"b\\c\n', "grüße €",
                               {"version": -7}, []]}
if d != want:
    print("  FAIL the writer produced %r" % d); sys.exit(1)
print("  the writer without a document: python reads back exactly what was written")
PY
    [ $? -eq 0 ] || report "$name: the JSON writer"

    # ----------------------------------------------------------- 4. crypto
    echo "  -- crypto"
    python3 tools/stdlib81/nist.py "$CCLI" "$W/vectors" "$W/work" \
        || report "$name: the NIST vectors"
    python3 tools/stdlib81/cryptocross.py "$CCLI" "$W/work" \
        || report "$name: the cross-check against python/openssl"

    if [ "$name" = "release-fast" ]; then
        for what in sha1 sha256 aes cfb8; do
            mb=8
            [ "$what" = "cfb8" ] && mb=2
            [ "$what" = "aes" ] && mb=4
            "$CCLI" speed $what $mb "$W/sp.txt" || continue
            python3 - "$W/sp.txt" "$what" "$mb" <<'PY'
import sys
us = int(open(sys.argv[1]).read().strip())
print("  %-8s %s MiB in %7.1f ms = %7.2f MiB/s"
      % (sys.argv[2], sys.argv[3], us / 1000.0, float(sys.argv[3]) / (us / 1e6)))
PY
        done
    fi
done

echo
if [ "$ERRORS" -eq 0 ]; then
    echo "RESULT ok"
    exit 0
fi
echo "RESULT $ERRORS failed"
exit 1
