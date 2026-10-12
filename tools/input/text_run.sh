#!/usr/bin/env bash
# tools/input/text_run.sh -- text input / IME (docs/INPUT.md):
#   tests/2810 textcore (pure: UTF-8/16, compose tables, dead keys, Compose sequences, pre-edit edits, 30000 random ops vs a model)
#   tests/2811 textsys callbacks (XIM pre-edit callbacks called THROUGH C ADDRESSES with hand-made call data)
#   tools/input/xim_probe.fi under a private Xvfb with XMODIFIERS=@im=none (Xlib's built-in input method): synthetic XKeyEvents
#   replayed into a second Xlib connection -> commits; dead keys + the Compose key through XFilterEvent
#   Windows builds of textcore + textsys.windows (compile only)
# Usage: bash tools/input/text_run.sh [firnc]
set -u
cd "$(dirname "$0")/../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
export FIRNLIB="$(pwd)/lib"
D=$(mktemp -d)
XPID=""
trap '[ -n "$XPID" ] && kill "$XPID" 2>/dev/null; rm -rf "$D"' EXIT
fail=0
for t in 2810_input_textcore 2811_input_textsys_cb; do
    if "$FIRNC" -o "$D/t" "tests/$t.fi" 2>"$D/err" && "$D/t"; then echo "   ok:   tests/$t"
    else echo "   FAIL: tests/$t (exit $?)"; head -5 "$D/err"; fail=1; fi
done
# ---- real X server (private Xvfb on a free display, our own PID only) ----
if command -v Xvfb >/dev/null; then
    Xvfb -displayfd 3 -screen 0 320x200x24 -nolisten tcp 3>"$D/disp" >/dev/null 2>&1 &
    XPID=$!
    for _ in $(seq 1 50); do [ -s "$D/disp" ] && break; sleep 0.1; done
    if [ -s "$D/disp" ]; then
        export DISPLAY=":$(head -1 "$D/disp")"
        if "$FIRNC" -o "$D/xim" tools/input/xim_probe.fi 2>"$D/err"; then
            if XMODIFIERS=@im=none LANG=C.UTF-8 LC_ALL=C.UTF-8 "$D/xim" >"$D/xim.out" 2>&1; then echo "   ok:   XIM (@im=none) on Xvfb $DISPLAY: $(tr '\n' ';' <"$D/xim.out")"
            else echo "   FAIL: xim_probe (exit $?)"; cat "$D/xim.out" | head -20; fail=1; fi
        else echo "   FAIL: xim_probe build"; head -5 "$D/err"; fail=1; fi
    else echo "   FAIL: Xvfb did not start"; fail=1; fi
else
    echo "   SKIP: Xvfb missing (XIM probe not run)"
fi
cat > "$D/win.fi" <<'FI'
import std.rt
import input.textcore
import input.textsys
fn main() -> i32 {
    var t: textcore.Text = textcore.text_new()
    var s: textsys.TextSys = textsys.sys_new()
    textsys.sys_open(&s, &t)
    textsys.sys_spot(&s, &t)
    textsys.sys_close(&s)
    return textcore.text_preedit_len(&t) as i32
}
FI
if "$FIRNC" --target=x86_64-windows -o "$D/win.exe" "$D/win.fi" 2>"$D/err"; then echo "   ok:   textcore + textsys (IMM32 via dynlib) build for Windows (NOT run)"
else echo "   FAIL: Windows build"; head -5 "$D/err"; fail=1; fi
[ "$fail" -eq 0 ] && echo "RESULT text: ok" || echo "RESULT text: FAIL"
exit "$fail"
