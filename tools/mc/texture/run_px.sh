#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
JAR=${MC_VANILLA_JAR:-/root/.w5/jars/client-1.20.1.jar}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-texture-px: FAIL"; exit 1; }
[ -f "$JAR" ] || fail "missing vanilla jar $JAR"
$FIRNC --opt-level=dev-fast -o "$W/dump" tools/mc/texture/atlas_dump.fi 2>"$W/err" || { head -10 "$W/err"; fail "atlas_dump does not compile"; }
MC_VANILLA_JAR="$JAR" MC_VANILLA_CACHE="$W/vcache" "$W/dump" "$W/atlas.bin" "$W/map.txt" || fail "atlas_dump crashed"
python3 -I tools/mc/texture/audit_atlas_px.py "$JAR" "$W/atlas.bin" "$W/map.txt" || fail "atlas differs from the vanilla jar"
python3 -I - "$W/atlas.bin" <<'PY'
import sys
p = sys.argv[1]
b = bytearray(open(p, "rb").read())
b[(40 * 512 + 40) * 4] ^= 0x55
open(p, "wb").write(b)
PY
if python3 -I tools/mc/texture/audit_atlas_px.py "$JAR" "$W/atlas.bin" "$W/map.txt" >/dev/null; then fail "negative control: corrupted atlas passed"; fi
echo "  negative control: corrupted atlas rejected"
echo "RESULT mc-texture-px: ok"
