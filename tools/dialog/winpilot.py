# SPDX-License-Identifier: MPL-2.0
import os, shutil, subprocess, sys, tempfile, time

WINE = shutil.which("wine") or shutil.which("wine64")


def tools_missing():
    for tool, found in (("wine", WINE), ("Xvfb", shutil.which("Xvfb"))):
        if not found:
            return tool
    return None


def wine_prefix():
    home = os.environ.get("HOME") or os.path.expanduser("~")
    return os.path.join(home, ".wine-firn")


def zpath(path):
    return "Z:" + path.replace("\\", "/")


def winpath(path):
    return zpath(path).replace("/", "\\")


class Checks:
    def __init__(self):
        self.failed = []

    def check(self, name, cond, extra=""):
        print(("  OK    " if cond else "  FAIL  ") + name + ((" -- " + str(extra)[:300]) if (extra and not cond) else ""))
        if not cond:
            self.failed.append(name)

    def finish(self, label):
        print("%s: %d failed" % (label, len(self.failed)) if self.failed else "%s: all checks passed" % label)
        return 1 if self.failed else 0


class Session:
    def __init__(self, main_exe, pilot_exe):
        self.main = os.path.abspath(main_exe)
        self.pilot = os.path.abspath(pilot_exe)
        number = 120 + (os.getpid() % 60)
        display = ":%d" % number
        self.xvfb = subprocess.Popen(["Xvfb", display, "-screen", "0", "1024x768x24"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for _ in range(50):
            if os.path.exists("/tmp/.X11-unix/X%d" % number):
                break
            time.sleep(0.1)
        self.env = dict(os.environ, DISPLAY=display, WINEDEBUG="-all")
        self.env.setdefault("WINEPREFIX", wine_prefix())
        self.dir = tempfile.mkdtemp(prefix="winpilot-")

    def run(self, args, title, steps, timeout=60, cwd=None):
        out_path = os.path.join(self.dir, "out.txt")
        with open(out_path, "w") as f:
            program = subprocess.Popen([WINE, self.main] + args, env=self.env, cwd=cwd, stdout=f, stderr=subprocess.DEVNULL)
        pilot = subprocess.run([WINE, self.pilot, title, steps], env=self.env, capture_output=True, text=True, timeout=timeout)
        try:
            program.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            program.kill()
            subprocess.run(["wineserver", "-k"], env=self.env)
            return "TIMEOUT pilot=" + pilot.stdout.strip() + pilot.stderr.strip()
        with open(out_path, encoding="utf-8", errors="replace") as f:
            return f.read()

    def run_plain(self, args, timeout=60, extra_env=None):
        env = dict(self.env, **(extra_env or {}))
        done = subprocess.run([WINE, self.main] + args, env=env, capture_output=True, text=True, timeout=timeout)
        return done.stdout

    def close(self):
        subprocess.run(["wineserver", "-k"], env=self.env)
        self.xvfb.terminate()
        self.xvfb.wait()
        shutil.rmtree(self.dir, ignore_errors=True)
