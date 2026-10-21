#!/usr/bin/env python3
"""Build gallery/index.html (one row per app, toolkits side by side, light+dark, labelled with
lines / MB / startup ms / RAM / SSIM from results/) and gallery/collage.png (also copied to ./collage.png).
Missing shots/results -> placeholders; never fails."""
import html
import json
import os

from PIL import Image, ImageDraw, ImageFont

SCRIPTS = os.path.dirname(os.path.abspath(__file__))
HERE = os.environ.get("UIVGL_DIR", SCRIPTS)  # data/shots/results/apps root
SHOTS = os.path.join(HERE, "shots")
RES = os.path.join(HERE, "results")
OUT = os.path.join(HERE, "gallery")
APPS = ["flei", "chat", "settings", "dash", "files", "form", "notes"]
TOOLKITS = ["original", "fui", "web", "egui", "gtk", "qt"]
THEMES = ["dark", "light"]
THUMB = (330, 210)


def jload(p):
    try:
        with open(p) as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def med(d):
    return d.get("median") if isinstance(d, dict) else None


def caption(app, tk, theme, cmp_rows):
    r = jload(os.path.join(RES, "%s-%s.json" % (app, tk))) or {}
    parts = []
    if r.get("lines"):
        parts.append("%d lines" % r["lines"]["total"])
    if r.get("size_bytes"):
        parts.append("%.2f MB" % (r["size_bytes"] / 1e6))
    if med(r.get("startup_ms")) is not None:
        parts.append("%.0f ms start" % med(r["startup_ms"]))
    if med(r.get("ram_pss_kb")) is not None:
        parts.append("%.0f MB RAM" % (med(r["ram_pss_kb"]) / 1024))
    for c in cmp_rows:
        if c.get("app") == app and c.get("toolkit") == tk and c.get("theme") == theme and "ssim" in c:
            parts.append("SSIM %.3f" % c["ssim"])
    return " | ".join(parts) if parts else "no data"


def shot_path(app, tk, theme):
    p = os.path.join(SHOTS, "%s-%s-%s.png" % (app, tk, theme))
    return p if os.path.exists(p) else None


def build_html(cmp_rows):
    h = ["<!doctype html><meta charset=utf-8><title>fUi UI comparison</title>",
         "<style>body{font:13px sans-serif;background:#16181d;color:#ddd;margin:16px}"
         "table{border-collapse:collapse}td,th{padding:6px;vertical-align:top;border-bottom:1px solid #333}"
         "img{width:330px;display:block;border:1px solid #444}.ph{width:330px;height:210px;background:#262a33;"
         "color:#777;display:flex;align-items:center;justify-content:center}.cap{color:#9aa;font-size:11px;"
         "max-width:330px}h1{font-size:18px}</style>",
         "<h1>fUi UI comparison (generated; numbers from results/*.json, empty = not measured)</h1><table>"]
    h.append("<tr><th>app</th>" + "".join("<th>%s</th>" % t for t in TOOLKITS) + "</tr>")
    for app in APPS:
        for theme in THEMES:
            h.append("<tr><td><b>%s</b><br>%s</td>" % (app, theme))
            for tk in TOOLKITS:
                p = shot_path(app, tk, theme)
                if p:
                    img = "<img src='../shots/%s' loading=lazy>" % html.escape(os.path.basename(p))
                else:
                    img = "<div class=ph>no shot</div>"
                h.append("<td>%s<div class=cap>%s</div></td>" % (img, html.escape(caption(app, tk, theme, cmp_rows))))
            h.append("</tr>")
    h.append("</table>")
    with open(os.path.join(OUT, "index.html"), "w") as f:
        f.write("\n".join(h))


def font(sz):
    for p in ("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",):
        if os.path.exists(p):
            return ImageFont.truetype(p, sz)
    return ImageFont.load_default()


def build_collage():
    lab_h = 16
    cw, ch = THUMB[0] + 4, THUMB[1] + lab_h + 4
    left = 90
    W = left + cw * len(TOOLKITS)
    H = 24 + ch * len(APPS) * len(THEMES)
    im = Image.new("RGB", (W, H), (22, 24, 29))
    d = ImageDraw.Draw(im)
    f, fs = font(12), font(11)
    for j, tk in enumerate(TOOLKITS):
        d.text((left + j * cw + 4, 5), tk, fill=(220, 220, 220), font=f)
    y = 24
    for app in APPS:
        for theme in THEMES:
            d.text((4, y + 4), "%s\n%s" % (app, theme), fill=(220, 220, 220), font=f)
            for j, tk in enumerate(TOOLKITS):
                x = left + j * cw
                p = shot_path(app, tk, theme)
                if p:
                    try:
                        t = Image.open(p).convert("RGB")
                        t.thumbnail(THUMB)
                        im.paste(t, (x + 2, y + 2))
                    except Exception:  # noqa: BLE001
                        p = None
                if not p:
                    d.rectangle([x + 2, y + 2, x + 2 + THUMB[0], y + 2 + THUMB[1]], fill=(38, 42, 51))
                    d.text((x + 120, y + 100), "no shot", fill=(120, 120, 120), font=f)
                d.text((x + 4, y + THUMB[1] + 3), "%s-%s-%s" % (app, tk, theme), fill=(150, 160, 170), font=fs)
            y += ch
    os.makedirs(OUT, exist_ok=True)
    im.save(os.path.join(OUT, "collage.png"))
    im.save(os.path.join(HERE, "collage.png"))


def main():
    os.makedirs(OUT, exist_ok=True)
    cmp_rows = jload(os.path.join(RES, "compare.json")) or []
    build_html(cmp_rows)
    build_collage()
    print("gallery:", os.path.join(OUT, "index.html"), "collage:", os.path.join(HERE, "collage.png"))


if __name__ == "__main__":
    main()
