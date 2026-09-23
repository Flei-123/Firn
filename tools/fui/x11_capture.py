#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/fui/x11_capture.py -- ECHTE PROTOKOLLMITSCHNITTE fuer tools/fui/x11_main.fi.
#
# WARUM ES DAS GIBT. Die Pruefung der X11-Rueckwand (lib/window/x11.fi) soll
# nicht ihre eigene Rechnung gegen sich selbst pruefen, sondern gegen das, was
# ein ECHTER X-Server schickt. Dieses Skript spricht deshalb mit einem echten
# Xvfb -- unabhaengig vom Firn-Client, roh ueber den Unix-Socket -- und legt die
# Antworten Oktett fuer Oktett unter testdata/x11/ ab:
#
#   setup.bin       die Antwort auf den Verbindungsaufbau (Xvfb 1400x900, -dpi 96)
#   keymap_us.bin   GetKeyboardMapping(min..max) mit der US-Belegung
#   keymap_de.bin   dasselbe nach `setxkbmap de`
#   xmodmap_us.txt, xmodmap_de.txt  `xmodmap -pke` -- die Gegenprobe eines
#                   zweiten, fremden Programms fuer dieselben Zahlen
#   modmap_us.bin, modmap_de.bin  GetModifierMapping -- welche Modifikator-
#                   Taste (Mod1..Mod5) AltGr bzw. NumLock ist
#   resman.bin      GetProperty(root, RESOURCE_MANAGER) nach
#                   `echo 'Xft.dpi: 144' | xrdb -merge`
#   events.bin      32-Oktett-Ereignisse, ausgeloest mit xdotool bzw. SendEvent,
#                   in der Reihenfolge von events.txt
#   xauthority      eine Datei, die `xauth` (nicht Firn) geschrieben hat
#
# Es ist ein WERKZEUG zum Erzeugen von Testdaten, kein Teil des Produkts; es
# wird nur neu laufen gelassen, wenn sich die Mitschnitte aendern sollen.
#     python3 tools/fui/x11_capture.py [ziel=testdata/x11] [anzeige=61]
import os, socket, struct, subprocess, sys, time

ZIEL = sys.argv[1] if len(sys.argv) > 1 else "testdata/x11"
ANZ = sys.argv[2] if len(sys.argv) > 2 else "61"
os.makedirs(ZIEL, exist_ok=True)
env = dict(os.environ, DISPLAY=":" + ANZ)
env.pop("XAUTHORITY", None)


def schreibe(name, data):
    with open(os.path.join(ZIEL, name), "wb") as f:
        f.write(data)


# EINE EIGENE ANZEIGE, NIE EINE FREMDE. Beim ersten Lauf lief auf der
# gewaehlten Nummer schon ein fremder Xvfb; das eigene Xvfb scheiterte still,
# und die Mitschnitte (samt `setxkbmap`/`xrdb`) gingen an den fremden Server.
# Also: ist die Anzeige belegt, wird abgebrochen, und nach dem Start muss
# unser eigener Prozess noch leben.
if os.path.exists("/tmp/.X11-unix/X" + ANZ) or os.path.exists("/tmp/.X%s-lock" % ANZ):
    raise SystemExit("Anzeige :%s ist belegt -- nimm eine andere" % ANZ)
xvfb = subprocess.Popen(["Xvfb", ":" + ANZ, "-screen", "0", "1400x900x24",
                         "-dpi", "96", "-nolisten", "tcp"],
                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
try:
    pfad = "/tmp/.X11-unix/X" + ANZ
    for _ in range(100):
        if os.path.exists(pfad):
            break
        time.sleep(0.05)
    time.sleep(0.3)
    if xvfb.poll() is not None:
        raise SystemExit("Xvfb :%s ist nicht gestartet" % ANZ)
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.connect(pfad)

    def lies(n):
        b = b""
        while len(b) < n:
            k = s.recv(n - len(b))
            if not k:
                raise SystemExit("Verbindung zu")
            b += k
        return b

    # Verbindungsaufbau ohne Berechtigung (Xvfb ohne -auth).
    s.sendall(struct.pack("<BxHHHHxx", 0x6C, 11, 0, 0, 0))
    kopf = lies(8)
    setup = kopf + lies(struct.unpack_from("<H", kopf, 6)[0] * 4)
    schreibe("setup.bin", setup)
    id_base, id_mask = struct.unpack_from("<II", setup, 12)
    vlen = struct.unpack_from("<H", setup, 24)[0]
    nfmt = setup[29]
    kmin, kmax = setup[34], setup[35]
    at = 40 + vlen + ((4 - vlen % 4) % 4) + nfmt * 8
    root = struct.unpack_from("<I", setup, at)[0]
    root_visual = struct.unpack_from("<I", setup, at + 32)[0]
    seq = [0]

    def antwort():
        # Ereignisse, die vor der Antwort kommen, werden uebersprungen.
        while True:
            b = lies(32)
            if b[0] == 1:
                n = struct.unpack_from("<I", b, 4)[0] * 4
                return b + lies(n)
            if b[0] == 0:
                raise SystemExit("X-Fehler %d" % b[1])

    def keymap():
        n = kmax - kmin + 1
        s.sendall(struct.pack("<BxHBBxx", 101, 2, kmin, n))
        return antwort()

    def modmap():
        s.sendall(struct.pack("<BxH", 119, 1))
        return antwort()

    schreibe("keymap_us.bin", keymap())
    schreibe("modmap_us.bin", modmap())
    schreibe("xmodmap_us.txt", subprocess.run(["xmodmap", "-pke"], env=env,
             capture_output=True).stdout)
    subprocess.run(["setxkbmap", "de"], env=env, check=True)
    time.sleep(0.2)
    schreibe("keymap_de.bin", keymap())
    schreibe("modmap_de.bin", modmap())
    schreibe("xmodmap_de.txt", subprocess.run(["xmodmap", "-pke"], env=env,
             capture_output=True).stdout)
    subprocess.run(["setxkbmap", "us"], env=env, check=True)
    time.sleep(0.2)

    subprocess.run(["xrdb", "-merge"], input=b"Xft.dpi: 144\n", env=env,
                   check=True)
    s.sendall(struct.pack("<BBHIIIII", 20, 0, 6, root, 23, 0, 0, 16384))
    schreibe("resman.bin", antwort())

    # Ein Fenster bei (0,0), 200x100, mit allen Ereignissen, die x11.fi liest.
    wid = id_base | 1
    maske = (1 | 2 | 4 | 8 | 16 | 32 | 64 | 32768 | 131072)
    s.sendall(struct.pack("<BBHIIhhHHHHIIII", 1, 24, 10, wid, root, 0, 0,
                          200, 100, 0, 1, root_visual, 2050, 0xFFFFFF, maske))
    # WM_PROTOCOLS / WM_DELETE_WINDOW: die Atome holen wie x11.fi.
    def atom(name):
        nb = name.encode()
        pad = (4 - len(nb) % 4) % 4
        s.sendall(struct.pack("<BBHHxx", 16, 0, 2 + (len(nb) + pad) // 4,
                              len(nb)) + nb + b"\0" * pad)
        return struct.unpack_from("<I", antwort(), 8)[0]
    a_prot = atom("WM_PROTOCOLS")
    a_del = atom("WM_DELETE_WINDOW")
    s.sendall(struct.pack("<BxHI", 8, 2, wid))  # MapWindow
    s.settimeout(3.0)
    ereignisse = []
    namen = []

    def sammle(name, bis_code, warte=2.0):
        # Liest Ereignisse, bis eines mit `bis_code` kam; alle werden behalten,
        # aber nur das erste passende bekommt den Namen.
        ende = time.time() + warte
        while time.time() < ende:
            b = lies(32)
            code = b[0] & 127
            if code == 1:
                lies(struct.unpack_from("<I", b, 4)[0] * 4)
                continue
            if code == bis_code:
                ereignisse.append(b)
                namen.append(name)
                return b
        raise SystemExit("kein Ereignis %d fuer %s" % (bis_code, name))

    sammle("expose", 12)
    time.sleep(0.2)

    def xdo(*a):
        subprocess.run(["xdotool"] + list(a), env=env, check=True)

    xdo("mousemove", "50", "40")
    sammle("motion 50 40", 6)
    xdo("click", "1")
    sammle("buttonpress 1", 4)
    sammle("buttonrelease 1", 5)
    xdo("click", "5")
    sammle("buttonpress 5", 4)
    sammle("buttonrelease 5", 5)
    xdo("key", "a")
    sammle("keypress a", 2)
    sammle("keyrelease a", 3)
    xdo("key", "shift+a")
    sammle("keypress shift", 2)
    sammle("keypress shift+a", 2)
    xdo("key", "Tab")
    sammle("keypress Tab", 2)
    xdo("key", "Escape")
    sammle("keypress Escape", 2)
    xdo("mousemove", "600", "500")
    sammle("leave", 8)
    xdo("mousemove", "20", "30")
    sammle("enter", 7)
    # Fenstergroesse aendern: ConfigureWindow(12), Maske Breite|Hoehe.
    s.sendall(struct.pack("<BxHIHxxII", 12, 5, wid, 4 | 8, 320, 240))
    sammle("configure 320x240", 22)
    # Schliessen, wie ein Fenstermanager es tut: SendEvent(25) mit einer
    # ClientMessage(33) WM_PROTOCOLS / WM_DELETE_WINDOW an das Fenster.
    cm = struct.pack("<BBHIII", 33, 32, 0, wid, a_prot, a_del) + b"\0" * 16
    s.sendall(struct.pack("<BBHII", 25, 0, 11, wid, 0) + cm)
    sammle("clientmessage delete", 33)
    schreibe("events.bin", b"".join(ereignisse))
    with open(os.path.join(ZIEL, "events.txt"), "w") as f:
        for i, n in enumerate(namen):
            f.write("%d %s\n" % (i, n))
        f.write("atom WM_PROTOCOLS %d\natom WM_DELETE_WINDOW %d\nwindow %d\n"
                % (a_prot, a_del, wid))
    s.close()
finally:
    xvfb.terminate()
    xvfb.wait()

# Eine Xauthority-Datei, die `xauth` schreibt -- drei Eintraege, damit die
# Auswahl nach Anzeigenummer etwas zu entscheiden hat.
xa = os.path.join(ZIEL, "xauthority")
if os.path.exists(xa):
    os.remove(xa)
for anz, keks in (("rechner/unix:5", "00112233445566778899aabbccddeeff"),
                  ("rechner/unix:7", "0f1e2d3c4b5a69788796a5b4c3d2e1f0"),
                  ("rechner/unix:12", "deadbeefdeadbeefdeadbeefdeadbeef")):
    subprocess.run(["xauth", "-f", xa, "add", anz, "MIT-MAGIC-COOKIE-1", keks],
                   check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
print("Mitschnitte in", ZIEL)
