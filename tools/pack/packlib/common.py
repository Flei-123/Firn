# SPDX-License-Identifier: MPL-2.0
"""Shared pieces of the package writers: the app description, hashing,
deterministic times, small file helpers."""

import hashlib
import json
import os
import re
import shutil
import stat
import sys
import time

ID_RE = re.compile(r"^[a-z0-9][a-z0-9._-]{0,39}$")
VERSION_RE = re.compile(r"^[0-9]+(\.[0-9]+){0,3}(-[0-9A-Za-z.-]+)?$")


class PackError(Exception):
    pass


def epoch():
    """The time stamped into every archive: $SOURCE_DATE_EPOCH, else a fixed
    day -- the same inputs give the same bytes (a golden test compares)."""
    v = os.environ.get("SOURCE_DATE_EPOCH")
    if v and v.isdigit():
        return int(v)
    return 1767225600  # 2026-01-01T00:00:00Z


def zip_time():
    t = time.gmtime(max(epoch(), 315532800))
    return (t.tm_year, t.tm_mon, t.tm_mday, t.tm_hour, t.tm_min, t.tm_sec & ~1)


class App(object):
    """What a package needs to know about the program (pack.ini)."""

    FIELDS = ("id", "name", "version", "vendor", "summary", "description", "url",
              "license", "category", "exe", "icon", "android_id", "maintainer",
              "desktop", "launch", "arch", "depends")

    def __init__(self, **kw):
        self.id = ""
        self.name = ""
        self.version = "0.1.0"
        self.vendor = "FleiTec"
        self.summary = ""
        self.description = ""
        self.url = ""
        self.license = "MPL-2.0"
        self.category = "Utility"
        self.exe = ""
        self.icon = ""
        self.android_id = ""
        self.maintainer = ""
        self.desktop = "1"
        self.launch = "1"
        self.arch = ""
        self.depends = ""
        for k, v in kw.items():
            if k in self.FIELDS and v is not None:
                setattr(self, k, v)
        if not self.exe:
            self.exe = self.id
        if not self.summary:
            self.summary = self.name
        if not self.maintainer:
            self.maintainer = "%s <noreply@example.invalid>" % self.vendor
        self.check()

    def check(self):
        if not ID_RE.match(self.id):
            raise PackError("id must be lower case letters, digits, '.', '_' or '-' "
                            "(at most 40): %r" % self.id)
        if not self.name:
            raise PackError("the app needs a name")
        if not VERSION_RE.match(self.version):
            raise PackError("version must look like 1.2.3 or 1.2.3-beta.1: %r" % self.version)
        if any(c in self.name for c in '\n\r"\\<>|:*?/'):
            raise PackError("the name %r has a character that cannot be in a file name" % self.name)

    @classmethod
    def from_ini(cls, path, **override):
        kv = {}
        with open(path, encoding="utf-8") as f:
            for line in f:
                line = line.rstrip("\r\n")
                if not line.strip() or line.lstrip().startswith("#") or "=" not in line:
                    continue
                k, v = line.split("=", 1)
                kv[k.strip()] = v.strip()
        kv.update({k: v for k, v in override.items() if v is not None})
        return cls(**kv)


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def read(path):
    with open(path, "rb") as f:
        return f.read()


def write(path, data, mode=None):
    d = os.path.dirname(os.path.abspath(path))
    os.makedirs(d, exist_ok=True)
    with open(path, "wb") as f:
        f.write(data)
    if mode is not None:
        os.chmod(path, mode)


def is_elf(data):
    return data[:4] == b"\x7fELF"


def is_pe(data):
    if data[:2] != b"MZ" or len(data) < 64:
        return False
    off = int.from_bytes(data[60:64], "little")
    return data[off:off + 4] == b"PE\0\0"


def elf_machine(data):
    if not is_elf(data) or len(data) < 20:
        return None
    return int.from_bytes(data[18:20], "little")


ELF_ARCH = {62: "x86_64", 183: "aarch64"}


def walk_files(root):
    """(relative path with '/', absolute path, is_dir) in a stable order."""
    out = []
    for base, dirs, files in os.walk(root):
        dirs.sort()
        rel = os.path.relpath(base, root)
        if rel != ".":
            out.append((rel.replace(os.sep, "/"), base, True))
        for fn in sorted(files):
            p = os.path.join(base, fn)
            out.append((os.path.relpath(p, root).replace(os.sep, "/"), p, False))
    return out


def log(msg):
    sys.stderr.write("pack: %s\n" % msg)
