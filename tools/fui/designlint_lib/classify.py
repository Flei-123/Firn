import re

_TOGGLE = re.compile(r"^(row_)?(toggle|checkbox|check|switch|radio)(_|$)")
_MEASURE = re.compile(r"(_width|_height|_w|_h|_x|_y|_size)$")
_DATA_MODULES = ("kitdata", "kitfmt", "jsontree", "json")
_TEXT_NAMES = {"text", "heading_text", "draw_text", "label_text", "title_text", "title", "subtitle", "caption", "status", "toast", "infobar"}


def is_icon_button(call):
    last = call.last
    return "icon_button" in last or "iconbtn" in last or "icon_btn" in last


def is_button(call):
    last = call.last
    if _MEASURE.search(last) or last.startswith(("set_", "is_", "on_", "get_")):
        return False
    if is_icon_button(call):
        return False
    return "button" in last or "btn" in last


def is_toggle(call):
    last = call.last
    if last.startswith(("set_", "is_")) or last.endswith("ed"):
        return False
    return bool(_TOGGLE.match(last))


def is_label(call):
    last = call.last
    if _MEASURE.search(last) or last.startswith(("set_", "is_", "get_", "on_")):
        return False
    module = call.name.rsplit(".", 2)[0] if "." in call.name else ""
    if module.split(".")[-1] in _DATA_MODULES:
        return False
    return "label" in last or last in _TEXT_NAMES


def is_visible(call):
    return is_label(call) or is_button(call) or is_icon_button(call) or is_toggle(call)


def is_icon_taker(call):
    last = call.last
    return ("icon" in last or "glyph" in last) and not _MEASURE.search(last) and not last.startswith(("set_", "is_", "get_"))


_KEY = re.compile(r"^[a-z0-9_]+(\.[a-z0-9_]+)+$")


def is_message_key(text):
    return bool(_KEY.match(text))


def display_strings(call):
    out = []
    for text, line in call.strings():
        if is_message_key(text):
            out.append((text.rsplit(".", 1)[-1].replace("_", " "), line, text))
        else:
            out.append((text, line, text))
    return out
