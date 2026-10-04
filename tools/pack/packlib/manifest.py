# SPDX-License-Identifier: MPL-2.0
"""The manifest of a release: one fragment per artifact -- what the store
catalog needs (art, platforms, version, size, SHA-256, file) and a detached
Ed25519 signature -- plus the `store add-app` commands that publish them.

THE SIGNATURE is over a text that binds the file to what it claims to be:

    pack1\\n<id>\\n<version>\\n<art>\\n<platform>\\n<sha256 hex>\\n<size>\\n

so a signature cannot be moved to another version or platform. The key is a
file named in the environment (`$PACK_SIGN_KEY`, else the store's own
`$ORIENTSTORE_SCHLUESSEL`): 32 raw octets or 64 hex digits. It is read here
and never copied anywhere; the public key goes into the manifest so anybody
can check (`verify`). The store's catalog signature (made by `store add-app`)
is a second, separate layer: this one covers files that are not in a catalog
(installers, packages) as well.
"""

import json
import os

from . import common
from .common import PackError

# art -> (store art word, store 'ziel' words) for what the store knows
STORE_ART = {"exe": "exe", "bin": "bin", "appimage": "appimage", "macos-app": "macos-app"}


def load_key(path=None):
    path = path or os.environ.get("PACK_SIGN_KEY") or os.environ.get("ORIENTSTORE_SCHLUESSEL")
    if not path:
        return None
    raw = common.read(path)
    if len(raw) == 32:
        return raw
    t = raw.strip()
    if len(t) == 64:
        try:
            return bytes.fromhex(t.decode("ascii"))
        except ValueError:
            pass
    raise PackError("%s is neither 32 raw octets nor 64 hex digits" % path)


def _signer(sk):
    try:
        from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
        from cryptography.hazmat.primitives import serialization
        k = Ed25519PrivateKey.from_private_bytes(sk)
        pub = k.public_key().public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw)
        return k.sign, pub
    except ImportError:
        pass
    for cand in (os.environ.get("ORIENTSTORE_TOOL", ""), "/root/orientstore/werkzeug/store"):
        d = os.path.dirname(cand) if cand else ""
        if d and os.path.isfile(os.path.join(d, "signatur.py")):
            import importlib.util
            spec = importlib.util.spec_from_file_location("signatur", os.path.join(d, "signatur.py"))
            m = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(m)
            return (lambda msg: m.signieren(sk, msg)), m.oeffentlich(sk)
    raise PackError("signing needs python3 'cryptography' or the orientstore tool (signatur.py)")


def message(app_id, version, art, platform, sha256, size):
    return ("pack1\n%s\n%s\n%s\n%s\n%s\n%d\n" % (app_id, version, art, platform, sha256, size)).encode("utf-8")


def verify_entry(entry, pub_hex, app_id, version):
    """True when the entry's signature is valid for its fields."""
    try:
        from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey
        from cryptography.exceptions import InvalidSignature
        k = Ed25519PublicKey.from_public_bytes(bytes.fromhex(pub_hex))
        try:
            k.verify(bytes.fromhex(entry["signature"]),
                     message(app_id, version, entry["art"], entry["platform"], entry["sha256"], entry["size"]))
            return True
        except InvalidSignature:
            return False
    except ImportError:
        raise PackError("verifying needs python3 'cryptography'")


def build(app, artifacts, key_path=None, base_dir=None):
    """artifacts: list of dicts {art, platform, file, store (bool), note}.
    Returns the manifest dict."""
    sk = load_key(key_path)
    sign = pub = None
    if sk is not None:
        sign, pub = _signer(sk)
    entries = []
    for a in artifacts:
        p = a["file"]
        sha = common.sha256_file(p)
        size = os.path.getsize(p)
        e = {"art": a["art"], "platform": a["platform"],
             "file": os.path.relpath(p, base_dir) if base_dir else os.path.basename(p),
             "size": size, "sha256": sha, "store": bool(a.get("store"))}
        if a.get("note"):
            e["note"] = a["note"]
        if sign:
            e["signature"] = sign(message(app.id, app.version, a["art"], a["platform"], sha, size)).hex()
        entries.append(e)
    man = {"manifest": 1, "id": app.id, "name": app.name, "version": app.version,
           "vendor": app.vendor, "artifacts": entries}
    if pub:
        man["signing_key"] = pub.hex()
        man["signature_over"] = "pack1\\n<id>\\n<version>\\n<art>\\n<platform>\\n<sha256>\\n<size>\\n (Ed25519)"
    else:
        man["unsigned"] = "no signing key: set PACK_SIGN_KEY (or ORIENTSTORE_SCHLUESSEL) to a key file"
    return man


def store_commands(man, notes="", channel="stabil", dist_dir="."):
    """The `store add-app` / `store add` lines that publish the store artifacts.
    Paths are relative to dist_dir. Nothing here talks to a store."""
    lines = ["#!/usr/bin/env bash",
             "# Publishes %s %s into an orientstore repository." % (man["id"], man["version"]),
             "# Generated by tools/pack/pack.py manifest -- review it, then run it with",
             "#   ORIENTSTORE_REPO=/path/to/repo bash store-add.sh   (the store's signing key is read",
             "# by the store tool from $ORIENTSTORE_SCHLUESSEL or the repo; this script never sees it)",
             "set -euo pipefail",
             'cd "$(dirname "$0")"',
             'STORE="python3 ${ORIENTSTORE_TOOL:-/root/orientstore/werkzeug/store}"',
             ': "${ORIENTSTORE_REPO:?set ORIENTSTORE_REPO}"', ""]
    for e in man["artifacts"]:
        if not e.get("store"):
            continue
        f = e["file"]
        if e["art"] == "apk":
            args = ['add', '--name', man["name"], '--kanal', channel]
            if notes:
                args += ['--aenderungen', notes]
            lines.append('$STORE ' + ' '.join(_q(x) for x in args) + ' ' + _q(f))
        elif e["art"] == "opk":
            args = ['add', '--name', man["name"], '--kanal', channel]
            lines.append('$STORE ' + ' '.join(_q(x) for x in args) + ' ' + _q(f))
        else:
            args = ['add-app', '--art', e["art"], '--id', man["id"], '--fassung', man["version"],
                    '--ziel', e["platform"], '--name', man["name"], '--kanal', channel]
            if notes:
                args += ['--aenderungen', notes]
            lines.append('$STORE ' + ' '.join(_q(x) for x in args) + ' ' + _q(f))
    lines.append("$STORE verify")
    return "\n".join(lines) + "\n"


def _q(s):
    import shlex
    return shlex.quote(s)
