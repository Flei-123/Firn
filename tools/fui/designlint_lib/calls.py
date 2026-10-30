class Call:
    __slots__ = ("name", "last", "line", "args", "span", "func", "statics", "pos", "locals_")

    def __init__(self, name, line, args, span, func, statics, pos, locals_):
        self.name = name
        self.last = name.rsplit(".", 1)[-1]
        self.line = line
        self.args = args
        self.span = span
        self.func = func
        self.statics = statics
        self.pos = pos
        self.locals_ = locals_

    def _latest(self, name):
        entries = [e for e in self.locals_.get((self.func, name), ()) if e[0] < self.pos]
        if not entries:
            return []
        best = max(e[0] for e in entries)
        return [v for p, v in entries if p == best]

    def strings(self):
        out = []
        for t in self.span:
            if t.kind == "str":
                out.append((t.text, t.line))
            elif t.kind == "id" and t.text in self.statics:
                out.append((self.statics[t.text], t.line))
            elif t.kind == "id":
                for value in self._latest(t.text):
                    out.append((value, t.line))
        return out

    def idents(self):
        return [t.text for t in self.span if t.kind == "id"]


class Assign:
    __slots__ = ("name", "line", "value", "func")

    def __init__(self, name, line, value, func):
        self.name = name
        self.line = line
        self.value = value
        self.func = func


class Field:
    __slots__ = ("name", "line", "value", "func")

    def __init__(self, name, line, value, func):
        self.name = name
        self.line = line
        self.value = value
        self.func = func


def _match_paren(tokens, i):
    depth = 0
    j = i
    n = len(tokens)
    while j < n:
        t = tokens[j]
        if t.kind == "p":
            if t.text in "([{":
                depth += 1
            elif t.text in ")]}":
                depth -= 1
                if depth == 0:
                    return j
        j += 1
    return n - 1


def _split_args(span):
    args = []
    cur = []
    depth = 0
    for t in span:
        if t.kind == "p" and t.text in "([{":
            depth += 1
        elif t.kind == "p" and t.text in ")]}":
            depth -= 1
        if t.kind == "p" and t.text == "," and depth == 0:
            args.append(cur)
            cur = []
        else:
            cur.append(t)
    if cur:
        args.append(cur)
    return args


def _chain_start(tokens, i):
    j = i
    while j >= 2 and tokens[j - 1].kind == "p" and tokens[j - 1].text == "." and tokens[j - 2].kind == "id":
        j -= 2
    return j


def collect_statics(tokens):
    out = {}
    n = len(tokens)
    for i, t in enumerate(tokens):
        if t.kind == "id" and t.text == "static":
            j = i + 1
            if j < n and tokens[j].text == "mut":
                j += 1
            if j < n and tokens[j].kind == "id":
                name = tokens[j].text
                k = j + 1
                while k < n and not (tokens[k].kind == "p" and tokens[k].text == "="):
                    if tokens[k].kind == "id" and tokens[k].text in ("static", "fn"):
                        break
                    k += 1
                if k + 1 < n and tokens[k].text == "=" and tokens[k + 1].kind == "str":
                    out[name] = tokens[k + 1].text.replace("\\0", "")
    return out


def _static_refs(tokens, i, statics):
    out = []
    n = len(tokens)
    j = i
    while j < n and j < i + 12:
        t = tokens[j]
        if t.kind == "id" and t.text in statics and j > i and tokens[j - 1].text in ("&", "("):
            out.append(statics[t.text])
        if t.kind == "p" and t.text in ";}" or (t.kind == "id" and t.text in ("let", "var", "fn", "if", "else", "return")):
            break
        j += 1
    return out


def extract(tokens):
    calls = []
    fields = []
    assigns = []
    locals_ = {}
    func = ""
    n = len(tokens)
    statics = collect_statics(tokens)
    for i, t in enumerate(tokens):
        if t.kind == "id" and t.text == "fn" and i + 1 < n and tokens[i + 1].kind == "id":
            func = tokens[i + 1].text
            continue
        if t.kind == "id" and i + 1 < n and tokens[i + 1].kind == "p" and tokens[i + 1].text == "=" and not (i + 2 < n and tokens[i + 2].text == "="):
            refs = _static_refs(tokens, i + 2, statics)
            if refs:
                locals_.setdefault((func, t.text), []).extend((i, v) for v in refs)
                for value in refs:
                    assigns.append(Assign(t.text, t.line, value, func))
        if t.kind == "id" and i + 1 < n and tokens[i + 1].kind == "p" and tokens[i + 1].text == ":" and i + 3 < n:
            k = i + 2
            while k < n and k < i + 8 and not (tokens[k].kind == "p" and tokens[k].text == "="):
                k += 1
            if k < n and k < i + 8 and tokens[k].text == "=":
                refs = _static_refs(tokens, k + 1, statics)
                if refs:
                    locals_.setdefault((func, t.text), []).extend((i, v) for v in refs)
                    for value in refs:
                        assigns.append(Assign(t.text, t.line, value, func))
        if t.kind == "id" and i + 1 < n and tokens[i + 1].kind == "p" and tokens[i + 1].text == "(":
            if i > 0 and tokens[i - 1].kind == "id" and tokens[i - 1].text == "fn":
                continue
            start = _chain_start(tokens, i)
            name = ".".join(x.text for x in tokens[start : i + 1] if x.kind == "id")
            end = _match_paren(tokens, i + 1)
            span = tokens[i + 2 : end]
            calls.append(Call(name, t.line, _split_args(span), span, func, statics, i, locals_))
        if t.kind == "id" and i + 2 < n and tokens[i + 1].kind == "p" and tokens[i + 1].text == ":":
            nxt = tokens[i + 2]
            if nxt.kind in ("str", "chr"):
                fields.append(Field(t.text, t.line, nxt.text, func))
    return calls, fields, assigns
