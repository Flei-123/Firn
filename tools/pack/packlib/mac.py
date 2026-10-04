# SPDX-License-Identifier: MPL-2.0
"""macOS: an .app bundle, a zip and a .dmg, written without macOS.

WHAT IS HERE: the bundle layout (Contents/Info.plist, PkgInfo, MacOS/<exe>,
Resources/<id>.icns), the zip that keeps the executable bit, and a disk image
(`xorriso` writes an ISO9660/HFS+ image that macOS mounts when named .dmg --
no hdiutil; `pack.py mac-dmg --compressed` is NOT offered: UDZO needs Apple's
format tools).

WHAT IS NOT: Firn has no macOS target yet, so there is no Mach-O program to
put in it; the packer takes a prebuilt one (and checks the magic number).
Nothing here was run on a Mac. CODE SIGNING AND NOTARISATION need an Apple
developer identity and macOS tools: `sign_script` writes the commands, it
does not run them. An unsigned bundle starts on Apple Silicon only with the
quarantine bit removed or after "Open Anyway".
"""

import os
import plistlib
import shutil
import subprocess
import tempfile
import zipfile

from . import common
from .common import PackError

CATEGORY = {"Utility": "public.app-category.utilities", "Game": "public.app-category.games",
            "Development": "public.app-category.developer-tools", "Network": "public.app-category.social-networking",
            "Office": "public.app-category.productivity", "Graphics": "public.app-category.graphics-design",
            "AudioVideo": "public.app-category.entertainment"}

MACHO_MAGICS = (b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe", b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca")


def is_macho(data):
    return data[:4] in MACHO_MAGICS


def bundle_id(app):
    v = "".join(c for c in app.vendor.lower() if c.isalnum()) or "app"
    return "com.%s.%s" % (v, app.id.replace("_", "-"))


def info_plist(app, min_os="11.0", bundle=None):
    d = {"CFBundleName": app.name, "CFBundleDisplayName": app.name,
         "CFBundleIdentifier": bundle or bundle_id(app), "CFBundleVersion": app.version,
         "CFBundleShortVersionString": app.version.split("-")[0], "CFBundleExecutable": app.id,
         "CFBundleIconFile": app.id + ".icns", "CFBundlePackageType": "APPL", "CFBundleSignature": "????",
         "CFBundleInfoDictionaryVersion": "6.0", "LSMinimumSystemVersion": min_os,
         "NSHighResolutionCapable": True, "NSHumanReadableCopyright": "%s %s" % (app.vendor, app.license),
         "LSApplicationCategoryType": CATEGORY.get(app.category, CATEGORY["Utility"])}
    return plistlib.dumps(d, fmt=plistlib.FMT_XML, sort_keys=True)


def app_files(app, exe_bytes, icns_bytes, allow_any=False, min_os="11.0"):
    """{relative path: (bytes, mode)} of the bundle, `<Name>.app/` first."""
    if not allow_any and not is_macho(exe_bytes):
        raise PackError("the program is not a Mach-O file (Firn cannot build one yet); pass --allow-any "
                        "to pack a placeholder for testing the bundle layout")
    top = "%s.app/Contents" % app.name
    f = {top + "/Info.plist": (info_plist(app, min_os), 0o644),
         top + "/PkgInfo": (b"APPL????", 0o644),
         top + "/MacOS/" + app.id: (exe_bytes, 0o755)}
    if icns_bytes:
        f[top + "/Resources/%s.icns" % app.id] = (icns_bytes, 0o644)
    return f


def bundle_dir(app, exe_path, icns_path, outdir, allow_any=False, min_os="11.0"):
    files = app_files(app, common.read(exe_path), common.read(icns_path) if icns_path else None,
                      allow_any, min_os)
    for rel, (data, mode) in files.items():
        common.write(os.path.join(outdir, rel), data, mode)
    return os.path.join(outdir, app.name + ".app")


def bundle_zip(app, exe_path, icns_path, out, allow_any=False, min_os="11.0"):
    files = app_files(app, common.read(exe_path), common.read(icns_path) if icns_path else None,
                      allow_any, min_os)
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        seen = set()
        for rel in sorted(files):
            parts = rel.split("/")
            for i in range(1, len(parts)):
                d = "/".join(parts[:i]) + "/"
                if d not in seen:
                    seen.add(d)
                    zi = zipfile.ZipInfo(d, common.zip_time())
                    zi.external_attr = (0o40755 << 16) | 0x10
                    z.writestr(zi, b"")
            data, mode = files[rel]
            zi = zipfile.ZipInfo(rel, common.zip_time())
            zi.create_system = 3
            zi.compress_type = zipfile.ZIP_DEFLATED
            zi.external_attr = (0o100000 | mode) << 16
            z.writestr(zi, data, zipfile.ZIP_DEFLATED, 9)
    return out


def dmg(app, exe_path, icns_path, out, allow_any=False, min_os="11.0"):
    xorriso = shutil.which("xorriso")
    if not xorriso:
        raise PackError("xorriso is needed for a disk image (apt install xorriso); the zip needs nothing")
    tmp = tempfile.mkdtemp(prefix="pack-dmg-")
    try:
        bundle_dir(app, exe_path, icns_path, tmp, allow_any, min_os)
        os.symlink("/Applications", os.path.join(tmp, "Applications"))
        os.makedirs(os.path.dirname(os.path.abspath(out)), exist_ok=True)
        if os.path.exists(out):
            os.remove(out)
        r = subprocess.run([xorriso, "-as", "mkisofs", "-quiet", "-V", app.name[:30], "-R", "-J",
                            "-hfsplus", "-o", out, tmp], stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        if r.returncode != 0:
            raise PackError("xorriso failed: %s" % r.stdout.decode("utf-8", "replace")[-400:])
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    return out


def check_app(path):
    """Problems of a bundle on disk (empty list = it has the shape macOS wants)."""
    probs = []
    if not path.endswith(".app") or not os.path.isdir(path):
        return ["%s is not a .app directory" % path]
    c = os.path.join(path, "Contents")
    pl = os.path.join(c, "Info.plist")
    if not os.path.isfile(pl):
        return ["Contents/Info.plist is missing"]
    try:
        d = plistlib.loads(common.read(pl))
    except Exception as e:
        return ["Info.plist does not parse: %s" % e]
    for k in ("CFBundleIdentifier", "CFBundleExecutable", "CFBundleName", "CFBundleVersion",
              "CFBundlePackageType", "CFBundleShortVersionString"):
        if k not in d:
            probs.append("Info.plist has no %s" % k)
    exe = os.path.join(c, "MacOS", d.get("CFBundleExecutable", "?"))
    if not os.path.isfile(exe):
        probs.append("Contents/MacOS/%s is missing" % d.get("CFBundleExecutable"))
    elif not os.access(exe, os.X_OK):
        probs.append("the executable has no execute bit")
    icon = d.get("CFBundleIconFile")
    if icon:
        ip = os.path.join(c, "Resources", icon if icon.endswith(".icns") else icon + ".icns")
        if not os.path.isfile(ip):
            probs.append("the icon %s is missing" % icon)
        else:
            data = common.read(ip)
            if data[:4] != b"icns" or int.from_bytes(data[4:8], "big") != len(data):
                probs.append("the .icns file is malformed")
    if not common.read(os.path.join(c, "PkgInfo")) == b"APPL????" if os.path.isfile(os.path.join(c, "PkgInfo")) else False:
        probs.append("PkgInfo is wrong or missing")
    return probs


def sign_script(app, app_name=None):
    """The commands that sign, notarise and wrap the bundle. NEVER RUN HERE."""
    n = app_name or app.name
    return """#!/bin/sh
# Generated by tools/pack/pack.py mac-sign-script for %(name)s %(version)s.
# RUN THIS ON A MAC with Xcode's command line tools. Nothing here was run by the
# packer: it needs an Apple Developer ID and a notarisation login.
#
#   MACOS_SIGN_IDENTITY   "Developer ID Application: Your Name (TEAMID)"
#   NOTARY_PROFILE        a keychain profile made once with:
#                           xcrun notarytool store-credentials NAME --apple-id ... --team-id ...
set -eu
APP="%(name)s.app"
: "${MACOS_SIGN_IDENTITY:?set MACOS_SIGN_IDENTITY}"
: "${NOTARY_PROFILE:?set NOTARY_PROFILE}"
cat > entitlements.plist <<'EOF2'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>com.apple.security.network.client</key><true/>
</dict></plist>
EOF2
# 1. sign the program inside, then the bundle (hardened runtime + secure timestamp)
codesign --force --options runtime --timestamp --entitlements entitlements.plist \\
         --sign "$MACOS_SIGN_IDENTITY" "$APP/Contents/MacOS/%(id)s"
codesign --force --options runtime --timestamp --entitlements entitlements.plist \\
         --sign "$MACOS_SIGN_IDENTITY" "$APP"
codesign --verify --deep --strict --verbose=2 "$APP"
# 2. notarise (Apple scans the upload), wait, and staple the ticket to the bundle
ditto -c -k --keepParent "$APP" "%(id)s-notarize.zip"
xcrun notarytool submit "%(id)s-notarize.zip" --keychain-profile "$NOTARY_PROFILE" --wait
xcrun stapler staple "$APP"
spctl --assess --type execute --verbose "$APP"
# 3. the files to hand out: a zip of the stapled bundle and a disk image
ditto -c -k --keepParent "$APP" "%(id)s-%(version)s-macos.zip"
hdiutil create -volname "%(name)s" -srcfolder "$APP" -ov -format UDZO "%(id)s-%(version)s-macos.dmg"
codesign --force --sign "$MACOS_SIGN_IDENTITY" "%(id)s-%(version)s-macos.dmg"
xcrun notarytool submit "%(id)s-%(version)s-macos.dmg" --keychain-profile "$NOTARY_PROFILE" --wait
xcrun stapler staple "%(id)s-%(version)s-macos.dmg"
echo "done: %(id)s-%(version)s-macos.zip and .dmg"
""" % {"name": n, "id": app.id, "version": app.version}
