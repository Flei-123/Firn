#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/desktop/run.sh -- the desktop libraries (docs/DESKTOP.md): net.dbus, desktop.tray, desktop.notify,
# desktop.watch, desktop.autostart, files dropped on a window, the clipboard, audio, the command line
# channel of appkit.single_instance -- on Linux against programs nobody here wrote (libdbus through
# dbus-python, GTK 3, GLib, a real pulseaudio, parec, pactl, ffmpeg, python-xlib) and, for the Windows
# builds, under Wine with MinGW's C runtime as the independent reader.
#
#   bash tools/desktop/run.sh            everything that this machine can run
#   bash tools/desktop/run.sh linux      the Linux half only
#   bash tools/desktop/run.sh windows    the Windows half only (Wine)
#
# A missing tool SKIPs its part with the reason; any wrong answer is a FAIL and the exit code is 1.
# Heavy: it builds about thirty programs; run it through /root/jarvis/bin/heavy on the shared build machine.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC="${FIRNC:-$ROOT/compiler/target/release/firnc}"
export FIRNLIB="$ROOT/lib"
WHICH="${1:-all}"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
rc=0
have() { command -v "$1" >/dev/null 2>&1; }
build() {      # build <stage opt flags> <out> <src>
    local flags="$1" out="$2" src="$3"
    if ! "$FIRNC" $flags -o "$out" "$src" 2> "$W/b.log"; then
        echo "  FAIL  $src does not build ($flags)"; grep -v RWX "$W/b.log" | head -5; rc=1; return 1
    fi
}
step() { echo; echo "== $* =="; }
run() { "$@" || rc=1; }

if [ "$WHICH" = all ] || [ "$WHICH" = linux ]; then
    step "1. build the Linux drivers in three stages (release-fast, --no-opt, dev-fast)"
    for SRC in dbus_main tray_main notify_main autostart_main audio_main sink_main drop_main clip_main clipboard_main; do
        for STAGE in "opt:--opt-level=release-fast" "noopt:--no-opt" "dev:--opt-level=dev-fast"; do
            build "${STAGE#*:}" "$W/${SRC}_${STAGE%%:*}" "tools/desktop/$SRC.fi" || true
        done
    done
    build "--opt-level=release-fast" "$W/mp3_ref" lib/ton/mp3_main.fi || true
    echo "   built"

    step "2. python3 tools/desktop/platforms.py (the five platform files of every module)"
    run python3 tools/desktop/platforms.py --firnc "$FIRNC"
    run python3 tools/appkit/platforms.py --firnc "$FIRNC"

    if have dbus-daemon && python3 -c "import dbus, gi" 2>/dev/null; then
        for STAGE in opt dev; do
            step "3. net.dbus against libdbus ($STAGE build): every type both ways, 1 MiB, errors, the queue"
            run python3 tools/desktop/dbus_check.py "$W/dbus_main_$STAGE"
            step "4. desktop.tray against a StatusNotifierWatcher and a dbusmenu client in libdbus ($STAGE)"
            run python3 tools/desktop/tray_check.py "$W/tray_main_$STAGE"
            step "5. desktop.notify against a notification server in libdbus ($STAGE)"
            run python3 tools/desktop/notify_check.py "$W/notify_main_$STAGE"
        done
    else
        echo "  SKIP  dbus-daemon or python3 dbus/gi missing: net.dbus, tray and notify were not checked"
    fi

    step "6. desktop.autostart: Python's reading of the Desktop Entry spec and GLib (gio launch)"
    if have gio; then run python3 tools/desktop/autostart_check.py "$W/autostart_main_opt"; else echo "  SKIP  gio missing"; fi

    step "7. audio: file sinks, volume, pause, stop, ffmpeg, a real pulseaudio + parec + pactl"
    if have pulseaudio && have parec && have pactl; then
        run python3 tools/desktop/audio_check.py "$W/audio_main_opt" "$W/mp3_ref" "$ROOT"
    else
        echo "  SKIP  pulseaudio/parec/pactl missing"
    fi

    step "8. files dropped on a window: XDND, GTK 3 as a real source and a python-xlib source"
    if have Xvfb && have xdotool && python3 -c "import Xlib, gi" 2>/dev/null; then
        run python3 tools/desktop/drop_check.py "$W/drop_main_opt"
    else
        echo "  SKIP  Xvfb/xdotool/python-xlib/gi missing"
    fi

    step "9. the clipboard against GTK 3's"
    if have Xvfb && python3 -c "import gi" 2>/dev/null; then
        run python3 tools/desktop/clip_check.py "$W/clip_main_opt"
    else
        echo "  SKIP  Xvfb or gi missing"
    fi

    step "9b. std.clipboard (the windowless wrapper) against GTK 3's: text, 1.5 MiB (INCR), PNG, both directions, clear"
    if have Xvfb && python3 -c "import gi" 2>/dev/null; then
        run python3 tools/desktop/clipboard_check.py "$W/clipboard_main_opt"
    else
        echo "  SKIP  Xvfb or gi missing"
    fi

    step "9c. std.clipboard: the Linux, Android and browser files agree, and the program builds for five targets"
    run python3 tools/desktop/clipboard_platforms.py --firnc "$FIRNC"
fi

if [ "$WHICH" = all ] || [ "$WHICH" = windows ]; then
    WINE=""
    for c in wine wine64; do have "$c" && { WINE=$c; break; }; done
    if [ -n "$WINE" ] && have x86_64-w64-mingw32-ld; then
        export WINEPREFIX=${WINEPREFIX:-${HOME:-$(getent passwd "$(id -u)" | cut -d: -f6)}/.wine-firn}
        export WINEDEBUG=${WINEDEBUG:--all}
        step "10. build the Windows programs (x86_64-windows; window programs through win32.fi)"
        WB="$W/win"; mkdir -p "$WB/wn/window"
        ln -s "$ROOT/lib/window/win32.fi" "$WB/wn/window/backend.fi"
        for SRC in tray_main wintray_poke notify_main autostart_main sink_main audio_main clipboard_main; do
            build "--target=x86_64-windows --opt-level=dev-fast" "$WB/$SRC.exe" "tools/desktop/$SRC.fi" || true
        done
        for SRC in windrop_main clip_main; do
            cp "tools/desktop/$SRC.fi" "$WB/wn/"
            build "--target=x86_64-windows --opt-level=dev-fast" "$WB/$SRC.exe" "$WB/wn/$SRC.fi" || true
        done
        build "--target=x86_64-windows --opt-level=dev-fast" "$WB/t2220.exe" tests/2220_desktop_watch.fi || true
        build "--target=x86_64-windows --opt-level=dev-fast" "$WB/t2221.exe" tests/2221_appkit_ipc.fi || true
        if have x86_64-w64-mingw32-gcc; then
            x86_64-w64-mingw32-gcc -municode -O1 -o "$WB/argv_dump.exe" tools/desktop/argv_dump.c && \
            x86_64-w64-mingw32-gcc -municode -O1 -o "$WB/launch.exe" tools/desktop/launch.c || rc=1
        fi

        step "11. desktop.watch and the command line channel under Wine (tests 2220, 2221)"
        run $WINE "$WB/t2220.exe"
        run $WINE "$WB/t2221.exe"
        echo "   both exit 0"

        step "12. desktop.autostart (the Run key): reg query and the C runtime's argv"
        if [ -f "$WB/argv_dump.exe" ]; then
            run python3 tools/desktop/autostart_check_win.py "$WB/autostart_main.exe" "$WB/argv_dump.exe" "$WB/launch.exe"
        else
            echo "  SKIP  mingw gcc missing"
        fi

        step "13. desktop.tray and desktop.notify: Wine's explorer as the shell, the icon read off the screen"
        if have Xvfb && have xwd; then
            run python3 tools/desktop/tray_check_win.py "$WB/tray_main.exe" "$WB/wintray_poke.exe" "$WB/notify_main.exe" "$ROOT"
        else
            echo "  SKIP  Xvfb/xwd missing"
        fi

        step "14. audio: waveOut -> Wine's ALSA driver -> ALSA's file plugin (bit for bit)"
        run python3 tools/desktop/audio_check_win.py "$WB/sink_main.exe" "$WB/audio_main.exe" "$W/mp3_ref" "$ROOT"

        step "15. the Windows clipboard against GTK 3's (Wine bridges it to the X selections)"
        if have Xvfb && python3 -c "import gi" 2>/dev/null; then
            run python3 tools/desktop/clip_check.py "$WB/clip_main.exe" wine
        else
            echo "  SKIP  Xvfb or gi missing"
        fi

        step "15b. std.clipboard as a Windows program against GTK 3's (text, 1.5 MiB, PNG, clear)"
        if have Xvfb && python3 -c "import gi" 2>/dev/null; then
            run python3 tools/desktop/clipboard_check.py "$WB/clipboard_main.exe" wine
        else
            echo "  SKIP  Xvfb or gi missing"
        fi

        step "16. WM_DROPFILES with a real HDROP (the program posts it to itself)"
        if have Xvfb; then
            D=$((RANDOM % 40 + 360)); Xvfb ":$D" -screen 0 800x600x24 >/dev/null 2>&1 & XV=$!
            sleep 1
            DISPLAY=":$D" $WINE explorer /desktop=firn,800x600 "$WB/windrop_main.exe" > "$W/windrop.out" 2>&1 &
            for _ in $(seq 1 60); do grep -q DONE "$W/windrop.out" 2>/dev/null && break; sleep 0.5; done
            kill $XV 2>/dev/null; wineserver -k 2>/dev/null
            if grep -q '^PATH C:\\Users\\me\\a b.txt' "$W/windrop.out" && grep -q "^PATH D:.*gr.*e.*\.txt" "$W/windrop.out" \
               && grep -q '^PATH C:\\only.txt' "$W/windrop.out" && [ "$(grep -c '^PATH' "$W/windrop.out")" = 4 ]; then
                echo "  OK    four paths, blanks and UTF-8 intact, in two drops"
            else
                echo "  FAIL  WM_DROPFILES: $(cat "$W/windrop.out")"; rc=1
            fi
        else
            echo "  SKIP  Xvfb missing"
        fi
    else
        echo "  SKIP  the Windows half: Wine or the MinGW linker is missing"
    fi
fi
echo
[ $rc -eq 0 ] && echo "desktop: all parts passed" || echo "desktop: FAILED"
exit $rc
