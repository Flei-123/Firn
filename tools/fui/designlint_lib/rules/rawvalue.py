import re

from . import register, finding
from .. import classify

_STRING_PATTERNS = [
    (re.compile(r"#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?\b"), "raw hex colour"),
    (re.compile(r"\b0[xX][0-9a-fA-F]{2,}"), "raw hex number"),
    (re.compile(r"\b(r|id|ptr|idx|addr|px|fd|pid|tid|ms|dpi|rc|err)=", re.I), "debug key=value"),
    (re.compile(r"\b\d+(\.\d+)?\s?px\b", re.I), "pixel value"),
    (re.compile(r"\b(debug|dbg|ptr|pid|tid)\b", re.I), "debug word"),
    (re.compile(r"%[0-9.]*[dsxuif]\b|\{\}"), "unformatted placeholder"),
]
_DEBUG_IDENT = re.compile(r"^(dbg|debug)([A-Z_]|$)|^(dbg|debug)[a-z_]", re.I)
_DEBUG_CALL = re.compile(r"(^|_)(hex|to_hex|fmt_hex|dump|debug|dbg)(_|$)")
_MSG_KEY = re.compile(r"^[a-z0-9_]+(\.[a-z0-9_]+)+$")
_PARSER_CALL = re.compile(r"parse|from_hex|hex_color|color")


@register("RAW_VALUE_IN_LABEL", "raw hex colour or debug value in a visible label")
def check(ctx):
    out = []
    for call in ctx.calls:
        if not classify.is_visible(call):
            continue
        for text, line in call.strings():
            if _MSG_KEY.match(text):
                continue
            for pattern, what in _STRING_PATTERNS:
                if pattern.search(text):
                    out.append(finding(ctx, "RAW_VALUE_IN_LABEL", line, text, what + " in visible text"))
                    break
        for i, tok in enumerate(call.span):
            if tok.kind != "id":
                continue
            nxt = call.span[i + 1] if i + 1 < len(call.span) else None
            is_call = nxt is not None and nxt.kind == "p" and nxt.text == "("
            if is_call and _DEBUG_CALL.search(tok.text) and not _PARSER_CALL.search(tok.text):
                out.append(finding(ctx, "RAW_VALUE_IN_LABEL", tok.line, tok.text + "()", "debug formatter in visible text"))
            elif not is_call and _DEBUG_IDENT.match(tok.text):
                out.append(finding(ctx, "RAW_VALUE_IN_LABEL", tok.line, tok.text, "debug value in visible text"))
    return out
