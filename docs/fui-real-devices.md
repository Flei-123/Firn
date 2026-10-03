# fUi on real devices -- what only a finger or a real keyboard can show

Everything below is built and has passed on an emulator or against a
protocol test; none of it has been run on the real thing yet (roadmap
r121 / r122). Each item says what to do and what has to happen. A failure
is a bug to report with the log line, not a "works for me".

## Android phone (arm64): keyboard, text area, touch

Install the three APKs (`tools/android/build.sh`, both ABIs, so arm64 is in
them): `fui-form.apk`, `fui-notes.apk`, `fui-touchpad.apk`
(`adb install -r <apk>`, or open the file on the phone: allow "unknown apps").

| App | Do | Has to happen |
|---|---|---|
| fUi Form | tap the name field | the on-screen keyboard (Gboard) comes up, the field has the caret |
| | type "Justin", tap Next / the other field, type an address, tap Send | the answer line shows both |
| | rotate the phone with the keyboard open, then rotate back | nothing is cut off, the field keeps its text |
| | press Home, come back | window and text are as they were |
| | press Back with the keyboard open, tap the field again | the keyboard comes back |
| fUi Notes | tap the area, type two lines with the Enter key of the keyboard | a line break in the text, the caret on line 2 |
| | type a long line | it wraps at the box |
| | drag with a finger inside the area | (the area does not scroll by touch yet: r45 note; the mouse wheel / keys do) |
| | long text, then Count | the number of characters |
| fUi Touchpad | one finger tap, two fingers pinch | ids 0 and 1, the second one not primary, the scale follows the fingers |

Known gaps (not bugs of this round): no extract UI handling, no hardware
keyboard shortcuts check, `inputType` flags per field kind (number, e-mail,
URL) are not set -- every field is plain text.

## Linux with a touch screen: XInput2 touch (lib/window/x11.fi)

Needs an X server with a touch device (`xinput list` shows a "touchscreen").

```
firnc -o /tmp/touchpad examples/fui/touchpad.fi && /tmp/touchpad
```

Tap, pan with one finger, pinch with two. Has to happen: the numbers on the
window change like in the browser; a second finger gets id 1 and is not
primary. If nothing reacts: `xinput test-xi2 --root` while touching shows
whether the server sends TouchBegin at all (then it is the program),
otherwise it is the driver / the XI2 version (needs 2.2).

## Windows: WM_POINTER (lib/window/win32.fi)

Builds only for the Windows target, not run on a Windows machine yet. When
a Windows build of a fUi program exists (roadmap r24 / r86), the same
touchpad program is the test: touch a touch screen or a laptop with a
touch pad that sends touch (precision touch pads send WM_POINTER pens /
touch).

## What to send back

The line the program prints with `--log` (touchpad) or a photo of the
screen, and the phone / device model.
