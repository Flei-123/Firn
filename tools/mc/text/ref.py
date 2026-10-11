#!/usr/bin/env python3
"""Independent reference for mc_text: the same flattening rules (style inheritance, extra, arrays,
translate with %s / %n$s / %%, fallback, keybind / score / selector placeholders) re-implemented in python,
plus a random component generator.

  ref.py gen <outdir> <count> <seed>     writes case<i>.json, case<i>.expected, and lang.json
  ref.py legacy <outdir>                 writes legacy<i>.txt / legacy<i>.expected (section sign codes)
"""
import json, os, random, sys

NAMED = {"black": 0x000000, "dark_blue": 0x0000AA, "dark_green": 0x00AA00, "dark_aqua": 0x00AAAA,
         "dark_red": 0xAA0000, "dark_purple": 0xAA00AA, "gold": 0xFFAA00, "gray": 0xAAAAAA,
         "dark_gray": 0x555555, "blue": 0x5555FF, "green": 0x55FF55, "aqua": 0x55FFFF,
         "red": 0xFF5555, "light_purple": 0xFF55FF, "yellow": 0xFFFF55, "white": 0xFFFFFF}
LEGACY = [NAMED[n] for n in ("black", "dark_blue", "dark_green", "dark_aqua", "dark_red", "dark_purple", "gold",
                             "gray", "dark_gray", "blue", "green", "aqua", "red", "light_purple", "yellow", "white")]
FLAGS = (("bold", 1), ("italic", 2), ("underlined", 4), ("strikethrough", 8), ("obfuscated", 16))
NO_COLOR = -1
MAX_DEPTH = 32


def parse_color(s):
    if len(s) == 7 and s[0] == "#":
        try:
            return int(s[1:], 16)
        except ValueError:
            return NO_COLOR
    return NAMED.get(s, NO_COLOR)


def apply_style(o, st):
    color, flags = st
    c = o.get("color")
    if isinstance(c, str):
        rgb = parse_color(c)
        if rgb != NO_COLOR:
            color = rgb
        elif c == "reset":
            color = NO_COLOR
    for name, bit in FLAGS:
        v = o.get(name)
        if v is True:
            flags |= bit
        elif v is False:
            flags &= ~bit
    return (color, flags)


class Out:
    def __init__(self):
        self.spans = []  # [color, flags, bytearray]

    def emit(self, text, st):
        b = text.encode("utf-8")
        if not b:
            return
        if self.spans and self.spans[-1][0] == st[0] and self.spans[-1][1] == st[1]:
            self.spans[-1][2] += b
        else:
            self.spans.append([st[0], st[1], bytearray(b)])


def prim_text(v):
    if v is True:
        return "true"
    if v is False:
        return "false"
    return str(int(v))


def translated(out, lang, fmt, with_, st, depth):
    i, seg, nxt = 0, 0, 0
    n = len(fmt)

    def arg(k):
        if isinstance(with_, list) and k < len(with_):
            flatten(out, lang, with_[k], st, depth + 1)

    while i < n:
        if fmt[i] == "%" and i + 1 < n:
            c = fmt[i + 1]
            if c == "%":
                out.emit(fmt[seg:i + 1], st)
                i += 2; seg = i
            elif c == "s":
                out.emit(fmt[seg:i], st)
                arg(nxt); nxt += 1
                i += 2; seg = i
            elif c in "123456789" and i + 3 < n and fmt[i + 2] == "$" and fmt[i + 3] == "s":
                out.emit(fmt[seg:i], st)
                arg(int(c) - 1)
                i += 4; seg = i
            else:
                i += 1
        else:
            i += 1
    out.emit(fmt[seg:], st)


def flatten(out, lang, node, st, depth):
    if depth > MAX_DEPTH:
        return
    if isinstance(node, str):
        out.emit(node, st); return
    if isinstance(node, (bool, int)):
        out.emit(prim_text(node), st); return
    if isinstance(node, list):
        if not node:
            return
        first = node[0]
        inherited = apply_style(first, st) if isinstance(first, dict) else st
        flatten(out, lang, first, st, depth + 1)
        for c in node[1:]:
            flatten(out, lang, c, inherited, depth + 1)
        return
    if not isinstance(node, dict):
        return
    s = apply_style(node, st)
    if "text" in node:
        t = node["text"]
        if isinstance(t, str):
            out.emit(t, s)
        elif isinstance(t, int) and not isinstance(t, bool):
            out.emit(str(t), s)
    elif isinstance(node.get("translate"), str):
        key = node["translate"]
        fmt = lang.get(key, "")
        if not fmt and isinstance(node.get("fallback"), str):
            fmt = node["fallback"]
        if not fmt:
            out.emit(key, s)
        else:
            translated(out, lang, fmt, node.get("with"), s, depth)
    else:
        if isinstance(node.get("keybind"), str):
            out.emit(node["keybind"], s)
        elif isinstance(node.get("score"), dict):
            sc = node["score"]
            if isinstance(sc.get("value"), str):
                out.emit(sc["value"], s)
            elif isinstance(sc.get("name"), str):
                out.emit(sc["name"], s)
        elif isinstance(node.get("selector"), str):
            out.emit(node["selector"], s)
    ex = node.get("extra")
    if isinstance(ex, list):
        for c in ex:
            flatten(out, lang, c, s, depth + 1)


def nearest(rgb):
    best, bd = 15, 10 ** 9
    for i, c in enumerate(LEGACY):
        d = ((c >> 16 & 255) - (rgb >> 16 & 255)) ** 2 + ((c >> 8 & 255) - (rgb >> 8 & 255)) ** 2 + ((c & 255) - (rgb & 255)) ** 2
        if d < bd:
            bd, best = d, i
    return best


def code(i):
    return "0123456789abcdef"[i]


def render(spans):
    lines = []
    for k, (color, flags, b) in enumerate(spans):
        lines.append("span %d color=%s flags=%d len=%d hex=%s" % (k, "-" if color == NO_COLOR else "%06x" % color, flags, len(b), bytes(b).hex()))
    plain = b"".join(bytes(s[2]) for s in spans)
    lines.append("plain hex=" + plain.hex())
    leg = bytearray()
    for i, (color, flags, b) in enumerate(spans):
        if color != NO_COLOR or flags:
            leg += "§r".encode()
            if color != NO_COLOR:
                leg += ("§" + code(nearest(color))).encode()
            for ch, bit in (("k", 16), ("l", 1), ("m", 8), ("n", 4), ("o", 2)):
                if flags & bit:
                    leg += ("§" + ch).encode()
        elif i > 0:
            leg += "§r".encode()
        leg += b
    lines.append("legacy hex=" + bytes(leg).hex())
    an = bytearray()
    for color, flags, b in spans:
        an += b"\x1b[0"
        if color != NO_COLOR:
            an += (";38;2;%d;%d;%d" % (color >> 16 & 255, color >> 8 & 255, color & 255)).encode()
        for bit, c in ((1, ";1"), (2, ";3"), (4, ";4"), (16, ";5"), (8, ";9")):
            if flags & bit:
                an += c.encode()
        an += b"m" + b
    an += b"\x1b[0m"
    lines.append("ansi hex=" + bytes(an).hex())
    return lines


# ------------------------------------------------------------------------------------ generator
LANG = {
    "chat.type.text": "<%s> %s",
    "death.attack.fall": "%1$s hit the ground too hard",
    "multiplayer.player.joined": "%s joined the game",
    "swap": "%2$s before %1$s",
    "percent": "100%% sure, %s",
    "plain.key": "no arguments here",
    "mixed": "a %s b %% c %2$s d %s e",
}
TEXTS = ["", "a", "Hello", "café", "日本語", "x" * 50, "line\nbreak", "50% done", "§a literal sign"]
KEYS = list(LANG) + ["unknown.key", "also.unknown"]


def rand_style(r):
    o = {}
    if r.random() < 0.4:
        o["color"] = r.choice(list(NAMED) + ["#ff8800", "#123456", "reset", "bogus", "#12345", "#gg0000"])
    for name, _ in FLAGS:
        if r.random() < 0.2:
            o[name] = r.choice([True, False])
    return o


def rand_comp(r, depth):
    k = r.random()
    if depth > 4 or k < 0.2:
        return r.choice(TEXTS)
    if k < 0.3:
        return [rand_comp(r, depth + 1) for _ in range(r.randint(0, 3))]
    o = rand_style(r)
    t = r.random()
    if t < 0.5:
        o["text"] = r.choice(TEXTS)
    elif t < 0.75:
        o["translate"] = r.choice(KEYS)
        if r.random() < 0.7:
            o["with"] = [rand_comp(r, depth + 1) for _ in range(r.randint(0, 3))]
        if r.random() < 0.3:
            o["fallback"] = "fb %s"
    elif t < 0.83:
        o["keybind"] = "key.jump"
    elif t < 0.9:
        o["score"] = r.choice([{"name": "Steve", "objective": "o"}, {"name": "x", "objective": "o", "value": "7"}])
    elif t < 0.95:
        o["selector"] = "@a[distance=..5]"
    else:
        o["text"] = r.randint(-5, 500)
    if r.random() < 0.5:
        o["extra"] = [rand_comp(r, depth + 1) for _ in range(r.randint(0, 3))]
    return o


def main():
    cmd, outdir = sys.argv[1], sys.argv[2]
    os.makedirs(outdir, exist_ok=True)
    if cmd == "gen":
        count, seed = int(sys.argv[3]), int(sys.argv[4])
        r = random.Random(seed)
        json.dump(LANG, open(outdir + "/lang.json", "w"))
        for i in range(count):
            comp = rand_comp(r, 0)
            txt = json.dumps(comp, ensure_ascii=r.random() < 0.5)
            open("%s/case%d.json" % (outdir, i), "w", encoding="utf-8").write(txt)
            out = Out()
            flatten(out, LANG, json.loads(txt), (NO_COLOR, 0), 0)
            open("%s/case%d.expected" % (outdir, i), "w").write("\n".join(render(out.spans)) + "\n")
    elif cmd == "legacy":
        cases = ["plain", "§aGreen §lbold§r plain", "§cred§lB§ogreen? no §4dark", "§kjunk§r§n§munder-strike",
                 "end §", "bad §z code", "§A§LUPPER", "§0§1§2§3§4§5§6§7§8§9§a§b§c§d§e§f x", "café §a日本"]
        for i, c in enumerate(cases):
            open("%s/legacy%d.txt" % (outdir, i), "w", encoding="utf-8").write(c)
            # python's own legacy parser
            spans, st, buf = [], (NO_COLOR, 0), ""
            out = Out()
            b = c
            idx, seg = 0, 0
            while idx < len(b):
                if b[idx] == "§" and idx + 1 < len(b):
                    out.emit(b[seg:idx], st)
                    ch = b[idx + 1].lower()
                    color, flags = st
                    if ch in "0123456789abcdef":
                        color, flags = LEGACY["0123456789abcdef".index(ch)], 0
                    elif ch == "k": flags |= 16
                    elif ch == "l": flags |= 1
                    elif ch == "m": flags |= 8
                    elif ch == "n": flags |= 4
                    elif ch == "o": flags |= 2
                    elif ch == "r": color, flags = NO_COLOR, 0
                    st = (color, flags)
                    idx += 2; seg = idx
                else:
                    idx += 1
            out.emit(b[seg:], st)
            open("%s/legacy%d.expected" % (outdir, i), "w").write("\n".join(render(out.spans)) + "\n")


main()
