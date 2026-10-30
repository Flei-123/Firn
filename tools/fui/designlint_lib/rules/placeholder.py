import re

from . import register, finding
from .. import classify

_SHAPES = set("■□▪▫●○◆◇▲▼◼◻▢⬛⬜◉◌▮▯◾◽")
_WORDS = {"icon", "todo", "placeholder", "img", "pic", "image", "[]", "[ ]", "...", "?", "!", "*"}
_SYMBOL_VARS = {"sym", "symbol", "glyph", "icon", "ico", "icon_text", "glyph_text"}
_FIELDS = {"icon", "glyph", "icon_text", "symbol"}


def placeholder_glyph(text, letters=True):
    s = text.strip()
    if not s:
        return False
    if all(c in _SHAPES for c in s):
        return True
    if s.lower() in _WORDS:
        return True
    return letters and len(s) == 1 and s.isascii() and s.isalnum()


@register("PLACEHOLDER_ICON", "placeholder glyph, letter or square used as an icon")
def check(ctx):
    out = []
    for call in ctx.calls:
        if classify.is_icon_taker(call):
            for text, line in call.strings():
                if placeholder_glyph(text, False):
                    out.append(finding(ctx, "PLACEHOLDER_ICON", line, text, "placeholder used as icon"))
        elif classify.is_button(call):
            for text, line in call.strings():
                s = text.strip()
                if len(s) == 1 and (s in _SHAPES or (s.isascii() and s.isalpha())) or s == "?":
                    out.append(finding(ctx, "PLACEHOLDER_ICON", line, text, "single glyph used as icon button"))
    for asg in ctx.assigns:
        if asg.name in _SYMBOL_VARS and (placeholder_glyph(asg.value) or asg.value.strip().lower() == "ok"):
            out.append(finding(ctx, "PLACEHOLDER_ICON", asg.line, asg.value, "placeholder text used as symbol"))
    for fld in ctx.fields:
        if fld.name in _FIELDS and placeholder_glyph(fld.value, False):
            out.append(finding(ctx, "PLACEHOLDER_ICON", fld.line, fld.value, "text used as icon field"))
    return out
