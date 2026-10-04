#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/appkit/platforms.py -- do all platform files say the same thing?

lib/appkit/platform.fi is the contract; the other platform files must export
the same names with the same signatures:

    lib/appkit/platform.fi            Linux (the reference)
    lib/appkit/platform.windows.fi    Windows
    lib/@android/appkit/platform.fi   Android
    lib/appkit/platform_macos.fi      macOS   (untested)
    lib/appkit/platform_osum.fi       OrientOS (untested)

Two checks per file:
  1. the exported names, and the signature of each function, equal the
     reference's (a missing function is a compile error in some other
     module's build, found here first);
  2. the file TYPE-CHECKS: a driver that imports it and calls every exported
     function is compiled with `firnc --emit=asm` (no linking, so the macOS
     and OrientOS files, whose externs have nothing to link against here,
     are still read by the compiler). The Linux and Windows files are
     additionally built for real by the end-to-end run.

    python3 tools/appkit/platforms.py [--firnc PATH]      exit 0 = all agree
"""
import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

FILES = [
    ("linux", "lib/appkit/platform.fi", [], "platform"),
    ("windows", "lib/appkit/platform.windows.fi", ["--target=x86_64-windows"], "platform"),
    ("android", "lib/@android/appkit/platform.fi", ["--target=x86_64-android", "--pic", "-c"], "platform"),
    ("macos", "lib/appkit/platform_macos.fi", [], "platform_macos"),
    ("osum", "lib/appkit/platform_osum.fi", [], "platform_osum"),
]


# entry points a platform file may export on top of the contract: the
# system calls them, the rest of appkit does not
EXTRA = {
    "android": {"Java_org_firn_FirnInstall_onReceive"},
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
        # parameter NAMES may differ; compare the types only
        types = ",".join(p.split(":", 1)[1].strip() for p in params.split(",") if ":" in p)
        ret = (m.group(4) or "").strip()
        sigs[name] = (types, ret)
    return sigs


def driver(modname, names, sigs):
    """A main() that calls every exported function once."""
    lines = ["import std.rt", "import appkit.util", "import appkit.%s" % modname, "",
             "fn main(start: u64) -> i32 {", "    var r: i64 = 0"]
    defaults = {
        "str": '""', "u32": "0 as u32", "u64": "0 as u64", "i64": "0", "i32": "0 as i32",
        "usize": "0", "bool": "false", "*mut rt.Buf": "&b", "*mut util.Args": "&a",
        "u8": "0 as u8", "f64": "0.0",
    }
    lines.append("    var b: rt.Buf = rt.buf_new()")
    lines.append("    var a: util.Args = util.args_new()")
    for n in sorted(names):
        if n not in sigs:
            continue                      # a constant
        types, ret = sigs[n]
        args = ", ".join(defaults.get(t.strip(), "0") for t in types.split(",") if t.strip())
        call = "%s.%s(%s)" % (modname, n, args)
        if ret in ("bool",):
            lines.append("    if %s { r = r + 1 }" % call)
        elif ret in ("", "()"):
            lines.append("    %s" % call)
        elif ret == "str":
            lines.append("    r = r + %s.n as i64" % call)
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
        for c in (os.path.join(ROOT, "compiler/target/release/firnc"),
                  "/root/firn/compiler/target/release/firnc"):
            if os.path.exists(c):
                firnc = c
                break
    ref_src = read(FILES[0][1])
    ref_names = exports(ref_src)
    ref_sigs = signatures(ref_src)
    bad = 0
    work = tempfile.mkdtemp(prefix="appkit-platforms-")
    env = dict(os.environ, FIRNLIB=os.path.join(ROOT, "lib"))
    for name, path, flags, mod in FILES:
        full = os.path.join(ROOT, path)
        if not os.path.exists(full):
            print("  MISSING  %-8s %s" % (name, path))
            bad += 1
            continue
        src = read(path)
        names = exports(src)
        sigs = signatures(src)
        ok = True
        for n in sorted(ref_names - names):
            print("  FAIL  %-8s does not export %s" % (name, n))
            ok = False
        for n in sorted(names - ref_names - EXTRA.get(name, set())):
            print("  FAIL  %-8s exports %s, which the reference does not" % (name, n))
            ok = False
        for n in sorted(ref_names & names):
            if n in ref_sigs and sigs.get(n) != ref_sigs[n]:
                print("  FAIL  %-8s %s: %s vs reference %s" % (name, n, sigs.get(n), ref_sigs[n]))
                ok = False
        if ok:
            print("  ok    %-8s exports and signatures equal the reference (%d names)" % (name, len(names)))
        else:
            bad += 1
            continue
        # type check
        if firnc is None:
            print("  --    %-8s no firnc: type check skipped" % name)
            continue
        main_fi = os.path.join(work, "drv_%s.fi" % name)
        open(main_fi, "w").write(driver(mod, ref_names, ref_sigs))
        # the module under test must be the one imported: macOS and OrientOS
        # have their own module names; Android is found by the platform
        # directory of its target
        out = os.path.join(work, "drv_%s.s" % name)
        cmd = [firnc, "--emit=asm", "-o", out, main_fi] if not flags else \
            [firnc] + flags + ["--emit=asm", "-o", out, main_fi]
        r = subprocess.run(cmd, env=env, capture_output=True, text=True)
        if r.returncode == 0:
            print("  ok    %-8s type-checks (firnc --emit=asm)" % name)
        else:
            bad += 1
            print("  FAIL  %-8s does not type-check:" % name)
            print("\n".join("        " + l for l in (r.stdout + r.stderr).splitlines()[:12]))
    subprocess.run(["rm", "-rf", work])
    print("platforms: %s" % ("all agree" if bad == 0 else "%d file(s) differ" % bad))
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
