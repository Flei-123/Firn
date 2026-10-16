"""tools/mail/mailmodel.py -- a small MIME model for the lib/mail e2e run.

Builds random messages from a TREE (so the ground truth for ENVELOPE, BODYSTRUCTURE and every
BODY[section] is known exactly, not parsed back): headers, multipart/alternative/related/mixed,
text parts, attachments (base64 / quoted-printable / 7bit), nested message/rfc822, 8-bit headers.
`dump_expected` writes the text format that tools/mail/e2e_main.fi prints for the same mailbox.
"""
import base64
import hashlib
import quopri
import random

CRLF = b"\r\n"
SPECIALS = set('()<>@,;:\\".[]')


def hx(b):
    if b is None:
        return "-"
    if isinstance(b, str):
        b = b.encode("utf-8")
    return b.hex() if b else "."


def phrase(name):
    """header form of a display name"""
    if any(c in SPECIALS for c in name) or name != name.strip():
        return '"' + name.replace("\\", "\\\\").replace('"', '\\"') + '"'
    return name


def fmt_addr(a):
    """header form of an address entry: ('a', name, mbox, host) or ('g', gname, [members])"""
    if a[0] == "g":
        return a[1] + ": " + ", ".join(fmt_addr(m) for m in a[2]) + ";"
    _, name, mbox, host = a
    if name is None:
        return "%s@%s" % (mbox, host)
    return "%s <%s@%s>" % (phrase(name), mbox, host)


def canon_addrs(lst):
    """what imap_addr_format prints for the ENVELOPE of this list"""
    if not lst:
        return None
    out = []
    first = True
    for a in lst:
        if a[0] == "g":
            if not first:
                out.append(", ")
            out.append(a[1] + ":")
            first = True
            for m in a[2]:
                if not first:
                    out.append(", ")
                out.append(canon_single(m))
                first = False
            out.append(";")
            first = False
        else:
            if not first:
                out.append(", ")
            out.append(canon_single(a))
            first = False
    return "".join(out)


def canon_single(a):
    _, name, mbox, host = a
    if name is not None:
        return "%s <%s@%s>" % (name, mbox, host)
    return "%s@%s" % (mbox, host)


class Part:
    def __init__(self, typ, sub, params=None, enc="7bit", body=b"", children=None, disp=None,
                 filename=None, cid=None, desc=None, inner=None, boundary=None, preamble=b""):
        self.typ, self.sub = typ, sub
        self.params = params or []
        self.enc, self.body_raw = enc, body
        self.children = children or []
        self.disp, self.filename, self.cid, self.desc = disp, filename, cid, desc
        self.inner = inner
        self.boundary = boundary
        self.preamble = preamble

    def is_multi(self):
        return self.typ == "multipart"

    def header_lines(self):
        ct = "%s/%s" % (self.typ, self.sub)
        params = list(self.params)
        if self.is_multi():
            params.append(("boundary", self.boundary))
        for k, v in params:
            ct += '; %s="%s"' % (k, v)
        h = [b"Content-Type: " + ct.encode()]
        if not self.is_multi():
            h.append(b"Content-Transfer-Encoding: " + self.enc.encode())
        if self.cid:
            h.append(b"Content-ID: " + self.cid.encode())
        if self.desc:
            h.append(b"Content-Description: " + self.desc.encode())
        if self.disp:
            d = self.disp
            if self.filename:
                d += '; filename="%s"' % self.filename
            h.append(b"Content-Disposition: " + d.encode())
        return h

    def body(self):
        if self.is_multi():
            out = self.preamble
            for c in self.children:
                out += b"--" + self.boundary.encode() + CRLF + c.full() + CRLF
            return out + b"--" + self.boundary.encode() + b"--" + CRLF
        if self.inner is not None:
            return self.inner.raw()
        return self.body_raw

    def mime(self):
        return CRLF.join(self.header_lines()) + CRLF + CRLF

    def full(self):
        return self.mime() + self.body()

    def walk(self, path="", parent=None):
        """document order; message/rfc822 is a leaf. yields (path, part, parent)"""
        yield path, self, parent
        if self.is_multi():
            for i, c in enumerate(self.children, 1):
                yield from c.walk((path + "." if path else "") + str(i), self)


class Msg:
    def __init__(self, hdrs, env, root):
        self.hdrs = hdrs  # list of bytes lines (without CRLF)
        self.env = env  # dict: date subject from sender reply_to to cc bcc in_reply_to message_id
        self.root = root
        self.uid = 0
        self.flags = set()

    def head(self):
        top = list(self.hdrs)
        top.append(b"MIME-Version: 1.0")
        top += self.root.header_lines()
        return CRLF.join(top) + CRLF + CRLF

    def raw(self):
        return self.head() + self.root.body()

    def text(self):
        return self.root.body()

    def section(self, sec):
        """octets of BODY[sec] or None. sec: '' HEADER TEXT n n.m n.MIME"""
        if sec == "":
            return self.raw()
        if sec == "HEADER":
            return self.head()
        if sec == "TEXT":
            return self.text()
        toks = sec.split(".")
        mime = False
        if toks[-1] == "MIME":
            mime = True
            toks = toks[:-1]
        if not self.root.is_multi():
            if toks == ["1"]:
                return self.root.mime() if mime else self.root.body()
            return None
        node = self.root
        for t in toks:
            if not t.isdigit() or not node.is_multi():
                return None
            k = int(t)
            if k < 1 or k > len(node.children):
                return None
            node = node.children[k - 1]
        return node.mime() if mime else node.body()


# ---------------------------------------------------------------- random generation

WORDS = ["alpha", "beta", "gamma", "delta", "mail", "test", "report", "lunch", "invoice", "hello",
         "world", "plan", "draft", "q3", "meeting", "x", "notes"]
NAMES = ["Al Ice", "Bob", "Carol D.", "Dave \"D\" Doe", "Smith, Jo", "Zoe", "Ünal Çelik", "A\\B", "Eve"]
HOSTS = ["ex.org", "ex.net", "mail.example.com", "localhost", "a-b.io"]
MBOX = ["al", "bob", "carol.d", "dave", "jo+tag", "zoe_x", "eve"]


def rword(r):
    return r.choice(WORDS)


def raddr(r):
    name = r.choice(NAMES) if r.random() < 0.6 else None
    return ("a", name, r.choice(MBOX), r.choice(HOSTS))


def raddrs(r, maxn=3, groups=True):
    n = r.randint(1, maxn)
    out = []
    for _ in range(n):
        if groups and r.random() < 0.12:
            out.append(("g", r.choice(["Team", "Undisclosed", "Fam"]), [raddr(r) for _ in range(r.randint(0, 2))]))
        else:
            out.append(raddr(r))
    return out


def rtext(r, utf8=False):
    lines = []
    for _ in range(r.randint(0, 6)):
        ln = " ".join(rword(r) for _ in range(r.randint(0, 9)))
        if utf8 and r.random() < 0.3:
            ln += " äöü€"
        lines.append(ln)
    return "\r\n".join(lines).encode("utf-8") + (b"\r\n" if lines and r.random() < 0.7 else b"")


def rpart(r, depth):
    k = r.random()
    if depth < 3 and k < 0.35:
        sub = r.choice(["mixed", "alternative", "related"])
        bnd = "=_%s_%d" % (rword(r), r.randint(0, 99999))
        ch = [rpart(r, depth + 1) for _ in range(r.randint(1, 4))]
        return Part("multipart", sub, children=ch, boundary=bnd, preamble=r.choice([b"", b"This is a multi-part message.\r\n"]))
    if depth < 2 and k < 0.42:
        return Part("message", "rfc822", enc="7bit", inner=rmsg(r, depth + 1), disp=r.choice([None, "attachment"]),
                    filename=None)
    if k < 0.75:
        cs = r.choice(["utf-8", "us-ascii", "iso-8859-1"])
        enc = r.choice(["7bit", "quoted-printable", "base64", "8bit"])
        utf8 = cs == "utf-8" and enc in ("8bit",)
        raw = rtext(r, utf8)
        if enc == "quoted-printable":
            body = quopri.encodestring(raw).replace(b"\n", b"\r\n").replace(b"\r\r\n", b"\r\n")
        elif enc == "base64":
            s = base64.b64encode(raw)
            body = CRLF.join(s[i:i + 76] for i in range(0, len(s), 76)) + (CRLF if s else b"")
        else:
            body = raw
        return Part("text", r.choice(["plain", "html"]), params=[("charset", cs)], enc=enc, body=body)
    payload = bytes(r.randint(0, 255) for _ in range(r.randint(0, 300)))
    s = base64.b64encode(payload)
    body = CRLF.join(s[i:i + 76] for i in range(0, len(s), 76)) + (CRLF if s else b"")
    name = r.choice(["a.bin", "report.pdf", "pic.png", "x y.txt", "q\\u.dat"]) if r.random() < 0.8 else None
    typ, sub = r.choice([("application", "octet-stream"), ("application", "pdf"), ("image", "png")])
    p = Part(typ, sub, params=[("name", name)] if name and r.random() < 0.5 else [], enc="base64", body=body,
             disp=r.choice([None, "attachment", "inline"]), filename=name, cid=("<c%d@ex.org>" % r.randint(0, 999)) if r.random() < 0.3 else None,
             desc=rword(r) if r.random() < 0.2 else None)
    if p.disp is None:
        p.filename = None
    return p


def rmsg(r, depth=0, utf8_headers=None):
    env = {}
    hdrs = []
    if utf8_headers is None:
        utf8_headers = r.random() < 0.3
    date = "Mon, %d Jan 2024 %02d:%02d:%02d +0000" % (r.randint(1, 28), r.randint(0, 23), r.randint(0, 59), r.randint(0, 59))
    env["date"] = date
    hdrs.append(("Date: " + date).encode())
    subj = " ".join(rword(r) for _ in range(r.randint(0, 6)))
    if r.random() < 0.3:
        subj += ' "quoted" back\\slash'
    if utf8_headers and r.random() < 0.7:
        subj += " Grüße €"
    if r.random() < 0.05:
        subj = None
    env["subject"] = subj
    if subj is not None:
        hdrs.append(b"Subject: " + subj.encode("utf-8"))
    for key, hname, maxn in [("from", "From", 2), ("sender", "Sender", 1), ("reply_to", "Reply-To", 2),
                             ("to", "To", 4), ("cc", "Cc", 3), ("bcc", "Bcc", 2)]:
        if key == "from" or r.random() < (0.7 if key in ("to",) else 0.3):
            lst = raddrs(r, maxn, groups=(key in ("to", "cc")))
            if not utf8_headers:
                lst = [fix_ascii(a) for a in lst]
            env[key] = lst
            hdrs.append(("%s: %s" % (hname, ", ".join(fmt_addr(a) for a in lst))).encode("utf-8"))
        else:
            env[key] = None
    # ENVELOPE: sender and reply-to default to from
    for key in ("sender", "reply_to"):
        if env[key] is None:
            env[key] = env["from"]
    env["in_reply_to"] = "<r%d@ex.org>" % r.randint(0, 9999) if r.random() < 0.3 else None
    if env["in_reply_to"]:
        hdrs.append(b"In-Reply-To: " + env["in_reply_to"].encode())
    env["message_id"] = "<m%d.%d@ex.org>" % (r.randint(0, 99999), depth) if r.random() < 0.9 else None
    if env["message_id"]:
        hdrs.append(b"Message-ID: " + env["message_id"].encode())
    root = rpart(r, depth)
    return Msg(hdrs, env, root)


def fix_ascii(a):
    if a[0] == "g":
        return ("g", a[1], [fix_ascii(m) for m in a[2]])
    name = a[1]
    if name is not None:
        name = name.encode("ascii", "ignore").decode() or "Anon"
    return ("a", name, a[2], a[3])


# ---------------------------------------------------------------- expected dump

def hdr_text(msg):
    return None


def env_fields(msg):
    e = msg.env
    return [e["date"], e["subject"], canon_addrs(e["from"]), canon_addrs(e["sender"]), canon_addrs(e["reply_to"]),
            canon_addrs(e["to"]), canon_addrs(e["cc"]), canon_addrs(e["bcc"]), e["in_reply_to"], e["message_id"]]


def flags_text(flags):
    order = ["\\Seen", "\\Answered", "\\Flagged", "\\Deleted", "\\Draft"]
    got = [f for f in order if f in flags]
    return ",".join(got) if got else "-"


def dump_expected(msgs, with_bodies=True):
    """lines like the driver's `dump` command prints them (seq numbers follow the list order)"""
    out = []
    for seq, m in enumerate(msgs, 1):
        raw = m.raw()
        out.append("M %d %d %s %d" % (seq, m.uid, flags_text(m.flags), len(raw)))
        out.append("E %d %s" % (seq, " ".join(hx(x) for x in env_fields(m))))
        for k, (path, p, parent) in enumerate(m.root.walk(), 0):
            ipath = path
            if not p.is_multi() and path == "":
                ipath = "1"
            if p.is_multi():
                out.append("P %d %d %s multipart %s - 0 - - %s %s" % (seq, k, ipath or ".", p.sub.lower(),
                                                                       disp_type(p), hx(p.boundary)))
            else:
                fname = p.filename
                if fname is None:
                    for kk, vv in p.params:
                        if kk.lower() == "name":
                            fname = vv
                cs = None
                for kk, vv in p.params:
                    if kk.lower() == "charset":
                        cs = vv
                out.append("P %d %d %s %s %s %s %d %s %s %s %s" % (
                    seq, k, ipath, p.typ.lower(), p.sub.lower(), p.enc.lower(), len(p.body()), hx(fname),
                    hx(p.cid), disp_type(p), hx(cs)))
        if with_bodies:
            secs = [("FULL", raw), ("HEADER", m.head()), ("TEXT", m.text())]
            for path, p, parent in m.root.walk():
                if not p.is_multi():
                    ipath = path if path else "1"
                    secs.append((ipath, p.body()))
            for name, data in secs:
                out.append("S %d %s %d %s" % (seq, name, len(data), hashlib.sha1(data).hexdigest()))
    return out


def disp_type(p):
    return hx(p.disp.lower()) if p.disp else "-"
