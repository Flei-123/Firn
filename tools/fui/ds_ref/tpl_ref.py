#!/usr/bin/env python3
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)
FONTS = os.path.abspath(os.path.join(HERE, "..", "..", "..", "lib", "fui", "fonts"))
LIB = os.path.abspath(os.path.join(HERE, "..", "..", "..", "lib", "fui"))
NAME_MAP = {"monitor": "display", "wifi": "wifi_1", "lock": "lock", "moon": "moon", "globe": "globe", "sun": "brightness",
            "clock": "clock", "hard-drive": "hard_drive", "chevron-down": "chevron_down", "chevron-right": "chevron_right",
            "menu": "list", "search": "search", "arrow-left": "back", "sparkles": "sparkle", "palette": "personalize"}
LIGHT = dict(base="#F3F3F3", surface="#F9F9F9", raised="#FFFFFF", button="#EEEEEE", border="#D0D0D0", sep="#E0E0E0",
             text="#1B1B1B", muted="#5C5C5C", disabled="#6B6B6B", on_accent="#FFFFFF", accent="#0067C0", select="#CCE4F7")
DARK = dict(base="#202020", surface="#282828", raised="#303030", button="#3A3A3A", border="#4A4A4A", sep="#3A3A3A",
            text="#FFFFFF", muted="#C4C4C4", disabled="#B0B0B0", on_accent="#000000", accent="#4CC2FF", select="#264F78")

PATHS = {}
NAMES = None


def load_names():
    import re
    src = open(os.path.join(LIB, "ds_icons_names.fi")).read()
    body = src[src.index("fn name_of"):]
    return {n: int(i) for i, n in re.findall(r'if id == (\d+) \{ return "([a-z0-9_]+)" \}', body)}


def load_grade(g):
    import re
    src = open(os.path.join(LIB, "ds_icons_data%d.fi" % g)).read()
    reg = src[src.index("fn regular"):src.index("fn filled")]
    return {int(i): d for i, d in re.findall(r'if id == (\d+) \{\s*return "([^"]*)"', reg)}


def icon(name, x, y, size, color):
    global NAMES
    if NAMES is None:
        NAMES = load_names()
        for g in (16, 20, 24):
            PATHS[g] = load_grade(g)
    iid = NAMES[NAME_MAP.get(name, name)]
    grade = 24 if size > 22 else (20 if size > 18 else 16)
    order = {16: (16, 20, 24), 20: (20, 24, 16), 24: (24, 20, 16)}[grade]
    d = ""
    for g in order:
        d = PATHS[g].get(iid, "")
        if d:
            break
    return ('<svg style="position:absolute;left:%spx;top:%spx" width="%s" height="%s" viewBox="0 0 24 24" fill="%s">'
            '<path d="%s"/></svg>') % (x, y, size, size, color, d)


def box(x, y, w, h, bg=None, r=0, border=None, extra=""):
    s = "position:absolute;left:%spx;top:%spx;width:%spx;height:%spx;" % (x, y, w, h)
    if bg:
        s += "background:%s;" % bg
    if r:
        s += "border-radius:%spx;" % r
    if border:
        s += "box-shadow:inset 0 0 0 1px %s;" % border
    return '<div style="%s%s"></div>' % (s, extra)


def text(x, y, h, size, weight, color, s, w=None, align="left"):
    st = "position:absolute;left:%spx;top:%spx;height:%spx;line-height:%spx;font-size:%spx;font-weight:%s;color:%s;white-space:nowrap;text-align:%s;" % (
        x, y, h, h, size, weight, color, align)
    if w is not None:
        st += "width:%spx;" % w
    return '<div style="%s">%s</div>' % (st, s)


def page(body, w, h, c):
    css = ""
    for name, f in (("400", "Inter-Regular"), ("500", "Inter-Medium"), ("600", "Inter-SemiBold"), ("700", "Inter-Bold")):
        css += "@font-face{font-family:'InterRef';font-weight:%s;src:url('file://%s/%s.ttf');}" % (name, FONTS, f)
    return ("<!doctype html><html><head><meta charset='utf-8'><style>%s html,body{margin:0;padding:0;width:%spx;height:%spx;overflow:hidden;"
            "background:%s;font-family:'InterRef';}</style></head><body style='position:relative'>%s</body></html>") % (css, w, h, c["base"], body)


def row(c, x, y, w, rh, ico, title, sub):
    o = box(x, y, w, rh, c["surface"], 8, c["sep"])
    tx = x + 16
    if ico:
        o += icon(ico, tx, y + (rh - 20) / 2, 20, c["text"])
        tx += 32
    if sub:
        o += text(tx, y + rh / 2 - 20, 20, 14, 400, c["text"], title)
        o += text(tx, y + rh / 2, 18, 12, 400, c["muted"], sub)
    else:
        o += text(tx, y, rh, 14, 400, c["text"], title)
    return o


def toggle(c, x, y, on):
    if on:
        return box(x, y, 42, 24, c["accent"], 12) + box(x + 22, y + 4, 16, 16, c["on_accent"], 8)
    return box(x, y, 42, 24, c["button"], 12, c["sep"]) + box(x + 4, y + 4, 16, 16, c["muted"], 8)


def combo(c, x, y, label):
    return box(x, y, 200, 32, c["button"], 4, c["sep"]) + text(x + 12, y, 32, 14, 400, c["text"], label) + icon("chevron-down", x + 172, y + 8, 16, c["muted"])


def slider(c, x, y, frac, val):
    tx = x + 80
    o = text(x, y, 24, 14, 400, c["muted"], val)
    o += box(tx, y + 9, 160, 6, c["button"], 3)
    kx = tx + (160 - 18) * frac
    o += box(tx, y + 9, kx - tx + 9, 6, c["accent"], 3)
    o += box(kx - 4, y + 3, 26, 26, c["accent"], 13, extra="opacity:.3;")
    o += box(kx, y + 3, 18, 18, c["accent"], 9)
    return o


def settings_wide(c, w, h, items, title, pages, cur, rows_demo):
    o = box(0, 0, 240, h, c["surface"]) + box(239, 0, 1, h, c["sep"])
    o += icon("menu", 22, 20, 16, c["muted"]) + text(52, 20, 18, 14, 400, c["muted"], "Menu")
    y = 60
    for i, (ico, name) in enumerate(pages):
        if i == cur:
            o += box(8, y, 223, 40, c["select"], 5) + box(0, y + 8, 4, 24, c["accent"], 2)
            o += icon(ico, 20, y + 10, 20, c["text"]) + text(52, y, 40, 14, 600, c["text"], name)
        else:
            o += icon(ico, 20, y + 10, 20, c["muted"]) + text(52, y, 40, 14, 400, c["muted"], name)
        y += 44
    iw = min(w - 240 - 64, 960)
    ix = 240 + 32
    o += text(ix, 24, 36, 28, 600, c["text"], title)
    o += box(ix + iw - 280, 28, 280, 36, c["button"], 4, c["border"]) + icon("search", ix + iw - 280 + 12, 38, 16, c["muted"])
    o += text(ix + iw - 280 + 36, 28, 36, 14, 400, c["disabled"], "Find a setting")
    return o, ix, iw


def settings_rows(c, ix, iw, y, demo):
    o = ""
    for kind, a in demo:
        if kind == "section":
            y += 16
            o += text(ix, y, 20, 14, 600, c["text"], a)
            y += 28
            continue
        ico, title, sub, ctl = a
        rh = 68 if sub else 52
        o += row(c, ix, y, iw, rh, ico, title, sub)
        cx = ix + iw - 16
        if ctl[0] == "toggle":
            o += toggle(c, cx - 42, y + (rh - 24) / 2, ctl[1])
        if ctl[0] == "combo":
            o += combo(c, cx - 200, y + (rh - 32) / 2, ctl[1])
        if ctl[0] == "slider":
            o += slider(c, cx - 240, y + (rh - 24) / 2, ctl[1], ctl[2])
        if ctl[0] == "chevron":
            o += icon("chevron-right", cx - 16, y + (rh - 16) / 2, 16, c["muted"])
        y += rh + 3
    return o


DEMO = [
    ("section", "Display"),
    ("row", ("moon", "Dark mode", "Switch the whole app instantly", ("toggle", False))),
    ("row", ("globe", "Language", "", ("combo", "English"))),
    ("row", ("sun", "Brightness", "", ("slider", 0.7, "70 %"))),
    ("section", "Power"),
    ("row", ("clock", "Sleep when idle", "Save energy after 10 minutes", ("toggle", False))),
    ("row", ("hard-drive", "Storage", "Manage disks and cleanup", ("chevron",))),
]
PAGES = [("monitor", "System"), ("wifi", "Network"), ("lock", "Privacy"), ("palette", "Personalization")]


def settings_desktop(c, w, h):
    o, ix, iw = settings_wide(c, w, h, None, "System", PAGES, 0, DEMO)
    o += settings_rows(c, ix, iw, 84, DEMO)
    return page(o, w, h, c)


def phone_list(c, w, h):
    o = text(16, 16, 36, 28, 600, c["text"], "Settings")
    o += box(16, 64, w - 32, 36, c["button"], 4, c["border"]) + icon("search", 28, 74, 16, c["muted"])
    o += text(52, 64, 36, 14, 400, c["disabled"], "Find a setting")
    y = 112
    for ico, name in PAGES:
        o += row(c, 16, y, w - 32, 52, ico, name, "")
        o += icon("chevron-right", w - 16 - 16 - 16, y + 18, 16, c["muted"])
        y += 55
    return page(o, w, h, c)


def phone_page(c, w, h):
    o = icon("arrow-left", 14, 18, 20, c["text"]) + text(56, 12, 36, 28, 600, c["text"], "System")
    y = 64
    for kind, a in DEMO:
        if kind == "section":
            y += 16
            o += text(16, y, 20, 14, 600, c["text"], a)
            y += 28
            continue
        ico, title, sub, ctl = a
        rh = 68 if sub else 52
        iw = w - 32
        o += row(c, 16, y, iw, rh, ico, title, sub)
        cx = 16 + iw - 16
        if ctl[0] == "toggle":
            o += toggle(c, cx - 42, y + (rh - 24) / 2, ctl[1])
        if ctl[0] == "combo":
            o += combo(c, cx - 200, y + (rh - 32) / 2, ctl[1])
        if ctl[0] == "slider":
            o += slider_narrow(c, cx - 176, y + (rh - 24) / 2, ctl[1], ctl[2])
        if ctl[0] == "chevron":
            o += icon("chevron-right", cx - 16, y + (rh - 16) / 2, 16, c["muted"])
        y += rh + 3
    return page(o, w, h, c)


def slider_narrow(c, x, y, frac, val):
    tx = x + 80
    o = text(x, y, 24, 14, 400, c["muted"], val)
    o += box(tx, y + 9, 96, 6, c["button"], 3)
    kx = tx + (96 - 18) * frac
    o += box(tx, y + 9, kx - tx + 9, 6, c["accent"], 3)
    o += box(kx - 4, y + 3, 26, 26, c["accent"], 13, extra="opacity:.3;")
    o += box(kx, y + 3, 18, 18, c["accent"], 9)
    return o


def button(c, x, y, w, label, primary):
    if primary:
        return box(x, y, w, 32, c["accent"], 4) + text(x, y, 32, 14, 400, c["on_accent"], label, w, "center")
    return box(x, y, w, 32, c["button"], 4, c["border"]) + text(x, y, 32, 14, 400, c["text"], label, w, "center")


def dots(c, cx, y, n, cur):
    total = n * 8 + (n - 1) * 8 + 16
    x = cx - total / 2
    o = ""
    for i in range(n):
        if i == cur:
            o += box(x, y, 24, 8, c["accent"], 4)
            x += 24 + 8
        else:
            o += box(x, y, 8, 8, c["disabled"], 4)
            x += 16
    return o


def wizard_welcome(c, w, h):
    y = 148
    o = box((w - 96) / 2, y, 96, 96, c["accent"], 24) + icon("sparkles", (w - 96) / 2 + 24, y + 24, 48, c["on_accent"])
    y += 96 + 32
    o += text(0, y, 52, 40, 600, c["text"], "Welcome", w, "center")
    y += 52 + 8
    o += text(0, y, 24, 18, 400, c["muted"], "Let's set up your device.", w, "center")
    o += text(0, y + 24, 24, 18, 400, c["muted"], "It only takes a minute.", w, "center")
    y += 48 + 40
    o += button(c, (w - 138) / 2, y, 138, "Get started", True)
    o += dots(c, w / 2, h - 40, 4, 0)
    return page(o, w, h, c)


def wizard_region(c, w, h):
    colw = min(w - 48, 560)
    x = (w - colw) / 2
    y = 72 if w >= 641 else 40
    o = text(x, y, 36, 28, 600, c["text"], "Choose your region")
    y += 44
    o += text(x, y, 20, 14, 400, c["muted"], "Pick language and time zone.")
    y += 20 + 24
    items = [("globe", "Language", "", ("combo", "English")), ("clock", "Time zone", "", ("combo", "UTC")),
             ("wifi", "Connect to Wi-Fi", "Use the network Home", ("toggle", True))]
    for ico, title, sub, ctl in items:
        rh = 68 if sub else 52
        o += row(c, x, y, colw, rh, ico, title, sub)
        cx = x + colw - 16
        if ctl[0] == "toggle":
            o += toggle(c, cx - 42, y + (rh - 24) / 2, ctl[1])
        if ctl[0] == "combo":
            o += combo(c, cx - 200, y + (rh - 32) / 2, ctl[1])
        y += rh + 3
    by = h - 32 - 32
    o += button(c, x, by, 96, "Back", False) + button(c, x + colw - 96, by, 96, "Next", True)
    o += dots(c, w / 2, h - 52, 4, 1)
    return page(o, w, h, c)


TARGETS = {
    "settings": settings_desktop,
    "settings_list": phone_list,
    "settings_page": phone_page,
    "wizard_welcome": wizard_welcome,
    "wizard_region": wizard_region,
    "wizard_region_m": wizard_region,
}


def main():
    import tpl_ref_files
    import tpl_ref_store
    TARGETS.update(tpl_ref_store.TARGETS)
    TARGETS.update(tpl_ref_files.TARGETS)
    name, theme, w, h, out = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4]), sys.argv[5]
    c = DARK if theme == "dark" else LIGHT
    open(out, "w").write(TARGETS[name](c, w, h))


if __name__ == "__main__":
    main()
