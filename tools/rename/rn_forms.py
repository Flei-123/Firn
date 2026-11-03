import re

NAME_RE = re.compile(r"^[a-z][a-z0-9_]*$")
ESCAPES = re.compile(r"\\[ntr]")
KNOWN_SUFFIXES = ("c", "lib", "fmt", "lint", "cam", "chat", "mc", "app", "run", "pkg", "bild", "metric", "frames", "tag", "exports", "gpu", "dom", "web", "notes", "faces", "audio")


class Forms:
    def __init__(self, old, new):
        self.old = old
        self.new = new
        self.pairs = [(old, new), (old.capitalize(), new.capitalize()), (old.upper(), new.upper())]
        alts = "|".join(re.escape(a) for a, _ in self.pairs)
        self.pattern = re.compile(alts)
        self.mixed = re.compile(re.escape(old), re.IGNORECASE)
        self.token = re.compile(r"[A-Za-z0-9_]*(?:%s)[A-Za-z0-9_]*" % alts)
        self.table = dict(self.pairs)
        o, u, c = re.escape(old), re.escape(old.upper()), re.escape(old.capitalize())
        self.categories = (
            ("env_variable", re.compile(r"^[A-Z0-9_]*%s[A-Z0-9_]*$" % u)),
            ("internal_symbol", re.compile(r"^_+%s" % o)),
            ("project_file", re.compile(r"^%s\.(package|lock|toolchain)$" % o)),
            ("compiler_name", re.compile(r"^%sc[0-9]?$" % o)),
            ("lowercase_word", re.compile(o)),
            ("capitalized_word", re.compile(c)),
        )

    def replace(self, text):
        return self.pattern.sub(lambda m: self.table[m.group(0)], text)

    def count(self, text):
        return len(self.pattern.findall(text))

    def odd_case(self, text):
        return [m.group(0) for m in self.mixed.finditer(text) if m.group(0) not in self.table]

    def tokens(self, text):
        return self.token.findall(ESCAPES.sub(" ", text))

    def classify(self, token):
        for name, rx in self.categories:
            if rx.search(token):
                return name
        return "other"

    def edge_reason(self, token):
        for a, _ in self.pairs:
            i = token.find(a)
            while i >= 0:
                if i > 0 and token[i - 1].isalpha():
                    return "letter before name"
                after = token[i + len(a):]
                if after and after[0].islower() and not after.startswith(KNOWN_SUFFIXES):
                    return "unknown suffix"
                i = token.find(a, i + 1)
        return None
