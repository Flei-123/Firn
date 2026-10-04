#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/pack/test/windows.sh -- the Windows installer, shortcuts, registry entry, uninstaller and
# the window, run under WINE (there is no real Windows here: FLEI-ONE cannot be reached from this
# server, see docs/APPKIT.md). Run through tools/pack/test/run.sh with PACK_WINE=1.
#
#   what is checked                                                        reader
#   silent install (/S --dir), files, uninstall.exe, .pack/install.txt     the file system
#   Start Menu + Desktop shortcut: structure, target, working dir, icon    tools/pack/test/lnkread.py
#                                  and that Wine's own shell starts them   `cmd /c start x.lnk`
#   HKCU\...\Uninstall\<id> (every value)                                  `wine reg query`
#   upgrade in place (old file gone, version in the registry changes)      the file system + reg
#   installed.sync_version (what an updated program calls)                 reg query
#   silent uninstall: files, shortcuts, registry, temp leftovers gone;     the file system + reg
#                     a file of the user's stays
#   the window: install (click), done page, uninstall (click)              xdotool on Xvfb + screenshots
#   an NSIS-built setup.exe (makensis) installs and uninstalls             reg query
#   a portable zip keeps its data next to the program                      the program's own output
#   (PACK_WINE_APP=1) the appkit template's window starts on the Win32 back end  --selftest
#
# Needs wine, xdotool, Xvfb, python3, the mingw binutils. SKIPs (exit 0) without wine.
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
export FIRNLIB=$ROOT/lib
command -v wine >/dev/null 2>&1 || { echo "SKIP: wine is not installed"; exit 0; }
command -v x86_64-w64-mingw32-ld >/dev/null 2>&1 || { echo "SKIP: needs the mingw binutils"; exit 0; }
OWN=0
W=${PACK_W:-}
if [ -z "$W" ]; then W=$(mktemp -d "${TMPDIR:-/tmp}/pack-win.XXXXXX"); OWN=1; fi
export WINEPREFIX=${WINEPREFIX:-/root/.wine-firn} WINEDEBUG=-all
T=$W/win; rm -rf "$T"; mkdir -p "$T"
PASS=0; FAIL=0
ok() { PASS=$((PASS+1)); }
bad() { FAIL=$((FAIL+1)); echo "FAIL $1"; }
check() { if eval "$2"; then ok; else bad "$1"; fi; }
winp() { printf '%s' "$1" | sed 's|^/|Z:\\|; s|/|\\|g'; }   # /tmp/x -> Z:\tmp\x
DISP=""
cleanup() {
    wineserver -k 2>/dev/null
    [ -n "$DISP" ] && kill "$XPID" 2>/dev/null
    [ "$OWN" = 1 ] && rm -rf "$W"
    rm -rf "$T"
}
trap cleanup EXIT

# an X display of our own
for n in 91 92 93 94 95; do
    if [ ! -e /tmp/.X11-unix/X$n ]; then DISP=:$n; break; fi
done
Xvfb $DISP -screen 0 1024x768x24 >/dev/null 2>&1 &
XPID=$!
sleep 1
export DISPLAY=$DISP
# A window created while Wine's server is still cold (its explorer has not loaded the X driver yet) fails
# with "no driver could be loaded" -- a Wine start-up race, not the programs'. A server that stays up
# (`wineserver -p`) and one window of Wine's own (notepad, closed by `timeout`) make every later start warm.
wineserver -k 2>/dev/null; sleep 1
wineserver -p
wine cmd /c exit >/dev/null 2>&1
( timeout 6 wine notepad >/dev/null 2>&1 )
sleep 1

# ---- build
P="python3 $ROOT/tools/pack/pack.py"
ICONS=$T/icons
BUILD=${PACK_BUILD:-$W/build}
[ -f "$BUILD/setup-stub.exe" ] || PACK_BUILD=$BUILD bash "$ROOT/tools/pack/pack.sh" stubs >/dev/null
"$BUILD/icons" "$ROOT/tools/pack/test/data/sample.svg" "$ICONS" --name hello --bg 1d9a5b >/dev/null
[ -f "$W/hello.exe" ] || "$FIRNC" --target=x86_64-windows -o "$W/hello.exe" "$ROOT/tools/pack/test/hello.fi" >/dev/null 2>&1
"$FIRNC" --target=x86_64-windows -o "$T/syncver.exe" "$ROOT/tools/pack/test/syncver.fi" >/dev/null 2>&1
"$FIRNC" --target=x86_64-windows -o "$T/portable.exe" "$ROOT/tools/pack/test/portable.fi" >/dev/null 2>&1
$P pe-icon --exe "$W/hello.exe" --ico "$ICONS/hello.ico" --out "$T/hello-i.exe" >/dev/null
$P pe-icon --exe "$BUILD/setup-stub.exe" --ico "$ICONS/hello.ico" --out "$T/stub-i.exe" >/dev/null
mkdir -p "$T/files1" "$T/files2"
echo "only in 1.0.0" > "$T/files1/old.txt"; mkdir -p "$T/files1/docs"; echo "doc 1" > "$T/files1/docs/readme.txt"
echo "only in 1.1.0" > "$T/files2/new.txt"; mkdir -p "$T/files2/docs"; echo "doc 2" > "$T/files2/docs/readme.txt"
APP=(--id hello --name "Hello App" --vendor FleiTec --exe-name hello.exe --url https://example.org/)
$P win-installer "${APP[@]}" --version 1.0.0 --stub "$T/stub-i.exe" --exe "$T/hello-i.exe" --out "$T/setup-1.exe" \
    --ico "$ICONS/hello.ico" --files "$T/files1" >/dev/null
$P win-installer "${APP[@]}" --version 1.1.0 --stub "$T/stub-i.exe" --exe "$T/hello-i.exe" --out "$T/setup-2.exe" \
    --ico "$ICONS/hello.ico" --files "$T/files2" >/dev/null
STUBSZ=$(stat -c %s "$T/stub-i.exe")
INST=$T/inst
DESK=$T/desk; PROGS=$T/progs
SK='HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders'
UK='HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\hello'

# one Wine session: point Desktop/Programs at test folders, run the commands, leave the answers in files
session() { # session <name> <command line>...   (the commands run in one cmd.exe)
    local bat=$T/$1.bat; shift
    {
        printf '@echo off\r\n'
        printf 'reg add "%s" /v Desktop /t REG_SZ /d "%s" /f >nul\r\n' "$SK" "$(winp "$DESK")"
        printf 'reg add "%s" /v Programs /t REG_SZ /d "%s" /f >nul\r\n' "$SK" "$(winp "$PROGS")"
        for c in "$@"; do printf '%s\r\n' "$c"; done
    } > "$bat"
    timeout 180 wine cmd /c "$(winp "$bat")" > "$bat.out" 2>&1
}
regval() { # regval <name>  -> the value of $UK\<name> (from a file made by `reg query`)
    sed -n "s/^ *$1 *REG_[A-Z_]* *//p" "$T/q.txt" | tr -d '\r' | head -1
}

rm -f "$WINEPREFIX"/drive_c/users/root/Temp/pack-uninstall-* "$WINEPREFIX"/drive_c/users/root/Temp/pack-clean-* 2>/dev/null
echo "-- silent install"
mkdir -p "$DESK" "$PROGS"
session s1 "$(winp "$T/setup-1.exe") /S --dir $(winp "$INST") --log $(winp "$T/s1.log")" \
    "echo exit=%errorlevel% > $(winp "$T/s1.rc")" \
    "reg query \"$UK\" > $(winp "$T/q.txt")"
check "silent install exits 0" "grep -q 'exit=0' $T/s1.rc"
check "the program is installed" "[ -f $INST/hello.exe ] && [ -f $INST/hello.ico ]"
check "files of the payload (folder and file)" "grep -q 'doc 1' $INST/docs/readme.txt && [ -f $INST/old.txt ]"
check "uninstall.exe is the stub without the payload ($STUBSZ octets)" "[ \$(stat -c %s $INST/uninstall.exe) -eq $STUBSZ ]"
check "the list of installed files exists" "grep -q '^id=hello' $INST/.pack/install.txt && grep -q '^f hello.exe' $INST/.pack/install.txt"
check "the log says done" "grep -q '^done, 0 warnings' $T/s1.log"
check "registry: DisplayName" "[ \"\$(regval DisplayName)\" = 'Hello App' ]"
check "registry: DisplayVersion" "[ \"\$(regval DisplayVersion)\" = '1.0.0' ]"
check "registry: Publisher" "[ \"\$(regval Publisher)\" = 'FleiTec' ]"
check "registry: InstallLocation" "[ \"\$(regval InstallLocation)\" = '$(winp "$INST")' ]"
check "registry: DisplayIcon is the .ico" "[ \"\$(regval DisplayIcon)\" = '$(winp "$INST")\\hello.ico' ]"
check "registry: UninstallString" "[ \"\$(regval UninstallString)\" = '\"$(winp "$INST")\\uninstall.exe\" /uninstall' ]"
check "registry: QuietUninstallString" "regval QuietUninstallString | grep -q '/uninstall /S'"
check "registry: URLInfoAbout" "[ \"\$(regval URLInfoAbout)\" = 'https://example.org/' ]"
check "registry: NoModify and NoRepair" "[ \"\$(regval NoModify)\" = 0x1 ] && [ \"\$(regval NoRepair)\" = 0x1 ]"
check "registry: EstimatedSize is a number" "regval EstimatedSize | grep -q '^0x'"
check "registry: InstallDate is today" "[ \"\$(regval InstallDate)\" = \"\$(date -u +%Y%m%d)\" ]"
LNK1=$PROGS/Hello\ App.lnk; LNK2=$DESK/Hello\ App.lnk
check "Start Menu shortcut written" "[ -f \"$LNK1\" ]"
check "Desktop shortcut written" "[ -f \"$LNK2\" ]"
for L in "$LNK1" "$LNK2"; do
    N=$(basename "$(dirname "$L")")
    R=$(python3 "$ROOT/tools/pack/test/lnkcheck.py" "$L" --target "$(winp "$INST")\\hello.exe" --workdir "$(winp "$INST")" \
        --icon-suffix hello.ico --desc "Hello App" --idlist 2>&1)
    check "$N shortcut: target, working directory, icon, name and ID list ($R)" "[ -z \"\$R\" ]"
done
rm -f "$INST/hello-ran.txt"
session s1b "start /wait \"\" \"$(winp "$PROGS")\\Hello App.lnk\""
sleep 1
check "Wine's shell starts the program through the shortcut (working dir honoured)" "[ -f $INST/hello-ran.txt ]"
rm -f "$INST/hello-ran.txt"
session s1c "start /wait \"\" \"$(winp "$DESK")\\Hello App.lnk\""
sleep 1
check "...and through the Desktop one" "[ -f $INST/hello-ran.txt ]"

echo "-- the icon in the program"
python3 -c "
import sys; sys.path.insert(0,'$ROOT/tools/pack')
from packlib import win, common
s=win.pe_icons(common.read('$INST/hello.exe'))
sys.exit(0 if s and (256,256) in s else 1)" && ok || bad "the installed program carries its icon resource"

echo "-- upgrade in place"
echo "my notes" > "$INST/notes.txt"
session s2 "$(winp "$T/setup-2.exe") /S --dir $(winp "$INST") --log $(winp "$T/s2.log")" \
    "echo exit=%errorlevel% > $(winp "$T/s2.rc")" \
    "reg query \"$UK\" > $(winp "$T/q.txt")"
check "upgrade exits 0" "grep -q 'exit=0' $T/s2.rc"
check "the file that 1.1.0 does not have is gone" "[ ! -f $INST/old.txt ]"
check "the new file is there, the changed one is new" "[ -f $INST/new.txt ] && grep -q 'doc 2' $INST/docs/readme.txt"
check "the user's file stays" "grep -q 'my notes' $INST/notes.txt"
check "the registry has the new version" "[ \"\$(regval DisplayVersion)\" = '1.1.0' ]"
check "the log says it removed the old file" "grep -q 'removed old.txt' $T/s2.log"

echo "-- an updated program tells Apps & features its version"
session s3 "$(winp "$T/syncver.exe") hello 1.2.7" "echo exit=%errorlevel% > $(winp "$T/s3.rc")" \
    "reg query \"$UK\" > $(winp "$T/q.txt")"
check "sync_version exits 0" "grep -q 'exit=0' $T/s3.rc"
check "DisplayVersion follows the running version" "[ \"\$(regval DisplayVersion)\" = '1.2.7' ]"
session s3b "$(winp "$T/syncver.exe") not-installed 1.0" "echo exit=%errorlevel% > $(winp "$T/s3b.rc")"
check "an id with no entry changes nothing (exit 1)" "grep -q 'exit=1' $T/s3b.rc"

echo "-- silent uninstall"
session s4 "$(winp "$INST")\\uninstall.exe /uninstall /S --log $(winp "$T/s4.log")" \
    "echo exit=%errorlevel% > $(winp "$T/s4.rc")"
for i in $(seq 1 40); do [ -f "$INST/uninstall.exe" ] || break; sleep 1; done
session s4b "reg query \"$UK\" > $(winp "$T/q.txt") 2>&1"
check "uninstall exits 0" "grep -q 'exit=0' $T/s4.rc"
check "the program and its files are gone" "[ ! -f $INST/hello.exe ] && [ ! -d $INST/docs ] && [ ! -f $INST/new.txt ] && [ ! -d $INST/.pack ]"
check "uninstall.exe itself is gone (removed by the clean-up batch)" "[ ! -f $INST/uninstall.exe ]"
check "the user's file stays, so does its folder" "grep -q 'my notes' $INST/notes.txt"
check "both shortcuts are gone" "[ ! -f \"$LNK1\" ] && [ ! -f \"$LNK2\" ]"
check "the registry entry is gone" "! grep -q DisplayName $T/q.txt"
TMPW=$WINEPREFIX/drive_c/users/root/Temp
check "no uninstaller copy or batch is left in the temp folder" "[ -z \"\$(ls $TMPW 2>/dev/null | grep -E 'pack-uninstall|pack-clean')\" ]"
rm -rf "$INST"

echo "-- the window"
install_gui() { # install_gui <setup> <dir>   (click Install, then Finish)
    local setup=$1 dir=$2
    rm -f "$T/gui.log"
    ( wine "$setup" --log "$(winp "$T/gui.log")" >/dev/null 2>&1 & )
    local id=""
    for i in $(seq 1 30); do
        id=$(xwininfo -root -tree 2>/dev/null | grep -E '" *- Setup"|Setup"' | head -1 | awk '{print $1}')
        [ -n "$id" ] && break; sleep 1
    done
    echo "$id"
}
GID=$(install_gui "$(winp "$T/setup-1.exe")" "")
check "the setup window opens" "[ -n \"$GID\" ]"
if [ -n "$GID" ]; then
    sleep 2
    GX=$(xwininfo -id $GID | sed -n 's/.*Absolute upper-left X: *//p'); GY=$(xwininfo -id $GID | sed -n 's/.*Absolute upper-left Y: *//p')
    GW=$(xwininfo -id $GID | sed -n 's/.*Width: *//p'); GH=$(xwininfo -id $GID | sed -n 's/.*Height: *//p')
    check "the window is 540x380" "[ $GW -eq 540 ] && [ $GH -eq 380 ]"
    xwd -id $GID -silent -out "$T/g1.xwd" && python3 "$ROOT/tools/fui/xwd2png.py" "$T/g1.xwd" "$T/g1.png" >/dev/null 2>&1
    python3 - "$T/g1.png" <<'PY' && ok || bad "the welcome page is drawn (text and a green button, not a blank window)"
import sys, struct, zlib
sys.path.insert(0, "")
data = open(sys.argv[1], "rb").read()
pos, idat, w = 8, b"", 0
while pos < len(data):
    n, = struct.unpack_from(">I", data, pos); tag = data[pos+4:pos+8]
    if tag == b"IHDR": w, h, d, ct = struct.unpack(">IIBB", data[pos+8:pos+18])
    if tag == b"IDAT": idat += data[pos+8:pos+8+n]
    pos += 12 + n
raw = zlib.decompress(idat); bpp = 4 if ct == 6 else 3; stride = w * bpp
rows, prev = [], bytearray(stride); p = 0
for _ in range(h):
    f = raw[p]; cur = bytearray(raw[p+1:p+1+stride]); p += 1 + stride
    for i in range(stride):
        a = cur[i-bpp] if i >= bpp else 0; b = prev[i]; c = prev[i-bpp] if i >= bpp else 0
        if f == 1: cur[i] = (cur[i]+a) & 255
        elif f == 2: cur[i] = (cur[i]+b) & 255
        elif f == 3: cur[i] = (cur[i]+((a+b)>>1)) & 255
        elif f == 4:
            pa, pb, pc = abs(b-c), abs(a-c), abs(a+b-2*c)
            cur[i] = (cur[i] + (a if pa <= pb and pa <= pc else (b if pb <= pc else c))) & 255
    rows.append(bytes(cur)); prev = cur
green = sum(1 for r in rows for x in range(w) if r[x*bpp+1] > 180 and r[x*bpp] < 80 and r[x*bpp+2] < 140)
light = sum(1 for r in rows for x in range(w) if r[x*bpp] > 200 and r[x*bpp+1] > 200 and r[x*bpp+2] > 200)
sys.exit(0 if green > 1500 and light > 300 else 1)
PY
    # Install button: bottom right
    xdotool mousemove $((GX + GW - 65)) $((GY + GH - 47)) click 1
    for i in $(seq 1 20); do grep -q '^done' "$T/gui.log" 2>/dev/null && break; sleep 1; done
    check "the click installs (the log says done)" "grep -q '^done, 0 warnings' $T/gui.log"
    GDIR=$(sed -n 's/^install .* into //p' "$T/gui.log" | head -1)
    check "...into the default folder %LOCALAPPDATA%\\Programs\\<name>" "echo '$GDIR' | grep -q 'AppData/Local/Programs/Hello App'"
    HOMEDIR="$WINEPREFIX/drive_c/users/root/AppData/Local/Programs/Hello App"
    check "the program is there" "[ -f \"$HOMEDIR/hello.exe\" ]"
    sleep 1
    xwd -id $GID -silent -out "$T/g2.xwd" 2>/dev/null && python3 "$ROOT/tools/fui/xwd2png.py" "$T/g2.xwd" "$T/g2.png" >/dev/null 2>&1
    check "the done page is a different picture" "[ -s $T/g2.png ] && ! cmp -s $T/g1.png $T/g2.png"
    rm -f "$HOMEDIR/hello-ran.txt"
    xdotool mousemove $((GX + GW - 65)) $((GY + GH - 47)) click 1      # Finish (with "Start Hello App" ticked)
    for i in $(seq 1 10); do [ -f "$HOMEDIR/hello-ran.txt" ] && break; sleep 1; done
    check "Finish starts the installed program (the check box is on)" "[ -f \"$HOMEDIR/hello-ran.txt\" ]"
    check "the window closed" "! xwininfo -id $GID >/dev/null 2>&1"
    # the uninstaller's window
    rm -f "$T/gui2.log"
    ( wine "$(winp "$HOMEDIR/uninstall.exe")" /uninstall --log "$(winp "$T/gui2.log")" >/dev/null 2>&1 & )
    UID_=""
    for i in $(seq 1 30); do
        UID_=$(xwininfo -root -tree 2>/dev/null | grep -E 'Uninstall"' | head -1 | awk '{print $1}'); [ -n "$UID_" ] && break; sleep 1
    done
    check "the uninstall window opens" "[ -n \"$UID_\" ]"
    if [ -n "$UID_" ]; then
        sleep 2
        UX=$(xwininfo -id $UID_ | sed -n 's/.*Absolute upper-left X: *//p'); UY=$(xwininfo -id $UID_ | sed -n 's/.*Absolute upper-left Y: *//p')
        xdotool mousemove $((UX + 540 - 75)) $((UY + 380 - 47)) click 1            # Uninstall (the red button)
        for i in $(seq 1 30); do [ -d "$HOMEDIR" ] || break; sleep 1; done
        for i in $(seq 1 30); do [ -f "$HOMEDIR/uninstall.exe" ] || break; sleep 1; done
        sleep 1
        xdotool mousemove $((UX + 540 - 65)) $((UY + 380 - 47)) click 1            # Finish
        sleep 2
        check "the click uninstalls (the files are gone)" "[ ! -f \"$HOMEDIR/hello.exe\" ]"
        session s5 "reg query \"$UK\" > $(winp "$T/q.txt") 2>&1"
        check "...and the registry entry too" "! grep -q DisplayName $T/q.txt"
        # hello-ran.txt is the program's own output (not in the list): it keeps the folder alive
        rm -rf "$HOMEDIR"
    fi
fi
# cancel leaves everything alone
rm -f "$T/gui3.log"
( wine "$(winp "$T/setup-1.exe")" --log "$(winp "$T/gui3.log")" >/dev/null 2>&1 & )
CID=""
for i in $(seq 1 30); do CID=$(xwininfo -root -tree 2>/dev/null | grep -E 'Setup"' | head -1 | awk '{print $1}'); [ -n "$CID" ] && break; sleep 1; done
if [ -n "$CID" ]; then
    sleep 2
    CX=$(xwininfo -id $CID | sed -n 's/.*Absolute upper-left X: *//p'); CY=$(xwininfo -id $CID | sed -n 's/.*Absolute upper-left Y: *//p')
    xdotool mousemove $((CX + 540 - 145)) $((CY + 380 - 47)) click 1                 # Cancel
    sleep 2
    check "Cancel closes the window and installs nothing" "! xwininfo -id $CID >/dev/null 2>&1 && [ ! -f \"$HOMEDIR/hello.exe\" ] && ! grep -q '^done' $T/gui3.log 2>/dev/null"
else
    bad "the window for the Cancel test did not open"
fi

# a setup.exe without a payload (a damaged download, or the bare stub) says so in a window
rm -f "$T/gui4.log"
( wine "$(winp "$T/stub-i.exe")" --log "$(winp "$T/gui4.log")" >/dev/null 2>&1 & )
EID=""
for i in $(seq 1 30); do EID=$(xwininfo -root -tree 2>/dev/null | grep -E 'Setup"' | head -1 | awk '{print $1}'); [ -n "$EID" ] && break; sleep 1; done
check "a setup without payload opens a window with the error" "[ -n \"$EID\" ] && grep -q 'no intact payload' $T/gui4.log"
if [ -n "$EID" ]; then
    EX=$(xwininfo -id $EID | sed -n 's/.*Absolute upper-left X: *//p'); EY=$(xwininfo -id $EID | sed -n 's/.*Absolute upper-left Y: *//p')
    xdotool mousemove $((EX + 540 - 65)) $((EY + 380 - 47)) click 1                  # Close
    sleep 2
    check "...and Close ends it" "! xwininfo -id $EID >/dev/null 2>&1"
fi

if command -v makensis >/dev/null 2>&1; then
    echo "-- the NSIS variant"
    mkdir -p "$T/nsis"
    $P win-nsis "${APP[@]}" --version 1.0.0 --exe "$T/hello-i.exe" --outdir "$T/nsis" --ico "$ICONS/hello.ico" --build >/dev/null
    NS=$T/nsis/hello-1.0.0-setup.exe
    check "makensis built the setup" "[ -f $NS ]"
    NINST=$T/ninst
    session s6 "$(winp "$NS") /S /D=$(winp "$NINST")" "echo exit=%errorlevel% > $(winp "$T/s6.rc")" \
        "reg query \"$UK\" > $(winp "$T/q.txt") 2>&1"
    sleep 2
    check "the NSIS setup installs the program" "[ -f $NINST/hello.exe ] && [ -f $NINST/uninstall.exe ]"
    check "...with the registry entry" "[ \"\$(regval DisplayVersion)\" = '1.0.0' ] && [ \"\$(regval Publisher)\" = 'FleiTec' ]"
    # NSIS asks the system for the folders (not the registry values the other tests point elsewhere)
    NSM="$WINEPREFIX/drive_c/users/root/AppData/Roaming/Microsoft/Windows/Start Menu/Programs/Hello App.lnk"
    NSD="$(readlink -f "$WINEPREFIX/drive_c/users/root/Desktop")/Hello App.lnk"
    check "...and the shortcuts" "[ -f \"$NSM\" ] && [ -f \"$NSD\" ]"
    session s7 "$(winp "$NINST")\\uninstall.exe /S _?=$(winp "$NINST")" "reg query \"$UK\" > $(winp "$T/q.txt") 2>&1"
    sleep 2
    check "the NSIS uninstaller removes the entry and the shortcuts" "! grep -q DisplayName $T/q.txt && [ ! -f \"$NSM\" ] && [ ! -f \"$NSD\" ]"
    rm -rf "$NINST"
else
    echo "SKIP: makensis not installed"
fi

echo "-- the portable zip"
mkdir -p "$T/pz"
$P win-zip --id demo --name Demo --version 1.0.0 --exe-name portable.exe --exe "$T/portable.exe" --out "$T/portable.zip" >/dev/null
python3 -c "import zipfile;zipfile.ZipFile('$T/portable.zip').extractall('$T/pz')"
PD=$T/pz/demo-1.0.0
check "the zip holds the program and portable.txt" "[ -f $PD/portable.exe ] && [ -f $PD/portable.txt ]"
OUT=$(cd "$PD" && timeout 60 wine portable.exe 2>&1 | tr -d '\r')
check "with portable.txt the data folders are next to the program" "echo \"\$OUT\" | grep -q 'config=Z:/tmp.*/pz/demo-1.0.0/data/config/demo'"
rm -f "$PD/portable.txt"
OUT=$(cd "$PD" && timeout 60 wine portable.exe 2>&1 | tr -d '\r')
check "without it they are in the user profile" "echo \"\$OUT\" | grep -q 'config=C:/users/.*/FleiTec/demo'"

if [ "${PACK_WINE_APP:-0}" = "1" ]; then
    echo "-- an appkit program: its window on the Win32 back end, with the icon patched in"
    A=$T/app
    bash "$ROOT/tools/newapp.sh" Hello hello --dir "$A" --store-key "$(printf 'ab%.0s' $(seq 1 32))" --version 1.2.3 >/dev/null 2>&1
    ( cd "$A" && FIRN_ROOT=$ROOT bash build-windows.sh >/dev/null 2>&1 )
    check "the template builds for Windows" "[ -f $A/build/hello.exe ]"
    if [ -f "$A/build/hello.exe" ]; then
        $P pe-icon --exe "$A/build/hello.exe" --ico "$ICONS/hello.ico" --out "$T/app-i.exe" >/dev/null
        mkdir -p "$T/apphome"
        OUT=$(APPKIT_HOME=$(winp "$T/apphome") timeout 120 wine "$T/app-i.exe" --selftest 2>&1 | tr -d '\r')
        check "its window opens and draws 30 frames (SELFTEST ok)" "echo \"\$OUT\" | grep -q 'SELFTEST ok frames=30'"
    fi
fi

echo "windows.sh: $((PASS+FAIL)) checks, $FAIL failed"
[ $FAIL -eq 0 ]
