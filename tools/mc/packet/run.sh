#!/usr/bin/env bash
# tools/mc/packet/run.sh -- cross-check of lib/mc/mc_packet.fi against the protodef
# serializer of node-minecraft-protocol (minecraft-data protocol.json, 1.20.1).
#
#   1. gen.cjs serialises random packets of 12 real 1.20.1 packet types and writes
#      the expected canonical text from the INPUT values.
#   2. xcheck.fi (Firn) reads the same bytes with mc_packet, prints the text and
#      re-encodes every packet; the text must be identical and RT must be ok.
#   3. Damaged inputs (truncation, flipped bytes): Firn may report errors but must
#      neither crash nor hang (exit code 0 or 1 only).
#   4. Counter-check: a damaged expectation MUST be reported as different.
#   5. Speed numbers (bench.fi), printed as `BENCH ...`.
#
# Usage: bash tools/mc/packet/run.sh [count]
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
export NODE_PATH=${NODE_PATH:-/root/firn/tools/mcserver/node_modules}
COUNT=${1:-1200}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0

node tools/mc/packet/gen.cjs "$W/ref" "$COUNT" > "$W/gen.log" 2>&1 || { cat "$W/gen.log"; echo "RESULT mc-packet: FAIL (node reference)"; exit 1; }
echo "  $(cat "$W/gen.log")"

# damaged inputs: truncated records / flipped bytes (deterministic)
python3 -I - "$W/ref/packets.bin" "$W/trunc.bin" "$W/flip.bin" <<'EOF'
import sys, random
src, trunc, flip = sys.argv[1:4]
data = open(src, 'rb').read()
rnd = random.Random(7)
recs = []
i = 0
while i + 5 <= len(data):
    n = int.from_bytes(data[i+1:i+5], 'big')
    recs.append((data[i], data[i+5:i+5+n])); i += 5 + n
with open(trunc, 'wb') as t, open(flip, 'wb') as f:
    for tag, body in recs:
        cut = body[:rnd.randrange(0, len(body))] if len(body) else body
        t.write(bytes([tag]) + len(cut).to_bytes(4, 'big') + cut)
        b = bytearray(body)
        for _ in range(1 + rnd.randrange(3)):
            if b: b[rnd.randrange(len(b))] = rnd.randrange(256)
        f.write(bytes([tag]) + len(b).to_bytes(4, 'big') + bytes(b))
EOF

for stage in "" "--no-opt" "--opt-level=dev-fast"; do
    name=${stage:-release-fast}
    if ! $FIRNC $stage -o "$W/xcheck" tools/mc/packet/xcheck.fi 2>"$W/err"; then
        echo "  FAIL  $name: xcheck.fi does not compile"; head -10 "$W/err"; ERR=1; continue
    fi
    "$W/xcheck" "$W/ref/packets.bin" > "$W/got.txt" 2>"$W/stderr"; rc=$?
    if [ $rc -eq 0 ] && cmp -s "$W/got.txt" "$W/ref/expect.txt"; then
        echo "  ok    $name: $COUNT packets == protodef reference (read + re-encode byte exact)"
    else
        echo "  FAIL  $name: rc=$rc"; diff "$W/got.txt" "$W/ref/expect.txt" | head -6 | cut -c1-200; ERR=1
    fi
    for d in trunc flip; do
        timeout 60 "$W/xcheck" "$W/$d.bin" > "$W/$d.out" 2>"$W/stderr"; rc=$?
        if [ $rc -gt 1 ]; then echo "  FAIL  $name: damaged input ($d) -> exit $rc (crash/hang)"; ERR=1
        else echo "  ok    $name: damaged input ($d): exit $rc, $(grep -c 'ERR=' "$W/$d.out") reader errors, no crash"; fi
    done
done

# counter-check
sed '1s/id=/id=9/' "$W/ref/expect.txt" > "$W/bad.txt"
if cmp -s "$W/got.txt" "$W/bad.txt"; then echo "  FAIL  counter-check: damaged expectation compared equal"; ERR=1
else echo "  ok    counter-check: damaged expectation is detected"; fi
# counter-check 2: a damaged binary must not reproduce the reference text
if cmp -s "$W/flip.out" "$W/ref/expect.txt"; then echo "  FAIL  counter-check: flipped input gave the reference text"; ERR=1
else echo "  ok    counter-check: flipped input differs from the reference"; fi

# speed
if [ -f tools/mc/packet/bench.fi ]; then
    if $FIRNC -o "$W/bench" tools/mc/packet/bench.fi 2>"$W/err"; then "$W/bench" | sed 's/^/  /'; else head -5 "$W/err"; fi
fi

[ $ERR -eq 0 ] && echo "RESULT mc-packet: ok" || echo "RESULT mc-packet: FAIL"
exit $ERR
