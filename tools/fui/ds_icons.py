#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import concurrent.futures
import os
import re
import sys
import urllib.parse
import urllib.request

RAW = "https://raw.githubusercontent.com/microsoft/fluentui-system-icons/main/"
SIZES = (16, 20, 24)
STYLES = ("regular", "filled")

ICONS = [
    ("store", "Store Microsoft"), ("apps", "Apps"), ("cart", "Cart"), ("folder", "Folder"),
    ("folder_open", "Folder Open"), ("folder_add", "Folder Add"), ("folder_zip", "Folder Zip"),
    ("document", "Document"), ("document_text", "Document Text"), ("document_pdf", "Document PDF"),
    ("image", "Image"), ("video", "Video"), ("music", "Music Note 1"), ("code", "Code"),
    ("settings", "Settings"), ("hand", "Hand Right"), ("rocket", "Rocket"), ("sparkle", "Sparkle"),
    ("wifi_1", "WiFi 1"), ("wifi_2", "WiFi 2"), ("wifi_3", "WiFi 3"), ("wifi_4", "WiFi 4"), ("wifi_off", "WiFi Off"),
    ("wifi_warning", "WiFi Warning"), ("wifi_lock", "WiFi Lock"),
    ("bluetooth", "Bluetooth"), ("battery_0", "Battery 0"), ("battery_3", "Battery 3"), ("battery_6", "Battery 6"),
    ("battery_10", "Battery 10"), ("battery_charge", "Battery Charge"),
    ("volume_0", "Speaker 0"), ("volume_1", "Speaker 1"), ("volume_2", "Speaker 2"), ("volume_mute", "Speaker Mute"),
    ("display", "Desktop"), ("laptop", "Laptop"), ("phone", "Phone"), ("keyboard", "Keyboard"), ("usb", "Usb Stick"),
    ("hard_drive", "Hard Drive"), ("user", "Person"), ("update", "Arrow Sync"), ("install", "Arrow Download"),
    ("upload", "Arrow Upload"), ("open", "Open"), ("search", "Search"), ("back", "Arrow Left"),
    ("forward", "Arrow Right"), ("up", "Arrow Up"), ("down", "Arrow Down"), ("close", "Dismiss"),
    ("more", "More Horizontal"), ("home", "Home"), ("trash", "Delete"), ("copy", "Copy"), ("paste", "Clipboard Paste"),
    ("cut", "Cut"), ("info", "Info"), ("warning", "Warning"), ("error", "Error Circle"), ("success", "Checkmark Circle"),
    ("globe", "Globe"), ("add", "Add"), ("remove", "Subtract"), ("edit", "Edit"), ("rename", "Rename"),
    ("share", "Share"), ("star", "Star"), ("heart", "Heart"), ("lock", "Lock Closed"), ("eye", "Eye"),
    ("calendar", "Calendar"), ("clock", "Clock"), ("mail", "Mail"), ("filter", "Filter"), ("sort", "Arrow Sort"),
    ("chevron_left", "Chevron Left"), ("chevron_right", "Chevron Right"), ("chevron_up", "Chevron Up"),
    ("chevron_down", "Chevron Down"), ("checkmark", "Checkmark"), ("list", "Text Bullet List"), ("grid", "Grid"),
    ("cloud", "Cloud"), ("power", "Power"), ("brightness", "Brightness High"), ("moon", "Weather Moon"),
    ("save", "Save"), ("link", "Link"), ("pin", "Pin"), ("tag", "Tag"), ("play", "Play"), ("pause", "Pause"),
    ("stop", "Stop"), ("mic", "Mic"), ("camera", "Camera"), ("print", "Print"), ("accessibility", "Accessibility"),
    ("language", "Local Language"), ("personalize", "Paint Brush"), ("shield", "Shield"), ("key", "Key"),
    ("notification", "Alert"), ("window", "Window"), ("book", "Book"), ("games", "Games"),
    ("maximize", "Maximize"), ("restore", "Square Multiple"), ("terminal", "Window Console"), ("chat", "Chat"),
    ("undo", "Arrow Undo"), ("redo", "Arrow Redo"), ("refresh", "Arrow Clockwise"), ("wallpaper", "Wallpaper"),
    ("color", "Color"), ("eye_off", "Eye Off"), ("font", "Text Font"), ("emoji", "Emoji"), ("play_circle", "Play Circle"),
    ("cursor", "Cursor Click"), ("apps_list", "Apps List"),
]

TOKEN = re.compile(r"([MmLlHhVvCcSsQqTtAaZz])|(-?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?)")
ARGS = {"M": 2, "L": 2, "H": 1, "V": 1, "C": 6, "S": 4, "Q": 4, "T": 2, "A": 7, "Z": 0}


def fmt(v):
    s = ("%.3f" % v).rstrip("0").rstrip(".")
    return "0" if s in ("", "-0") else s


def scale_path(d, k):
    out = []
    pos = 0
    cmd = None
    nums = []
    flat = []
    for m in TOKEN.finditer(d):
        if m.group(1):
            flat.append(("c", m.group(1)))
        else:
            flat.append(("n", m.group(2)))
    i = 0
    while i < len(flat):
        kind, val = flat[i]
        if kind == "c":
            cmd = val
            out.append(cmd)
            i += 1
            if cmd in "Zz":
                continue
            continue
        up = cmd.upper()
        n = ARGS[up]
        if up == "A":
            vals = []
            j = i
            for idx in range(7):
                if idx in (3, 4):
                    tok = flat[j][1]
                    if len(tok) > 1 and tok[0] in "01":
                        flat[j] = ("n", tok[1:])
                        vals.append(tok[0])
                        continue
                    vals.append(tok)
                    j += 1
                else:
                    vals.append(flat[j][1])
                    j += 1
            i = j
            rx, ry, rot, large, sweep, x, y = vals
            out.append("%s %s %s %s %s %s %s" % (fmt(float(rx) * k), fmt(float(ry) * k), rot, large, sweep, fmt(float(x) * k), fmt(float(y) * k)))
        else:
            vals = [flat[i + t][1] for t in range(n)]
            i += n
            out.append(" ".join(fmt(float(v) * k) for v in vals))
        if cmd == "M":
            cmd = "L"
        elif cmd == "m":
            cmd = "l"
    return " ".join(out)


def fetch(url, cache):
    key = os.path.join(cache, urllib.parse.quote(url, safe="").replace("%", "_"))
    if os.path.exists(key):
        return open(key, "rb").read().decode()
    try:
        data = urllib.request.urlopen(url, timeout=60).read()
    except Exception:
        return None
    open(key, "wb").write(data)
    return data.decode()


def svg_paths(svg):
    if "evenodd" in svg:
        pass
    return re.findall(r'<path[^>]*?\sd="([^"]+)"', svg)


def asset(job, cache):
    name, folder, size, style = job
    base = "ic_fluent_" + folder.lower().replace(" ", "_") + "_%d_%s.svg" % (size, style)
    url = RAW + "assets/" + urllib.parse.quote(folder) + "/SVG/" + base
    svg = fetch(url, cache)
    if svg is None:
        return job, None
    ds = svg_paths(svg)
    if not ds:
        return job, None
    k = 24.0 / size
    return job, " ".join(scale_path(d, k) for d in ds)


def ident(name):
    return name.upper()


def emit(out_dir, icons, data):
    names = [n for n, _ in icons]
    head = "// SPDX-License-Identifier: MIT\n// Copyright (c) Microsoft Corporation. Fluent System Icons, MIT, LICENSES/Fluent-System-Icons-MIT.txt\n// GENERATED by tools/fui/ds_icons.py. Do not edit.\n"
    names_fi = "// SPDX-License-Identifier: MPL-2.0\n// GENERATED by tools/fui/ds_icons.py. Do not edit.\n\nexport {\n    COUNT, name_of,\n"
    names_fi += ",\n".join("    " + ", ".join(ident(n) for n in names[i:i + 8]) for i in range(0, len(names), 8)) + ",\n}\n\n"
    names_fi += "const COUNT: i64 = %d\n" % len(names)
    for i, n in enumerate(names):
        names_fi += "const %s: i64 = %d\n" % (ident(n), i)
    names_fi += "\nfn name_of(id: i64) -> str {\n"
    for i, n in enumerate(names):
        names_fi += '    if id == %d { return "%s" }\n' % (i, n)
    names_fi += '    return ""\n}\n'
    open(os.path.join(out_dir, "ds_icons_names.fi"), "w").write(names_fi)
    for size in SIZES:
        o = head + "\nexport { regular, filled }\n"
        for style in STYLES:
            o += "\nfn %s(id: i64) -> str {\n" % style
            for i, (n, f) in enumerate(icons):
                d = data.get((n, size, style))
                if d:
                    o += '    if id == %d {\n        return "%s"\n    }\n' % (i, d)
            o += '    return ""\n}\n'
        open(os.path.join(out_dir, "ds_icons_data%d.fi" % size), "w").write(o)


def main():
    out_dir = sys.argv[1]
    cache = sys.argv[2]
    os.makedirs(cache, exist_ok=True)
    jobs = [(n, f, s, st) for n, f in ICONS for s in SIZES for st in STYLES]
    data = {}
    missing = {}
    with concurrent.futures.ThreadPoolExecutor(16) as ex:
        for job, d in ex.map(lambda j: asset(j, cache), jobs):
            if d is None:
                missing.setdefault(job[0], []).append("%d%s" % (job[2], job[3][0]))
            else:
                data[(job[0], job[2], job[3])] = d
    dead = [n for n, _ in ICONS if not any((n, s, st) in data for s in SIZES for st in STYLES)]
    for n in dead:
        sys.stderr.write("NO ASSET: %s\n" % n)
    icons = [(n, f) for n, f in ICONS if n not in dead]
    emit(out_dir, icons, data)
    sys.stderr.write("icons %d, variants %d of %d\n" % (len(icons), len(data), len(jobs)))
    thin = [n for n, _ in icons if not all((n, s, st) in data for s in (24,) for st in STYLES)]
    if thin:
        sys.stderr.write("missing 24px variant: %s\n" % ", ".join(thin))


main()
