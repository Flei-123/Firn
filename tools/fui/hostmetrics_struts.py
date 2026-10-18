# SPDX-License-Identifier: MPL-2.0
# tools/fui/hostmetrics_struts.py -- W47-b (r386): stands in for panels/docks on an Xvfb without a window manager.
#   python3 -I hostmetrics_struts.py <ready-file> <spec>...      spec = P:l,r,t,b,ls,le,rs,re,ts,te,bs,be  (_NET_WM_STRUT_PARTIAL)
#                                                                      S:l,r,t,b                          (the old _NET_WM_STRUT)
# One window per spec, all listed in the root's _NET_CLIENT_LIST. Writes <ready-file> when set and then stays alive: the windows
# (and so their struts) are gone when this process ends. The caller stops it by its own PID.
import sys
import time
from Xlib import display, Xatom

d = display.Display()
root = d.screen().root
ids = []
for spec in sys.argv[2:]:
    kind, vals = spec.split(":", 1)
    nums = [int(v) for v in vals.split(",")]
    w = root.create_window(0, 0, 10, 10, 0, d.screen().root_depth)
    name = "_NET_WM_STRUT_PARTIAL" if kind == "P" else "_NET_WM_STRUT"
    w.change_property(d.intern_atom(name), Xatom.CARDINAL, 32, nums)
    ids.append(w.id)
root.change_property(d.intern_atom("_NET_CLIENT_LIST"), Xatom.WINDOW, 32, ids)
d.sync()
open(sys.argv[1], "w").write("ok\n")
while True:
    time.sleep(1)
