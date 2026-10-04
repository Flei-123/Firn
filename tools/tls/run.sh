#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/tls/run.sh -- TLS 1.3 and the cryptography under it, held against
# implementations this repository did not write.
#
#   1. the primitives (big integers, X25519, ChaCha20-Poly1305, AES-GCM,
#      SHA-384/512, HKDF, RSA, ECDSA verify) against Python's
#      `cryptography`, with counter-checks          (crypto_check.py)
#   2. X.509 chains and their refusals               (cert_check.py)
#   3. the TLS CLIENT against openssl s_server and, when there is a
#      route, real hosts                              (tls_check.py)
#   4. ECDSA P-256 SIGNING: RFC 6979 vectors, OpenSSL verifies every
#      signature                                      (p256_check.py)
#   5. the TLS SERVER against openssl s_client, curl and Python's ssl,
#      HelloRetryRequest, KeyUpdate, and the refusals (server_check.py)
#   6. the TLS 1.2 half of the client: suites x groups x signature schemes
#      against openssl, 512 KiB through Python's ssl, man-in-the-middle and
#      hostile-server counter-checks, a fuzz run, real hosts (tls12_check.py)
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
export FIRNLIB="$(pwd)/lib"
WORK=".tls-work"
mkdir -p "$WORK"
rc=0
build() {
    "$FIRNC" -o "$WORK/$1" "$2" > "$WORK/$1.log" 2>&1 || { echo "  FAIL $2 does not build"; grep -v RWX "$WORK/$1.log" | head -5; rc=1; }
}
build cry lib/std/crypto/crypto_main.fi
build x509 lib/tls/x509_main.fi
build tls lib/tls/tls_main.fi
build p256 tools/tls/p256_main.fi
build tlsserver tools/tls/server_main.fi
[ $rc -eq 0 ] || exit 1
python3 tools/tls/crypto_check.py "$WORK/cry" 2>&1 | tail -2 || rc=1
python3 tools/tls/cert_check.py "$WORK/x509" 2>&1 | tail -3 || rc=1
python3 tools/tls/tls_check.py "$WORK/tls" 2>&1 | tail -3 || rc=1
python3 tools/tls/p256_check.py "$WORK/p256" 2>&1 | tail -3 || rc=1
python3 tools/tls/server_check.py "$WORK/tlsserver" "$WORK/srv" 2>&1 | tail -2 || rc=1
python3 tools/tls/tls12_check.py "$WORK/tls" 2>&1 | tail -6 || rc=1
exit $rc
