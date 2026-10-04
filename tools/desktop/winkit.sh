#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/desktop/winkit.sh -- the Windows half of tools/desktop/run.sh as a ZIP for a real Windows PC.
#
#   bash tools/desktop/winkit.sh [out.zip]
#
# The kit needs only Python 3 on the other side (see tools/desktop/winkit/run.py): the Windows programs
# built here, MinGW's argv_dump/launch, the test MP3, the reference checksums computed here, a README.
# This machine cannot put files on a Windows PC itself; the zip is what travels (as a chat attachment).
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
OUT=${1:-$ROOT/build/desktop-windows-kit.zip}
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB=$ROOT/lib
command -v x86_64-w64-mingw32-ld >/dev/null || { echo "mingw linker missing"; exit 1; }
command -v x86_64-w64-mingw32-gcc >/dev/null || { echo "mingw gcc missing"; exit 1; }
W=$(mktemp -d "${TMPDIR:-/tmp}/desktop-winkit.XXXXXX")
trap 'rm -rf "$W"' EXIT
K=$W/desktop-kit
mkdir -p "$K" "$W/wn/window"
ln -s "$ROOT/lib/window/win32.fi" "$W/wn/window/backend.fi"
F="--target=x86_64-windows --opt-level=release-fast"
for SRC in tray_main wintray_poke notify_main autostart_main sink_main audio_main; do
    "$FIRNC" $F -o "$K/$SRC.exe" "tools/desktop/$SRC.fi" 2> "$W/b.log" || { echo "build $SRC failed"; grep -v RWX "$W/b.log" | head; exit 1; }
done
# the window programs are CONSOLE programs: the script reads what they print
for SRC in windrop_main clip_main drop_main; do
    cp "tools/desktop/$SRC.fi" "$W/wn/"
    "$FIRNC" $F -o "$K/$SRC.exe" "$W/wn/$SRC.fi" 2> "$W/b.log" || { echo "build $SRC failed"; exit 1; }
done
"$FIRNC" $F -o "$K/t2220.exe" tests/2220_desktop_watch.fi 2> "$W/b.log" || { echo "build t2220 failed"; exit 1; }
"$FIRNC" $F -o "$K/t2221.exe" tests/2221_appkit_ipc.fi 2> "$W/b.log" || { echo "build t2221 failed"; exit 1; }
x86_64-w64-mingw32-gcc -municode -O1 -o "$K/argv_dump.exe" tools/desktop/argv_dump.c || exit 1
x86_64-w64-mingw32-gcc -municode -O1 -o "$K/launch.exe" tools/desktop/launch.c || exit 1
cp testdata/ton/stereo44.mp3 "$K/"
# reference checksums: the plain decoder, the generated pattern
"$FIRNC" --opt-level=release-fast -o "$W/mp3_ref" lib/ton/mp3_main.fi 2> /dev/null || exit 1
"$W/mp3_ref" testdata/ton/stereo44.mp3 "$W/ref.pcm" > /dev/null
python3 - "$W/ref.pcm" "$K/reference.txt" <<'PY'
import array, sys, zlib
pcm = open(sys.argv[1], "rb").read()
def scale(pcm, percent):
    g = percent * 65536 // 100
    a = array.array("h"); a.frombytes(pcm); o = array.array("h")
    for v in a:
        x = abs(v) * g // 65536
        o.append(max(-32768, min(32767, x if v >= 0 else -x)))
    return o.tobytes()
frames = 3 * 44100
pat = bytearray()
for n in range(frames):
    l = (n * 7 + 1000) & 65535
    r = l ^ 65535
    pat += bytes([l & 255, l >> 8, r & 255, r >> 8])
open(sys.argv[2], "w").write("pattern_crc %d\nmp3_len %d\nmp3_crc_100 %d\nmp3_crc_50 %d\n" % (
    zlib.crc32(bytes(pat)), len(pcm), zlib.crc32(pcm), zlib.crc32(scale(pcm, 50))))
PY
cp tools/desktop/winkit/run.py "$K/run.py"
cat > "$K/README.txt" <<'TXT'
Firn desktop libraries -- the Windows check
===========================================
What it is: the programs that were checked under Wine on the build machine (tray icon, notifications,
files dropped on a window, the clipboard, sound, folder watcher, autostart, command line channel), built
for Windows, plus a script that runs them and checks the answers. Under Wine some things cannot be proven;
this is the run on a real desktop.

Needs: Windows 10 or 11, Python 3 ("py" launcher), a logged-in desktop session. Nothing is installed;
a registry key HKCU\Software\FirnDesktopKit is made and removed again.

  1. Unzip the whole folder somewhere (not into Program Files).
  2. Open a command prompt in it:   py run.py --interactive
     (plain "py run.py" skips the questions that need your eyes and ears)
  3. When it asks, look at the notification area, listen, drag a file onto the window that opens.
  4. At the end it prints "desktop kit: all checks passed" or the list of failures and writes
     kit-report.txt next to run.py. Send that file back.

If the antivirus complains about the .exe files: they are unsigned test programs built by the Firn compiler.
TXT
mkdir -p "$(dirname "$OUT")"
rm -f "$OUT"
python3 -c "import sys,zipfile,os
z=zipfile.ZipFile(sys.argv[1],'w',zipfile.ZIP_DEFLATED)
base=sys.argv[2]
for f in sorted(os.listdir(base)):
    z.write(os.path.join(base,f), 'desktop-kit/'+f)
z.close()" "$OUT" "$K"
echo "kit: $OUT ($(stat -c %s "$OUT") bytes, $(ls "$K" | wc -l) files)"
