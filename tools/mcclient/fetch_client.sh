#!/usr/bin/env bash
# fetch_client.sh <dest_dir> [version=1.20.1]
#
# Test helper: downloads the OFFICIAL Minecraft Java client JAR for <version> into <dest_dir>/client.jar
# (piston-meta version_manifest_v2.json -> version JSON -> downloads.client, SHA-1 verified).
# The JAR is a local test input of the vanilla-pack default (lib/mc/mc_vanilla.fi). Never commit it.
# Optional: MCCLIENT_JAR_CACHE=<dir> keeps a verified copy as client-<version>.jar and reuses it.
set -euo pipefail

DEST="${1:?usage: fetch_client.sh <dest_dir> [version]}"
VERSION="${2:-1.20.1}"
MANIFEST_URL="https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"

mkdir -p "$DEST"
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT

CACHE="${MCCLIENT_JAR_CACHE:-}"
if [ -n "$CACHE" ] && [ -f "$CACHE/client-$VERSION.jar" ] && [ -f "$CACHE/client-$VERSION.sha1" ]; then
  want=$(cat "$CACHE/client-$VERSION.sha1")
  have=$(sha1sum "$CACHE/client-$VERSION.jar" | cut -d' ' -f1)
  if [ "$want" = "$have" ]; then
    cp "$CACHE/client-$VERSION.jar" "$DEST/client.jar"
    echo "fetch_client: reused cached $VERSION jar (sha1 $have)"
    exit 0
  fi
fi

curl -fsS --retry 3 --max-time 60 -o "$T/manifest.json" "$MANIFEST_URL"
VURL=$(node -e '
  const m = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"));
  const v = m.versions.find(x => x.id === process.argv[2]);
  if (!v) { console.error("version not in manifest: " + process.argv[2]); process.exit(1); }
  console.log(v.url);
' "$T/manifest.json" "$VERSION")
curl -fsS --retry 3 --max-time 60 -o "$T/version.json" "$VURL"
read -r JURL JSHA < <(node -e '
  const v = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"));
  const s = v.downloads && v.downloads.client;
  if (!s) { console.error("no client download for this version"); process.exit(1); }
  console.log(s.url + " " + s.sha1);
' "$T/version.json")

curl -fsS --retry 3 --max-time 300 -o "$T/client.jar" "$JURL"
GOT=$(sha1sum "$T/client.jar" | cut -d' ' -f1)
if [ "$GOT" != "$JSHA" ]; then
  echo "fetch_client: SHA-1 mismatch (want $JSHA, got $GOT)" >&2
  exit 1
fi
mv "$T/client.jar" "$DEST/client.jar"
if [ -n "$CACHE" ]; then
  mkdir -p "$CACHE"
  cp "$DEST/client.jar" "$CACHE/client-$VERSION.jar"
  echo "$JSHA" > "$CACHE/client-$VERSION.sha1"
fi
echo "fetch_client: downloaded $VERSION jar (sha1 $GOT verified) -> $DEST/client.jar"
