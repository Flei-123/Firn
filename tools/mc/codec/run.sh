#!/usr/bin/env bash
# tools/mc/codec/run.sh -- mc_codec (framing, compression, AES-128-CFB8) against an
# INDEPENDENT python peer (own VarInt, own framing, zlib, `cryptography` CFB8).
#
#  * echo: python sends ~60 packets (sizes around the threshold, 0 .. 1.9 MB, random /
#    zero / text data, random write splits) and Firn echoes each back: byte identical,
#    for {no compression, threshold 256, threshold 1} x {plain, encrypted};
#  * reject: hostile frames (length > 2 MiB, zero length, endless VarInt, inflated size
#    above the cap, bad zlib checksum, wrong announced size, peer closes mid-packet) must
#    end in a clean error, not in a hang or a crash;
#  * counter-check: a Firn build with a WRONG key must make the echo test fail.
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0
python3 -c "import cryptography" 2>/dev/null || { echo "SKIP: python cryptography missing"; exit 0; }
for stage in "" "--no-opt"; do
    name=${stage:-release-fast}
    if ! $FIRNC $stage -o "$W/peer" tools/mc/codec/peer.fi 2>"$W/err"; then
        echo "  FAIL  $name: peer.fi does not compile"; head -10 "$W/err"; ERR=1; continue
    fi
    thrs="- 256 1"; encs="0 1"
    # the slow unoptimised stage only runs the hardest combination
    [ -n "$stage" ] && { thrs="256"; encs="1"; }
    for thr in $thrs; do
        for enc in $encs; do
            if out=$(python3 -I tools/mc/codec/peer.py "$W/peer" "$thr" "$enc" 7 2>&1); then
                echo "  ok    [$name] $out" | cut -c1-200
            else
                echo "  FAIL  [$name] thr=$thr enc=$enc: $out" | head -5; ERR=1
            fi
        done
    done
    python3 -I tools/mc/codec/reject.py "$W/peer" || ERR=1
done
# counter-check: the Firn side uses a different key than the peer -> the test MUST fail
sed 's/key\[i\] = hexval(rt.ld8(ka, i \* 2)) \* 16 as u8 + hexval(rt.ld8(ka, i \* 2 + 1))/key[i] = hexval(rt.ld8(ka, i * 2)) * 16 as u8 + 0 as u8/' tools/mc/codec/peer.fi > "$W/peer_bad.fi"
if $FIRNC -o "$W/peer_bad" "$W/peer_bad.fi" 2>/dev/null; then
    if python3 -I tools/mc/codec/peer.py "$W/peer_bad" 256 1 7 >/dev/null 2>&1; then
        echo "  FAIL  counter-check: a wrong key was NOT detected"; ERR=1
    else
        echo "  ok    counter-check: a wrong key is detected"
    fi
else
    echo "  FAIL  counter-check build"; ERR=1
fi
[ $ERR -eq 0 ] && echo "RESULT mc-codec: ok" || echo "RESULT mc-codec: FAIL"
exit $ERR
