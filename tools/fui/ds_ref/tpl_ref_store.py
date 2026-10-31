import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)

from tpl_ref import icon, box, page
from tpl_ref import text
raw_text = text
import gen_shots

NAMES = "Notes|Calculator|Browser|Mail|Calendar|Photos|Music|Videos|Terminal|Editor|Maps|Weather|Files|Camera|Clock|Contacts|Paint|Chess|Podcasts|Books|Drive|Chat|Recorder|Sketch".split("|")
SUBS = "Productivity|Utilities|Internet|Communication|Productivity|Media|Media|Media|Developer tools|Developer tools|Utilities|Utilities|System|Media|Utilities|Communication|Creative|Games|Media|Reading|Cloud|Communication|Media|Creative".split("|")
CATS = [("home", "Home"), ("apps", "Apps"), ("games", "Games"), ("sparkle", "Create")]
DESC = ["A fast and simple app for everyday use. It starts instantly, works offline and keeps your data on your device.",
        "Sync is optional and encrypted end to end. The interface follows your system theme and scales from phones",
        "to large desktop screens."]


def dtext(x, y, h, size, weight, color, s, w=None, align="left"):
    return raw_text(x, y + 1, h, size, weight, color, s, w, align)


def blend(fg, bg, a):
    f = [int(fg[i:i + 2], 16) for i in (1, 3, 5)]
    b = [int(bg[i:i + 2], 16) for i in (1, 3, 5)]
    return "#%02X%02X%02X" % tuple(round(b[k] + (f[k] - b[k]) * a) for k in range(3))


def scene(c, x, y, index):
    rects = gen_shots.load()[index % len(gen_shots.load())]
    keys = {"s": c["surface"], "r": c["raised"], "p": c["sep"], "b": c["border"], "a": c["accent"]}
    done = []
    o = ""
    for rx, ry, rw, rh, rr, k in rects:
        if k == "t":
            under = c["raised"]
            for ux, uy, uw, uh, ur, uk in done:
                if ux <= rx and uy <= ry and rx + rw <= ux + uw and ry + rh <= uy + uh and uk != "t":
                    under = keys[uk]
            col = blend(c["accent"], under, 0.188)
        else:
            col = keys[k]
        done.append((rx, ry, rw, rh, rr, k))
        o += box(x + rx, y + ry, rw, rh, col, rr)
    return o


def nav(c, h, cur, back, cats=None, foot=True):
    cats = cats or CATS
    o = box(0, 0, 240, h, c["surface"]) + box(239, 0, 1, h, c["sep"])
    y = 20
    if back:
        o += icon("back", 20, y - 2, 20, c["muted"]) + text(52, y - 2, 20, 14, 400, c["muted"], "Back")
        y += 44
    o += icon("list", 22, y, 16, c["muted"]) + text(52, y, 18, 14, 400, c["muted"], "Menu")
    y += 40 if not back else 40
    for i, (ico, name) in enumerate(cats):
        if i == cur:
            o += box(8, y, 223, 40, c["select"], 5) + box(0, y + 8, 4, 24, c["accent"], 2)
            o += icon(ico, 20, y + 10, 20, c["text"]) + text(52, y, 40, 14, 600, c["text"], name)
        else:
            o += icon(ico, 20, y + 10, 20, c["muted"]) + text(52, y, 40, 14, 400, c["muted"], name)
        y += 44
    if foot:
        o += icon("book", 20, h - 3 - 40 + 10, 20, c["muted"]) + text(52, h - 43, 40, 14, 400, c["muted"], "Library")
    return o


def search(c, x, y, w):
    return (box(x, y, w, 38, c["button"], 4, c["border"]) + icon("search", x + 12, y + 11, 16, c["muted"])
            + text(x + 36, y, 38, 14, 400, c["disabled"], "Search apps"))


def card(c, x, y, w, i, state):
    tile = blend(c["accent"], c["raised"], 0.19)
    o = box(x, y, w, 132, c["raised"], 8, c["sep"]) + box(x + 16, y + 16, 56, 56, tile, 8)
    o += icon("apps", x + 16 + 16, y + 16 + 16, 24, c["accent"])
    o += text(x + 84, y + 16, 20, 15, 700, c["text"], NAMES[i % 24])
    o += text(x + 84, y + 36, 18, 12, 400, c["muted"], SUBS[i % 24])
    o += text(x + 84, y + 54, 18, 12, 400, c["muted"], "4.8 stars")
    if state:
        o += box(x + 16, y + 84, 104, 32, c["button"], 4, c["sep"]) + text(x + 16, y + 84, 32, 14, 600, c["text"], "Open", 104, "center")
    else:
        o += box(x + 16, y + 84, 104, 32, c["accent"], 4) + text(x + 16, y + 84, 32, 14, 600, c["on_accent"], "Install", 104, "center")
    return o


def store_catalog(c, w, h):
    o = nav(c, h, 0, False)
    o += text(264, 24, 40, 28, 700, c["text"], "Home") + search(c, 756, 25, 320)
    cw = (800 - 24) / 3
    for i in range(15):
        r, k = divmod(i, 3)
        o += card(c, 264 + k * (cw + 12), 96 + r * 144, cw, i, i == 1)
    o += box(1093, 75, 2, 328, c["muted"], 1)
    return page(o, w, h, c)


def store_detail(c, w, h):
    o = nav(c, h, 0, True)
    o += icon("back", 272, 32, 16, c["text"])
    tile = blend(c["accent"], c["raised"], 0.19)
    o += box(264, 68, 96, 96, tile, 12) + icon("apps", 264 + 24, 68 + 24, 48, c["accent"])
    o += dtext(376, 74, 40, 28, 700, c["text"], "Calculator") + dtext(376, 117, 20, 14, 400, c["muted"], "Utilities")
    o += dtext(376, 139, 20, 14, 400, c["muted"], "4.8 stars")
    o += box(264, 184, 160, 40, c["accent"], 4) + dtext(264, 184, 40, 14, 400, c["on_accent"], "Open", 160, "center")
    o += dtext(264, 254, 20, 16, 700, c["text"], "Screenshots")
    for k in range(3):
        x = 264 + k * 332
        o += box(x, 284, 320, 200, c["raised"], 8, c["sep"]) + scene(c, x + 12, 296, k)
        o += dtext(x + 12, 450, 20, 12, 400, c["muted"], "Screenshot %d" % (k + 1))
    o += dtext(264, 514, 20, 16, 700, c["text"], "Description")
    for k, ln in enumerate(DESC):
        o += dtext(264, 544 + k * 22, 22, 14, 400, c["text"], ln)
    return page(o, w, h, c)


def store_phone(c, w, h):
    o = box(0, 0, w, 48, c["surface"]) + icon("list", 12, 12, 24, c["text"])
    o += text(16, 65, 40, 28, 700, c["text"], "Home") + search(c, 16, 108, w - 32)
    for i in range(5):
        o += card(c, 24, 178 + i * 144, w - 60, i, i == 1)
    o += box(383, 156, 2, 120, c["muted"], 1)
    return page(o, w, h, c)


TARGETS = {"store_phone": store_phone, "store": store_catalog, "store_detail": store_detail}
