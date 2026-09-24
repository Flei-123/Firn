#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/fui/x11live.py -- DIE DEMO IN EINEM ECHTEN FENSTER AUF EINEM ECHTEN X-SERVER.
#
# WARUM ES DAS GIBT. tools/fui/x11_main.fi rechnet das Protokoll und den Wirt
# ohne Bildschirm nach. Ob daraus ein FENSTER wird, das man bedienen kann,
# sagt nur ein X-Server. Also startet dieser Lauf einen eigenen Xvfb, laesst
# demos/x11demo darauf laufen und bedient sie von AUSSEN -- mit xdotool, wie
# ein Mensch mit Maus und Tastatur, und mit einer ClientMessage wie ein
# Fenstermanager. Was geprueft wird, kommt aus drei Quellen, die das
# Programm nicht faelschen kann:
#   * dem Server: xwininfo (gibt es das Fenster, wie gross), xwd (was steht
#     WIRKLICH darin -- nicht im Speicherpuffer des Programms),
#   * dem Kern: /proc/<pid>/stat (wie viel Rechenzeit im Leerlauf),
#   * dem Protokoll der Demo (--protokoll): Bilder, Zustand, Knoten.
#
#     python3 tools/fui/x11live.py <x11demo-programm> <belegordner>
#
# Kein Xvfb/xdotool/xwd auf dem Rechner: "SKIP" mit Grund, Rueckgabe 0 --
# ein fehlender X-Server ist kein Fehler der Demo. Jede andere Abweichung:
# "WRONG" und Rueckgabe 1.
import os, shutil, socket, struct, subprocess, sys, time

PROG = sys.argv[1]
BELEG = sys.argv[2]
os.makedirs(BELEG, exist_ok=True)
HIER = os.path.dirname(os.path.abspath(__file__))
fehler = 0
zahl = 0


def chk(nr, text, ok, got, want):
    global fehler, zahl
    zahl += 1
    if not ok:
        fehler += 1
    print("%-50s%s got %s want %s" % ("%s %s" % (nr, text),
          "OK  " if ok else "WRONG   ", got, want), flush=True)


try:
    from PIL import Image  # nur um das xwd-Bild zu lesen
except ImportError:
    print("SKIP: python3-PIL fehlt -- das Bild vom Server laesst sich nicht lesen")
    sys.exit(0)

for werk in ("Xvfb", "xdotool", "xwd", "xwininfo", "xrdb"):
    if shutil.which(werk) is None:
        print("SKIP: %s fehlt -- kein echter X-Server fuer den Beleg" % werk)
        sys.exit(0)


def freie_anzeige():
    for n in range(140, 200):
        if not os.path.exists("/tmp/.X11-unix/X%d" % n) and \
           not os.path.exists("/tmp/.X%d-lock" % n):
            return n
    raise SystemExit("keine freie Anzeige")


class Server:
    def __init__(self, breite, hoehe, dpi):
        self.n = freie_anzeige()
        self.env = dict(os.environ, DISPLAY=":%d" % self.n)
        self.env.pop("XAUTHORITY", None)
        # -noreset: sonst setzt Xvfb beim Abgang des letzten Programms alles
        # zurueck, auch RESOURCE_MANAGER (Xft.dpi).
        self.p = subprocess.Popen(["Xvfb", ":%d" % self.n, "-screen", "0",
                                   "%dx%dx24" % (breite, hoehe), "-dpi",
                                   str(dpi), "-nolisten", "tcp", "-noreset"],
                                  stdout=subprocess.DEVNULL,
                                  stderr=subprocess.DEVNULL)
        for _ in range(100):
            if os.path.exists("/tmp/.X11-unix/X%d" % self.n):
                break
            time.sleep(0.05)
        time.sleep(0.3)
        if self.p.poll() is not None:
            raise SystemExit("Xvfb :%d startete nicht" % self.n)

    def run(self, *a, **kw):
        return subprocess.run(list(a), env=self.env, capture_output=True,
                              **kw)

    def zu(self):
        self.p.terminate()
        self.p.wait()


class Demo:
    def __init__(self, srv, *args):
        self.srv = srv
        self.logpfad = os.path.join(BELEG, "x11demo-%d.log" % srv.n)
        self.log = open(self.logpfad, "w")
        self.p = subprocess.Popen([PROG, "--protokoll"] + list(args),
                                  env=srv.env, stdout=self.log,
                                  stderr=subprocess.STDOUT)
        self.knoten = {}

    def zeilen(self):
        self.log.flush()
        return open(self.logpfad).read().splitlines()

    def warte(self, pruef, sek=5.0):
        ende = time.time() + sek
        while time.time() < ende:
            z = self.zeilen()
            r = pruef(z)
            if r:
                return r
            time.sleep(0.05)
        return None

    def bilder(self):
        return sum(1 for z in self.zeilen() if z.startswith("bild "))

    def letzter_zustand(self):
        for z in reversed(self.zeilen()):
            if z.startswith("zustand "):
                return [int(v) for v in z.split()[1:]]
        return None

    def lies_knoten(self):
        for z in self.zeilen():
            if z.startswith("knoten "):
                t = z.split()
                self.knoten[t[1]] = [int(v) for v in t[2:6]]
        return self.knoten

    def mitte(self, name):
        x, y, w, h = self.knoten[name]
        return x + w // 2, y + h // 2

    def ruhig(self, sek=0.6):
        # Wartet, bis eine ganze Weile kein neues Bild mehr kam.
        alt = -1
        while True:
            n = self.bilder()
            if n == alt:
                return n
            alt = n
            time.sleep(sek)

    def cpu(self):
        f = open("/proc/%d/stat" % self.p.pid).read().split(")")[1].split()
        return int(f[11]) + int(f[12])


def fenster_id(srv):
    r = srv.run("xwininfo", "-root", "-tree").stdout.decode()
    for z in r.splitlines():
        if "fUi -- beschriebene Seite, X11" in z:
            t = z.split()
            geo = [s for s in t if "x" in s and "+" in s][0]
            w, rest = geo.split("x")
            h = rest.split("+")[0]
            return int(t[0], 16), int(w), int(h)
    return None


def xwd(srv, wid, name):
    roh = os.path.join(BELEG, name + ".xwd")
    with open(roh, "wb") as f:
        subprocess.run(["xwd", "-silent", "-id", str(wid)], env=srv.env,
                       stdout=f, check=True)
    png = os.path.join(BELEG, name + ".png")
    subprocess.run([sys.executable, os.path.join(HIER, "xwd2png.py"), roh,
                    png], check=True, capture_output=True)
    from PIL import Image
    return Image.open(png).convert("RGB"), png


def wm_delete(srv, wid):
    # So schliesst ein Fenstermanager: SendEvent mit einer ClientMessage
    # WM_PROTOCOLS / WM_DELETE_WINDOW. Roh ueber den Socket, ohne Xlib.
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.connect("/tmp/.X11-unix/X%d" % srv.n)
    s.sendall(struct.pack("<BxHHHHxx", 0x6C, 11, 0, 0, 0))
    kopf = s.recv(8)
    rest = struct.unpack_from("<H", kopf, 6)[0] * 4
    while rest > 0:
        rest -= len(s.recv(rest))

    def atom(name):
        nb = name.encode()
        pad = (4 - len(nb) % 4) % 4
        s.sendall(struct.pack("<BBHHxx", 16, 1, 2 + (len(nb) + pad) // 4,
                              len(nb)) + nb + b"\0" * pad)
        while True:
            b = s.recv(32)
            if b[0] == 1:
                return struct.unpack_from("<I", b, 8)[0]
    ap, ad = atom("WM_PROTOCOLS"), atom("WM_DELETE_WINDOW")
    cm = struct.pack("<BBHIII", 33, 32, 0, wid, ap, ad) + b"\0" * 16
    s.sendall(struct.pack("<BBHII", 25, 0, 11, wid, 0) + cm)
    s.sendall(struct.pack("<BxH", 43, 1))  # GetInputFocus: Rundreise
    s.recv(32)
    s.close()


# ======================================================== 1x, 96 dpi
srv = Server(1400, 900, 96)
try:
    d = Demo(srv)
    ok = d.warte(lambda z: any(l.startswith("bild ") for l in z), 10)
    fw = fenster_id(srv)
    chk("L1", "Fenster auf dem Server (xwininfo)", fw is not None,
        fw and "%dx%d" % fw[1:], "1240x720")
    wid = fw[0] if fw else 0
    chk("L1", "Fenster 1240x720 bei 96 dpi", fw is not None and fw[1:] ==
        (1240, 720), fw and fw[1:], (1240, 720))
    d.ruhig()
    z = d.letzter_zustand()
    chk("L1", "Massstab 1000 (Xvfb -dpi 96, kein Xft.dpi)", z and z[6] == 1000,
        z and z[6], 1000)
    k = d.lies_knoten()
    chk("L1", "Liste bei y 168, 520x528 (wie gallery9)", k.get("liste") ==
        [24, 168, 520, 528], k.get("liste"), [24, 168, 520, 528])
    bild, png1 = xwd(srv, wid, "x11demo-1x")
    sx, sy = d.mitte("starten")
    akz = bild.getpixel((k["starten"][0] + 4, sy))
    grund = bild.getpixel((5, 5))
    chk("L1", "Bild vom Server: Starten traegt den Akzent", akz != grund and
        akz[2] > 200 and akz[0] < 60, akz, "blau")
    farben = len(bild.getcolors(maxcolors=1 << 24))
    chk("L1", "Bild vom Server hat Text und Flaechen (Farben)", farben > 200,
        farben, "> 200")

    # ---------------------------------------------- DER LEERLAUF
    n0 = d.bilder()
    c0 = d.cpu()
    time.sleep(3.0)
    c1 = d.cpu()
    chk("L2", "3 s Leerlauf: keine neuen Bilder", d.bilder() == n0,
        d.bilder() - n0, 0)
    chk("L2", "3 s Leerlauf: <= 2 CPU-Ticks (je 10 ms)", c1 - c0 <= 2,
        c1 - c0, "<= 2")

    # ---------------------------------------------- MAUS
    srv.run("xdotool", "mousemove", str(sx), str(sy))
    ok = d.warte(lambda z: sum(1 for l in z if l.startswith("bild ")) > n0)
    chk("L3", "Zeiger auf Starten: ein Bild (Ueberfahren)", ok is not None,
        d.bilder() - n0, ">= 1")
    x, y = d.mitte("schalter")
    srv.run("xdotool", "mousemove", str(k["schalter"][0] + 12), str(y),
            "click", "1")
    ok = d.warte(lambda z: (d.letzter_zustand() or [0, 0, 1])[2] == 0)
    chk("L3", "Klick auf den Schalter: 1 -> 0", ok is not None,
        d.letzter_zustand()[2], 0)
    x, y = d.mitte("kaestchen")
    srv.run("xdotool", "mousemove", str(k["kaestchen"][0] + 8), str(y),
            "click", "1")
    ok = d.warte(lambda z: (d.letzter_zustand() or [0, 0, 0, 1])[3] == 0)
    chk("L3", "Klick aufs Kaestchen: 1 -> 0", ok is not None,
        d.letzter_zustand()[3], 0)
    lx, ly = d.mitte("liste")
    srv.run("xdotool", "mousemove", str(lx), str(ly), "click", "--repeat",
            "3", "--delay", "30", "5")
    ok = d.warte(lambda z: (d.letzter_zustand() or [0, 0])[1] == 216)
    chk("L3", "3 Rasten Rad ueber der Liste: 216", ok is not None,
        d.letzter_zustand()[1], 216)
    bild_r, png_r = xwd(srv, wid, "x11demo-1x-gerollt")

    # ---------------------------------------------- TASTATUR
    srv.run("xdotool", "key", "End")
    ok = d.warte(lambda z: (d.letzter_zustand() or [0, 0])[1] > 216)
    unten = d.letzter_zustand()[1]
    chk("L4", "Taste Ende: ganz unten (602)", unten == 602, unten, 602)
    srv.run("xdotool", "key", "Home")
    ok = d.warte(lambda z: (d.letzter_zustand() or [0, 1])[1] == 0)
    chk("L4", "Taste Pos1: oben", ok is not None, d.letzter_zustand()[1], 0)
    srv.run("xdotool", "key", "Tab")
    ok = d.warte(lambda z: (d.letzter_zustand() or [0] * 6)[5] != 7 and
                 (d.letzter_zustand() or [0] * 6)[5] >= 0)
    f1 = d.letzter_zustand()[5]
    srv.run("xdotool", "key", "shift+Tab")
    ok2 = d.warte(lambda z: (d.letzter_zustand() or [0] * 6)[5] != f1)
    f2 = d.letzter_zustand()[5]
    chk("L4", "Tab / Umschalt+Tab bewegen den Fokus", ok is not None and
        ok2 is not None and f1 != f2, "%d -> %d" % (f1, f2), "verschieden")
    srv.run("xdotool", "key", "space")
    ok = d.warte(lambda z: (d.letzter_zustand() or [0, 0, 0, 0])[3] == 1)
    chk("L4", "Leertaste auf dem Kaestchen (Fokus): 0 -> 1",
        ok is not None, d.letzter_zustand()[3], 1)

    # ---------------------------------------------- DIE ANIMATION
    d.ruhig()
    n1 = d.bilder()
    c2 = d.cpu()
    sx, sy = d.mitte("starten")
    srv.run("xdotool", "mousemove", str(sx), str(sy), "click", "1")
    ok = d.warte(lambda z: (d.letzter_zustand() or [0] * 5)[4] == 1000, 8)
    n2 = d.ruhig(0.8)
    chk("L5", "Starten: Fortschritt laeuft bis 1000", ok is not None,
        d.letzter_zustand()[4], 1000)
    werte = [int(l.split()[5]) for l in d.zeilen()
             if l.startswith("zustand ")][n1:]
    steigt = all(b >= a for a, b in zip(werte, werte[1:]))
    zwischen = [v for v in werte if 0 < v < 1000]
    chk("L5", "Bild fuer Bild steigend, mit Zwischenstaenden",
        steigt and len(zwischen) >= 2, werte, "steigend, >= 2 dazwischen")
    zeiten = [int(l.split()[1]) for l in d.zeilen() if l.startswith("zeit ")]
    bz = [int(l.split()[3]) for l in d.zeilen() if l.startswith("zeit ")]
    print("   (gemessen: fUi malt ein Vollbild in %d ms im Mittel, "
          "PutImage %d ms -- die Bildrate haengt am Rasterer)"
          % (sum(zeiten) // max(1, len(zeiten)), sum(bz) // max(1, len(bz))))
    time.sleep(2.0)
    chk("L5", "danach wieder Ruhe: 0 Bilder in 2 s", d.bilder() == n2,
        d.bilder() - n2, 0)

    # ---------------------------------------------- GROESSE AENDERN
    srv.run("xdotool", "windowsize", str(wid), "900", "600")
    ok = d.warte(lambda z: any(l.startswith("bild ") and " 900 600 " in l
                               for l in z))
    fw2 = fenster_id(srv)
    chk("L6", "windowsize 900x600: Bild in 900x600", ok is not None and
        fw2 and fw2[1:] == (900, 600), fw2 and fw2[1:], (900, 600))
    d.ruhig()
    k2 = d.lies_knoten()
    bild_k, png_k = xwd(srv, wid, "x11demo-900x600")
    chk("L6", "die Liste bleibt 520 breit, der Rest schrumpft",
        k2["liste"][2] == 520 and k2["fortschritt"][2] < 616,
        (k2["liste"][2], k2["fortschritt"][2]), "(520, < 616)")

    # ---------------------------------------------- SCHLIESSEN
    wm_delete(srv, wid)
    try:
        rc = d.p.wait(5)
    except subprocess.TimeoutExpired:
        rc = None
        d.p.kill()
    chk("L7", "WM_DELETE_WINDOW (SendEvent): Ende mit 0", rc == 0, rc, 0)
    chk("L7", "und die Demo sagt 'geschlossen'", "geschlossen" in
        d.zeilen(), d.zeilen()[-1:], "geschlossen")

    # ---------------------------------------------- Xft.dpi 192
    srv.run("xrdb", "-merge", input=b"Xft.dpi: 192\n")
    d2 = Demo(srv)
    d2.warte(lambda z: any(l.startswith("bild ") for l in z), 10)
    d2.ruhig()
    z2 = d2.letzter_zustand()
    chk("L8", "Xft.dpi 192 im Server -> Massstab 2000", z2 and z2[6] == 2000,
        z2 and z2[6], 2000)
    fw3 = fenster_id(srv)
    chk("L8", "Fenster 2480x1440 (1240x720 Entwurf * 2)", fw3 and fw3[1:] ==
        (2480, 1440), fw3 and fw3[1:], (2480, 1440))
    k3 = d2.lies_knoten()
    chk("L8", "Liste bei y 336, 1040x1056 (genau doppelt)", k3.get("liste")
        == [48, 336, 1040, 1056], k3.get("liste"), [48, 336, 1040, 1056])
    srv.run("xdotool", "key", "--window", str(fw3[0]), "Escape")
    try:
        rc2 = d2.p.wait(5)
    except subprocess.TimeoutExpired:
        rc2 = None
        d2.p.kill()
    chk("L8", "Escape beendet mit 0", rc2 == 0, rc2, 0)
finally:
    srv.zu()

# =========================================== 2x auf einem 192-dpi-Schirm
srv2 = Server(2800, 1800, 192)
try:
    d3 = Demo(srv2)
    d3.warte(lambda z: any(l.startswith("bild ") for l in z), 15)
    d3.ruhig()
    z3 = d3.letzter_zustand()
    chk("L9", "Xvfb -dpi 192 (mm) -> Massstab 2000", z3 and z3[6] == 2000,
        z3 and z3[6], 2000)
    fw4 = fenster_id(srv2)
    k4 = d3.lies_knoten()
    bild2, png2 = xwd(srv2, fw4[0], "x11demo-2x")
    chk("L9", "Bild vom Server 2480x1440", bild2.size == (2480, 1440),
        bild2.size, (2480, 1440))
    x, y = d3.mitte("schalter")
    srv2.run("xdotool", "mousemove", str(k4["schalter"][0] + 24), str(y),
             "click", "1")
    ok = d3.warte(lambda z: (d3.letzter_zustand() or [0, 0, 1])[2] == 0)
    chk("L9", "bei 2x trifft der Klick den Schalter", ok is not None,
        d3.letzter_zustand()[2], 0)
    lx, ly = d3.mitte("liste")
    srv2.run("xdotool", "mousemove", str(lx), str(ly), "click", "5")
    ok = d3.warte(lambda z: (d3.letzter_zustand() or [0, 0])[1] == 144)
    chk("L9", "ein Rasten bei 2x: 144 Bildpunkte", ok is not None,
        d3.letzter_zustand()[1], 144)
    srv2.run("xdotool", "key", "Escape")
    try:
        d3.p.wait(5)
    except subprocess.TimeoutExpired:
        d3.p.kill()
finally:
    srv2.zu()

print("Belege: %s" % ", ".join(os.path.join(BELEG, n) for n in
      ("x11demo-1x.png", "x11demo-1x-gerollt.png", "x11demo-900x600.png",
       "x11demo-2x.png")))
if fehler:
    print("X11 LIVE NOT PASSED (%d von %d falsch)" % (fehler, zahl))
    sys.exit(1)
print("X11 LIVE PASSED (%d Pruefungen auf einem echten Xvfb)." % zahl)
