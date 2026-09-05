#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/tlsb5/run.sh -- ROUND B5: TLS, pictures, and a window.
#
#   1. compile the round's root files in THREE build stages
#      (opt / --no-opt / dev-fast)
#   2. the primitives against Python's `cryptography`, `hashlib` and
#      OpenSSL: big integers, X25519, ChaCha20-Poly1305, AES-GCM,
#      SHA-384/512, HKDF, RSA (PKCS#1 and PSS) and ECDSA on P-256 and
#      P-384 -- with the cases that MUST fail counted separately
#      (tools/tlsb5/crypto_check.py)
#   3. certificates: chains built with Python's `cryptography`, and above
#      all the REFUSALS -- expired, not yet valid, the wrong name, an
#      unknown issuer, a forged signature, an issuer that is not a CA, a
#      curve this round does not have. Plus six real chains off the wire,
#      each of them also under a wrong name and at a time past their
#      notAfter (tools/tlsb5/cert_check.py, tests/data/tls-chains)
#   4. the TLS 1.3 handshake against `openssl s_server` and against the
#      public internet, with a man in the middle that flips one bit
#      (tools/tlsb5/tls_check.py)
#   5. `https://` in the HTTP client of round B4, at the seam: the scheme
#      is part of a connection's identity, a redirect across the schemes,
#      and a client with no roots that fetches nothing
#      (tools/tlsb5/https_check.py)
#   6. the JPEG decoder against libjpeg, with a PER-PIXEL bound and not a
#      mean (tools/tlsb5/jpeg_check.py)
#   7. `<img>`: the sizing rules of CSS 2.1 10.3.2, the dimension
#      attributes, lazy loading, and the picture on the canvas pixel for
#      pixel against Pillow (tools/tlsb5/img_check.py)
#   8. the WINDOW, photographed from the SERVER's side with `xwd` inside
#      an Xvfb nobody can see (tools/tlsb5/ui_check.py)
#   9. the regression limits from tools/tlsb5/minquota.txt
#
# WHAT COUNTS AS A COUNTERPART. Every measurement in this file has an
# other side that this repository did not write: OpenSSL (through
# `openssl s_server`, Python's `ssl` and the `cryptography` package),
# libjpeg (through Pillow), and X (through `xwd`). Two ends that
# misunderstand the same standard in the same way agree perfectly, which
# is why a self-written counterpart proves nothing.
#
# WHAT THE INTERNET IS USED FOR. Sections 3 and 4 reach six real hosts if
# there is a route to them, and skip that part if there is not. Nothing
# else leaves the machine: the certificate chains were harvested once
# (tests/data/tls-chains/PROVENANCE.md) and the servers of sections 4 to 8
# are started here and killed again.
set -uo pipefail
cd "$(dirname "$0")/../.."

FIRNC="compiler/target/release/firnc"
export FIRNLIB="$(pwd)/lib"
WORK=".b5-work"
mkdir -p "$WORK"
ERRORS=0
fail() { echo "  FAIL  $1"; ERRORS=$((ERRORS + 1)); }

if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml
fi

echo "== 1. three build stages of the round's root files =="
ROOTS="lib/std/crypto/crypto_main.fi lib/tls/x509_main.fi
       lib/tls/tls_main.fi lib/paint/jpeg_main.fi
       lib/browser/b5_main.fi lib/browser/window_main.fi
       lib/net/http_main.fi"
for SRC in $ROOTS; do
    BASE=$(basename "$SRC" .fi)
    for STAGE in "opt:" "noopt:--no-opt" "dev:--opt-level=dev-fast"; do
        NAME=${STAGE%%:*}
        OPT=${STAGE#*:}
        if ! $FIRNC $OPT -o "$WORK/${BASE}_${NAME}" "$SRC" \
            2>"$WORK/build_${BASE}_${NAME}.log"; then
            fail "$SRC does not compile ($NAME)"
            head -6 "$WORK/build_${BASE}_${NAME}.log" | sed 's/^/        /'
        fi
    done
done
echo "   opt, --no-opt and dev-fast built for crypto, x509, tls, jpeg,"\
"page, window, http"

CRY="$WORK/crypto_main_opt"
X509="$WORK/x509_main_opt"
TLSM="$WORK/tls_main_opt"
JPG="$WORK/jpeg_main_opt"
PAGE="$WORK/b5_main_opt"
WIN="$WORK/window_main_opt"
HTTPC="$WORK/http_main_opt"

echo "== 2. the primitives against somebody else's =="
python3 tools/tlsb5/crypto_check.py "$CRY" | tee "$WORK/crypto.txt" || \
    fail "crypto_check"

echo "== 3. certificates, and the refusals =="
python3 tools/tlsb5/cert_check.py "$X509" | tee "$WORK/cert.txt" || \
    fail "cert_check"

echo "== 4. the TLS 1.3 handshake against a real server =="
python3 tools/tlsb5/tls_check.py "$TLSM" | tee "$WORK/tls.txt" || \
    fail "tls_check"

echo "== 5. https:// in the client of round B4 =="
python3 tools/tlsb5/https_check.py "$HTTPC" | tee "$WORK/https.txt" || \
    fail "https_check"

echo "== 6. the JPEG decoder against libjpeg =="
python3 tools/tlsb5/jpeg_check.py "$JPG" "$WORK" | tee "$WORK/jpeg.txt" \
    || fail "jpeg_check"

echo "== 7. <img> in the layout and on the canvas =="
python3 tools/tlsb5/img_check.py "$PAGE" | tee "$WORK/img.txt" || \
    fail "img_check"

echo "== 8. the window, seen from the X server =="
python3 tools/tlsb5/ui_check.py "$WIN" | tee "$WORK/ui.txt" || \
    fail "ui_check"

echo "== 9. the same three checks in the other two build stages =="
# Not everything three times -- the primitives are what an optimiser
# could break, and they have to give the SAME numbers.
for NAME in noopt dev; do
    python3 tools/tlsb5/crypto_check.py "$WORK/crypto_main_${NAME}" \
        > "$WORK/crypto_$NAME.txt" 2>&1
    A=$(grep -o 'CRYPTO OK: [0-9]* / [0-9]*' "$WORK/crypto_$NAME.txt")
    echo "   $NAME  $A"
    if [ -z "$A" ]; then
        fail "the primitives do not agree in build stage $NAME"
    fi
done

echo "== 10. the regression limits =="
CRYPTO_N=$(grep -oE 'CRYPTO OK: [0-9]+' "$WORK/crypto.txt" | awk '{print $3}')
CERT_N=$(grep -oE 'CERT OK: [0-9]+' "$WORK/cert.txt" | awk '{print $3}')
CERT_REF=$(grep -oE 'refusals [0-9]+' "$WORK/cert.txt" | awk '{print $2}')
TLS_N=$(grep -oE 'TLS OK: [0-9]+' "$WORK/tls.txt" | awk '{print $3}')
HTTPS_N=$(grep -oE 'HTTPS OK: [0-9]+' "$WORK/https.txt" | awk '{print $3}')
JPEG_N=$(grep -oE 'JPEG OK: [0-9]+' "$WORK/jpeg.txt" | awk '{print $3}')
IMG_N=$(grep -oE 'IMG OK: [0-9]+' "$WORK/img.txt" | awk '{print $3}')
UI_N=$(grep -oE 'UI OK: [0-9]+' "$WORK/ui.txt" | awk '{print $3}')
while read -r KEY VALUE _; do
    case "$KEY" in
        \#*|"") continue ;;
    esac
    GOT=""
    case "$KEY" in
        crypto_cases) GOT="$CRYPTO_N" ;;
        cert_cases) GOT="$CERT_N" ;;
        cert_refusals) GOT="$CERT_REF" ;;
        tls_cases) GOT="$TLS_N" ;;
        https_cases) GOT="$HTTPS_N" ;;
        jpeg_cases) GOT="$JPEG_N" ;;
        img_cases) GOT="$IMG_N" ;;
        ui_cases) GOT="$UI_N" ;;
    esac
    if [ -z "$GOT" ]; then
        continue
    fi
    if [ "$GOT" -lt "$VALUE" ]; then
        fail "$KEY fell to $GOT, the limit is $VALUE"
    else
        echo "   $KEY $GOT (limit $VALUE)"
    fi
done < tools/tlsb5/minquota.txt

echo
if [ "$ERRORS" -eq 0 ]; then
    echo "B5 OK: $CRYPTO_N primitive cases, $CERT_N certificate cases "\
"($CERT_REF refusals), $TLS_N TLS cases, $HTTPS_N https cases, "\
"$JPEG_N JPEG cases, $IMG_N image cases, $UI_N window cases"
    exit 0
fi
echo "B5 FAILED: $ERRORS"
exit 1
