#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/newapp.sh -- make a new Firn program on the appkit.
#
#   bash tools/newapp.sh <Name> <app-id> [options]
#
#   Name      the display name, e.g. "FleiLauncher"
#   app-id    lower case id: letters, digits, '-' (the name in the store --
#             exe:<id>, bin:<id> -- and the folder of the settings)
#
# Options:
#   --dir DIR            create the project here (default: ./<app-id>)
#   --vendor NAME        vendor folder on Windows (default: FleiTec)
#   --store-url URL      the store (default: https://store.fleitec.com/)
#   --store-key HEX      the store's Ed25519 public key, 64 hex digits
#   --store-key-file F   ... read from a file holding 32 raw octets or 64 hex digits
#   --android-id ID      reverse-DNS package for the Android build (default: org.firn.<id>)
#   --version V          the first version (default: 0.1.0)
#   --force              write into a directory that is not empty
#
# THE TRUST ANCHOR. Without --store-key[-file], the key comes from (in order)
# $FIRN_STORE_KEY, /srv/store/oeffentlich.key when this machine hosts the
# store, or <store-url>oeffentlich.key over https. The last one is TRUST ON
# FIRST USE: the key is shown with its fingerprint; check it against the store's
# (`store key`) if it matters. The key is compiled into the program, so a
# wrong one means a program that never accepts an update.
#
# What it makes: templates/app/ with the placeholders filled in -- a window
# program with a sidebar, Settings, Updates and About pages and an update
# banner, build scripts for Linux, Windows and Android, release.sh, and the
# texts in English and German. Needs the compiler (compiler/target/release/
# firnc) to build it.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
usage() { sed -n '3,30p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
[ $# -ge 2 ] || usage
NAME=$1; ID=$2; shift 2
DIR=""; VENDOR=FleiTec; STORE_URL=https://store.fleitec.com/; KEY=""; KEYFILE=""
ANDROID_ID=""; VERSION=0.1.0; FORCE=0
while [ $# -gt 0 ]; do
    case "$1" in
        --dir) DIR=$2; shift 2 ;;
        --vendor) VENDOR=$2; shift 2 ;;
        --store-url) STORE_URL=$2; shift 2 ;;
        --store-key) KEY=$2; shift 2 ;;
        --store-key-file) KEYFILE=$2; shift 2 ;;
        --android-id) ANDROID_ID=$2; shift 2 ;;
        --version) VERSION=$2; shift 2 ;;
        --force) FORCE=1; shift ;;
        -h|--help) usage ;;
        *) echo "unknown option $1" >&2; usage ;;
    esac
done
if ! printf '%s' "$ID" | grep -Eq '^[a-z0-9][a-z0-9-]{0,39}$'; then
    echo "app-id must be lower case letters, digits and '-' (at most 40): '$ID'" >&2; exit 2
fi
if ! printf '%s' "$VERSION" | grep -Eq '^[0-9]+(\.[0-9]+){0,3}(-[0-9A-Za-z.-]+)?$'; then
    echo "--version must look like 0.1.0 or 0.2.0-beta.1: '$VERSION'" >&2; exit 2
fi
if printf '%s%s%s' "$NAME" "$VENDOR" "$STORE_URL" | grep -q '[|"\\&]'; then
    echo "Name, vendor and store address must not contain | \" \\ or &" >&2; exit 2
fi
[ -n "$DIR" ] || DIR=$PWD/$ID
[ -n "$ANDROID_ID" ] || ANDROID_ID=org.firn.$(printf '%s' "$ID" | tr -d '-')
case "$STORE_URL" in */) ;; *) STORE_URL=$STORE_URL/ ;; esac

# ---- the trust anchor
KEY_SOURCE=""
hex_of_file() { # 32 raw octets or 64 hex digits
    python3 - "$1" <<'PY'
import sys
raw = open(sys.argv[1], "rb").read()
if len(raw) == 32:
    print(raw.hex())
else:
    t = raw.strip()
    if len(t) == 64:
        int(t, 16)
        print(t.decode().lower())
    else:
        sys.exit("neither 32 octets nor 64 hex digits")
PY
}
if [ -n "$KEY" ]; then KEY_SOURCE="--store-key"
elif [ -n "$KEYFILE" ]; then KEY=$(hex_of_file "$KEYFILE"); KEY_SOURCE="$KEYFILE"
elif [ -n "${FIRN_STORE_KEY:-}" ]; then KEY=$FIRN_STORE_KEY; KEY_SOURCE='$FIRN_STORE_KEY'
elif [ -f /srv/store/oeffentlich.key ]; then KEY=$(hex_of_file /srv/store/oeffentlich.key); KEY_SOURCE=/srv/store/oeffentlich.key
else
    tmp=$(mktemp)
    if curl -fsS -m 20 -o "$tmp" "${STORE_URL}oeffentlich.key"; then
        KEY=$(hex_of_file "$tmp"); KEY_SOURCE="${STORE_URL}oeffentlich.key (trust on first use)"
        rm -f "$tmp"
    else
        rm -f "$tmp"
        echo "cannot find the store's public key: give --store-key HEX or --store-key-file FILE" >&2; exit 2
    fi
fi
printf '%s' "$KEY" | grep -Eq '^[0-9a-f]{64}$' || { echo "the store key is not 64 hex digits: '$KEY'" >&2; exit 2; }
echo "store key  $KEY   (from $KEY_SOURCE)"

# ---- the project
if [ -e "$DIR" ] && [ -n "$(ls -A "$DIR" 2>/dev/null)" ] && [ "$FORCE" -ne 1 ]; then
    echo "$DIR is not empty (--force to write into it)" >&2; exit 2
fi
mkdir -p "$DIR"
FIRN_ROOT=$ROOT
python3 - "$ROOT/templates/app" "$DIR" "$NAME" "$ID" "$VENDOR" "$STORE_URL" "$KEY" "$KEY_SOURCE" "$ANDROID_ID" "$VERSION" "$FIRN_ROOT" <<'PY'
import os
import shutil
import sys

src, dst, name, ident, vendor, store, key, key_source, android_id, version, root = sys.argv[1:]
subs = {
    "@NAME@": name, "@ID@": ident, "@VENDOR@": vendor, "@STORE_URL@": store,
    "@STORE_KEY@": key, "@KEY_SOURCE@": key_source, "@ANDROID_ID@": android_id,
    "@VERSION@": version, "@FIRN_ROOT@": root,
}
count = 0
for base, dirs, files in os.walk(src):
    rel = os.path.relpath(base, src)
    out = dst if rel == "." else os.path.join(dst, rel)
    os.makedirs(out, exist_ok=True)
    for f in files:
        p = os.path.join(base, f)
        q = os.path.join(out, f)
        data = open(p, "rb").read()
        try:
            text = data.decode("utf-8")
        except UnicodeDecodeError:
            open(q, "wb").write(data)       # a binary file is copied as it is
        else:
            for k, v in subs.items():
                text = text.replace(k, v)
            open(q, "w", encoding="utf-8").write(text)
        shutil.copymode(p, q)
        count += 1
print("wrote %d files" % count)
PY
echo "project    $DIR"
cat <<EOF

Next:
  cd $DIR
  bash build.sh                 # build/$ID  (Linux; needs the compiler: $ROOT/compiler/target/release/firnc)
  build/$ID                     # run it
  bash release.sh               # a dry-run release into a throw-away store
EOF
