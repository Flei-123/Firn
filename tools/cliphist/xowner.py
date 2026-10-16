# SPDX-License-Identifier: MPL-2.0
# tools/cliphist/xowner.py -- a foreign X11 clipboard owner that offers EXTRA formats (python-xlib): the check for
# html and for the password-manager hint (which Tk and GTK cannot offer).
#   python3 xowner.py <hold-seconds> text=<utf8 text> [html=<html>] [hint=<value of x-kde-passwordManagerHint>]
# It owns CLIPBOARD for `hold` seconds and answers TARGETS and every format given.
import sys
import time
from Xlib import X, display, Xatom
from Xlib.protocol import event

hold = float(sys.argv[1])
data = {}
for a in sys.argv[2:]:
    k, _, v = a.partition("=")
    data[k] = v.encode()

d = display.Display()
win = d.screen().root.create_window(0, 0, 1, 1, 0, d.screen().root_depth)
clip = d.intern_atom("CLIPBOARD")
utf8 = d.intern_atom("UTF8_STRING")
targets = d.intern_atom("TARGETS")
formats = {}  # atom -> (bytes, type atom)
formats[utf8] = (data["text"], utf8)
formats[Xatom.STRING] = (data["text"], Xatom.STRING)
if "html" in data:
    formats[d.intern_atom("text/html")] = (data["html"], d.intern_atom("text/html"))
if "hint" in data:
    formats[d.intern_atom("x-kde-passwordManagerHint")] = (data["hint"], d.intern_atom("x-kde-passwordManagerHint"))

win.set_selection_owner(clip, X.CurrentTime)
d.flush()
if d.get_selection_owner(clip) != win:
    sys.exit("could not own the selection")

end = time.time() + hold
while time.time() < end:
    while d.pending_events():
        e = d.next_event()
        if e.type == X.SelectionRequest:
            prop = e.property if e.property != X.NONE else e.target
            ok = False
            if e.target == targets:
                lst = [targets] + list(formats.keys())
                e.requestor.change_property(prop, Xatom.ATOM, 32, lst)
                ok = True
            elif e.target in formats:
                payload, typ = formats[e.target]
                e.requestor.change_property(prop, typ, 8, payload)
                ok = True
            n = event.SelectionNotify(time=e.time, requestor=e.requestor, selection=e.selection, target=e.target,
                                      property=prop if ok else X.NONE)
            e.requestor.send_event(n)
            d.flush()
        elif e.type == X.SelectionClear:
            end = 0
    time.sleep(0.02)
