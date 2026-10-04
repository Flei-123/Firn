#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/desktop/drop_check.py -- files dropped on a Firn window on a real X server (Xvfb): XDND.
#
#   usage: drop_check.py <drop_main binary> [wine]
#   with `wine` the binary is the Windows build, run under Wine (whose X11 driver is an XDND target
#   that turns the drop into WM_DROPFILES): the same drops, the paths come back as Windows paths (Z:\...).
#
# Two sources that are not our code:
#   * GTK 3 -- a real toolkit's drag source, driven by xdotool (mouse down, move, up) like a
#     person dragging a file out of a file manager;
#   * python-xlib -- a source written from the XDND specification for the cases a toolkit does
#     not produce: other types than files, a type list in the XdndTypeList property, remote
#     hosts, percent escapes, comments, a drag that leaves again.
import os, shutil, subprocess, sys, time, signal
from Xlib import X, display, protocol, Xatom

FAILED = []
def check(name, cond, extra=""):
    print(("  OK    " if cond else "  FAIL  ") + name + ((" -- " + str(extra)[:300]) if (extra and not cond) else ""))
    if not cond:
        FAILED.append(name)

for tool in ("Xvfb", "xdotool"):
    if shutil.which(tool) is None:
        print("  SKIP  %s not installed" % tool)
        sys.exit(0)

binary = sys.argv[1]
WINE = len(sys.argv) > 2 and sys.argv[2] == "wine"
here = os.path.dirname(os.path.abspath(__file__))

def free_display():
    for n in range(150, 250):
        if not os.path.exists("/tmp/.X11-unix/X%d" % n) and not os.path.exists("/tmp/.X%d-lock" % n):
            return n
    raise SystemExit("no free display")

n = free_display()
xvfb = subprocess.Popen(["Xvfb", ":%d" % n, "-screen", "0", "800x600x24"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
env = dict(os.environ, DISPLAY=":%d" % n)
os.environ["DISPLAY"] = ":%d" % n
for _ in range(100):
    if os.path.exists("/tmp/.X11-unix/X%d" % n):
        break
    time.sleep(0.1)
time.sleep(0.3)

procs = [xvfb]
try:
    if WINE:
        env["WINEPREFIX"] = os.environ.get("WINEPREFIX", os.path.expanduser("~/.wine-firn"))
        env["WINEDEBUG"] = "-all"
    prog = subprocess.Popen((["wine", "explorer", "/desktop=firn,800x600"] if WINE else []) + [binary], stdout=subprocess.PIPE, text=True, env=env, bufsize=1)
    procs.append(prog)
    first = prog.stdout.readline().strip()
    def host(p):          # what the program reports for the Unix path p
        return ("Z:" + p.replace("/", "\\")) if WINE else p
    check("the window comes up and accepts drops (READY)", first == "READY", first)
    time.sleep(0.5)
    for _ in range(50):
        found = subprocess.run(["xdotool", "search", "--name", "firn-drop-test"], capture_output=True, text=True, env=env).stdout.split()
        if found:
            break
        time.sleep(0.2)
    win_id = int(found[0])

    d = display.Display()
    A = lambda s: d.intern_atom(s)
    aware = d.create_resource_object("window", win_id).get_full_property(A("XdndAware"), Xatom.ATOM)
    check("the window carries XdndAware = 5", aware is not None and list(aware.value) == [5], aware)

    # ------------------------------------------------ 1. GTK as the source
    files = ["/tmp/firn-drop-a.txt", "/tmp/firn drop b.txt", "/tmp/über.txt"]
    uris = ["file://" + __import__("urllib.parse").parse.quote(f) for f in files]
    gtk = subprocess.Popen([sys.executable, os.path.join(here, "gtk_source.py")] + uris, stdout=subprocess.PIPE, text=True, env=env, bufsize=1)
    procs.append(gtk)
    ready = gtk.stdout.readline().strip()
    check("GTK source window is up", ready == "READY", ready)
    def x(*a):
        subprocess.run(["xdotool"] + [str(v) for v in a], env=env, check=True)
    x("mousemove", 50, 350); time.sleep(0.2)
    x("mousedown", 1); time.sleep(0.2)
    for (px, py) in ((60, 340), (80, 300), (100, 250), (120, 180), (130, 120), (140, 100)):
        x("mousemove", px, py); time.sleep(0.15)
    time.sleep(0.4)
    x("mouseup", 1)
    got = []
    t_end = time.time() + 10
    while time.time() < t_end and len([g for g in got if g.startswith("PATH")]) < 3:
        l = prog.stdout.readline()
        if not l:
            break
        got.append(l.rstrip("\n"))
    check("GTK drag: one drop of 3 files", "DROP 3" in got, got)
    check("GTK drag: the paths, percent decoding and UTF-8 right", [g[5:] for g in got if g.startswith("PATH ")] == [host(f) for f in files], got)
    time.sleep(0.3)

    # ------------------------------------------------ 2. a source written from the specification
    class Source:
        def __init__(self, uri_bytes, types, typelist=None, version=5):
            self.d = display.Display()
            self.root = self.d.screen().root
            self.win = self.root.create_window(0, 0, 10, 10, 0, self.d.screen().root_depth)
            self.A = lambda s: self.d.intern_atom(s)
            self.uri_bytes = uri_bytes
            self.types = types
            self.typelist = typelist
            self.version = version
            self.target = self.d.create_resource_object("window", win_id)
            self.status = None
            self.finished = None
            self.requests = 0
            self.win.change_attributes(event_mask=X.PropertyChangeMask)
            self.win.set_selection_owner(self.A("XdndSelection"), X.CurrentTime)
            self.d.flush()
        def send(self, mtype, data):
            ev = protocol.event.ClientMessage(window=self.target, client_type=self.A(mtype), data=(32, (list(data) + [0] * 5)[:5]))
            self.target.send_event(ev, event_mask=0)
            self.d.flush()
        def pump(self, until, timeout=3.0):
            end = time.time() + timeout
            while time.time() < end:
                while self.d.pending_events():
                    e = self.d.next_event()
                    if e.type == X.SelectionRequest:
                        self.requests += 1
                        t = e.target
                        prop = e.property if e.property else t
                        ok = False
                        if t == self.A("text/uri-list"):
                            e.requestor.change_property(prop, self.A("text/uri-list"), 8, self.uri_bytes)
                            ok = True
                        notify = protocol.event.SelectionNotify(time=e.time, requestor=e.requestor, selection=e.selection,
                                                                target=t, property=prop if ok else X.NONE)
                        e.requestor.send_event(notify, event_mask=0)
                        self.d.flush()
                    elif e.type == X.ClientMessage:
                        name = self.d.get_atom_name(e.client_type)
                        if name == "XdndStatus":
                            self.status = list(e.data[1])
                        elif name == "XdndFinished":
                            self.finished = list(e.data[1])
                if until():
                    return True
                time.sleep(0.02)
            return until()
        def enter(self):
            flags = self.version << 24
            ts = [self.A(t) for t in self.types[:3]] + [0] * (3 - len(self.types[:3]))
            if len(self.types) > 3:
                flags |= 1
                self.win.change_property(self.A("XdndTypeList"), Xatom.ATOM, 32, [self.A(t) for t in self.types])
            self.send("XdndEnter", [self.win.id, flags] + ts)
        def position(self, px=40, py=40):
            self.status = None
            self.send("XdndPosition", [self.win.id, 0, (px << 16) | py, X.CurrentTime, self.A("XdndActionCopy")])
            return self.pump(lambda: self.status is not None)
        def drop(self):
            self.send("XdndDrop", [self.win.id, 0, X.CurrentTime])
            return self.pump(lambda: self.finished is not None)
        def leave(self):
            self.send("XdndLeave", [self.win.id, 0, 0, 0, 0])
        def close(self):
            self.win.destroy()
            self.d.close()

    LIST = (b"# a comment\r\nfile:///tmp/a%20b.txt\r\nfile://localhost/tmp/%C3%BCber.txt\r\nfile://otherhost/etc/passwd\r\n"
            b"http://example.org/\r\n\r\nfile:///tmp/plain.txt\r\nfile:///tmp/with%00nul\r\nfile:///tmp/100%25\r\n")
    WANT = ["/tmp/a b.txt", "/tmp/über.txt", "/tmp/plain.txt", "/tmp/100%"]

    s = Source(LIST, ["text/uri-list", "text/plain"])
    s.enter()
    ok = s.position()
    check("XDND: Status accepts (bit 0) and names the copy action", ok and s.status[1] == 1 and s.status[4] == A("XdndActionCopy"), s.status)
    check("XDND: Status names our window", ok and s.status[0] == win_id, s.status)
    ok = s.position(120, 90)
    check("XDND: a second Position is answered again", ok and (s.status[1] & 1) == 1)
    ok = s.drop()
    check("XDND: XdndFinished with accepted = 1 and the copy action", ok and s.finished[1] == 1 and s.finished[2] == A("XdndActionCopy"), s.finished)
    check("XDND: the source was asked for text/uri-list exactly once", s.requests == 1, s.requests)
    s.close()

    # the type list in the property (more than three types, uri-list is the fifth)
    s = Source(LIST, ["text/plain", "text/html", "application/x-foo", "image/png", "text/uri-list"])
    s.enter()
    ok = s.position()
    check("XdndTypeList: uri-list found among five types", ok and (s.status[1] & 1) == 1, s.status)
    ok = s.drop()
    check("XdndTypeList: drop accepted", ok and s.finished[1] == 1, s.finished)
    s.close()

    # a source that offers only text: refused, and no drop reaches the program
    s = Source(b"hello", ["text/plain", "UTF8_STRING"])
    s.enter()
    ok = s.position()
    check("other types: Status refuses (bit 0 clear, no action)", ok and s.status[1] == 0 and s.status[4] == 0, s.status)
    ok = s.drop()
    check("other types: Finished with accepted = 0", ok and s.finished[1] == 0, s.finished)
    check("other types: the source was never asked for data", s.requests == 0, s.requests)
    s.close()

    # a drag that leaves again, then a drop without Enter (a confused source): nothing happens
    s = Source(LIST, ["text/uri-list"])
    s.enter(); s.position(); s.leave(); time.sleep(0.2)
    s.finished = None
    s.send("XdndDrop", [s.win.id, 0, X.CurrentTime])
    ok = s.pump(lambda: s.finished is not None, 2.0)
    check("after Leave a Drop is refused (accepted = 0)", ok and s.finished[1] == 0, s.finished)
    s.close()

    # nothing but remote files and comments: accepted by the protocol, but no usable path -> refused
    s = Source(b"# nothing\r\nfile://otherhost/etc/passwd\r\nhttp://example.org/x\r\n", ["text/uri-list"])
    s.enter(); s.position()
    ok = s.drop()
    check("a list without local files: Finished with accepted = 0", ok and s.finished[1] == 0, s.finished)
    s.close()

    # the program is still alive and a last good drop works
    s = Source(LIST, ["text/uri-list"])
    s.enter(); s.position()
    ok = s.drop()
    check("a last good drop still works", ok and s.finished[1] == 1, s.finished)
    s.close()
    time.sleep(0.5)
    prog.terminate()
    rest = prog.stdout.read()
    lines = got + rest.splitlines()
    drops = [i for i, l in enumerate(lines) if l.startswith("DROP")]
    check("the program saw 4 drops (GTK, list, type list, last) and no more", len(drops) == 4, lines)
    chunks = []
    for i in drops:
        chunks.append([l[5:] for l in lines[i + 1:i + 1 + int(lines[i].split()[1])] if l.startswith("PATH ")])
    check("drops 2-4: exactly the four local files, decoded (%20, UTF-8, %25; NUL, remote, http, comments dropped)",
          chunks[1:] == [[host(w) for w in WANT]] * 3, chunks)
    check("DROP counts say 4 for the spec source", [lines[i] for i in drops][1:] == ["DROP 4"] * 3, [lines[i] for i in drops])
finally:
    for p in procs[::-1]:
        try:
            p.terminate()
        except Exception:
            pass
print("drop: %d failed" % len(FAILED) if FAILED else "drop: all checks passed")
sys.exit(1 if FAILED else 0)
