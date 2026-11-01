import os
import re
import subprocess
import sys

from . import register_project, finding_at

_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))))
_LIB = os.path.join(_ROOT, "lib", "fui")
_GENERATOR = os.path.join(_ROOT, "tools", "fui", "ds_icons.py")
_NAME_REF = re.compile(r"ds_icons_names\.([A-Z][A-Z0-9_]*)")
_STRING_REF = re.compile(r"ds_icons\.(?:draw|find)\([^\"\n)]*\"([a-z][a-z0-9_]*)\"")
_INSTALL = re.compile(r"ds_icons_(?:full|set)\.install\(")
_SET_MODULE = os.path.join(os.path.dirname(_GENERATOR), "ds_icons_set.py")
_SET_FILE = "ds_icons_set.fi"
_IGNORED_NAMES = {"COUNT"}


def core_names():
    try:
        with open(_GENERATOR, encoding="utf-8") as fh:
            m = re.search(r"^CORE\s*=\s*\(([^)]*)\)", fh.read(), re.M)
    except OSError:
        return set()
    return set(re.findall(r"[a-z0-9_]+", m.group(1))) if m else set()


def installer_modules():
    mods = set()
    try:
        names = sorted(os.listdir(_LIB))
    except OSError:
        return mods
    for name in names:
        if not name.endswith(".fi") or name in ("ds_icons.fi", "ds_icons_full.fi"):
            continue
        with open(os.path.join(_LIB, name), encoding="utf-8", errors="replace") as fh:
            if _INSTALL.search(fh.read()):
                mods.add(name[:-3])
    return mods


def app_dirs(files):
    dirs = []
    for path, src in files:
        if re.search(r"^fn\s+main\s*\(", src, re.M):
            d = os.path.dirname(path)
            if d not in dirs:
                dirs.append(d)
    return dirs


def read_tree(directory):
    out = []
    for root, dirs, names in os.walk(directory):
        dirs[:] = sorted(d for d in dirs if not d.startswith("."))
        for name in sorted(names):
            if name.endswith(".fi"):
                path = os.path.join(root, name)
                with open(path, encoding="utf-8", errors="replace") as fh:
                    out.append((path, fh.read()))
    return out


def installs(tree, modules):
    for _path, src in tree:
        if _INSTALL.search(src):
            return True
        for m in modules:
            if re.search(r"^import\s+fui\." + re.escape(m) + r"\s*$", src, re.M):
                return True
    return False


def full_refs(tree, core):
    upper = {c.upper() for c in core}
    refs = []
    for path, src in tree:
        for no, text in enumerate(src.split("\n"), 1):
            code = text.split("//", 1)[0]
            for m in _NAME_REF.finditer(code):
                if m.group(1) not in upper and m.group(1) not in _IGNORED_NAMES:
                    refs.append((path, no, m.group(1).lower()))
            for m in _STRING_REF.finditer(code):
                if m.group(1) not in core:
                    refs.append((path, no, m.group(1)))
    return refs


@register_project("ICON_NOT_INSTALLED", "app uses icons outside the core set but never installs the full icon data")
def check(files):
    core = core_names()
    modules = installer_modules()
    out = []
    for directory in app_dirs(files):
        tree = read_tree(directory)
        refs = full_refs(tree, core)
        if not refs or installs(tree, modules):
            continue
        path, no, name = refs[0]
        rel = os.path.relpath(path)
        out.append(finding_at(rel if not rel.startswith("..") else path, "ICON_NOT_INSTALLED", no, name,
                              "icon outside the core set is drawn but ds_icons_full.install() is never called"))
    return out


def set_is_stale(directory):
    proc = subprocess.run([sys.executable, "-I", _SET_MODULE, "--check", directory], capture_output=True)
    return proc.returncode != 0


@register_project("ICON_SET_STALE", "generated ds_icons_set.fi does not match the icons the app uses (run tools/fui/ds_icons_set.py <app dir>)")
def check_set(files):
    out = []
    for directory in app_dirs(files):
        target = os.path.join(directory, _SET_FILE)
        if os.path.exists(target) and set_is_stale(directory):
            rel = os.path.relpath(target)
            out.append(finding_at(rel if not rel.startswith("..") else target, "ICON_SET_STALE", 1, _SET_FILE,
                                  "icon set is out of date: run python3 tools/fui/ds_icons_set.py " + os.path.relpath(directory)))
    return out
