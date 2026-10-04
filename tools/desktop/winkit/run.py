#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""The Windows half of the desktop libraries' checks, as a kit for ONE machine with Python 3.

On the build machine these programs only ever ran under Wine (docs/DESKTOP.md says what that proves and
what it does not). This kit runs the same programs on a real Windows desktop:

    py run.py                  automatic checks; takes a few minutes
    py run.py --interactive    also asks you to look at / listen to / drag things (recommended once)
    py run.py --keep           keep the temporary folder

Needs only Python 3 (and, for the tray/clipboard/drop parts, a logged-in desktop session). Nothing is
installed; the registry key HKCU\\Software\\FirnDesktopKit is made and removed again; the folder watcher test
writes into the temporary folder; the Run-key test uses a value name under that key, never the real Run key.
Exit code 0 = every automatic check passed. Send back the printed report (or kit-report.txt).
"""
import array, ctypes, os, shutil, subprocess, sys, tempfile, time, zlib, urllib.parse

HERE = os.path.dirname(os.path.abspath(__file__))
INTERACTIVE = "--interactive" in sys.argv
KEEP = "--keep" in sys.argv
FAILED, SKIPPED, LOG = [], [], []
td = tempfile.mkdtemp(prefix="firn-desktop-kit-")

def out(s=""):
    print(s, flush=True)
    LOG.append(s)

def check(name, cond, extra=""):
    out(("  OK    " if cond else "  FAIL  ") + name + ((" -- " + str(extra)[:400]) if (extra and not cond) else ""))
    if not cond:
        FAILED.append(name)

def skip(name, why):
    out("  SKIP  %s: %s" % (name, why))
    SKIPPED.append(name)

def ask(question):
    if not INTERACTIVE:
        return None
    a = input("  ?  %s [y/n/s=skip] " % question).strip().lower()
    return None if a.startswith("s") else a.startswith("y")

def exe(name):
    p = os.path.join(HERE, name)
    return p if os.path.exists(p) else None

def run(prog, *args, env=None, timeout=120):
    e = dict(os.environ)
    if env:
        e.update(env)
    return subprocess.run([prog] + list(args), capture_output=True, env=e, timeout=timeout)

def text(b):
    return b.decode("utf-8", "replace")

def section(n, title):
    out("")
    out("== %d. %s ==" % (n, title))

def scale(pcm, percent):
    g = min(percent, 400) * 65536 // 100
    a = array.array("h"); a.frombytes(pcm)
    o = array.array("h")
    for v in a:
        x = abs(v) * g // 65536
        x = x if v >= 0 else -x
        o.append(max(-32768, min(32767, x)))
    return o.tobytes()

if os.name != "nt":
    out("This kit is for Windows (the programs are .exe files). On Linux use tools/desktop/run.sh.")
    sys.exit(2)

out("desktop kit: Windows %s, Python %s, interactive=%s" % (sys.getwindowsversion().major, sys.version.split()[0], INTERACTIVE))
ref = {}
for line in open(os.path.join(HERE, "reference.txt")):
    k, v = line.split()
    ref[k] = int(v)

try:
    # ---------------------------------------------------------------- 1
    section(1, "folder watcher (ReadDirectoryChangesW) and the command line channel (named pipe): tests 2220, 2221")
    for name in ("t2220.exe", "t2221.exe"):
        p = exe(name)
        if not p:
            skip(name, "not in the kit"); continue
        r = run(p, timeout=300)
        check("%s exits 0" % name, r.returncode == 0, (r.returncode, text(r.stdout)[-300:], text(r.stderr)[-300:]))
    # ---------------------------------------------------------------- 2
    section(2, "autostart: the Run key value, split by the C runtime exactly as the shell would")
    main_exe, dump, launch = exe("autostart_main.exe"), exe("argv_dump.exe"), exe("launch.exe")
    if main_exe and dump and launch:
        KEY = "Software\\FirnDesktopKit\\Run"
        spaced = os.path.join(td, "my app dir"); os.makedirs(spaced)
        shutil.copy(dump, spaced)
        CASES = [("simple", False, ["--minimized"]), ("blank", False, ["--profile=a b", "plain"]),
                 ("quotes", False, ['say "hi"', "it's", "back\\slash", "trail\\", "two\\\\", 'q\\"x', "50%", "$HOME"]),
                 ("empty", False, ["", "x", ""]), ("shell", False, ["a;b", "a|b", "a&b", "a>b", "a<b", "a^b", "(y)"]),
                 ("none", False, []), ("spaced-exe", True, ["one", "two words"])]
        for name, sp, args in CASES:
            prog = os.path.join(spaced, "argv_dump.exe") if sp else dump
            af = os.path.join(td, name + ".args")
            open(af, "w", encoding="utf-8").write("\n".join(args))
            r = run(main_exe, "set", KEY, "firn.kit." + name, "T", prog, "@" + af)
            check("%s: set" % name, r.returncode == 0, text(r.stdout + r.stderr))
            q = run(main_exe, "command", KEY, "firn.kit." + name)
            line = text(q.stdout).strip("\r\n")
            o = os.path.join(td, name + ".out")
            run(launch, line, env={"OUT": o})
            got = open(o, "rb").read().decode("utf-8").split("\0")[:-1] if os.path.exists(o) else None
            check("%s: the C runtime splits it into exactly the arguments" % name, got == args, (got, args, line[:200]))
            check("%s: is_enabled, then remove" % name, run(main_exe, "enabled", KEY, "firn.kit." + name).returncode == 0
                  and run(main_exe, "remove", KEY, "firn.kit." + name).returncode == 0
                  and run(main_exe, "enabled", KEY, "firn.kit." + name).returncode == 1)
        subprocess.run(["reg", "delete", "HKCU\\Software\\FirnDesktopKit", "/f"], capture_output=True)
    else:
        skip("autostart", "programs missing from the kit")
    # ---------------------------------------------------------------- 3
    section(3, "audio: waveOut (the sound card is not needed for the automatic checks: FIRN_AUDIO writes a file)")
    sink, audio = exe("sink_main.exe"), exe("audio_main.exe")
    mp3 = os.path.join(HERE, "stereo44.mp3")
    if sink and audio:
        o = os.path.join(td, "pattern.raw")
        r = run(sink, "3", "1152", env={"FIRN_AUDIO": "raw:" + o})
        d = open(o, "rb").read() if os.path.exists(o) else b""
        check("pattern through the file sink: 3 s, CRC-32 as computed on the build machine", len(d) == 529200 and zlib.crc32(d) == ref["pattern_crc"], (len(d), zlib.crc32(d)))
        for pc in (100, 50):
            o = os.path.join(td, "mp3_%d.raw" % pc)
            r = run(audio, mp3, str(pc), "play", env={"FIRN_AUDIO": "raw:" + o})
            d = open(o, "rb").read() if os.path.exists(o) else b""
            check("MP3 at %d %% through the player: CRC-32 equals the decoder's (scaled)" % pc,
                  zlib.crc32(d) == ref["mp3_crc_%d" % pc] and len(d) == ref["mp3_len"], (len(d), zlib.crc32(d)))
        # the real device: a program that opens the default output and plays 3 s of the pattern
        r = run(sink, "3", "1152", timeout=60)
        check("the real waveOut device takes 3 s of the pattern and drains (no FIRN_AUDIO)",
              "SINK 132300" in text(r.stdout) and r.returncode == 0, (text(r.stdout), r.returncode))
        a = ask("Did you just hear a rising buzz / sawtooth tone for about 3 seconds?")
        if a is not None:
            check("(you) audible", a)
        r = run(audio, mp3, "60", "play", timeout=60)
        check("the MP3 plays to the end through the real device", "RESULT 3" in text(r.stdout), text(r.stdout))
        a = ask("Did you just hear music (a short clip) at 60 % volume?")
        if a is not None:
            check("(you) audible music", a)
    else:
        skip("audio", "programs missing from the kit")
    # ---------------------------------------------------------------- 4
    section(4, "clipboard: one program offers text, a file list and a private type; another one reads them")
    clip = exe("clip_main.exe")
    if clip:
        t1 = os.path.join(td, "t.txt")
        TEXT = "h\u00e9llo w\u00f6rld \u65e5\u672c\u8a9e\nline two\n\nlast line"
        open(t1, "w", encoding="utf-8", newline="").write(TEXT)
        f1, f2 = os.path.join(td, "clip one.txt"), os.path.join(td, "\u00fcber.txt")
        open(f1, "w").write("x"); open(f2, "w").write("y")
        uris = "".join("file:///" + urllib.parse.quote(p.replace("\\", "/"), safe="/:") + "\r\n" for p in (f1, f2))
        uf = os.path.join(td, "u.txt")
        open(uf, "w", newline="").write(uris)
        po = open(os.path.join(td, "set.out"), "wb")
        pa = subprocess.Popen([clip, "set", t1, uf], stdout=po, stderr=subprocess.DEVNULL)
        end = time.time() + 30
        while time.time() < end and b"READY" not in open(os.path.join(td, "set.out"), "rb").read():
            time.sleep(0.2)
        s_out = text(open(os.path.join(td, "set.out"), "rb").read())
        check("the first program set all three types", "SET text ok" in s_out and "SET uris ok" in s_out and "SET private ok" in s_out, s_out)
        r = run(clip, "get", "text/plain;charset=utf-8")
        o = text(r.stdout)
        check("a second program reads the text exactly (UTF-8, line breaks)", o.startswith("GOT ") and o.split("\n", 1)[1].rstrip("\n") == TEXT, o[:200])
        r = run(clip, "get", "text/uri-list")
        o = text(r.stdout)
        got_uris = [urllib.parse.unquote(l) for l in o.split("\n", 1)[1].split("\r\n") if l.startswith("file://")] if o.startswith("GOT ") else []
        want_uris = [urllib.parse.unquote(l.strip()) for l in uris.split("\r\n") if l]
        check("... and the file list (two files)", [u.lower() for u in got_uris] == [u.lower() for u in want_uris], (got_uris, want_uris))
        r = run(clip, "get", "application/x-firn-test")
        check("... and the private type (16 octets)", text(r.stdout).startswith("GOT 16"), text(r.stdout)[:80])
        r = run(clip, "types")
        check("types lists text and the file list", "text/plain" in text(r.stdout) and "text/uri-list" in text(r.stdout), text(r.stdout))
        a = ask("Paste into Notepad now (Ctrl+V) while this is waiting: do you get the two text lines?") if INTERACTIVE and False else None
        pa.terminate()
    else:
        skip("clipboard", "clip_main.exe missing")
    # ---------------------------------------------------------------- 5
    section(5, "files dropped on a window: WM_DROPFILES with a real HDROP (the program posts it to itself)")
    wd = exe("windrop_main.exe")
    if wd:
        r = run(wd, timeout=60)
        o = text(r.stdout)
        check("four paths in two drops, blanks and UTF-8 intact",
              o.count("PATH ") == 4 and "PATH C:\\Users\\me\\a b.txt" in o and "PATH C:\\only.txt" in o and "gr\u00fc\u00dfe" in o and "\u65e5\u672c\u8a9e" in o, o)
    else:
        skip("drop", "windrop_main.exe missing")
    dm = exe("drop_main.exe")
    if dm and INTERACTIVE:
        out("  A window titled firn-drop-test opens. Drag one or more files from Explorer onto it (you have 60 s).")
        p = subprocess.Popen([dm], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        t0 = time.time(); got = []
        while time.time() - t0 < 60:
            l = p.stdout.readline().decode("utf-8", "replace")
            if not l:
                break
            got.append(l.strip())
            if l.startswith("PATH"):
                out("      got " + l.strip())
                break
        p.terminate()
        check("(you) a real Explorer drag delivered a path", any(g.startswith("PATH ") for g in got), got)
    # ---------------------------------------------------------------- 6
    section(6, "tray icon, balloon and the shell's messages")
    tray, poke, notify = exe("tray_main.exe"), exe("wintray_poke.exe"), exe("notify_main.exe")
    if tray and poke:
        o = os.path.join(td, "tray.out")
        p = subprocess.Popen([tray, "balloon"], stdout=open(o, "wb"), stderr=subprocess.DEVNULL)
        end = time.time() + 30
        while time.time() < end and b"BALLOON" not in open(o, "rb").read():
            time.sleep(0.2)
        s = text(open(o, "rb").read())
        check("the shell accepts the icon (SHOWN yes) and the balloon", "SHOWN yes" in s and "BALLOON yes" in s, s)
        a = ask("Do you see a small icon (red | green | blue | white columns) in the notification area (maybe behind the ^ arrow), and a balloon / toast 'Firn: balloon text'?")
        if a is not None:
            check("(you) icon and balloon visible", a)
        for args in (("click",), ("middle",), ("right",), ("cmd", "3"), ("cancel",), ("click",), ("cmd", "1"), ("cmd", "4")):
            run(poke, *args); time.sleep(1.0)
        try:
            p.wait(timeout=30)
        except subprocess.TimeoutExpired:
            p.kill()
        ev = [tuple(int(x) for x in l.split()[1:]) for l in text(open(o, "rb").read()).splitlines() if l.startswith("EV ")]
        check("click -> 1, middle -> 2, right -> 4 (then the menu), menu items 4, 1, 2 arrive as EV_MENU",
              any(e[0] == 1 for e in ev) and any(e[0] == 2 for e in ev) and any(e[0] == 4 for e in ev)
              and (3, 4, 0, 0) in ev and (3, 1, 0, 0) in ev and (3, 2, 0, 0) in ev, ev)
        if INTERACTIVE:
            out("  Now the REAL thing: an icon appears for 20 s. Left-click it, then right-click it and choose 'Quit'.")
            o2 = os.path.join(td, "tray2.out")
            p2 = subprocess.Popen([tray], stdout=open(o2, "wb"), stderr=subprocess.DEVNULL)
            try:
                p2.wait(timeout=40)
            except subprocess.TimeoutExpired:
                p2.kill()
            ev2 = [l for l in text(open(o2, "rb").read()).splitlines() if l.startswith("EV ")]
            check("(you) a real click and a real menu choice reached the program", len(ev2) >= 1, ev2)
    else:
        skip("tray", "programs missing from the kit")
    if notify and poke:
        o = os.path.join(td, "notify.out")
        p = subprocess.Popen([notify], stdout=open(o, "wb"), stderr=subprocess.DEVNULL)
        end = time.time() + 30
        while time.time() < end and b"GO" not in open(o, "rb").read():
            time.sleep(0.2)
        run(poke, "balloonclick"); time.sleep(1.0)
        run(poke, "balloontimeout"); time.sleep(1.0)
        p.terminate()
        L = text(open(o, "rb").read()).splitlines()
        check("notifier: ready, ids, close, a balloon click is the action `default`, a timeout closes it",
              "READY yes" in L and "ID 1" in L and "ID2 1" in L and "CLOSE ok" in L and any(l.startswith("ACTION") and l.endswith("default") for l in L) and "CLOSED 0 1" in L, L)
finally:
    subprocess.run(["reg", "delete", "HKCU\\Software\\FirnDesktopKit", "/f"], capture_output=True)
    if KEEP:
        out("kept: " + td)
    else:
        shutil.rmtree(td, ignore_errors=True)

out("")
out("desktop kit: %d failed, %d skipped" % (len(FAILED), len(SKIPPED)) if FAILED or SKIPPED else "desktop kit: all checks passed")
try:
    open(os.path.join(HERE, "kit-report.txt"), "w", encoding="utf-8").write("\n".join(LOG) + "\n")
except OSError:
    pass
sys.exit(1 if FAILED else 0)
