"""tools/mail/fake_mail.py -- fake SMTP and IMAP servers (asyncio, localhost) for the lib/mail e2e run.

Not a real mail server: just enough protocol for the e2e test, written from the RFCs and checked
against Python's own clients (smtplib / imaplib run in tools/mail/e2e.py too, so a bug in the fake
server shows up as a difference between Python's client and ours).
  SMTP: EHLO/HELO, STARTTLS (or implicit TLS), AUTH PLAIN/LOGIN, MAIL (SIZE=, BODY=), RCPT, DATA,
        RSET, NOOP, QUIT. Knobs: reject recipients "bad*", 451 for sender "tempfail*", `evil` injects
        bytes behind the STARTTLS 220, `drop_after_data` closes without the final answer.
  IMAP: CAPABILITY, STARTTLS, LOGIN, AUTHENTICATE PLAIN (SASL-IR or continuation), LIST/LSUB, SELECT,
        EXAMINE, STATUS, FETCH/UID FETCH (UID FLAGS RFC822.SIZE INTERNALDATE ENVELOPE BODYSTRUCTURE
        BODY[...] BODY.PEEK[...], partial), SEARCH/UID SEARCH, STORE, EXPUNGE/UID EXPUNGE, COPY, NOOP,
        CLOSE, IDLE (a mail arrives while idling), LOGOUT, NAMESPACE.
"""
import asyncio
import base64
import random
import re
import ssl

import mailmodel as mm

CRLF = b"\r\n"


# ------------------------------------------------------------------ modified UTF-7

def utf7_encode(s):
    out, buf = [], []

    def flush():
        if buf:
            b = "".join(buf).encode("utf-16-be")
            out.append("&" + base64.b64encode(b).decode().rstrip("=").replace("/", ",") + "-")
            buf.clear()

    for ch in s:
        if 0x20 <= ord(ch) <= 0x7E:
            flush()
            out.append("&-" if ch == "&" else ch)
        else:
            buf.append(ch)
    flush()
    return "".join(out)


def utf7_decode(s):
    def rep(m):
        if m.group(1) == "":
            return "&"
        b = m.group(1).replace(",", "/")
        b += "=" * (-len(b) % 4)
        return base64.b64decode(b).decode("utf-16-be")

    return re.sub(r"&([^-]*)-", rep, s)


# ------------------------------------------------------------------ IMAP wire values

def istr(v, lit=False):
    """NIL / quoted string / literal (octets outside 32..126 need a literal)"""
    if v is None:
        return b"NIL"
    if isinstance(v, str):
        v = v.encode("utf-8")
    if lit or any(c < 32 or c > 126 for c in v):
        return b"{%d}\r\n" % len(v) + v
    return b'"' + v.replace(b"\\", b"\\\\").replace(b'"', b'\\"') + b'"'


def addr_list(lst, r):
    if not lst:
        return b"NIL"
    out = []
    for a in lst:
        if a[0] == "g":
            out.append(b"(NIL NIL " + istr(a[1]) + b" NIL)")
            for m in a[2]:
                out.append(b"(" + istr(m[1], r.random() < 0.1) + b" NIL " + istr(m[2]) + b" " + istr(m[3]) + b")")
            out.append(b"(NIL NIL NIL NIL)")
        else:
            out.append(b"(" + istr(a[1], r.random() < 0.1) + b" NIL " + istr(a[2]) + b" " + istr(a[3]) + b")")
    return b"(" + b"".join(out) + b")"


def envelope(m, r):
    e = m.env
    return (b"(" + istr(e["date"]) + b" " + istr(e["subject"], r.random() < 0.1) + b" "
            + addr_list(e["from"], r) + b" " + addr_list(e["sender"], r) + b" " + addr_list(e["reply_to"], r) + b" "
            + addr_list(e["to"], r) + b" " + addr_list(e["cc"], r) + b" " + addr_list(e["bcc"], r) + b" "
            + istr(e["in_reply_to"]) + b" " + istr(e["message_id"]) + b")")


def params_wire(params):
    if not params:
        return b"NIL"
    return b"(" + b" ".join(istr(k.upper()) + b" " + istr(v) for k, v in params) + b")"


def disp_wire(p):
    if not p.disp:
        return b"NIL"
    pl = b"NIL" if not p.filename else b"(" + istr("FILENAME") + b" " + istr(p.filename) + b")"
    return b"(" + istr(p.disp.upper()) + b" " + pl + b")"


def bodystructure(p, r, ext=True):
    if p.is_multi():
        ch = b"".join(bodystructure(c, r, ext) for c in p.children)
        out = b"(" + ch + b" " + istr(p.sub.upper())
        if ext:
            out += b" " + params_wire([("boundary", p.boundary)]) + b" NIL NIL NIL"
        return out + b")"
    body = p.body()
    out = (b"(" + istr(p.typ.upper()) + b" " + istr(p.sub.upper()) + b" " + params_wire(p.params) + b" "
           + istr(p.cid) + b" " + istr(p.desc) + b" " + istr(p.enc.upper()) + b" " + b"%d" % len(body))
    if p.typ == "message" and p.sub == "rfc822" and p.inner is not None:
        out += b" " + envelope(p.inner, r) + b" " + bodystructure(p.inner.root, r, ext) + b" %d" % body.count(b"\n")
    elif p.typ == "text":
        out += b" %d" % body.count(b"\n")
    if ext:
        out += b" NIL " + disp_wire(p) + b" NIL NIL"
    return out + b")"


# ------------------------------------------------------------------ IMAP tokenizer / sets

def tokenize(s):
    """arguments of a command -> list of str / bytes-free tokens; '(' ')' lists become Python lists.
    Quoted strings come back as ('q', text); atoms (with a [...] section) as str."""
    pos = 0
    stack = [[]]
    n = len(s)
    while pos < n:
        ch = s[pos]
        if ch == " ":
            pos += 1
        elif ch == "(":
            stack.append([])
            pos += 1
        elif ch == ")":
            top = stack.pop()
            stack[-1].append(top)
            pos += 1
        elif ch == '"':
            j = pos + 1
            val = []
            while s[j] != '"':
                if s[j] == "\\":
                    j += 1
                val.append(s[j])
                j += 1
            stack[-1].append(("q", "".join(val)))
            pos = j + 1
        else:
            j = pos
            while j < n and s[j] not in ' ()"':
                if s[j] == "[":
                    while s[j] != "]":
                        j += 1
                j += 1
            stack[-1].append(s[pos:j])
            pos = j
    return stack[0]


def tv(t):
    return t[1] if isinstance(t, tuple) else t


def parse_set(spec, top):
    out = []
    for part in spec.split(","):
        if ":" in part:
            a, b = part.split(":")
            a = top if a == "*" else int(a)
            b = top if b == "*" else int(b)
            lo, hi = min(a, b), max(a, b)
            out.extend(range(lo, hi + 1))
        else:
            out.append(top if part == "*" else int(part))
    return out


class Mailbox:
    def __init__(self):
        self.msgs = []
        self.uidvalidity = 1000
        self.uidnext = 1

    def add(self, m, flags=()):
        m.uid = self.uidnext
        self.uidnext += 1
        m.flags = set(flags)
        self.msgs.append(m)
        return m


class ImapCfg:
    def __init__(self, tls="starttls", sasl_ir=True, require_tls=False, evil=False, user="bob", pw="p\"a\\ss wd",
                 inject_idle=0.15, seed=1):
        self.tls, self.sasl_ir, self.require_tls, self.evil = tls, sasl_ir, require_tls, evil
        self.user, self.pw = user, pw
        self.inject_idle = inject_idle
        self.rand = random.Random(seed)
        self.boxes = {"INBOX": Mailbox()}
        self.ssl_ctx = None
        self.log = []
        self.idle_new = 0


FLAGS = ["\\Seen", "\\Answered", "\\Flagged", "\\Deleted", "\\Draft"]


def flags_wire(fl):
    return "(" + " ".join(f for f in FLAGS + sorted(x for x in fl if x not in FLAGS) if f in fl) + ")"


class ImapConn:
    def __init__(self, cfg, reader, writer):
        self.cfg, self.r, self.w = cfg, reader, writer
        self.tls = cfg.tls == "implicit"
        self.authed = False
        self.sel = None
        self.ro = False

    async def send(self, b):
        if isinstance(b, str):
            b = b.encode("utf-8")
        self.w.write(b)
        await self.w.drain()

    def caps(self):
        c = ["IMAP4rev1", "IDLE", "UIDPLUS", "LITERAL+"]
        if self.cfg.sasl_ir:
            c.append("SASL-IR")
        if self.cfg.tls == "starttls" and not self.tls:
            c.append("STARTTLS")
        if self.cfg.require_tls and not self.tls:
            c.append("LOGINDISABLED")
        else:
            c.append("AUTH=PLAIN")
        return c

    async def run(self):
        try:
            await self.send("* OK [CAPABILITY %s] fake imap ready\r\n" % " ".join(self.caps()))
            while True:
                line = await self.r.readline()
                if not line:
                    return
                line = line.rstrip(b"\r\n").decode("utf-8", "replace")
                if not line:
                    continue
                if not await self.command(line):
                    return
        except (ConnectionError, asyncio.IncompleteReadError, ssl.SSLError):
            return
        finally:
            try:
                self.w.close()
            except Exception:
                pass

    async def command(self, line):
        parts = line.split(" ", 2)
        if len(parts) < 2:
            await self.send("* BAD missing command\r\n")
            return True
        tag, cmd = parts[0], parts[1].upper()
        rest = parts[2] if len(parts) > 2 else ""
        self.cfg.log.append(line)
        uid = False
        if cmd == "UID":
            sub = rest.split(" ", 1)
            cmd = sub[0].upper()
            rest = sub[1] if len(sub) > 1 else ""
            uid = True
            cmd = "UID " + cmd
        fn = getattr(self, "cmd_" + cmd.replace(" ", "_"), None)
        if fn is None:
            await self.send("%s BAD unknown command\r\n" % tag)
            return True
        try:
            res = await fn(tag, rest)
        except (IndexError, ValueError, KeyError) as e:
            await self.send("%s BAD %s\r\n" % (tag, type(e).__name__))
            return True
        return res is not False

    async def cmd_CAPABILITY(self, tag, rest):
        await self.send("* CAPABILITY %s\r\n%s OK done\r\n" % (" ".join(self.caps()), tag))

    async def cmd_NOOP(self, tag, rest):
        if self.sel is not None:
            await self.send("* %d EXISTS\r\n" % len(self.cfg.boxes[self.sel].msgs))
        await self.send("%s OK done\r\n" % tag)

    async def cmd_STARTTLS(self, tag, rest):
        if self.tls or self.cfg.tls != "starttls":
            await self.send("%s BAD no\r\n" % tag)
            return
        data = ("%s OK Begin TLS negotiation now\r\n" % tag).encode()
        if self.cfg.evil:
            data += b"* CAPABILITY IMAP4rev1 EVIL AUTH=PLAIN\r\n"
        await self.send(data)
        await self.w.start_tls(self.cfg.ssl_ctx)
        self.tls = True

    def login_ok(self, user, pw):
        if self.cfg.require_tls and not self.tls:
            return False
        return user == self.cfg.user and pw == self.cfg.pw

    async def cmd_LOGIN(self, tag, rest):
        a = tokenize(rest)
        if self.cfg.require_tls and not self.tls:
            await self.send("%s NO [PRIVACYREQUIRED] LOGIN disabled\r\n" % tag)
        elif self.login_ok(tv(a[0]), tv(a[1])):
            self.authed = True
            await self.send("%s OK [CAPABILITY %s] logged in\r\n" % (tag, " ".join(self.caps())))
        else:
            await self.send("%s NO [AUTHENTICATIONFAILED] bad credentials\r\n" % tag)

    async def cmd_AUTHENTICATE(self, tag, rest):
        a = rest.split(" ")
        if a[0].upper() != "PLAIN":
            await self.send("%s NO unsupported mechanism\r\n" % tag)
            return
        if len(a) > 1:
            ir = a[1]
        else:
            await self.send("+ \r\n")
            ir = (await self.r.readline()).rstrip(b"\r\n").decode()
        try:
            _, user, pw = base64.b64decode(ir).decode().split("\0")
        except Exception:
            await self.send("%s BAD bad base64\r\n" % tag)
            return
        if self.login_ok(user, pw):
            self.authed = True
            await self.send("%s OK [CAPABILITY %s] logged in\r\n" % (tag, " ".join(self.caps())))
        else:
            await self.send("%s NO [AUTHENTICATIONFAILED] bad credentials\r\n" % tag)

    async def need_auth(self, tag):
        if not self.authed:
            await self.send("%s NO login first\r\n" % tag)
            return False
        return True

    async def cmd_LIST(self, tag, rest):
        return await self.list_(tag, rest, "LIST")

    async def cmd_LSUB(self, tag, rest):
        return await self.list_(tag, rest, "LSUB")

    async def list_(self, tag, rest, word):
        if not await self.need_auth(tag):
            return
        a = tokenize(rest)
        pat = tv(a[1])
        rx = re.compile("^" + "".join(".*" if c == "*" else "[^/]*" if c == "%" else re.escape(c) for c in utf7_decode(pat)) + "$")
        out = []
        for name in sorted(self.cfg.boxes):
            if rx.match(name):
                out.append("* %s (\\HasNoChildren) \"/\" %s\r\n" % (word, self.mbox_wire(name)))
        await self.send("".join(out) + "%s OK done\r\n" % tag)

    def mbox_wire(self, name):
        enc = utf7_encode(name)
        if self.cfg.rand.random() < 0.15 or any(c in enc for c in ' "\\'):
            return '"' + enc.replace("\\", "\\\\").replace('"', '\\"') + '"'
        return enc

    def boxname(self, t):
        return utf7_decode(tv(t))

    async def cmd_SELECT(self, tag, rest):
        return await self.select(tag, rest, False)

    async def cmd_EXAMINE(self, tag, rest):
        return await self.select(tag, rest, True)

    async def select(self, tag, rest, ro):
        if not await self.need_auth(tag):
            return
        name = self.boxname(tokenize(rest)[0])
        if name.upper() == "INBOX":
            name = "INBOX"
        if name not in self.cfg.boxes:
            self.sel = None
            await self.send("%s NO [NONEXISTENT] no such mailbox\r\n" % tag)
            return
        self.sel, self.ro = name, ro
        b = self.cfg.boxes[name]
        unseen = [i for i, m in enumerate(b.msgs, 1) if "\\Seen" not in m.flags]
        out = "* FLAGS (%s)\r\n* OK [PERMANENTFLAGS (%s \\*)] ok\r\n* %d EXISTS\r\n* 0 RECENT\r\n" % (
            " ".join(FLAGS), " ".join(FLAGS), len(b.msgs))
        if unseen:
            out += "* OK [UNSEEN %d] first unseen\r\n" % unseen[0]
        out += "* OK [UIDVALIDITY %d] uids\r\n* OK [UIDNEXT %d] next\r\n" % (b.uidvalidity, b.uidnext)
        out += "%s OK [%s] done\r\n" % (tag, "READ-ONLY" if ro else "READ-WRITE")
        await self.send(out)

    async def cmd_STATUS(self, tag, rest):
        if not await self.need_auth(tag):
            return
        a = tokenize(rest)
        name = self.boxname(a[0])
        if name not in self.cfg.boxes:
            await self.send("%s NO no such mailbox\r\n" % tag)
            return
        b = self.cfg.boxes[name]
        vals = []
        for it in a[1]:
            it = it.upper()
            if it == "MESSAGES":
                vals.append("MESSAGES %d" % len(b.msgs))
            elif it == "UNSEEN":
                vals.append("UNSEEN %d" % sum(1 for m in b.msgs if "\\Seen" not in m.flags))
            elif it == "UIDNEXT":
                vals.append("UIDNEXT %d" % b.uidnext)
            elif it == "UIDVALIDITY":
                vals.append("UIDVALIDITY %d" % b.uidvalidity)
            elif it == "RECENT":
                vals.append("RECENT 0")
        await self.send("* STATUS %s (%s)\r\n%s OK done\r\n" % (self.mbox_wire(name), " ".join(vals), tag))

    def box(self):
        return self.cfg.boxes[self.sel]

    async def need_sel(self, tag):
        if self.sel is None:
            await self.send("%s BAD select a mailbox first\r\n" % tag)
            return False
        return True

    def targets(self, spec, uid):
        b = self.box()
        if uid:
            top = b.msgs[-1].uid if b.msgs else 0
            want = set(parse_set(spec, top))
            return [(i, m) for i, m in enumerate(b.msgs, 1) if m.uid in want]
        want = set(parse_set(spec, len(b.msgs)))
        return [(i, m) for i, m in enumerate(b.msgs, 1) if i in want]

    def fetch_item(self, m, item, st):
        """-> (name, value bytes) ; sets \\Seen for BODY[] (non-peek) in st"""
        r = self.cfg.rand
        up = item.upper()
        if up == "UID":
            return b"UID", b"%d" % m.uid
        if up == "FLAGS":
            return b"FLAGS", flags_wire(m.flags).encode()
        if up == "RFC822.SIZE":
            return b"RFC822.SIZE", b"%d" % len(m.raw())
        if up == "INTERNALDATE":
            return b"INTERNALDATE", b'"01-Jan-2024 10:00:00 +0000"'
        if up == "ENVELOPE":
            return b"ENVELOPE", envelope(m, r)
        if up == "BODYSTRUCTURE":
            return b"BODYSTRUCTURE", bodystructure(m.root, r, True)
        if up == "BODY":
            return b"BODY", bodystructure(m.root, r, False)
        if up in ("RFC822", "RFC822.HEADER", "RFC822.TEXT"):
            data = {"RFC822": m.raw(), "RFC822.HEADER": m.head(), "RFC822.TEXT": m.text()}[up]
            if up != "RFC822.HEADER":
                st["seen"] = True
            return up.encode(), istr(data, True)
        mt = re.match(r"^BODY(\.PEEK)?\[(.*)\](?:<(\d+)\.(\d+)>)?$", item, re.I)
        if mt:
            peek, sec, p0, p1 = mt.groups()
            sec = sec.strip()
            hf = re.match(r"^HEADER\.FIELDS(\.NOT)?\s*\((.*)\)$", sec, re.I)
            if hf:
                names = {n.lower() for n in hf.group(2).split()}
                lines = m.head().split(CRLF)[:-2]
                keep = [ln for ln in lines if ((ln.split(b":")[0].decode().lower() in names) != bool(hf.group(1)))]
                data = CRLF.join(keep) + (CRLF if keep else b"") + CRLF
            else:
                data = m.section(sec.upper() if not sec[:1].isdigit() else sec.upper())
            if not peek:
                st["seen"] = True
            name = b"BODY[" + sec.encode() + b"]"
            if data is None:
                return name, b"NIL"
            if p0 is not None:
                a, n = int(p0), int(p1)
                data = data[a:a + n]
                name += b"<%d>" % a
            return name, istr(data, True)
        raise ValueError("item " + item)

    async def cmd_FETCH(self, tag, rest, uid=False):
        if not await self.need_auth(tag) or not await self.need_sel(tag):
            return
        a = rest.split(" ", 1)
        spec, items_s = a[0], a[1]
        items = tokenize(items_s)
        if len(items) == 1 and isinstance(items[0], list):
            items = items[0]
        else:
            macro = {"ALL": ["FLAGS", "INTERNALDATE", "RFC822.SIZE", "ENVELOPE"], "FAST": ["FLAGS", "INTERNALDATE", "RFC822.SIZE"],
                     "FULL": ["FLAGS", "INTERNALDATE", "RFC822.SIZE", "ENVELOPE", "BODY"]}
            items = macro.get(tv(items[0]).upper(), items)
        items = [tv(i) for i in items]
        if uid and not any(i.upper() == "UID" for i in items):
            items = ["UID"] + items
        out = []
        for seq, m in self.targets(spec, uid):
            st = {}
            vals = []
            for it in items:
                nm, v = self.fetch_item(m, it, st)
                vals.append(nm + b" " + v)
            if st.get("seen") and "\\Seen" not in m.flags and not self.ro:
                m.flags.add("\\Seen")
                vals.append(b"FLAGS " + flags_wire(m.flags).encode())
            out.append(b"* %d FETCH (" % seq + b" ".join(vals) + b")\r\n")
        await self.send(b"".join(out) + ("%s OK done\r\n" % tag).encode())

    async def cmd_UID_FETCH(self, tag, rest):
        return await self.cmd_FETCH(tag, rest, True)

    # --- SEARCH
    def match(self, m, seq, toks, i):
        """evaluate one criterion at toks[i:] -> (bool, next i)"""
        t = tv(toks[i])
        u = t.upper() if isinstance(t, str) else t
        if isinstance(t, list):
            res = True
            j = 0
            while j < len(t):
                ok, j = self.match(m, seq, t, j)
                res = res and ok
            return res, i + 1
        if u == "ALL":
            return True, i + 1
        simple = {"SEEN": "\\Seen", "ANSWERED": "\\Answered", "FLAGGED": "\\Flagged", "DELETED": "\\Deleted", "DRAFT": "\\Draft"}
        if u in simple:
            return simple[u] in m.flags, i + 1
        if u.startswith("UN") and u[2:] in simple:
            return simple[u[2:]] not in m.flags, i + 1
        if u == "NOT":
            ok, j = self.match(m, seq, toks, i + 1)
            return (not ok), j
        if u == "OR":
            a, j = self.match(m, seq, toks, i + 1)
            b, k = self.match(m, seq, toks, j)
            return (a or b), k
        if u in ("FROM", "TO", "CC", "BCC", "SUBJECT"):
            needle = tv(toks[i + 1]).lower()
            e = m.env
            if u == "SUBJECT":
                hay = (e["subject"] or "")
            else:
                hay = mm.canon_addrs(e[u.lower()]) or ""
            return needle in hay.lower(), i + 2
        if u in ("LARGER", "SMALLER"):
            n = int(tv(toks[i + 1]))
            sz = len(m.raw())
            return (sz > n if u == "LARGER" else sz < n), i + 2
        if u == "UID":
            top = self.box().msgs[-1].uid if self.box().msgs else 0
            return m.uid in set(parse_set(tv(toks[i + 1]), top)), i + 2
        if u == "TEXT" or u == "BODY":
            needle = tv(toks[i + 1]).lower().encode()
            return needle in (m.raw() if u == "TEXT" else m.text()).lower(), i + 2
        if re.match(r"^[0-9*:,]+$", u):
            return seq in set(parse_set(u, len(self.box().msgs))), i + 1
        raise ValueError("criterion " + u)

    async def cmd_SEARCH(self, tag, rest, uid=False):
        if not await self.need_auth(tag) or not await self.need_sel(tag):
            return
        toks = tokenize(rest)
        found = []
        for seq, m in enumerate(self.box().msgs, 1):
            res, j = True, 0
            while j < len(toks):
                ok, j = self.match(m, seq, toks, j)
                res = res and ok
            if res:
                found.append(m.uid if uid else seq)
        await self.send("* SEARCH%s\r\n%s OK done\r\n" % ("".join(" %d" % n for n in found), tag))

    async def cmd_UID_SEARCH(self, tag, rest):
        return await self.cmd_SEARCH(tag, rest, True)

    async def cmd_STORE(self, tag, rest, uid=False):
        if not await self.need_auth(tag) or not await self.need_sel(tag):
            return
        a = rest.split(" ", 2)
        spec, mode, fl = a[0], a[1].upper(), tokenize(a[2])
        fl = fl[0] if fl and isinstance(fl[0], list) else fl
        fl = {tv(x) for x in fl}
        silent = mode.endswith(".SILENT")
        mode = mode.replace(".SILENT", "")
        out = []
        for seq, m in self.targets(spec, uid):
            if mode == "FLAGS":
                m.flags = set(fl)
            elif mode == "+FLAGS":
                m.flags |= fl
            elif mode == "-FLAGS":
                m.flags -= fl
            else:
                raise ValueError("mode")
            if not silent:
                extra = (" UID %d" % m.uid) if uid else ""
                out.append("* %d FETCH (FLAGS %s%s)\r\n" % (seq, flags_wire(m.flags), extra))
        await self.send("".join(out) + "%s OK done\r\n" % tag)

    async def cmd_UID_STORE(self, tag, rest):
        return await self.cmd_STORE(tag, rest, True)

    async def expunge(self, only_uids=None):
        b = self.box()
        out = []
        i = 0
        while i < len(b.msgs):
            m = b.msgs[i]
            if "\\Deleted" in m.flags and (only_uids is None or m.uid in only_uids):
                out.append("* %d EXPUNGE\r\n" % (i + 1))
                del b.msgs[i]
            else:
                i += 1
        return "".join(out)

    async def cmd_EXPUNGE(self, tag, rest):
        if not await self.need_auth(tag) or not await self.need_sel(tag):
            return
        await self.send((await self.expunge()) + "%s OK done\r\n" % tag)

    async def cmd_UID_EXPUNGE(self, tag, rest):
        if not await self.need_auth(tag) or not await self.need_sel(tag):
            return
        b = self.box()
        top = b.msgs[-1].uid if b.msgs else 0
        await self.send((await self.expunge(set(parse_set(rest.strip(), top)))) + "%s OK done\r\n" % tag)

    async def cmd_COPY(self, tag, rest, uid=False):
        if not await self.need_auth(tag) or not await self.need_sel(tag):
            return
        a = rest.split(" ", 1)
        name = self.boxname(tokenize(a[1])[0])
        if name not in self.cfg.boxes:
            await self.send("%s NO [TRYCREATE] no such mailbox\r\n" % tag)
            return
        dst = self.cfg.boxes[name]
        src_uids, dst_uids = [], []
        for seq, m in self.targets(a[0], uid):
            c = mm.Msg(m.hdrs, m.env, m.root)
            dst.add(c, m.flags)
            src_uids.append(m.uid)
            dst_uids.append(c.uid)
        code = ""
        if src_uids:
            code = "[COPYUID %d %s %s] " % (dst.uidvalidity, ",".join(map(str, src_uids)), ",".join(map(str, dst_uids)))
        await self.send("%s OK %sdone\r\n" % (tag, code))

    async def cmd_UID_COPY(self, tag, rest):
        return await self.cmd_COPY(tag, rest, True)

    async def cmd_CLOSE(self, tag, rest):
        if not await self.need_auth(tag) or not await self.need_sel(tag):
            return
        if not self.ro:
            await self.expunge()
        self.sel = None
        await self.send("%s OK done\r\n" % tag)

    async def cmd_NAMESPACE(self, tag, rest):
        await self.send("* NAMESPACE ((\"\" \"/\")) NIL NIL\r\n%s OK done\r\n" % tag)

    async def cmd_IDLE(self, tag, rest):
        if not await self.need_auth(tag) or not await self.need_sel(tag):
            return
        await self.send("+ idling\r\n")
        b = self.box()
        task = None

        async def inject():
            await asyncio.sleep(self.cfg.inject_idle)
            m = mm.rmsg(self.cfg.rand)
            b.add(m)
            self.cfg.idle_new += 1
            await self.send("* %d EXISTS\r\n* 1 RECENT\r\n" % len(b.msgs))

        if self.cfg.inject_idle >= 0:
            task = asyncio.ensure_future(inject())
        line = await self.r.readline()
        if task is not None:
            await task
        if line.strip().upper() != b"DONE":
            await self.send("%s BAD expected DONE\r\n" % tag)
            return
        await self.send("%s OK IDLE terminated\r\n" % tag)

    async def cmd_LOGOUT(self, tag, rest):
        await self.send("* BYE logging out\r\n%s OK done\r\n" % tag)
        return False


async def start_imap(cfg, port=0):
    async def handler(r, w):
        await ImapConn(cfg, r, w).run()

    srv = await asyncio.start_server(handler, "127.0.0.1", port, ssl=cfg.ssl_ctx if cfg.tls == "implicit" else None)
    return srv, srv.sockets[0].getsockname()[1]


# ------------------------------------------------------------------ SMTP

class SmtpCfg:
    def __init__(self, tls="starttls", auth="PLAIN LOGIN", size=None, user="alice", pw="s3cr3t!", evil=False,
                 drop_after_data=False, require_tls_for_auth=False, helo_only=False, eight=True):
        self.tls, self.auth, self.size, self.user, self.pw = tls, auth, size, user, pw
        self.evil, self.drop_after_data = evil, drop_after_data
        self.require_tls_for_auth, self.helo_only, self.eight = require_tls_for_auth, helo_only, eight
        self.received = []
        self.log = []
        self.ssl_ctx = None


class SmtpConn:
    def __init__(self, cfg, reader, writer):
        self.cfg, self.r, self.w = cfg, reader, writer
        self.tls = cfg.tls == "implicit"
        self.authed = False
        self.reset()

    def reset(self):
        self.mail_from = None
        self.rcpts = []

    async def send(self, b):
        if isinstance(b, str):
            b = b.encode("utf-8")
        self.w.write(b)
        await self.w.drain()

    async def run(self):
        try:
            await self.send("220 fake.example ESMTP ready\r\n")
            while True:
                line = await self.r.readline()
                if not line:
                    return
                s = line.rstrip(b"\r\n").decode("latin-1")
                self.cfg.log.append(s)
                if not await self.handle(s):
                    return
        except (ConnectionError, asyncio.IncompleteReadError, ssl.SSLError):
            return
        finally:
            try:
                self.w.close()
            except Exception:
                pass

    async def handle(self, s):
        up = s.upper()
        cfg = self.cfg
        if up.startswith("EHLO") or up.startswith("HELO"):
            self.reset()
            if up.startswith("HELO") or cfg.helo_only:
                if up.startswith("EHLO"):
                    await self.send("502 5.5.1 EHLO not implemented\r\n")
                else:
                    await self.send("250 fake.example\r\n")
                return True
            lines = ["fake.example greets you"]
            if cfg.size:
                lines.append("SIZE %d" % cfg.size)
            if cfg.eight:
                lines.append("8BITMIME")
            lines.append("PIPELINING")
            if cfg.tls == "starttls" and not self.tls:
                lines.append("STARTTLS")
            if cfg.auth and not (cfg.require_tls_for_auth and not self.tls):
                lines.append("AUTH " + cfg.auth)
            out = "".join("250-%s\r\n" % x for x in lines[:-1]) + "250 %s\r\n" % lines[-1]
            await self.send(out)
        elif up == "STARTTLS":
            if self.tls or cfg.tls != "starttls":
                await self.send("503 5.5.1 no\r\n")
                return True
            data = b"220 2.0.0 Ready to start TLS\r\n"
            if cfg.evil:
                data += b"250-injected\r\n250 AUTH PLAIN\r\n"
            await self.send(data)
            await self.w.start_tls(cfg.ssl_ctx)
            self.tls = True
            self.reset()
            self.authed = False
        elif up.startswith("AUTH"):
            await self.auth(s)
        elif up.startswith("MAIL FROM:"):
            m = re.match(r"^MAIL FROM:\s*<([^>]*)>(.*)$", s, re.I)
            if not m:
                await self.send("501 5.5.4 syntax\r\n")
                return True
            if cfg.auth and not self.authed and cfg.user:
                pass
            sz = re.search(r"SIZE=(\d+)", m.group(2), re.I)
            if cfg.size and sz and int(sz.group(1)) > cfg.size:
                await self.send("552 5.3.4 message too big\r\n")
                return True
            if m.group(1).startswith("tempfail"):
                await self.send("451 4.3.0 try later\r\n")
                return True
            if cfg.user and cfg.auth and not self.authed and cfg.auth == "REQUIRED":
                await self.send("530 5.7.0 authentication required\r\n")
                return True
            self.mail_from = m.group(1)
            self.rcpts = []
            await self.send("250 2.1.0 ok\r\n")
        elif up.startswith("RCPT TO:"):
            m = re.match(r"^RCPT TO:\s*<([^>]*)>", s, re.I)
            if not m or self.mail_from is None:
                await self.send("503 5.5.1 need MAIL first\r\n")
                return True
            if m.group(1).startswith("bad"):
                await self.send("550 5.1.1 no such user here\r\n")
            else:
                self.rcpts.append(m.group(1))
                await self.send("250 2.1.5 ok\r\n")
        elif up == "DATA":
            if self.mail_from is None or not self.rcpts:
                await self.send("503 5.5.1 need RCPT first\r\n")
                return True
            await self.send("354 end with <CRLF>.<CRLF>\r\n")
            chunks = []
            while True:
                ln = await self.r.readline()
                if not ln:
                    return False
                if ln == b".\r\n":
                    break
                if ln.startswith(b"."):
                    ln = ln[1:]
                chunks.append(ln)
            cfg.received.append({"from": self.mail_from, "rcpts": list(self.rcpts), "data": b"".join(chunks), "tls": self.tls,
                                 "authed": self.authed})
            self.reset()
            if cfg.drop_after_data:
                return False
            await self.send("250 2.0.0 queued as %d\r\n" % len(cfg.received))
        elif up == "RSET":
            self.reset()
            await self.send("250 2.0.0 ok\r\n")
        elif up == "NOOP":
            await self.send("250 2.0.0 ok\r\n")
        elif up == "QUIT":
            await self.send("221 2.0.0 bye\r\n")
            return False
        else:
            await self.send("500 5.5.2 unknown command\r\n")
        return True

    async def auth(self, s):
        cfg = self.cfg
        a = s.split(" ")
        mech = a[1].upper() if len(a) > 1 else ""
        if cfg.require_tls_for_auth and not self.tls:
            await self.send("530 5.7.0 Must issue a STARTTLS command first\r\n")
            return
        user = pw = None
        if mech == "PLAIN":
            if len(a) > 2:
                ir = a[2]
            else:
                await self.send("334 \r\n")
                ir = (await self.r.readline()).rstrip(b"\r\n").decode()
            try:
                _, user, pw = base64.b64decode(ir).decode().split("\0")
            except Exception:
                await self.send("501 5.5.2 cannot decode\r\n")
                return
        elif mech == "LOGIN":
            await self.send("334 " + base64.b64encode(b"Username:").decode() + "\r\n")
            user = base64.b64decode((await self.r.readline()).strip()).decode()
            await self.send("334 " + base64.b64encode(b"Password:").decode() + "\r\n")
            pw = base64.b64decode((await self.r.readline()).strip()).decode()
        else:
            await self.send("504 5.5.4 unrecognized mechanism\r\n")
            return
        if user == cfg.user and pw == cfg.pw:
            self.authed = True
            await self.send("235 2.7.0 Authentication successful\r\n")
        else:
            await self.send("535 5.7.8 Authentication credentials invalid\r\n")


async def start_smtp(cfg, port=0):
    async def handler(r, w):
        await SmtpConn(cfg, r, w).run()

    srv = await asyncio.start_server(handler, "127.0.0.1", port, ssl=cfg.ssl_ctx if cfg.tls == "implicit" else None)
    return srv, srv.sockets[0].getsockname()[1]


def make_ssl(cert, key):
    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    ctx.load_cert_chain(cert, key)
    return ctx
