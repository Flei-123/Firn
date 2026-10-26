#!/usr/bin/env python3
import io
import os
import sys
import zipfile

from PIL import Image

sys.path.insert(0, __file__.rsplit("/", 1)[0] + "/../menu")
import layout_check as lc

WIDGETS = "assets/minecraft/textures/gui/widgets.png"
ICONS = "assets/minecraft/textures/gui/icons.png"
INVENTORY = "assets/minecraft/textures/gui/container/inventory.png"
CONTAINER_TEX = {
    "generic": "generic_54",
    "furnace": "furnace",
    "craft": "crafting_table",
    "dispenser": "dispenser",
    "hopper": "hopper",
    "blast_furnace": "blast_furnace",
    "smoker": "smoker",
    "brewing": "brewing_stand",
    "grindstone": "grindstone",
    "stonecutter": "stonecutter",
    "loom": "loom",
    "smithing": "smithing",
    "cartography": "cartography_table",
}
CONTAINER_H = {"furnace": 166, "craft": 166, "dispenser": 166, "hopper": 133, "blast_furnace": 166, "smoker": 166, "brewing": 166,
               "grindstone": 166, "stonecutter": 166, "loom": 166, "smithing": 166, "cartography": 166}


def load(jar, name):
    with zipfile.ZipFile(jar) as z:
        return Image.open(io.BytesIO(z.read(name))).convert("RGBA")


def match(img, tex, s, gx, gy, u, v, w, h, min_alpha=255):
    good = 0
    total = 0
    for dy in range(h * s):
        for dx in range(w * s):
            tr, tg, tb, ta = tex.getpixel((u + dx // s, v + dy // s))
            if ta < min_alpha:
                continue
            px = (gx * s + dx, gy * s + dy)
            if px[0] >= img.size[0] or px[1] >= img.size[1]:
                continue
            total += 1
            if img.getpixel(px)[:3] == (tr, tg, tb):
                good += 1
    return good, total


def read(path):
    w, h, data = lc.read_ppm(path)
    return Image.frombytes("RGB", (w, h), data)


def expect(name, good, total, errors, floor=0.97):
    ok = total > 0 and good / total >= floor
    print(f"  {'ok  ' if ok else 'FAIL'}  hud {name}: {good}/{total}")
    if not ok:
        errors.append(name)


def check_bars(img, jar, errors):
    widgets = load(jar, WIDGETS)
    icons = load(jar, ICONS)
    w, h = img.size
    s = lc.scale_for(w, h)
    gw = (w + s - 1) // s
    gh = (h + s - 1) // s
    hx = gw // 2 - 91
    g, t = match(img, widgets, s, hx + 1, gh - 21, 1, 1, 180, 20, 255)
    expect("hotbar", g, t, errors, 0.4)
    g, t = match(img, widgets, s, hx - 1, gh - 23, 0, 22, 24, 24, 255)
    expect("selected slot", g, t, errors, 0.9)
    top = gh - 39
    g, t = match(img, icons, s, hx, top, 52, 0, 9, 9, 255)
    expect("heart full", g, t, errors, 0.9)
    g, t = match(img, icons, s, gw // 2 + 91 - 9, top, 52, 27, 9, 9, 255)
    expect("food full", g, t, errors, 0.9)


def check_inventory(img, jar, errors):
    w, h = img.size
    s = lc.scale_for(w, h)
    gw = (w + s - 1) // s
    gh = (h + s - 1) // s
    left = (gw - 176) // 2
    top = (gh - 166) // 2
    if os.path.isfile(jar):
        tex = load(jar, INVENTORY)
        g1, t1 = match(img, tex, s, left, top + 82, 0, 82, 176, 84, 255)
        expect("inventory slots area", g1, t1, errors, 0.85)
        g2, t2 = match(img, tex, s, left + 76, top, 76, 0, 100, 80, 255)
        expect("inventory crafting area", g2, t2, errors, 0.8)
        return
    grey = sum(1 for x in range(left * s, (left + 176) * s, 2) for y in range((top + 84) * s, (top + 160) * s, 2) if img.getpixel((x, y)) == (198, 198, 198))
    ok = grey > 200
    print(f"  {'ok  ' if ok else 'FAIL'}  plain inventory panel grey pixels={grey}")
    if not ok:
        errors.append("inv")


def match_px(img, tex, s, ox, oy, u, v, w, h):
    good = 0
    total = 0
    for dy in range(h * s):
        for dx in range(w * s):
            tr, tg, tb, ta = tex.getpixel((u + dx // s, v + dy // s))
            if ta < 255:
                continue
            px = (ox + dx, oy + dy)
            if px[0] >= img.size[0] or px[1] >= img.size[1]:
                continue
            total += 1
            if img.getpixel(px)[:3] == (tr, tg, tb):
                good += 1
    return good, total


def check_container(img, jar, kind, rows, errors):
    w, h = img.size
    s = lc.scale_for(w, h)
    if not os.path.isfile(jar):
        print("  skip  container panel needs the vanilla jar")
        return
    tex = load(jar, f"assets/minecraft/textures/gui/container/{CONTAINER_TEX[kind]}.png")
    pw = 176
    if kind == "generic":
        top = rows * 18 + 17
        ph = 114 + rows * 18
    else:
        ph = CONTAINER_H[kind]
        top = ph - 96
    ox = (w - pw * s) // 2
    oy = (h - ph * s) // 2
    if kind == "generic":
        g1, t1 = match_px(img, tex, s, ox, oy, 0, 0, pw, top)
        expect(f"{kind}{rows} top part", g1, t1, errors, 0.85)
        g2, t2 = match_px(img, tex, s, ox, oy + top * s, 0, 126, pw, 96)
        expect(f"{kind}{rows} bottom part", g2, t2, errors, 0.85)
    else:
        g1, t1 = match_px(img, tex, s, ox, oy, 0, 0, pw, top)
        expect(f"{kind} upper part", g1, t1, errors, 0.8)
        g2, t2 = match_px(img, tex, s, ox, oy + top * s, 0, top, pw, 96)
        expect(f"{kind} player part", g2, t2, errors, 0.85)


def sprite_or_sheet(jar, sprite, sheet_u, sheet_v, w, h):
    with zipfile.ZipFile(jar) as z:
        names = set(z.namelist())
    path = "assets/minecraft/textures/gui/sprites/container/" + sprite + ".png"
    if path in names:
        return load(jar, path), 0, 0
    return load(jar, "assets/minecraft/textures/gui/container/furnace.png"), sheet_u, sheet_v


def check_furnace_lit(img, jar, errors):
    w, h = img.size
    s = lc.scale_for(w, h)
    if not os.path.isfile(jar):
        print("  skip  furnace lit needs the vanilla jar")
        return
    ox = (w - 176 * s) // 2
    oy = (h - 166 * s) // 2
    flame, fu, fv = sprite_or_sheet(jar, "furnace/lit_progress", 176, 0, 14, 14)
    g, t = match_px(img, flame, s, ox + 56 * s, oy + 36 * s, fu, fv, 14, 14)
    expect("furnace flame lit", g, t, errors, 0.6)
    arrow, au, av = sprite_or_sheet(jar, "furnace/burn_progress", 176, 14, 24, 16)
    g, t = match_px(img, arrow, s, ox + 79 * s, oy + 34 * s, au, av, 4, 16)
    expect("furnace arrow started", g, t, errors, 0.6)


def has_hotbar(img, jar):
    widgets = load(jar, WIDGETS)
    w, h = img.size
    s = lc.scale_for(w, h)
    gw = (w + s - 1) // s
    gh = (h + s - 1) // s
    g, t = match(img, widgets, s, gw // 2 - 91 + 1, gh - 21, 1, 1, 180, 20, 255)
    return t > 0 and g / t >= 0.4


def region_diff(a, b, x0, y0, x1, y1):
    return sum(1 for x in range(x0, x1) for y in range(y0, y1) if a.getpixel((x, y)) != b.getpixel((x, y)))


SPRITES = "assets/minecraft/textures/gui/sprites/container/"
SPR_CHECKS = {
    "enchant": [("enchanting_table/enchantment_slot", 60, 14), ("enchanting_table/enchantment_slot", 60, 33),
                ("enchanting_table/enchantment_slot", 60, 52), ("enchanting_table/level_1", 61, 15),
                ("enchanting_table/level_2", 61, 34), ("enchanting_table/level_3", 61, 53)],
    "brewing": [("brewing_stand/fuel_length", 60, 44), ("brewing_stand/brew_progress", 97, 16, 14)],
    "anvil": [("anvil/text_field", 59, 20)],
    "horse": [("slot", 7, 17), ("slot", 7, 35), ("horse/chest_slots", 79, 17)],
}
CREATIVE_TABS = SPRITES + "creative_inventory/"


def check_sprites(img, jar, kind, errors, s=2):
    for item in SPR_CHECKS[kind]:
        tex = load(jar, SPRITES + item[0] + ".png")
        h = item[3] if len(item) > 3 else tex.size[1]
        good, total = match_px(img, tex, s, item[1] * s, item[2] * s, 0, 0, tex.size[0], h)
        expect(f"{kind} sprite {item[0]}", good, total, errors, 0.6)


def check_creative_sprites(img, jar, errors, s=2, px=40, py=60):
    cases = [("tab_bottom_unselected_4", 27 * 3, 136, 4, 28), ("tab_top_unselected_2", 27, 0 - 28, 0, 28)]
    for name, x, y, v, h in cases:
        tex = load(jar, CREATIVE_TABS + name + ".png")
        good, total = match_px(img, tex, s, px + x * s, py + y * s, 0, v, tex.size[0], h)
        expect(f"creative sprite {name}", good, total, errors, 0.95)


def main():
    mode, ppm, jar = sys.argv[1], sys.argv[2], sys.argv[3]
    img = read(ppm)
    errors = []
    plain = not os.path.isfile(jar)
    if plain and mode in ("bars", "hidden", "boss", "png"):
        w, h = img.size
        s = lc.scale_for(w, h)
        if mode == "png":
            ok = (w, h) == (int(sys.argv[4]), int(sys.argv[5]))
            print(f"  {'ok  ' if ok else 'FAIL'}  screenshot size {w}x{h}")
            errors += [] if ok else ["png"]
        elif mode == "bars":
            colours = len(set(img.getpixel((x, y)) for x in range(0, w, 3) for y in range(0, h, 3)))
            ok = colours > 3
            print(f"  {'ok  ' if ok else 'FAIL'}  plain frame colours={colours}")
            errors += [] if ok else ["bars"]
        else:
            ref = read(sys.argv[4])
            if mode == "hidden":
                d = region_diff(img, ref, w // 2 - 100 * s, h - 30 * s, w // 2 + 100 * s, h)
            else:
                d = region_diff(img, ref, w // 2 - 100 * s, 0, w // 2 + 100 * s, 30 * s)
            ok = d > 100
            print(f"  {'ok  ' if ok else 'FAIL'}  plain {mode} changed pixels={d}")
            errors += [] if ok else [mode]
        sys.exit(1 if errors else 0)
    if mode == "bars":
        check_bars(img, jar, errors)
    elif mode == "inv":
        check_inventory(img, jar, errors)
    elif mode == "spr":
        if sys.argv[4] == "creative":
            check_creative_sprites(img, jar, errors)
        else:
            check_sprites(img, jar, sys.argv[4], errors)
    elif mode == "cont":
        check_container(img, jar, sys.argv[4], int(sys.argv[5]) if len(sys.argv) > 5 else 3, errors)
    elif mode == "furnace_lit":
        check_furnace_lit(img, jar, errors)
    elif mode == "hidden":
        ok = not has_hotbar(img, jar)
        print(f"  {'ok  ' if ok else 'FAIL'}  hud hidden with F1")
        if not ok:
            errors.append("hidden")
    elif mode == "debug":
        s = lc.scale_for(*img.size)
        px = img.getpixel((2 * s + 1, 2 * s + 1))
        bright = sum(1 for x in range(2 * s, 40 * s) for y in range(2 * s, 9 * s) if sum(img.getpixel((x, y))) > 600)
        ok = bright > 20
        print(f"  {'ok  ' if ok else 'FAIL'}  hud debug text pixels={bright}")
        if not ok:
            errors.append("debug")
    elif mode == "boss":
        bars = load(jar, "assets/minecraft/textures/gui/bars.png")
        s = lc.scale_for(*img.size)
        gw = (img.size[0] + s - 1) // s
        g, t = match(img, bars, s, gw // 2 - 91, 12, 0, 25, 182, 5, 255)
        expect("bossbar", g, t, errors, 0.9)
    elif mode == "title":
        s = lc.scale_for(*img.size)
        w, h = img.size
        white = sum(1 for x in range(w // 2 - 40 * s, w // 2 + 40 * s) for y in range(h // 2 - 22 * s, h // 2 - 2 * s) if min(img.getpixel((x, y))) > 240)
        ok = white > 200
        print(f"  {'ok  ' if ok else 'FAIL'}  hud title white pixels={white}")
        if not ok:
            errors.append("title")
    elif mode == "side":
        before = read(sys.argv[4])
        w, h = img.size
        s = lc.scale_for(w, h)
        diff = sum(1 for x in range(w - 30 * s, w - 3 * s) for y in range(h // 2 - 20 * s, h // 2 + 20 * s) if img.getpixel((x, y)) != before.getpixel((x, y)))
        ok = diff > 300
        print(f"  {'ok  ' if ok else 'FAIL'}  hud sidebar changed pixels={diff}")
        if not ok:
            errors.append("side")
    elif mode == "png":
        w, h = img.size
        ok = (w, h) == (int(sys.argv[4]), int(sys.argv[5]))
        print(f"  {'ok  ' if ok else 'FAIL'}  screenshot size {w}x{h}")
        if not ok:
            errors.append("png")
        check_bars(img, jar, errors)
    sys.exit(1 if errors else 0)


main()
