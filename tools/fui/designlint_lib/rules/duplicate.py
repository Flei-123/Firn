import re

from . import register, finding
from .. import classify

STATE_WORDS = {
    "on", "off", "an", "aus", "ein", "enabled", "disabled", "aktiv", "inaktiv", "active", "inactive",
    "connected", "disconnected", "verbunden", "getrennt", "online", "offline", "muted", "unmuted", "stumm",
    "running", "stopped", "paused", "locked", "unlocked", "yes", "no", "ja", "nein",
}
_STATE_IDENT = re.compile(r"state|status", re.I)


def state_word(text):
    return text.strip().rstrip(":").strip().lower() in STATE_WORDS


def _by_function(calls):
    groups = {}
    for call in calls:
        groups.setdefault(call.func, []).append(call)
    return groups


def _last_arg_key(call):
    if not call.args:
        return None
    arg = call.args[-1]
    if not any(t.kind == "id" and _STATE_IDENT.search(t.text) for t in arg):
        return None
    return "".join(t.text for t in arg)


def _branch_between(ctx, a, b):
    return any(t.kind == "id" and t.text in ("return", "else") for t in ctx.tokens[a:b])


@register("DUPLICATE_STATE", "same state shown twice (toggle plus On/Off text, status text plus state button)")
def check(ctx):
    out = []
    for func, calls in _by_function(ctx.calls).items():
        displays = []
        toggles = []
        for call in calls:
            if classify.is_toggle(call):
                toggles.append(call)
            if classify.is_label(call) or classify.is_button(call):
                words = [(t, ln) for t, ln in call.strings() if state_word(t)]
                if words:
                    displays.append((call, words[0]))
        if toggles:
            for call, (text, line) in displays:
                if not classify.is_label(call):
                    continue
                out.append(finding(ctx, "DUPLICATE_STATE", line, text, "state text next to a toggle that already shows it"))
        elif len(displays) >= 2:
            for call, (text, line) in displays[1:]:
                out.append(finding(ctx, "DUPLICATE_STATE", line, text, "second display of the same state"))
        seen = {}
        for call in calls:
            if not (classify.is_label(call) or classify.is_button(call)):
                continue
            key = _last_arg_key(call)
            if key is None:
                continue
            if key in seen and not _branch_between(ctx, seen[key], call.pos):
                out.append(finding(ctx, "DUPLICATE_STATE", call.line, key, "same state expression displayed twice"))
            else:
                seen[key] = call.pos
    return out
