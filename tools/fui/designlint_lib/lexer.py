import re

IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
NUMBER = re.compile(r"0[xX][0-9a-fA-F_]+|[0-9][0-9_]*(?:\.[0-9]+)?")


class Token:
    __slots__ = ("kind", "text", "line")

    def __init__(self, kind, text, line):
        self.kind = kind
        self.text = text
        self.line = line


def tokenize(src):
    tokens = []
    i = 0
    n = len(src)
    line = 1
    while i < n:
        c = src[i]
        if c == "\n":
            line += 1
            i += 1
        elif c in " \t\r":
            i += 1
        elif src.startswith("//", i):
            while i < n and src[i] != "\n":
                i += 1
        elif src.startswith("/*", i):
            end = src.find("*/", i + 2)
            end = n if end < 0 else end + 2
            line += src.count("\n", i, end)
            i = end
        elif c == '"':
            j = i + 1
            buf = []
            while j < n and src[j] != '"':
                if src[j] == "\\" and j + 1 < n:
                    buf.append(src[j : j + 2])
                    j += 2
                    continue
                if src[j] == "\n":
                    line += 1
                buf.append(src[j])
                j += 1
            tokens.append(Token("str", "".join(buf), line))
            i = j + 1
        elif c == "'":
            j = i + 1
            while j < n and src[j] != "'" and src[j] != "\n":
                j += 2 if src[j] == "\\" else 1
            tokens.append(Token("chr", src[i + 1 : j], line))
            i = j + 1
        else:
            m = IDENT.match(src, i)
            if m:
                tokens.append(Token("id", m.group(), line))
                i = m.end()
                continue
            m = NUMBER.match(src, i)
            if m:
                tokens.append(Token("num", m.group(), line))
                i = m.end()
                continue
            tokens.append(Token("p", c, line))
            i += 1
    return tokens
