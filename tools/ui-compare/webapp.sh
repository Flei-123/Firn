#!/bin/sh
# Run a web reference app in Chromium for the harness.
#   webapp.sh <app> <theme> [shot.png]      shot given -> headless, writes the PNG (1100x700) and exits
#                                           no shot    -> interactive window (--app mode) on $DISPLAY
# Needs a static server for $UIVGL_DIR on $UIVGL_HTTP (default http://127.0.0.1:14290), started by run_all.sh
# (python3 -m http.server). Profile dir is wiped on every start = cold start, like a first launch.
APP="$1"; THEME="$2"; SHOT="$3"
BASE="${UIVGL_HTTP:-http://127.0.0.1:14290}"
D="${UIVGL_DIR:-$(cd "$(dirname "$0")" && pwd)}"
P="$D/.chrome-profile-$APP"
rm -rf "$P"
URL="$BASE/apps/$APP/web/index.html?theme=$THEME"
COMMON="--no-sandbox --disable-gpu --hide-scrollbars --no-first-run --no-default-browser-check --user-data-dir=$P"
if [ -n "$SHOT" ]; then
    exec chromium $COMMON --headless=new --window-size=1100,700 --force-device-scale-factor=1 \
        --virtual-time-budget=500 --screenshot="$SHOT" "$URL&shot=1"
fi
exec chromium $COMMON --app="$URL" --window-size=1100,700 --window-position=0,0 --force-device-scale-factor=1
