#!/usr/bin/env bash
# tools/mc/crypto/run.sh -- cross-check of lib/mc/mc_crypto.fi against node
# (OpenSSL) and python `cryptography`: CFB8 byte-identical for randomly chunked
# streams in both directions, SPKI fields == node JWK, RSA PKCS#1 v1.5
# ciphertexts decrypt with node / python to the message, the padded block has the
# right shape. Includes a counter-check that MUST go red, and the benchmark.
#
# Usage: bash tools/mc/crypto/run.sh [firnc-binary]
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0
# RSA test keys: generated here with openssl (fresh per run, inside the temp dir, removed on exit)
# so no stage depends on files left over from another stage or on a manual setup step.
mkdir -p "$W/keys"
for bits in 512 1024 2048 3072 4096; do
    if ! openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:$bits -out "$W/keys/priv$bits.pem" 2>/dev/null; then
        echo "  FAIL  openssl cannot generate a $bits bit RSA key"; echo "RESULT mc-crypto: FAIL"; exit 1
    fi
done
for stage in "" "--no-opt" "--opt-level=dev-fast"; do
    name=${stage:-release-fast}
    echo "== build stage: $name =="
    if ! $FIRNC $stage -o "$W/xcheck" tools/mc/crypto/xcheck.fi 2>"$W/err"; then
        echo "  FAIL  xcheck.fi does not compile [$name]"; head -10 "$W/err"; ERR=1; continue
    fi
    mkdir -p "$W/t"
    cp "$W"/keys/priv*.pem "$W/t/"
    node --security-revert=CVE-2023-46809 tools/mc/crypto/check.js "$W/xcheck" "$W/t" >"$W/node.out" 2>&1
    rc=$?
    grep -v "SECURITY WARNING" "$W/node.out"
    if [ $rc -ne 0 ]; then echo "  FAIL  node cross-check [$name]"; ERR=1; else echo "  ok    node cross-check [$name]"; fi
    # python cryptography as a second, independent decryptor (release stage only)
    if [ -z "$stage" ]; then
        if python3 -I tools/mc/crypto/pycheck.py "$W/xcheck" "$W/t"; then
            echo "  ok    python cryptography decrypts our PKCS#1 v1.5 ciphertexts"
        else
            echo "  FAIL  python cryptography check"; ERR=1
        fi
        # counter-check: with one octet flipped in every Firn output the run MUST fail
        MC_TAMPER=1 node --security-revert=CVE-2023-46809 tools/mc/crypto/check.js "$W/xcheck" "$W/t" >"$W/tamper.out" 2>&1
        trc=$?
        if [ $trc -eq 1 ] && grep -q "  FAIL  " "$W/tamper.out"; then
            echo "  ok    counter-check: tampered outputs are detected ($(grep -c '  FAIL  ' "$W/tamper.out") failing checks)"
        else
            echo "  FAIL  counter-check: tampered run exit $trc, was not detected properly"; head -3 "$W/tamper.out"; ERR=1
        fi
        echo "== benchmark (release-fast) =="
        "$W/xcheck" bench | sed 's/^/  /'
    fi
    rm -rf "$W/t"
done
[ $ERR -eq 0 ] && echo "RESULT mc-crypto: ok" || echo "RESULT mc-crypto: FAIL"
exit $ERR
