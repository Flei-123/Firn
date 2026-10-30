import re

from . import register, finding
from .. import classify

_TEXT = [
    re.compile(r"\b(next|prev(ious)?|first|last|n(ä|ae)chste|vorherige|letzte|erste)\s+(page|seite)\b", re.I),
    re.compile(r"^\s*(page|seite)\s*\d*\s*$", re.I),
    re.compile(r"^\s*(page|seite)\s*\d+(\s*(/|of|von)\s*\d+)?\s*$", re.I),
    re.compile(r"^\s*\d+\s*(/|of|von)\s*\d+\s*$", re.I),
    re.compile(r"^\s*(page|seite)\s*%", re.I),
]
_CALLBACK = re.compile(r"(^|_)(next|prev|previous)_?(page|pg)(_|$)|(^|_)page_?(next|prev|up|down|forward|back|index)(_|$)|(^|_)(goto|flip)_?page(_|$)", re.I)


@register("PAGING", "page-by-page navigation instead of scrolling")
def check(ctx):
    out = []
    for call in ctx.calls:
        if not classify.is_visible(call):
            continue
        for text, line, orig in classify.display_strings(call):
            if any(p.search(text) for p in _TEXT):
                out.append(finding(ctx, "PAGING", line, orig, "paging control; use scrolling"))
        if classify.is_button(call) or classify.is_icon_button(call):
            for tok in call.span:
                if tok.kind == "id" and _CALLBACK.search(tok.text):
                    out.append(finding(ctx, "PAGING", tok.line, tok.text, "paging callback; use scrolling"))
    return out
