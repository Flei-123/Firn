#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
FIRNC=$(readlink -f "$FIRNC"); export FIRNC
export FIRNLIB="$ROOT/lib"
OUT=${OUT:-/root/projects/minecraft-neu/dist/engine}
VERSION=${VERSION:-$(date +%Y%m%d).$(git rev-parse --short HEAD)}
BASE_URL=${BASE_URL:-https://fleilauncher.fleitec.com/download/engine}
OPT=${OPT:-release-fast}
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
fail() { echo "FAIL: $1"; echo "RESULT mc-engine-release: FAIL"; exit 1; }
mkdir -p "$OUT" "$W/win/sounds" "$W/linux/sounds"

"$FIRNC" --target=x86_64-windows --win-subsystem=windows --opt-level="$OPT" -o "$W/win/mcplay.exe" demos/mcplay/main.fi 2>"$W/e1" || { head -10 "$W/e1"; fail "windows build"; }
"$FIRNC" --opt-level="$OPT" -o "$W/linux/mcplay" demos/mcplay/main.fi 2>"$W/e2" || { head -10 "$W/e2"; fail "linux build"; }
head -c 2 "$W/win/mcplay.exe" | grep -q MZ || fail "mcplay.exe is not a PE file"
head -c 4 "$W/linux/mcplay" | grep -q ELF || fail "mcplay is not an ELF file"

"$FIRNC" -o "$W/gen" tools/sounds/gen/main.fi 2>"$W/e3" || { head -10 "$W/e3"; fail "sound generator"; }
"$W/gen" tools/sounds/mc.json "$W/win/sounds" >/dev/null || fail "sound bank"
cp -r "$W/win/sounds/." "$W/linux/sounds/"
[ -s "$W/win/sounds/bank.json" ] || fail "no bank.json"

if find "$W/win" "$W/linux" \( -iname '*.jar' -o -name blockstates -o -name textures -o -name pack.mcmeta -o -iname '*.ogg' \) | grep -q .; then fail "package contains Mojang-style assets"; fi

python3 -I - "$W" "$OUT" "$VERSION" "$BASE_URL" <<'PY' || fail "packaging"
import hashlib, json, os, sys, zipfile
work, out, version, base = sys.argv[1:5]
manifest = {"version": version}
for plat, exe in (("windows", "mcplay.exe"), ("linux", "mcplay")):
    src = os.path.join(work, "win" if plat == "windows" else "linux")
    name = f"engine-{plat}.zip"
    path = os.path.join(out, name)
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        for root, _, files in sorted(os.walk(src)):
            for f in sorted(files):
                full = os.path.join(root, f)
                info = zipfile.ZipInfo(os.path.relpath(full, src), (2026, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                info.external_attr = (0o755 if f == exe else 0o644) << 16
                with open(full, "rb") as fh:
                    z.writestr(info, fh.read())
    data = open(path, "rb").read()
    manifest[plat] = {"url": f"{base}/{name}", "sha256": hashlib.sha256(data).hexdigest(), "size": len(data)}
with open(os.path.join(out, "manifest.json"), "w") as fh:
    json.dump(manifest, fh, indent=2)
    fh.write("\n")
PY
SIGN_KEY=${SIGN_KEY:-/root/.fleilauncher-engine-signing/engine_signing.pem}
if [ -f "$SIGN_KEY" ]; then
  openssl pkeyutl -sign -rawin -inkey "$SIGN_KEY" -in "$OUT/manifest.json" -out "$W/manifest.sig" || fail "manifest signing"
  [ "$(stat -c %s "$W/manifest.sig")" = 64 ] || fail "signature is not 64 bytes"
  od -An -tx1 -v "$W/manifest.sig" | tr -d ' \n' > "$OUT/manifest.json.sig"
  openssl pkey -in "$SIGN_KEY" -pubout -out "$W/pub.pem"
  openssl pkeyutl -verify -rawin -pubin -inkey "$W/pub.pem" -in "$OUT/manifest.json" -sigfile "$W/manifest.sig" >/dev/null || fail "signature self-check"
elif [ "${ALLOW_UNSIGNED:-0}" = 1 ]; then
  rm -f "$OUT/manifest.json.sig"
else
  fail "signing key $SIGN_KEY missing (set SIGN_KEY or ALLOW_UNSIGNED=1)"
fi
echo "  ok  $OUT: $(ls "$OUT" | tr '\n' ' ')"
echo "RESULT mc-engine-release: ok"
