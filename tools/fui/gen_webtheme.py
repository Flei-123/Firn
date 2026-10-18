#!/usr/bin/env python3
"""tools/fui/gen_webtheme.py -- design tokens of the JARVIS web app -> lib/fui/webtheme_gen.fi (and JSON / CSS-variable export).

Source of truth: public/style.css of the JARVIS web app (`:root` = theme "fleitec", `html[data-theme="claude"]` = theme "claude",
plus a few pinned literals of the phone layout). The generated Firn file holds every token as a constant; webtheme.fi builds the
runtime theme on top of it and tools/fui/webtheme_main.fi checks the constants against literal CSS values.

    python3 tools/fui/gen_webtheme.py                      write lib/fui/webtheme_gen.fi from the default CSS
    python3 tools/fui/gen_webtheme.py --check              exit 1 when the file on disk differs from a fresh generation (drift)
    python3 tools/fui/gen_webtheme.py --css PATH           other source
    python3 tools/fui/gen_webtheme.py --json OUT.json      also write the tokens as JSON (design-token export, r161)
    python3 tools/fui/gen_webtheme.py --css-vars OUT.css   also write the tokens back as CSS custom properties

--check without a readable CSS file prints SKIP and exits 0 (the CSS lives in the JARVIS checkout, not in the Firn repo).
"""
import json
import os
import re
import sys

DEFAULT_CSS = "/root/jarvis/public/style.css"
HERE = os.path.dirname(os.path.abspath(__file__))
OUT_FI = os.path.join(HERE, "..", "..", "lib", "fui", "webtheme_gen.fi")

# CSS variable -> (kind). Colours keep their alpha, gradients keep both ends and the angle.
COLOR_VARS = ["bg", "bg-sidebar", "panel", "bg-hover", "line", "txt", "dim", "accent", "accent2", "user", "warn", "ok",
              "code-bg", "field-bg", "field-line", "ring"]
GRAD_VARS = ["accent-grad", "user-grad"]
LEN_VARS = ["radius-sm", "radius", "radius-card", "radius-pill"]
SHADOW_VARS = ["shadow-sm", "shadow", "shadow-lg"]


def fail(msg):
    sys.stderr.write("gen_webtheme: " + msg + "\n")
    sys.exit(2)


def strip_comments(s):
    return re.sub(r"/\*.*?\*/", "", s, flags=re.S)


def block(css, head_re):
    """Body of the first rule whose selector matches head_re (a regex over the selector text)."""
    m = re.search(r"(?:^|\})\s*(" + head_re + r")\s*\{", css)
    if not m:
        fail("block not found: " + head_re)
    i = m.end()
    depth = 1
    j = i
    while depth and j < len(css):
        if css[j] == "{":
            depth += 1
        elif css[j] == "}":
            depth -= 1
        j += 1
    return css[i:j - 1]


def decls(body):
    out = {}
    for m in re.finditer(r"--([a-z0-9-]+)\s*:\s*([^;]+);", body):
        out[m.group(1)] = " ".join(m.group(2).split())
    return out


def parse_color(v):
    """-> (rgb:int, alpha:int 0..255)"""
    v = v.strip()
    m = re.fullmatch(r"#([0-9a-fA-F]{6})", v)
    if m:
        return int(m.group(1), 16), 255
    m = re.fullmatch(r"#([0-9a-fA-F]{3})", v)
    if m:
        h = m.group(1)
        return int("".join(c * 2 for c in h), 16), 255
    m = re.fullmatch(r"rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*(?:,\s*([0-9.]+)\s*)?\)", v)
    if m:
        r, g, b = int(m.group(1)), int(m.group(2)), int(m.group(3))
        a = 255 if m.group(4) is None else int(round(float(m.group(4)) * 255))
        return (r << 16) | (g << 8) | b, a
    fail("cannot parse colour: " + v)


def parse_grad(v):
    """linear-gradient(135deg, c0, c1) or a plain colour -> (c0, c1, angle)"""
    m = re.fullmatch(r"linear-gradient\(\s*(\d+)deg\s*,\s*(#[0-9a-fA-F]{3,6})\s*,\s*(#[0-9a-fA-F]{3,6})\s*\)", v)
    if m:
        return parse_color(m.group(2))[0], parse_color(m.group(3))[0], int(m.group(1))
    c = parse_color(v)[0]
    return c, c, 0


def parse_len(v):
    m = re.fullmatch(r"(\d+)px", v)
    if not m:
        fail("cannot parse length: " + v)
    return int(m.group(1))


def parse_shadow(v):
    """0 8px 24px rgba(0,0,0,.4) -> (dx, dy, blur, rgb, alpha)"""
    m = re.fullmatch(r"(-?\d+)(?:px)?\s+(-?\d+)(?:px)?\s+(\d+)(?:px)?\s+(.+)", v)
    if not m:
        fail("cannot parse shadow: " + v)
    rgb, a = parse_color(m.group(4))
    return int(m.group(1)), int(m.group(2)), int(m.group(3)), rgb, a


def pinned(css, name, pattern, group=1):
    m = re.search(pattern, css, flags=re.S)
    if not m:
        fail("pinned literal not found (CSS changed shape?): " + name)
    return m.group(group)


def theme_tokens(vars_):
    t = {}
    for k in COLOR_VARS:
        if k not in vars_:
            fail("missing colour var --" + k)
        t[k] = parse_color(vars_[k])
    for k in GRAD_VARS:
        t[k] = parse_grad(vars_[k])
    for k in LEN_VARS:
        t[k] = parse_len(vars_[k])
    for k in SHADOW_VARS:
        t[k] = parse_shadow(vars_[k])
    return t


def load(css_path):
    with open(css_path, encoding="utf-8") as f:
        raw = f.read()
    css = strip_comments(raw)
    root = decls(block(css, r":root"))
    claude_over = decls(block(css, r'html\[data-theme="claude"\]'))
    claude = dict(root)
    claude.update(claude_over)
    themes = {"fleitec": theme_tokens(root), "claude": theme_tokens(claude)}
    # literals that are not CSS variables (phone layout); a regex that stops matching = the CSS moved on = regenerate on purpose
    pins = {
        "body_px": int(pinned(css, "body font 14px on phones",
                              r"@media \(max-width: 760px\)\s*\{\s*body\s*\{\s*font-size:\s*(\d+)px")),
        "brand_px_x100": int(round(float(pinned(css, "brand size", r"\.brand\s*\{[^}]*?font-size:\s*([0-9.]+)rem")) * 16 * 100)),
        "brand_weight": int(pinned(css, "brand weight", r"\.brand\s*\{[^}]*?font-weight:\s*(\d+)")),
        "brand_tracking": int(pinned(css, "brand letter-spacing", r"\.brand\s*\{[^}]*?letter-spacing:\s*(\d+)px")),
        "pill_px": int(pinned(css, "pill font", r"\n\.pill\s*\{[^}]*?font-size:\s*(\d+)px")),
        "header_h": int(pinned(css, "header min-height", r"\nheader\s*\{[^}]*?min-height:\s*(\d+)px")),
        "bubble_r": int(pinned(css, "user bubble radius", r"\.msg\.user\s*\{[^}]*?border-radius:\s*(\d+)px;\s*border-bottom-right-radius:\s*\d+px")),
        "bubble_r_tail": int(pinned(css, "user bubble tail radius", r"\.msg\.user\s*\{[^}]*?border-bottom-right-radius:\s*(\d+)px")),
        "send_btn": int(pinned(css, "#send size", r"\n#send\s*\{[^}]*?width:\s*(\d+)px")),
        "icon_btn": int(pinned(css, "phone icon button", r"\.topbar \.icon-btn\s*\{\s*width:\s*(\d+)px")),
        "icon_btn_desktop": int(pinned(css, "desktop icon button", r"\n\.icon-btn\s*\{[^}]*?width:\s*(\d+)px")),
    }
    return themes, pins


def hexc(v):
    return "0x%06x" % v


def emit_fi(themes, pins):
    L = []
    w = L.append
    w("// SPDX-License-Identifier: MPL-2.0")
    w("// lib/fui/webtheme_gen.fi -- GENERATED by tools/fui/gen_webtheme.py from the JARVIS web app's public/style.css. DO NOT EDIT.")
    w("//")
    w("// Names: <THEME>_<TOKEN> with THEME = FLEITEC (the `:root` block, dark) or CLAUDE (html[data-theme=\"claude\"], values it does not")
    w("// override are inherited from `:root` as in CSS). Colours are 0xRRGGBB; a token with transparency has a `_A` constant (0..255).")
    w("// Gradients: `_G0`, `_G1` (0xRRGGBB) and `_GDEG` (CSS angle). Shadows: `_X`, `_Y`, `_BLUR` (px), `_RGB`, `_A`.")
    w("// Lengths are CSS px = fUi design points. Drift check: python3 tools/fui/gen_webtheme.py --check")
    w("export {")
    names = []
    body = []

    def const(name, typ, val):
        names.append(name)
        body.append("const %s: %s = %s" % (name, typ, val))

    for tn in ("fleitec", "claude"):
        T = themes[tn]
        P = tn.upper() + "_"
        for k in COLOR_VARS:
            rgb, a = T[k]
            n = P + k.upper().replace("-", "_")
            const(n, "u32", hexc(rgb))
            if a != 255:
                const(n + "_A", "u32", str(a))
        for k in GRAD_VARS:
            c0, c1, deg = T[k]
            n = P + k.upper().replace("-", "_")
            const(n + "_G0", "u32", hexc(c0))
            const(n + "_G1", "u32", hexc(c1))
            const(n + "_GDEG", "usize", str(deg))
        for k in SHADOW_VARS:
            dx, dy, blur, rgb, a = T[k]
            n = P + k.upper().replace("-", "_")
            const(n + "_X", "i64", str(dx))
            const(n + "_Y", "i64", str(dy))
            const(n + "_BLUR", "usize", str(blur))
            const(n + "_RGB", "u32", hexc(rgb))
            const(n + "_A", "u32", str(a))
    F = themes["fleitec"]
    for k in LEN_VARS:
        const("CSS_" + k.upper().replace("-", "_"), "usize", str(F[k]))
        for tn in ("claude",):
            if themes[tn][k] != F[k]:
                fail("radius differs between themes: " + k)
    const("CSS_BODY_PX", "usize", str(pins["body_px"]))
    const("CSS_BRAND_PX_X100", "usize", str(pins["brand_px_x100"]))
    const("CSS_BRAND_WEIGHT", "usize", str(pins["brand_weight"]))
    const("CSS_BRAND_TRACKING", "i64", str(pins["brand_tracking"]))
    const("CSS_PILL_PX", "usize", str(pins["pill_px"]))
    const("CSS_HEADER_H", "usize", str(pins["header_h"]))
    const("CSS_BUBBLE_R", "usize", str(pins["bubble_r"]))
    const("CSS_BUBBLE_R_TAIL", "usize", str(pins["bubble_r_tail"]))
    const("CSS_SEND_BTN", "usize", str(pins["send_btn"]))
    const("CSS_ICON_BTN", "usize", str(pins["icon_btn"]))
    const("CSS_ICON_BTN_DESKTOP", "usize", str(pins["icon_btn_desktop"]))
    # export list, wrapped
    line = "   "
    for n in names:
        if len(line) + len(n) + 2 > 130:
            w(line)
            line = "   "
        line += " " + n + ","
    w(line)
    w("}")
    w("")
    L.extend(body)
    return "\n".join(L) + "\n"


def to_json(themes, pins):
    def ser(t):
        o = {}
        for k, v in t.items():
            if k in COLOR_VARS:
                o[k] = {"rgb": "#%06x" % v[0], "alpha": v[1]}
            elif k in GRAD_VARS:
                o[k] = {"from": "#%06x" % v[0], "to": "#%06x" % v[1], "deg": v[2]}
            elif k in SHADOW_VARS:
                o[k] = {"x": v[0], "y": v[1], "blur": v[2], "rgb": "#%06x" % v[3], "alpha": v[4]}
            else:
                o[k] = v
        return o
    return json.dumps({"themes": {k: ser(v) for k, v in themes.items()}, "pins": pins}, indent=2, sort_keys=True) + "\n"


def to_css(themes, pins):
    out = []
    for tn, sel in (("fleitec", ":root"), ("claude", 'html[data-theme="claude"]')):
        T = themes[tn]
        out.append(sel + " {")
        for k, v in T.items():
            if k in COLOR_VARS:
                if v[1] == 255:
                    out.append("  --%s: #%06x;" % (k, v[0]))
                else:
                    out.append("  --%s: rgba(%d, %d, %d, %.3g);" % (k, v[0] >> 16, (v[0] >> 8) & 255, v[0] & 255, v[1] / 255))
            elif k in GRAD_VARS:
                if v[0] == v[1]:
                    out.append("  --%s: #%06x;" % (k, v[0]))
                else:
                    out.append("  --%s: linear-gradient(%ddeg, #%06x, #%06x);" % (k, v[2], v[0], v[1]))
            elif k in SHADOW_VARS:
                out.append("  --%s: %dpx %dpx %dpx rgba(%d, %d, %d, %.3g);" % (
                    k, v[0], v[1], v[2], v[3] >> 16, (v[3] >> 8) & 255, v[3] & 255, v[4] / 255))
            else:
                out.append("  --%s: %dpx;" % (k, v))
        out.append("}")
    return "\n".join(out) + "\n"


def main(argv):
    css = DEFAULT_CSS
    check = False
    jpath = None
    cpath = None
    i = 1
    while i < len(argv):
        a = argv[i]
        if a == "--check":
            check = True
        elif a == "--css":
            i += 1
            css = argv[i]
        elif a == "--json":
            i += 1
            jpath = argv[i]
        elif a == "--css-vars":
            i += 1
            cpath = argv[i]
        else:
            fail("unknown argument " + a)
        i += 1
    if not os.path.exists(css):
        if check:
            print("webtheme drift check: SKIP (no %s)" % css)
            return 0
        fail("no CSS at " + css)
    themes, pins = load(css)
    text = emit_fi(themes, pins)
    if jpath:
        with open(jpath, "w") as f:
            f.write(to_json(themes, pins))
    if cpath:
        with open(cpath, "w") as f:
            f.write(to_css(themes, pins))
    if check:
        with open(OUT_FI) as f:
            cur = f.read()
        if cur != text:
            sys.stderr.write("webtheme drift: %s differs from %s -- run: python3 tools/fui/gen_webtheme.py\n" % (OUT_FI, css))
            return 1
        print("webtheme drift check: OK")
        return 0
    with open(OUT_FI, "w") as f:
        f.write(text)
    print("wrote", os.path.normpath(OUT_FI))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
