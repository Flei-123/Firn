# SPDX-License-Identifier: MPL-2.0
# tools/cliphist/foreign_owner.py -- a FOREIGN clipboard owner for the app check: a Tk process (not Firn) that takes
# the CLIPBOARD selection with each text given, one after the other, 1.2 s apart, and then keeps it for `hold` seconds.
#   python3 foreign_owner.py <hold-seconds> text...
#   python3 foreign_owner.py read            prints what the clipboard holds now (empty when nothing)
import sys
import time
import tkinter

root = tkinter.Tk()
root.withdraw()


def pump(seconds):
    end = time.time() + seconds
    while time.time() < end:
        root.update()
        time.sleep(0.02)


if sys.argv[1] == "read":
    try:
        sys.stdout.write(root.clipboard_get())
    except tkinter.TclError:
        pass
    sys.exit(0)

hold = float(sys.argv[1])
for text in sys.argv[2:]:
    root.clipboard_clear()
    root.clipboard_append(text)
    root.update()
    pump(1.2)
pump(hold)
