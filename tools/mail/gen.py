#!/usr/bin/env python3
"""tools/mail/gen.py -- random cases + Python's answers for lib/mail/rfc822.fi.

    gen.py gen <seed> <count> <cases file> <expected file>
    gen.py cmp <cases file> <expected file> <results file>

Case line:     <kind> <hex input>
Expected line: <kind> <hex answer>          (E/F/S/X lines: "-": checked by `cmp` itself)
Results line:  <hex answer of the Firn side>

Kinds
  M  whole message      answer = headers (unfolded) + part tree (type, file name, charset, cte, decoded body)
  A  address-list value answer = M[name|spec] / G[name: ... ] flat list
  W  unstructured value answer = RFC 2047 decoded UTF-8
  D  date value         answer = "Y-M-D h:m:s tz" or "ERR"
  Q  quoted-printable   answer = binascii.a2b_qp
  B  base64             answer = binascii.a2b_base64
  E  encode_words       result decodes (email.header) to the input
  F  qp_encode          result decodes (a2b_qp) to the input with CRLF line ends
  S  base64 lines       result decodes to the input, lines <= 76
  X  builder spec       result is a message that Python's parser reads back as the spec
All numbers come from random.Random(seed): a failing case can be replayed.
"""
import base64
import binascii
import email
import email.header
import email.policy
import email.utils
import quopri
import random
import re
import sys

CHARSETS = ["utf-8", "iso-8859-1", "iso-8859-15", "windows-1252", "us-ascii"]


def hx(b):
    return b.hex()


def unhx(s):
    return bytes.fromhex(s)


def unfold(b):
    return re.sub(rb"\r?\n(?=[ \t])", b"", b)


# ------------------------------------------------------------------ oracles

def dump_msg(raw):
    msg = email.message_from_bytes(raw)
    out = []
    for name, val in msg.raw_items():
        v = val.encode("ascii", "surrogateescape") if isinstance(val, str) else val
        out.append(b"H " + name.encode("ascii", "surrogateescape") + b": " + unfold(v))

    def walk(part, depth):
        ct = part.get_content_type()
        fn = part.get_filename()
        cs = part.get_content_charset()
        cte = str(part.get("content-transfer-encoding", "")).strip().lower()
        if part.is_multipart():
            body = "-"
        else:
            body = hx(part.get_payload(decode=True) or b"")
        out.append(("P %d %s %s %s %s %s" % (
            depth, ct, hx(fn.encode("utf-8", "surrogateescape")) if fn is not None else "-",
            cs if cs else "-", cte if cte else "-", body)).encode())
        if part.is_multipart():
            for c in part.get_payload():
                walk(c, depth + 1)

    walk(msg, 0)
    return b"\n".join(out)


def dump_addr(value):
    hdr = email.policy.default.header_factory("To", value.decode("utf-8", "surrogateescape"))
    out = []
    for g in hdr.groups:
        if g.display_name is None:
            for a in g.addresses:
                out.append("M[%s|%s]" % (a.display_name, a.addr_spec))
        else:
            out.append("G[%s:" % g.display_name)
            for a in g.addresses:
                out.append("M[%s|%s]" % (a.display_name, a.addr_spec))
            out.append("]")
    return "\n".join(out).encode("utf-8", "surrogateescape")


def dump_words(value):
    parts = email.header.decode_header(value.decode("ascii", "surrogateescape"))
    out = b""
    for chunk, cs in parts:
        if isinstance(chunk, str):
            out += chunk.encode("utf-8")
        elif cs is None:
            out += chunk
        else:
            out += chunk.decode(cs, "replace").encode("utf-8")
    return out


def dump_date(value):
    t = email.utils.parsedate_tz(value.decode("ascii", "surrogateescape"))
    if t is None:
        return b"ERR"
    tz = "none" if t[9] is None else str(t[9] // 60)
    return ("%d-%d-%d %d:%d:%d %s" % (t[0], t[1], t[2], t[3], t[4], t[5], tz)).encode()


def dump_qp(data):
    return binascii.a2b_qp(data)


def dump_b64(data):
    return binascii.a2b_base64(data)


# ---------------------------------------------------------------- generators

LETTERS = "abcdefghijklmnopqrstuvwxyz"
UNI = ["ä", "ö", "ü", "ß", "é", "è", "ñ", "€", "ж", "я", "中", "文", "😀", "ø", "å", "ç"]
LATIN1 = ["ä", "ö", "ü", "ß", "é", "è", "ñ", "ø", "å", "ç", "Ä", "Ü"]
LATIN9 = LATIN1 + ["€", "Š", "ž", "œ"]
CP1252 = LATIN1 + ["€", "“", "”", "–", "—", "™", "…"]


def word(r, n=None):
    n = n or r.randint(1, 9)
    return "".join(r.choice(LETTERS) for _ in range(n))


def uni_word(r, cs):
    pool = {"utf-8": UNI + list(LETTERS), "iso-8859-1": LATIN1 + list(LETTERS),
            "iso-8859-15": LATIN9 + list(LETTERS), "windows-1252": CP1252 + list(LETTERS),
            "us-ascii": list(LETTERS)}[cs]
    return "".join(r.choice(pool) for _ in range(r.randint(1, 10)))


def enc_word(r, text, cs, enc=None):
    enc = enc or r.choice("BQ")
    raw = text.encode(cs)
    if enc == "B":
        body = base64.b64encode(raw).decode()
        if r.random() < 0.2:
            body = body.rstrip("=")
    else:
        body = ""
        for b in raw:
            if chr(b) in LETTERS or chr(b) in "0123456789":
                body += chr(b)
            elif b == 32:
                body += "_"
            else:
                body += "=%02X" % b
    cname = cs if r.random() < 0.7 else cs.upper()
    return "=?%s?%s?%s?=" % (cname, enc if r.random() < 0.7 else enc.lower(), body)


def fold(r, s):
    """insert folds at random blanks"""
    out = []
    for ch in s:
        if ch == " " and r.random() < 0.15:
            out.append(r.choice(["\r\n ", "\n ", "\r\n\t", "\r\n  "]))
        else:
            out.append(ch)
    return "".join(out)


def gen_words_value(r):
    """unstructured value with encoded words, ASCII text around them"""
    toks = []
    last_enc = False
    for _ in range(r.randint(1, 6)):
        if r.random() < 0.55 and not last_enc:
            cs = r.choice(CHARSETS)
            toks.append(enc_word(r, uni_word(r, cs), cs))
            last_enc = True
        elif r.random() < 0.2 and last_enc:
            cs = r.choice(CHARSETS)
            toks.append(enc_word(r, uni_word(r, cs), cs))
        else:
            toks.append(word(r))
            last_enc = False
    sep = " "
    v = sep.join(toks)
    if r.random() < 0.3:
        v = fold(r, v)
    return v


def local_part(r):
    parts = [word(r, r.randint(1, 8)) for _ in range(r.randint(1, 3))]
    return r.choice([".", ".", "-", "+"]).join(parts) if r.random() < 0.3 else ".".join(parts)


def domain(r):
    return ".".join(word(r, r.randint(2, 7)) for _ in range(r.randint(2, 3)))


def quote(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def gen_display(r):
    k = r.random()
    if k < 0.3:
        return " ".join(word(r) for _ in range(r.randint(1, 3)))
    if k < 0.6:
        txt = " ".join(word(r) for _ in range(r.randint(1, 3)))
        if r.random() < 0.5:
            txt += r.choice([", ", " ; ", " <x> ", " \\ ", ' " '][r.randint(0, 4)]) + word(r)
        return quote(txt)
    if k < 0.8:
        cs = r.choice(CHARSETS)
        return enc_word(r, uni_word(r, cs), cs)
    if k < 0.9:
        cs = r.choice(CHARSETS)
        return word(r) + " " + enc_word(r, uni_word(r, cs), cs) + " " + word(r)
    return word(r) + " (" + word(r) + ") " + word(r)


def gen_mailbox(r):
    addr = local_part(r) + "@" + domain(r)
    k = r.random()
    if k < 0.2:
        return addr + (" (" + word(r) + ")" if r.random() < 0.3 else "")
    if k < 0.3:
        return "<" + addr + ">"
    sp = " " if r.random() < 0.9 else ""
    if r.random() < 0.08:
        addr = quote(word(r) + " " + word(r)) + "@" + domain(r)
    return gen_display(r) + sp + "<" + addr + ">"


def gen_addr_value(r):
    items = []
    for _ in range(r.randint(1, 4)):
        if r.random() < 0.2:
            members = ", ".join(gen_mailbox(r) for _ in range(r.randint(0, 3)))
            items.append(gen_display(r) + ": " + members + ";")
        else:
            items.append(gen_mailbox(r))
    v = r.choice([", ", ",", " , "]).join(items)
    if r.random() < 0.3:
        v = fold(r, v)
    return v


MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
ZONES = ["UT", "UTC", "GMT", "Z", "EST", "EDT", "CST", "CDT", "MST", "MDT", "PST", "PDT"]


def gen_date_value(r):
    day = r.randint(1, 28)
    mon = r.choice(MONTHS)
    year = r.choice([r.randint(1970, 2060), r.randint(0, 99)])
    ys = "%d" % year if year >= 100 else "%02d" % year
    hh, mm, ss = r.randint(0, 23), r.randint(0, 59), r.randint(0, 60)
    tm = "%02d:%02d" % (hh, mm) + (":%02d" % ss if r.random() < 0.8 else "")
    k = r.random()
    if k < 0.6:
        tz = r.choice(["+", "-"]) + "%02d%02d" % (r.randint(0, 14), r.choice([0, 0, 30, 45]))
    elif k < 0.9:
        tz = r.choice(ZONES)
    else:
        tz = "-0000"
    d = ""
    if r.random() < 0.7:
        d = r.choice(DAYS) + ", "
    s = "%s%d %s %s %s %s" % (d, day, mon, ys, tm, tz)
    if r.random() < 0.2:
        s += " (" + word(r, 3).upper() + ")"
    if r.random() < 0.1:
        s = s.replace(" ", "  ", 1)
    if r.random() < 0.05:
        s = word(r)
    return s


def rand_bytes(r, n):
    return bytes(r.randint(0, 255) for _ in range(n))


def text_lines(r, nl, uni=False):
    lines = []
    for _ in range(r.randint(0, 6)):
        ln = " ".join(word(r) for _ in range(r.randint(0, 8)))
        if uni and r.random() < 0.3:
            ln += " " + uni_word(r, "utf-8")
        lines.append(ln)
    return nl.join(lines)


def encode_body(r, kind, payload, nl):
    if kind == "base64":
        s = base64.b64encode(payload).decode()
        w = r.choice([76, 60, 64])
        return nl.join(s[i:i + w] for i in range(0, len(s), w)).encode()
    if kind == "quoted-printable":
        enc = quopri.encodestring(payload)
        enc = enc.replace(b"\r\n", b"\n").replace(b"\n", nl.encode())
        return enc.rstrip(b"\r\n")
    return payload


def gen_params(r, filename):
    """a Content-Disposition value with the file name in one of the 5 forms"""
    k = r.random()
    kind = r.choice(["attachment", "inline"])
    ascii_name = filename.encode("ascii", "replace").decode().replace("?", "x")
    if k < 0.2:
        return "%s; filename=%s" % (kind, ascii_name.replace(" ", "_"))
    if k < 0.5:
        return "%s; filename=%s" % (kind, quote(ascii_name))
    if k < 0.7:
        pct = "".join("%%%02X" % b for b in filename.encode("utf-8"))
        return "%s; filename*=utf-8''%s" % (kind, pct)
    if k < 0.85:
        mid = len(ascii_name) // 2
        return '%s; filename*0="%s"; filename*1="%s"' % (kind, ascii_name[:mid], ascii_name[mid:])
    pct = ["%%%02X" % b for b in (filename + "é").encode("utf-8")]
    mid = len(pct) // 2
    return "%s; filename*0*=utf-8''%s; filename*1*=%s" % (kind, "".join(pct[:mid]), "".join(pct[mid:]))


def gen_msg(r, depth=0, nl=None):
    nl = nl or r.choice(["\r\n", "\r\n", "\n"])
    hdrs = []
    for _ in range(r.randint(0, 5)):
        name = r.choice(["Subject", "X-Foo", "Message-ID", "References", "X-Mailer", "Keywords", "subject", "TO"])
        v = gen_words_value(r) if r.random() < 0.5 else " ".join(word(r) for _ in range(r.randint(0, 5)))
        if r.random() < 0.4:
            v = fold(r, v)
        hdrs.append("%s:%s%s" % (name, r.choice([" ", "", "  ", "\t"]), v))
    kind = r.random()
    body = b""
    if depth < 3 and kind < 0.45:
        sub = r.choice(["mixed", "alternative", "related"])
        bnd = r.choice(["=_", "----=_Part_", "b", "x-"]) + word(r, 12) + r.choice(["", "'(+)", ":=", "-1"])
        needs_q = any(c in bnd for c in "'()+:=?/")
        ct = 'multipart/%s; boundary=%s' % (sub, quote(bnd) if needs_q or r.random() < 0.5 else bnd)
        if r.random() < 0.3:
            ct = ct.replace("; ", ";\r\n\t" if nl == "\r\n" else ";\n\t")
        hdrs.append("Content-Type: " + ct)
        pre = text_lines(r, nl)
        body = (pre + nl).encode() if pre and r.random() < 0.5 else b""
        ws = ""
        for _ in range(r.randint(0, 4)):
            ws = r.choice(["", "", " ", "\t"])
            body += ("--" + bnd + ws + nl).encode()
            body += gen_msg(r, depth + 1, nl)
            body += nl.encode()
        body += ("--" + bnd + "--" + r.choice(["", ws]) + nl).encode()
        if r.random() < 0.4:
            body += (text_lines(r, nl) + nl).encode()
    elif depth < 2 and kind < 0.52:
        hdrs.append("Content-Type: message/rfc822")
        body = gen_msg(r, depth + 1, nl)
    else:
        cte = r.choice([None, "7bit", "8bit", "quoted-printable", "base64", "binary", "Base64", "QUOTED-PRINTABLE"])
        ctype = r.choice(["text/plain", "text/html", "application/octet-stream", "image/png", "TEXT/Plain", None])
        if ctype:
            cs = r.choice(CHARSETS) if ctype.lower().startswith("text") and r.random() < 0.7 else None
            ct = ctype + ("; charset=" + (quote(cs) if r.random() < 0.3 else cs) if cs else "")
            if ctype.startswith("application") or ctype.startswith("image"):
                ct += "; name=" + quote(word(r) + ".bin") if r.random() < 0.5 else ""
            hdrs.append("Content-Type: " + ct)
        if cte:
            hdrs.append("Content-Transfer-Encoding: " + cte)
        if r.random() < 0.4:
            hdrs.append("Content-Disposition: " + gen_params(r, word(r, 5) + r.choice([" ", ".", "-", "_"]) + word(r, 3) + r.choice(["", ".txt", "é.txt", "中.pdf"])))
        low = (cte or "").lower()
        if low in ("base64", "quoted-printable"):
            payload = rand_bytes(r, r.randint(0, 200)) if r.random() < 0.5 else text_lines(r, "\n", True).encode()
            body = encode_body(r, low, payload, nl)
        else:
            body = text_lines(r, nl).encode()
            if r.random() < 0.3:
                body += nl.encode()
    head = nl.join(hdrs)
    if depth == 0 or r.random() < 0.9:
        raw = head.encode("utf-8") + (nl if hdrs else "").encode() + nl.encode() + body
    else:
        raw = nl.encode() + body
    return raw


# ------------------------------------------------------------------- driver

QP_ALPHA = b"abc XYZ 019=\r\n\t\x7f\xe4=3D=4a=4G=\n=\r\n_?.;,"


def gen_qp_case(r):
    if r.random() < 0.4:
        return encode_body(r, "quoted-printable", rand_bytes(r, r.randint(0, 120)), "\r\n")
    return bytes(r.choice(QP_ALPHA) for _ in range(r.randint(0, 80)))


def gen_x_spec(r):
    atts = []
    for _ in range(r.randint(0, 3)):
        atts.append((word(r, 6) + r.choice([".txt", ".png", " x.bin", "ä.pdf", "中.zip"]),  # english: ok
                     r.choice(["application/octet-stream", "image/png", "application/pdf", ""]),
                     rand_bytes(r, r.randint(0, 300))))
    to = [(gen_display_plain(r), local_part(r) + "@" + domain(r)) for _ in range(r.randint(1, 3))]
    return {
        "from": (gen_display_plain(r), local_part(r) + "@" + domain(r)),
        "to": to,
        "subject": " ".join((uni_word(r, "utf-8") if r.random() < 0.4 else word(r)) for _ in range(r.randint(0, 14))),
        "text": text_lines(r, "\n", True) if r.random() < 0.8 else "",
        "html": ("<p>" + text_lines(r, "\n", True) + "</p>") if r.random() < 0.5 else "",
        "atts": atts,
    }


def gen_display_plain(r):
    k = r.random()
    if k < 0.4:
        return " ".join(word(r) for _ in range(r.randint(1, 3)))
    if k < 0.6:
        return word(r) + ", " + word(r) + r.choice(["", ' "q"', " (x)"])
    if k < 0.9:
        return uni_word(r, "utf-8") + " " + word(r)
    return ""


def x_line(spec):
    parts = ["from=%s:%s" % (hx(spec["from"][0].encode()), hx(spec["from"][1].encode()))]
    for n, a in spec["to"]:
        parts.append("to=%s:%s" % (hx(n.encode()), hx(a.encode())))
    parts.append("subject=" + hx(spec["subject"].encode()))
    parts.append("text=" + hx(spec["text"].encode()))
    parts.append("html=" + hx(spec["html"].encode()))
    for n, c, d in spec["atts"]:
        parts.append("att=%s:%s:%s" % (hx(n.encode()), hx(c.encode()), hx(d)))
    return ";".join(parts)


def parse_x_line(s):
    spec = {"to": [], "atts": []}
    for p in s.split(";"):
        k, v = p.split("=", 1)
        if k == "from":
            a, b = v.split(":")
            spec["from"] = (unhx(a).decode(), unhx(b).decode())
        elif k == "to":
            a, b = v.split(":")
            spec["to"].append((unhx(a).decode(), unhx(b).decode()))
        elif k == "att":
            a, b, c = v.split(":")
            spec["atts"].append((unhx(a).decode(), unhx(b).decode(), unhx(c)))
        else:
            spec[k] = unhx(v).decode()
    return spec


def check_x(spec, built):
    """None if Python reads the built message back as the spec, else a reason"""
    msg = email.message_from_bytes(built, policy=email.policy.default)
    if not built.endswith(b"\r\n") or re.search(rb"(?<!\r)\n", built):
        return "line ends are not CRLF"
    for line in built.split(b"\r\n"):
        if len(line) > 998:
            return "line too long"
    fr = msg["From"].addresses
    if len(fr) != 1 or fr[0].addr_spec != spec["from"][1] or fr[0].display_name != spec["from"][0]:
        return "From differs: %r" % (msg["From"],)
    to = [(a.display_name, a.addr_spec) for a in msg["To"].addresses]
    if to != [tuple(t) for t in spec["to"]]:
        return "To differs: %r" % (to,)
    if str(msg["Subject"]) != spec["subject"].strip() and str(msg["Subject"]).strip() != spec["subject"].strip():
        return "Subject differs: %r vs %r" % (str(msg["Subject"]), spec["subject"])
    if not re.match(r"^<[^<>@\s]+@[^<>\s]+>$", str(msg["Message-ID"])):
        return "Message-ID: %r" % (str(msg["Message-ID"]),)
    if email.utils.parsedate_tz(str(msg["Date"])) is None:
        return "Date: %r" % (str(msg["Date"]),)
    if str(msg["MIME-Version"]) != "1.0":
        return "MIME-Version"
    if spec["text"]:
        p = msg.get_body(("plain",))
        if p is None or p.get_content().replace("\r\n", "\n").rstrip("\n") != spec["text"].rstrip("\n"):
            return "text body differs"
    if spec["html"]:
        p = msg.get_body(("html",))
        if p is None or p.get_content().replace("\r\n", "\n").rstrip("\n") != spec["html"].rstrip("\n"):
            return "html body differs"
    atts = list(msg.iter_attachments())
    if len(atts) != len(spec["atts"]):
        return "attachment count %d vs %d" % (len(atts), len(spec["atts"]))
    for a, (n, c, d) in zip(atts, spec["atts"]):
        if a.get_filename() != n:
            return "attachment name %r vs %r" % (a.get_filename(), n)
        if a.get_content_type() != (c or "application/octet-stream"):
            return "attachment type"
        if a.get_payload(decode=True) != d:
            return "attachment data differs"
    return None


def gen(seed, count, cases_path, exp_path):
    r = random.Random(seed)
    kinds = ["M"] * 30 + ["A"] * 22 + ["W"] * 16 + ["D"] * 10 + ["Q"] * 6 + ["B"] * 4 + ["E"] * 5 + ["F"] * 3 + ["S"] * 2 + ["X"] * 8
    with open(cases_path, "w") as cf, open(exp_path, "w") as ef:
        for _ in range(count):
            k = r.choice(kinds)
            if k == "M":
                data = gen_msg(r)
                exp = hx(dump_msg(data))
            elif k == "A":
                data = gen_addr_value(r).encode("utf-8")
                data = unfold(data)
                exp = hx(dump_addr(data))
            elif k == "W":
                data = unfold(gen_words_value(r).encode())
                exp = hx(dump_words(data))
            elif k == "D":
                data = gen_date_value(r).encode()
                exp = hx(dump_date(data))
            elif k == "Q":
                data = gen_qp_case(r)
                exp = hx(dump_qp(data))
            elif k == "B":
                data = base64.encodebytes(rand_bytes(r, r.randint(0, 150)))
                if r.random() < 0.3:
                    data = data.replace(b"\n", b"\r\n")
                exp = hx(dump_b64(data))
            elif k == "E":
                data = " ".join(uni_word(r, "utf-8") if r.random() < 0.7 else word(r) for _ in range(r.randint(1, 30))).encode()
                exp = "-"
            elif k == "F":
                data = (text_lines(r, r.choice(["\n", "\r\n"]), True) + r.choice(["", " ", "\n"])).encode()
                if r.random() < 0.3:
                    data += rand_bytes(r, 20).replace(b"\r", b"").replace(b"\n", b"")
                exp = "-"
            elif k == "S":
                data = rand_bytes(r, r.randint(0, 400))
                exp = "-"
            else:
                spec = gen_x_spec(r)
                data = x_line(spec).encode()
                exp = "-"
            cf.write("%s %s\n" % (k, hx(data) if k != "X" else data.decode()))
            ef.write("%s %s\n" % (k, exp))


def esc(b):
    t = b.decode("latin-1").encode("unicode_escape").decode()
    return t if len(t) < 400 else t[:400] + "..."


def cmp(cases_path, exp_path, res_path):
    cases = open(cases_path).read().split("\n")
    exps = open(exp_path).read().split("\n")
    ress = open(res_path).read().split("\n")
    total = 0
    diffs = 0
    per = {}
    shown = 0
    for i, c in enumerate(cases):
        if not c:
            continue
        total += 1
        kind, data = c.split(" ", 1)
        got = ress[i] if i < len(ress) else ""
        exp = exps[i].split(" ", 1)[1]
        bad = None
        if kind in "MAWDQB":
            if got != exp:
                bad = "got  %s\n      want %s" % (esc(unhx(got)), esc(unhx(exp)))
        else:
            gotb = unhx(got) if got and got != "-" else b""
            if kind == "E":
                inp = unhx(data)
                try:
                    back = dump_words(gotb)
                except Exception as e:  # noqa
                    back = b"<python: %r>" % (e,)
                if back != inp or not all(len(l) <= 76 for l in gotb.split(b"\r\n")):
                    bad = "E: %s -> %s -> %s" % (esc(inp), esc(gotb), esc(back))
            elif kind == "F":
                inp = unhx(data)
                back = binascii.a2b_qp(gotb)
                want = inp.replace(b"\r\n", b"\n").replace(b"\n", b"\r\n")
                if back.rstrip(b"\r\n \t") != want.rstrip(b"\r\n \t") or any(len(l) > 76 for l in gotb.split(b"\r\n")):
                    bad = "F: %s -> %s -> %s" % (esc(inp), esc(gotb), esc(back))
            elif kind == "S":
                inp = unhx(data)
                back = binascii.a2b_base64(gotb)
                if back != inp or any(len(l) > 76 for l in gotb.split(b"\r\n")):
                    bad = "S: mismatch (%d bytes)" % len(inp)
            elif kind == "X":
                spec = parse_x_line(data)
                try:
                    why = check_x(spec, gotb)
                except Exception as e:  # noqa
                    why = "python could not read it: %r" % (e,)
                if why:
                    bad = "X: %s\n%s" % (why, esc(gotb)[:600])
        per.setdefault(kind, [0, 0])
        per[kind][0] += 1
        if bad:
            per[kind][1] += 1
            diffs += 1
            if shown < 8:
                shown += 1
                print("DIFF case %d kind %s\n  input %s\n  %s" % (i + 1, kind, (data if kind == "X" else esc(unhx(data)))[:500], bad))
    print("mail cross-check: %d cases %d differences (%s)" % (
        total, diffs, ", ".join("%s %d/%d" % (k, v[0] - v[1], v[0]) for k, v in sorted(per.items()))))
    return 1 if diffs else 0


if __name__ == "__main__":
    if sys.argv[1] == "gen":
        gen(int(sys.argv[2]), int(sys.argv[3]), sys.argv[4], sys.argv[5])
    else:
        sys.exit(cmp(sys.argv[2], sys.argv[3], sys.argv[4]))
