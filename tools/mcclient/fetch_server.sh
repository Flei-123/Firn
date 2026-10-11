#!/usr/bin/env bash
# fetch_server.sh <dest_dir> [version=1.20.1]
#
# Downloads the OFFICIAL Minecraft Java server for <version> into <dest_dir>/server.jar
# through piston-meta.mojang.com (version_manifest_v2.json -> version JSON ->
# downloads.server.url) and verifies the SHA-1 given in that JSON.
#
# Clean-room rule: the JAR is only downloaded and later started as a black-box test
# subject. It is never opened, unpacked, decompiled, inspected or put into the repo.
#
# Optional: MCCLIENT_JAR_CACHE=<dir> keeps a verified copy there and reuses it.
set -euo pipefail

DEST="${1:?usage: fetch_server.sh <dest_dir> [version]}"
VERSION="${2:-1.20.1}"
MANIFEST_URL="https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"

mkdir -p "$DEST"
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT

CACHE="${MCCLIENT_JAR_CACHE:-}"
if [ -n "$CACHE" ] && [ -f "$CACHE/server-$VERSION.jar" ] && [ -f "$CACHE/server-$VERSION.sha1" ]; then
  want=$(cat "$CACHE/server-$VERSION.sha1")
  have=$(sha1sum "$CACHE/server-$VERSION.jar" | cut -d' ' -f1)
  if [ "$want" = "$have" ]; then
    cp "$CACHE/server-$VERSION.jar" "$DEST/server.jar"
    echo "fetch_server: reused cached $VERSION jar (sha1 $have)"
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
  const s = v.downloads && v.downloads.server;
  if (!s) { console.error("no server download for this version"); process.exit(1); }
  console.log(s.url + " " + s.sha1);
' "$T/version.json")

curl -fsS --retry 3 --max-time 300 -o "$T/server.jar" "$JURL"
GOT=$(sha1sum "$T/server.jar" | cut -d' ' -f1)
if [ "$GOT" != "$JSHA" ]; then
  echo "fetch_server: SHA-1 mismatch (want $JSHA, got $GOT)" >&2
  exit 1
fi
mv "$T/server.jar" "$DEST/server.jar"
if [ -n "$CACHE" ]; then
  mkdir -p "$CACHE"
  cp "$DEST/server.jar" "$CACHE/server-$VERSION.jar"
  echo "$JSHA" > "$CACHE/server-$VERSION.sha1"
fi
echo "fetch_server: downloaded $VERSION jar (sha1 $GOT verified) -> $DEST/server.jar"
