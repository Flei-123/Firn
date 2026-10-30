import re

from . import register, finding
from .. import classify

ARROWS = "<>‹›«»←→↑↓^◀▶◄►⟵⟶˂˃"
_ONLY_ARROWS = re.compile(r"^[" + re.escape(ARROWS) + r"\-\s]+$")
_STRONG = {"back", "zurueck", "zurück", "up", "hoch", "forward", "vor", "parent", "go back", "go up", "one level up"}
_SOFT = {"next", "weiter", "prev", "previous", "continue", "vorher", "naechste", "nächste", "vorherige", "more"}
_DECOR = re.compile(r"^[" + re.escape(ARROWS) + r"\-\s]*([A-Za-zäöüÄÖÜ ]+?)[" + re.escape(ARROWS) + r"\-\s]*$")


def nav_label(text):
    s = text.strip()
    if not s:
        return False
    if _ONLY_ARROWS.match(s) and any(c in ARROWS for c in s):
        return True
    low = s.lower()
    if low in _STRONG:
        return True
    m = _DECOR.match(s)
    if m and any(c in ARROWS for c in s):
        word = m.group(1).strip().lower()
        return word in _STRONG or word in _SOFT
    return False


@register("NAV_TEXT_BUTTON", "text button used for navigation instead of IconButton")
def check(ctx):
    out = []
    for call in ctx.calls:
        if not classify.is_button(call):
            continue
        for text, line, orig in classify.display_strings(call):
            if nav_label(text):
                out.append(finding(ctx, "NAV_TEXT_BUTTON", line, orig, "navigation text button; use IconButton"))
    return out
