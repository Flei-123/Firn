#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/desktop/platforms.py -- do all platform files of the desktop libraries say the same thing?

For each module the Linux file is the reference (lib/desktop/<m>.fi, lib/audio/dev.fi); the other
files must export the same names with the same signatures:

    Linux     lib/desktop/<m>.fi           lib/audio/dev.fi
    Windows   lib/desktop/<m>.windows.fi   lib/audio/dev.windows.fi
    Android   lib/@android/desktop/<m>.fi  lib/@android/audio/dev.fi      (stubs)
    macOS     lib/desktop/<m>_macos.fi     lib/audio/dev_macos.fi          (UNTESTED stubs)
    OrientOS  lib/desktop/<m>_osum.fi      lib/audio/dev_osum.fi           (stubs)

and every file has to TYPE-CHECK: a driver that calls every exported function once is compiled with
`firnc --emit=asm` (no linking, so the Android, macOS and OrientOS files, which have nothing to link
against here, are still read by the compiler). The Linux and Windows files are also built for real by
tools/desktop/run.sh.

    python3 tools/desktop/platforms.py [--firnc PATH]       exit 0 = all agree
"""
import os, re, subprocess, sys, tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# module -> (directory, [(platform, path, extra firnc flags, import name)], constructors for struct parameters)
def files(mod, base):
    d = "lib/" + base
    return [
        ("linux", "%s/%s.fi" % (d, mod), [], "%s.%s" % (base, mod)),
        ("windows", "%s/%s.windows.fi" % (d, mod), ["--target=x86_64-windows"], "%s.%s" % (base, mod)),
        ("android", "lib/@android/%s/%s.fi" % (base, mod), ["--target=x86_64-android", "--pic", "-c"], "%s.%s" % (base, mod)),
        ("macos", "%s/%s_macos.fi" % (d, mod), [], "%s.%s_macos" % (base, mod)),
        ("osum", "%s/%s_osum.fi" % (d, mod), [], "%s.%s_osum" % (base, mod)),
    ]

MODULES = {
    "tray": ("desktop", {"*mut Tray": ("var t: {m}.Tray = {m}.tray_new(\"\", \"\")", "&t"),
                         "*mut TrayEvent": ("var te: {m}.TrayEvent = {m}.tray_event_zero()", "&te")}),
    "notify": ("desktop", {"*mut Notifier": ("var n: {m}.Notifier = {m}.notify_open(\"\")", "&n"),
                           "*mut Note": ("var no: {m}.Note = {m}.note_new(\"\", \"\")", "&no"),
                           "*mut NotifyEvent": ("var ne: {m}.NotifyEvent = {m}.notify_event_zero()", "&ne")}),
    "watch": ("desktop", {"*mut Watcher": ("var w: {m}.Watcher = {m}.watch_new()", "&w"),
                          "*mut WatchEvent": ("var we: {m}.WatchEvent = {m}.watch_event_zero()", "&we")}),
    "autostart": ("desktop", {}),
    "dev": ("audio", {"*mut Dev": ("var d: {m}.Dev = {m}.dev_null()", "&d")}),
}

def read(path):
    return open(os.path.join(ROOT, path), encoding="utf-8").read()

def exports(src):
    m = re.search(r"^export\s*\{(.*?)\}", src, re.S | re.M)
    if not m:
        return set()
    body = re.sub(r"//[^\n]*", "", m.group(1))
    return set(x.strip() for x in body.replace("\n", " ").split(",") if x.strip())

def signatures(src):
    sigs = {}
    for m in re.finditer(r"^fn\s+(\w+)\s*\((.*?)\)\s*(->\s*([^{]+?))?\s*\{", src, re.S | re.M):
        name = m.group(1)
        params = re.sub(r"\s+", " ", m.group(2)).strip()
        types = ",".join(p.split(":", 1)[1].strip() for p in params.split(",") if ":" in p)
        sigs[name] = (types, (m.group(4) or "").strip())
    return sigs

DEFAULTS = {"str": '""', "u32": "0 as u32", "u64": "0 as u64", "i64": "0", "i32": "0 as i32", "usize": "0",
            "bool": "false", "*mut rt.Buf": "&b", "u8": "0 as u8"}

def driver(imp, mod, names, sigs, ctors):
    m = imp.split(".")[-1]
    lines = ["import std.rt", "import %s" % imp, "", "fn main(start: u64) -> i32 {", "    var r: i64 = 0",
             "    var b: rt.Buf = rt.buf_new()"]
    used = {}
    for tname, (decl, ref) in ctors.items():
        lines.append("    " + decl.format(m=m))
    for n in sorted(names):
        if n not in sigs:
            continue
        types, ret = sigs[n]
        args = []
        for t in [x.strip() for x in types.split(",") if x.strip()]:
            if t in ctors:
                args.append(ctors[t][1])
            else:
                args.append(DEFAULTS.get(t, "0"))
        call = "%s.%s(%s)" % (m, n, ", ".join(args))
        if ret == "bool":
            lines.append("    if %s { r = r + 1 }" % call)
        elif ret in ("", "()"):
            lines.append("    %s" % call)
        elif ret == "str":
            lines.append("    r = r + %s.n as i64" % call)
        elif ret.startswith("tray.") or ret in ctors or re.match(r"^[A-Z]", ret):
            lines.append("    let _x%s = %s" % (n, call))
        else:
            lines.append("    r = r + (%s) as i64" % call)
    lines.append("    return r as i32")
    lines.append("}")
    return "\n".join(lines) + "\n"

def main():
    firnc = None
    if "--firnc" in sys.argv:
        firnc = sys.argv[sys.argv.index("--firnc") + 1]
    if not firnc:
        for c in (os.path.join(ROOT, "compiler/target/release/firnc"), "/root/firn/compiler/target/release/firnc"):
            if os.path.exists(c):
                firnc = c
                break
    bad = 0
    work = tempfile.mkdtemp(prefix="desktop-platforms-")
    env = dict(os.environ, FIRNLIB=os.path.join(ROOT, "lib"))
    for mod, (base, ctors) in MODULES.items():
        fl = files(mod, base)
        ref_src = read(fl[0][1])
        ref_names, ref_sigs = exports(ref_src), signatures(ref_src)
        for name, path, flags, imp in fl:
            full = os.path.join(ROOT, path)
            if not os.path.exists(full):
                print("  MISSING  %-8s %s" % (name, path)); bad += 1; continue
            src = read(path)
            names, sigs = exports(src), signatures(src)
            ok = True
            for n in sorted(ref_names - names):
                print("  FAIL  %s/%-8s does not export %s" % (mod, name, n)); ok = False
            for n in sorted(names - ref_names):
                print("  FAIL  %s/%-8s exports %s, which the reference does not" % (mod, name, n)); ok = False
            for n in sorted(ref_names & names):
                if n in ref_sigs and sigs.get(n) != ref_sigs[n]:
                    print("  FAIL  %s/%-8s %s: %s vs reference %s" % (mod, name, n, sigs.get(n), ref_sigs[n])); ok = False
            if not ok:
                bad += 1; continue
            if firnc is None:
                print("  ok    %-9s %-8s exports and signatures equal the reference (%d names); no firnc: type check skipped" % (mod, name, len(names)))
                continue
            # standalone files (macOS, OrientOS) are imported by their own module name; the others by the
            # common one (Windows by the twin rule, Android by the platform directory)
            main_fi = os.path.join(work, "drv_%s_%s.fi" % (mod, name))
            open(main_fi, "w").write(driver(imp, mod, ref_names, ref_sigs, ctors))
            out = os.path.join(work, "drv_%s_%s.s" % (mod, name))
            cmd = [firnc] + flags + ["--emit=asm", "-o", out, main_fi] if "-c" not in flags else [firnc] + flags + ["--emit=asm", "-o", out, main_fi]
            r = subprocess.run(cmd, env=env, capture_output=True, text=True)
            if r.returncode == 0:
                print("  ok    %-9s %-8s %d names, signatures equal, type-checks" % (mod, name, len(names)))
            else:
                bad += 1
                print("  FAIL  %-9s %-8s does not type-check:" % (mod, name))
                print("\n".join("        " + l for l in (r.stdout + r.stderr).splitlines()[:12]))
    subprocess.run(["rm", "-rf", work])
    print("desktop platforms: %s" % ("all agree" if bad == 0 else "%d file(s) differ" % bad))
    return 1 if bad else 0

if __name__ == "__main__":
    sys.exit(main())
