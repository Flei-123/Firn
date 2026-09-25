#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/firnchat_bild_check.sh -- FIRNCHAT's photos on the emulator,
# the way a person sends one: the picture button opens the system's chooser
# (camera + picker), a photo chosen there WAITS as a thumbnail over the
# input line (nothing is sent yet), the x takes it away again, text can be
# written to it, Send puts out ONE message "[bild:<id> <w>x<h>]" + the text,
# and the photo itself went up to the relay (src/blob.fi) -- the octets
# there hash to the id. A photo from the other side comes down and is
# painted in its bubble. The camera works the same way; Back in the chooser
# sends nothing. (lib/plat/android/pick.fi, FIRNCHAT src/gui/app.fi)
#
#   bash tools/android/firnchat_bild_check.sh <firnchat checkout> <apk>
#
# The APK has to be built with --push --pick. Own relay on RELAY_PORT
# (default 7783) with its own picture directory, throw-away identities.
# SERIAL / ADB as in firnchat_check.sh. Needs python3 with PIL (pixels).
FCDIR=${1:?firnchat checkout}
APK=${2:?apk}
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
ADB=${ADB:-/root/android-sdk/platform-tools/adb}
[ -n "${SERIAL:-}" ] && ADB="$ADB -s $SERIAL"
PORT=${RELAY_PORT:-7783}
PKG=org.firn.firnchat
D=/sdcard/Android/data/$PKG/files
FC=$FCDIR/bin/firnchat
W=$(mktemp -d /tmp/fcbild.XXXX)
PASS=0; FAIL=0
check() { if [ "$2" = "$3" ]; then echo "  OK    $1 (got $2)"; PASS=$((PASS+1));
          else echo "  FAIL  $1 (got '$2', want '$3')"; FAIL=$((FAIL+1)); fi; }
out() { $ADB shell cat $D/stdout.txt 2>/dev/null; }
ui() { local i=0; while [ $i -lt 5 ]; do
        $ADB shell uiautomator dump /sdcard/ui.xml 2>&1 | grep -q dumped && break
        sleep 1; i=$((i+1)); done
    $ADB shell cat /sdcard/ui.xml | tr '>' '\n'; }
mid() { echo "$1" | tr '[],' '   ' | awk '{print int(($1+$3)/2), int(($2+$4)/2)}'; }
# tap the first element whose text or content-desc matches $1 (a regex)
tap_ui() { local b; b=$(ui | grep -E "(text|content-desc)=\"$1" | grep -oP 'bounds="\K[^"]+' | head -1)
    [ -n "$b" ] || { echo "  (nothing called $1 on the screen)"; return 1; }
    $ADB shell input tap $(mid "$b"); }
spot() { out | grep '^spot ' | tail -1 | awk -v i="$1" '{print $(i+1)}'; }
tap_spot() { $ADB shell input tap "$(spot "$1")" "$(spot "$2")"; }
count() { out | grep -c "^$1"; }
# how many sampled pixels of the screen are red (the photo alice sends)
reds() { $ADB exec-out screencap -p > "$W/s.png"; python3 -c "
from PIL import Image
im=Image.open('$W/s.png').convert('RGB'); w,h=im.size
print(sum(1 for x in range(0,w,4) for y in range(0,h,4) if (lambda p: p[0]>180 and p[1]<80 and p[2]<80)(im.getpixel((x,y)))))"; }
relay() { (cd "$W" && setsid nohup "$FC" serve "$PORT" "$W/data" 0 --bilder-dir "$W/pics" >> "$W/relay.log" 2>&1 &); sleep 1; }
cleanup() { pkill -f "$FC serve $PORT" 2>/dev/null; return 0; }
trap cleanup EXIT

[ -x "$FC" ] || { echo "no $FC (build FIRNCHAT first)"; exit 2; }
relay
(cd "$W" && "$FC" id alice.id >/dev/null && "$FC" id bob.id > bob.txt \
    && "$FC" name Alice "$PORT" alice.id >/dev/null \
    && "$FC" name "Bob Handy" "$PORT" bob.id >/dev/null)
BOB=$(grep -oP '^key \K[0-9a-f]+' "$W/bob.txt")
(cd "$W" && "$FC" send "$BOB" "Hallo Bob, schick mal ein Foto" "$PORT" alice.id >/dev/null)
python3 -c "
from PIL import Image
im=Image.new('RGB',(2400,1800))
px=im.load()
for y in range(0,1800):
    for x in range(0,2400): px[x,y]=((x//9)%256,(y//7)%256,((x+y)//5)%256)
im.save('$W/gross.jpg',quality=95)
Image.new('RGB',(300,200),(220,30,30)).save('$W/rot.jpg',quality=90)
Image.new('RGB',(800,600),(30,60,200)).save('$W/blau.jpg',quality=90)"

$ADB root >/dev/null 2>&1; sleep 1
$ADB install -r "$APK" >/dev/null || { echo "install failed"; exit 1; }
$ADB shell pm grant $PKG android.permission.POST_NOTIFICATIONS
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null; sleep 3
$ADB shell am force-stop $PKG
$ADB shell "rm -rf $D/bob.id* $D/stdout.txt"
$ADB push "$W/bob.id" $D/bob.id >/dev/null
printf -- '--id\n%s/bob.id\n--host\n10.0.2.2\n--port\n%s\n--font\n/system/fonts/RobotoStatic-Regular.ttf\n' \
    "$D" "$PORT" > "$W/args.txt"
$ADB push "$W/args.txt" $D/args.txt >/dev/null
# a photo in the gallery, bigger than the app may send (2400 x 1800)
$ADB shell "rm -f /sdcard/Pictures/firnbild*.jpg"
$ADB push "$W/gross.jpg" /sdcard/Pictures/firnbild.jpg >/dev/null
# dated 2030, so it is the first of "Recent" in the picker whatever else
# the gallery holds
$ADB shell touch -t 203001011200 /sdcard/Pictures/firnbild.jpg
$ADB push "$W/blau.jpg" /sdcard/Pictures/firnbild2.jpg >/dev/null
$ADB shell touch -t 203001021200 /sdcard/Pictures/firnbild2.jpg
$ADB shell am broadcast -a android.intent.action.MEDIA_SCANNER_SCAN_FILE \
    -d file:///sdcard/Pictures/firnbild2.jpg >/dev/null
$ADB shell am broadcast -a android.intent.action.MEDIA_SCANNER_SCAN_FILE \
    -d file:///sdcard/Pictures/firnbild.jpg >/dev/null
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null; sleep 15
P1=$($ADB shell pidof $PKG)
mkdir -p "$ROOT/build/android/belege"
check "the picture button is there (its place is said)" "$( [ -n "$(spot 3)" ] && echo yes)" yes

echo "== 1. the picture button opens the system's chooser"
tap_spot 3 4; sleep 5
U=$(ui)
check "chooser: the camera" "$(echo "$U" | grep -c 'text="Camera"')" 1
check "chooser: a picker for photos" "$(echo "$U" | grep -cE 'text="(Media picker|Photos|Files|Gallery)"' | awk '{print ($1>0)?"yes":"no"}')" yes

echo "== 2. a photo from the gallery waits over the input line"
tap_ui "(Media picker|Photos|Files|Gallery)"; sleep 6
tap_ui "Photo taken on Jan 1, 2030"; sleep 9
check "the system gave a photo" "$(count 'bild-gewaehlt ')" 1
ID=$(out | grep -oP '^bild-ok \K\S+' | tail -1)
check "it went up to the relay at once" "$( [ -f "$W/pics/$ID" ] && echo yes)" yes
check "the name is the SHA-256 of the octets" "$(sha256sum "$W/pics/$ID" 2>/dev/null | cut -c1-64)" "${ID%%.*}"
check "at most 190 kB went up" "$( [ "$(stat -c%s "$W/pics/$ID" 2>/dev/null || echo 999999)" -le 190000 ] && echo yes)" yes
check "the thumbnail was decoded (bild 1)" "$(out | grep -c '^bild 1' | awk '{print ($1>0)?"yes":"no"}')" yes
$ADB exec-out screencap -p > "$ROOT/build/android/belege/firnchat-bild-wartet.png"
check "nothing was sent yet" "$(cd "$W" && "$FC" hist "$PORT" alice.id | grep -c 'bild:')" 0

echo "== 3. the x takes it away"
tap_spot 5 6; sleep 3
check "taken away" "$(count 'bild-weg ')" 1
check "still nothing sent" "$(cd "$W" && "$FC" hist "$PORT" alice.id | grep -c 'bild:')" 0

echo "== 4. two photos + text, Send: ONE message"
tap_spot 3 4; sleep 5
tap_ui "(Media picker|Photos|Files|Gallery)"; sleep 6
tap_ui "Photo taken on Jan 1, 2030"; sleep 9
tap_spot 3 4; sleep 5
tap_ui "(Media picker|Photos|Files|Gallery)"; sleep 6
tap_ui "Photo taken on Jan 2, 2030"; sleep 9
ID2=$(out | grep -oP '^bild-ok \K\S+' | tail -1)
check "the second photo went up too" "$( [ -f "$W/pics/$ID2" ] && [ "$ID2" != "$ID" ] && echo yes)" yes
check "still nothing sent" "$(cd "$W" && "$FC" hist "$PORT" alice.id | grep -c 'bild:')" 0
$ADB shell input tap 600 "$(spot 2)"; sleep 1
$ADB shell input text 'Schau%smal%sdie%sFotos'; sleep 2
tap_spot 1 2; sleep 6
check "sent" "$(count 'bild-gesendet ')" 1
H=$(cd "$W" && "$FC" hist "$PORT" alice.id)
LINE=$(echo "$H" | grep -oP '\[bild:[0-9a-f]{64}\.jpg [0-9]+x[0-9]+\]' | grep "$ID" | tail -1)
check "alice got the first photo's line" "$(echo "$LINE" | grep -c "$ID")" 1
check "... the second one's under it (one message)" "$(echo "$H" | grep -A1 "$ID" | tail -1 | grep -c "$ID2")" 1
check "... and the text under both" "$(echo "$H" | grep -A2 "$ID" | tail -1)" "Schau mal die Fotos"
WH=$(echo "$LINE" | grep -oP ' \K[0-9]+x[0-9]+')
check "made small by the system: 1600 x 1200" "$WH" 1600x1200
(cd "$W" && "$FC" bildholen "$ID" back.jpg "$PORT" alice.id >/dev/null)
check "alice fetches the same octets from the relay" "$(cmp -s "$W/back.jpg" "$W/pics/$ID" && echo yes)" yes
$ADB exec-out screencap -p > "$ROOT/build/android/belege/firnchat-bild-gesendet.png"

echo "== 5. a photo from alice comes down and is painted"
R0=$(reds)
RL=$(cd "$W" && "$FC" bildhoch rot.jpg "$PORT" alice.id)
RID=$(echo "$RL" | awk '{print $2}'); RDIM=$(echo "$RL" | awk '{print $3}')
(cd "$W" && "$FC" send "$BOB" "[bild:$RID $RDIM]
Rot von Alice" "$PORT" alice.id >/dev/null); sleep 7
R1=$(reds)
check "her photo stands on the screen (red pixels)" "$( [ "$R1" -gt $((R0 + 1000)) ] && echo yes)" yes
check "and was kept on the phone" "$($ADB shell ls $D/bob.id.bilder/ 2>/dev/null | grep -c "$RID")" 1
$ADB exec-out screencap -p > "$ROOT/build/android/belege/firnchat-bild-empfangen.png"

echo "== 6. the camera"
tap_spot 3 4; sleep 5
tap_ui "Camera"; sleep 10
tap_ui "Shutter"; sleep 6
tap_ui "Done"; sleep 10
check "the camera's photo went up" "$(out | grep -c '^bild-ok ')" 4
tap_spot 1 2; sleep 6
check "and out" "$(count 'bild-gesendet ')" 2
check "alice got it" "$(cd "$W" && "$FC" hist "$PORT" alice.id | grep -c 'bild:')" 3

echo "== 7. Back in the chooser sends nothing"
tap_spot 3 4; sleep 5
$ADB shell input keyevent KEYCODE_BACK; sleep 5
check "no photo, said so" "$(count 'bild-kein 3')" 1
check "nothing more went out" "$(cd "$W" && "$FC" hist "$PORT" alice.id | grep -c 'bild:')" 3
check "the same process all along (no crash)" "$($ADB shell pidof $PKG)" "$P1"
check "no crash in the log" "$($ADB logcat -d -b crash | grep -c "$PKG")" 0

echo "ANDROID FIRNCHAT PICTURES: $PASS passed, $FAIL failed (relay log $W/relay.log)"
[ $FAIL -eq 0 ]
