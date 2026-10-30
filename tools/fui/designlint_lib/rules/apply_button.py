import re

from . import register, finding
from .. import classify

_STRONG = re.compile(
    r"^(apply( changes| now| settings)?|uebernehmen|übernehmen|anwenden|(ae|ä)nderungen (uebernehmen|übernehmen|anwenden))$",
    re.I,
)
_WEAK = re.compile(r"^(ok|save changes|save settings|(ae|ä)nderungen speichern|confirm|bestaetigen|bestätigen)$", re.I)
_PATH = re.compile(r"setting|prefs|preferences|einstell", re.I)
_FUNC = re.compile(r"setting|prefs|preferences|einstell", re.I)
_IMPORTS = {"kitset", "ds_settings", "ds_tpl_settings"}


def settings_file(ctx):
    if _PATH.search(ctx.path):
        return True
    return any(t.kind == "id" and t.text in _IMPORTS for t in ctx.tokens)


@register("APPLY_BUTTON", "apply/save button in a settings page; settings must apply live")
def check(ctx):
    out = []
    file_ctx = settings_file(ctx)
    for call in ctx.calls:
        if not classify.is_button(call):
            continue
        if not (file_ctx or _FUNC.search(call.func)):
            continue
        for text, line, orig in classify.display_strings(call):
            literal = orig == text
            if _STRONG.match(text.strip()) or (literal and _WEAK.match(text.strip())):
                out.append(finding(ctx, "APPLY_BUTTON", line, orig, "settings must apply live; remove the button"))
    return out
