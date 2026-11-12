import re

RECORD_RE = re.compile(r"^(\d*)([\^*=~@&])(.*)$")


class MiSyntaxError(ValueError):
    pass


def parse_cstring(text, pos):
    out = []
    pos += 1
    escapes = {"n": "\n", "t": "\t", '"': '"', "\\": "\\", "r": "\r"}
    while pos < len(text):
        ch = text[pos]
        if ch == '"':
            return "".join(out), pos + 1
        if ch == "\\" and pos + 1 < len(text):
            out.append(escapes.get(text[pos + 1], text[pos + 1]))
            pos += 2
            continue
        out.append(ch)
        pos += 1
    raise MiSyntaxError("unterminated string")


def parse_name(text, pos):
    end = text.index("=", pos)
    return text[pos:end], end + 1


def parse_value(text, pos):
    if pos >= len(text):
        raise MiSyntaxError("unexpected end")
    ch = text[pos]
    if ch == '"':
        return parse_cstring(text, pos)
    if ch == "{":
        return parse_tuple(text, pos)
    if ch == "[":
        return parse_list(text, pos)
    raise MiSyntaxError("bad value at %d" % pos)


def parse_tuple(text, pos):
    result = {}
    pos += 1
    while text[pos] != "}":
        name, pos = parse_name(text, pos)
        value, pos = parse_value(text, pos)
        result[name] = value
        if text[pos] == ",":
            pos += 1
    return result, pos + 1


def parse_list(text, pos):
    result = []
    pos += 1
    while text[pos] != "]":
        if text[pos] in '"{[':
            value, pos = parse_value(text, pos)
        else:
            _, pos = parse_name(text, pos)
            value, pos = parse_value(text, pos)
        result.append(value)
        if text[pos] == ",":
            pos += 1
    return result, pos + 1


def parse_results(text):
    result = {}
    pos = 0
    while pos < len(text):
        if text[pos] == ",":
            pos += 1
            continue
        name, pos = parse_name(text, pos)
        value, pos = parse_value(text, pos)
        result[name] = value
    return result


def parse_record(line):
    match = RECORD_RE.match(line)
    if not match:
        return None
    token, kind, rest = match.groups()
    if kind in "~@&":
        text, _ = parse_cstring(rest, 0)
        return {"kind": kind, "token": token, "text": text}
    klass, _, payload = rest.partition(",")
    return {"kind": kind, "token": token, "class": klass, "data": parse_results(payload)}
