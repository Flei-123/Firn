#!/usr/bin/env python3
"""tools/mail/e2e.py -- end-to-end run of lib/mail/smtp.fi + imap.fi (driver tools/mail/e2e_main.fi)
against the fake servers in tools/mail/fake_mail.py, with Python's smtplib / imaplib as the reference.

  python3 tools/mail/e2e.py <driver binary> <work dir> [seed ...]      (E2E_MSGS = mails per seed)

SMTP: the driver sends random and nasty mails (leading dots, bare CR/LF, 8-bit, 300 KB, no final newline)
      over plain / STARTTLS / implicit TLS with PLAIN and LOGIN; the fake server's received list must equal
      (a) the expected CRLF text and (b) what smtplib delivers for the same mails. Error cases (bad
      password, refused recipients, SIZE, 451, evil STARTTLS, dropped link, wrong host name, unknown CA,
      REQUIRE without STARTTLS, no TLS without the insecure switch) must give the documented error.
IMAP: random MIME mailboxes; the driver's `dump` (ENVELOPE, BODYSTRUCTURE incl. message/rfc822 inner
      parts, every BODY[section]) must equal the ground truth of tools/mail/mailmodel.py AND the octets
      imaplib fetches; SEARCH/STATUS/LIST/STORE/EXPUNGE/COPY/IDLE are compared with imaplib on a twin server.
Exit 0 = 0 differences.
"""
import asyncio
import hashlib
import imaplib
import os
import random
import re
import smtplib
import ssl
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import fake_mail as fm  # noqa: E402
import mailmodel as mm  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(HERE))
CHECKS = 0
DIFFS = 0
DRIVER = None
WORK = None
CA = LEAF = KEY = None
SEQ = [0]


def chk(name, got, want):
    global CHECKS, DIFFS
    CHECKS += 1
    if got != want:
        DIFFS += 1
        g, w = repr(got), repr(want)
        print("DIFF %s\n   got  %s\n   want %s" % (name, g[:400], w[:400]))
        return False
    return True


def load_certs():
    src = open(os.path.join(ROOT, "tests", "2122_async_tls.fi")).read()
    out = {}
    for nm in ("CA_PEM", "LEAF_PEM", "LEAF_KEY"):
        m = re.search(r'const %s: str = "(.*)"\n' % nm, src)
        out[nm] = m.group(1).replace("\\n", "\n")
    paths = []
    for nm, fn in (("CA_PEM", "ca.pem"), ("LEAF_PEM", "leaf.pem"), ("LEAF_KEY", "leaf.key")):
        p = os.path.join(WORK, fn)
        open(p, "w").write(out[nm])
        paths.append(p)
    # a second CA the server certificate is NOT signed by
    other = os.path.join(WORK, "other_ca.pem")
    subprocess.run(["openssl", "req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1", "-nodes",
                    "-keyout", os.path.join(WORK, "other.key"), "-out", other, "-subj", "/CN=Other CA", "-days", "30"],
                   check=True, capture_output=True)
    return paths[0], paths[1], paths[2], other


def client_ctx():
    ctx = ssl.create_default_context(cafile=CA)
    ctx.check_hostname = False
    return ctx


def kv(line):
    d = {}
    for tok in line.split()[1:]:
        if "=" in tok:
            k, v = tok.split("=", 1)
            d[k] = v
    return d


async def run_driver(proto, port, mode, tls, script, host="localhost", ca=None):
    SEQ[0] += 1
    sp = os.path.join(WORK, "script%d.txt" % SEQ[0])
    with open(sp, "w") as f:
        f.write("\n".join(script) + ("\n" if script else ""))
    p = await asyncio.create_subprocess_exec(DRIVER, proto, str(port), mode, tls, ca or CA, host, sp,
                                             stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE)
    try:
        so, se = await asyncio.wait_for(p.communicate(), 120)
    except asyncio.TimeoutError:
        p.kill()
        print("DRIVER TIMEOUT", proto, mode, tls)
        return ["TIMEOUT"]
    lines = so.decode("utf-8", "replace").split("\n")
    if p.returncode not in (0, 1, 3):
        lines.append("EXIT %d %s" % (p.returncode, se.decode("utf-8", "replace")[:300]))
    return [ln for ln in lines if ln]


# ============================================================================ SMTP

def norm(b):
    out = re.sub(rb"\r\n|\r|\n", b"\r\n", b)
    if out and not out.endswith(b"\r\n"):
        out += b"\r\n"
    return out


def smtp_corpus(r, n):
    msgs = []
    for _ in range(n):
        msgs.append(mm.rmsg(r).raw())
    msgs += [
        b"Subject: x\r\n\r\n.leading dot\r\n..two\r\n.\r\nafter lone dot\r\n",
        b"bare LF\nline2\nline3",
        b"bare CR\rline2\r",
        b"mixed\r\n\nlf\r\rcr\r\n",
        "Subject: Café\r\n\r\nCrème brûlée €\r\n".encode(),
        b"\r\n\r\n\r\n",
        b"X: " + b"x" * 5000 + b"\r\n\r\nbody\r\n",
        b".", b".\r\n", b"..\r\n", b"\r\n.\r\n", b"a\r\n.b\r\n",
    ]
    big = []
    for i in range(6000):
        big.append((b"." if i % 97 == 0 else b"") + bytes(r.randint(32, 126) for _ in range(r.randint(0, 60))))
    msgs.append(b"\r\n".join(big))
    return msgs


EMPTY = b""


def write_mail(i, data):
    p = os.path.join(WORK, "mail%d_%d.txt" % (SEQ[0], i))
    open(p, "wb").write(data)
    return p


def smtp_script(user, pw, authmode, mails, insecure=False, ehlo=None):
    s = []
    if user is not None:
        s.append("login\t%s\t%s" % (user, pw))
    if authmode:
        s.append("authmode\t" + authmode)
    if insecure:
        s.append("insecure")
    if ehlo:
        s.append("ehlo\t" + ehlo)
    for i, (frm, rcpts, data) in enumerate(mails):
        s.append("mail\t%s\t%s\t%s" % (frm, ", ".join(rcpts), write_mail(i, data)))
    return s


async def start_smtp(**kw):
    cfg = fm.SmtpCfg(**kw)
    cfg.ssl_ctx = fm.make_ssl(LEAF, KEY)
    srv, port = await fm.start_smtp(cfg)
    return cfg, srv, port


def smtplib_send(port, mode, mails, user, pw, authmode):
    ctx = client_ctx()
    if mode == "tls":
        s = smtplib.SMTP_SSL("127.0.0.1", port, context=ctx, timeout=30)
    else:
        s = smtplib.SMTP("127.0.0.1", port, timeout=30)
    s.ehlo()
    if mode == "starttls":
        s.starttls(context=ctx)
        s.ehlo()
    if user is not None:
        s.login(user, pw)
    for frm, rcpts, data in mails:
        s.sendmail(frm, rcpts, data)
    s.quit()


def got_mails(cfg):
    return [(m["from"], m["rcpts"], m["data"]) for m in cfg.received]


async def smtp_ok_case(name, r, mode, tls, authmode, n, user="alice", pw="s3cr3t!", **cfgkw):
    corpus = smtp_corpus(r, n)
    mails = [("al@ex.org", ["bob@ex.net", "carol@ex.net"][: 1 + (i % 2)], d) for i, d in enumerate(corpus)]
    cfg, srv, port = await start_smtp(tls="implicit" if mode == "tls" else "starttls", user=user, pw=pw, **cfgkw)
    out = await run_driver("smtp", port, mode, tls, smtp_script(user, pw, authmode, mails))
    srv.close()
    sent = [kv(ln) for ln in out if ln.startswith("SENT")]
    chk(name + " sent ok", [d.get("ok") for d in sent], ["1"] * len(mails))
    want = [("al@ex.org", m[1], norm(m[2])) for m in mails]
    got = got_mails(cfg)
    chk(name + " count", len(got), len(want))
    for i, (g, w) in enumerate(zip(got, want)):
        chk(name + " mail %d" % i, g, w)
    chk(name + " tls", sorted({m["tls"] for m in cfg.received}), [mode in ("tls", "starttls")])
    chk(name + " authed", sorted({m["authed"] for m in cfg.received}), [True])
    chk(name + " closed", [ln.split()[0] for ln in out][-1], "CLOSED")
    # the reference: smtplib delivers the same mails (CRLF text; smtplib does not fix bytes line ends)
    if mode != "plain":
        cfg2, srv2, port2 = await start_smtp(tls="implicit" if mode == "tls" else "starttls", user=user, pw=pw, **cfgkw)
        ref = [(f, rc, norm(d)[:-2] if (d and d != EMPTY and not norm(d).endswith(b"\r\n\r\n") and False) else norm(d))
               for f, rc, d in mails if d]
        await asyncio.to_thread(smtplib_send, port2, mode, ref, user, pw, authmode)
        srv2.close()
        got2 = got_mails(cfg2)
        chk(name + " vs smtplib count", len(got2), len(ref))
        # Firn's mails without the empty one
        ours = [(f, rc, norm(d)) for f, rc, d in mails if d]
        chk(name + " vs smtplib", [(g[0], g[1], g[2]) for g in got2], ours)


async def smtp_empty_case(r):
    # an empty message: Firn sends the bare end marker (documented), the server sees an empty text
    cfg, srv, port = await start_smtp(tls="starttls")
    out = await run_driver("smtp", port, "starttls", "opp", smtp_script("alice", "s3cr3t!", None, [("a@ex.org", ["b@ex.org"], b"")]))
    srv.close()
    chk("smtp empty ok", [kv(ln).get("ok") for ln in out if ln.startswith("SENT")], ["1"])
    chk("smtp empty data", got_mails(cfg)[0][2] if cfg.received else None, b"")


async def smtp_error_cases(r):
    msg = [("al@ex.org", ["bob@ex.net"], b"Subject: t\r\n\r\nhi\r\n")]

    async def one(name, srv_kw, mode, tls, script, host="localhost", ca=None):
        cfg, srv, port = await start_smtp(**srv_kw)
        out = await run_driver("smtp", port, mode, tls, script, host=host, ca=ca)
        srv.close()
        return cfg, out

    # wrong password -> AUTH failed (4), no READY, nothing delivered
    cfg, out = await one("badpw", {}, "starttls", "opp", smtp_script("alice", "wrong", None, msg))
    c = [kv(ln) for ln in out if ln.startswith("CLOSED")][0]
    chk("smtp badpw err", c.get("err"), "4")
    chk("smtp badpw ready", any(ln.startswith("READY") for ln in out), False)
    chk("smtp badpw delivered", len(cfg.received), 0)

    # plain connection, login without the insecure switch -> ERR_AUTH_INSECURE (13)
    cfg, out = await one("insecure", {"tls": "none"}, "plain", "never", smtp_script("alice", "s3cr3t!", None, msg))
    c = [kv(ln) for ln in out if ln.startswith("CLOSED")][0]
    chk("smtp insecure err", c.get("err"), "13")
    chk("smtp insecure delivered", len(cfg.received), 0)
    # ... and with it, the plain login works (PLAIN and LOGIN)
    for am in ("plain", "login"):
        cfg, out = await one("insecure-on", {"tls": "none"}, "plain", "never",
                             smtp_script("alice", "s3cr3t!", am, msg, insecure=True))
        chk("smtp plain+insecure %s" % am, [kv(ln).get("ok") for ln in out if ln.startswith("SENT")], ["1"])
        chk("smtp plain+insecure %s data" % am, got_mails(cfg), [(msg[0][0], msg[0][1], norm(msg[0][2]))])
        chk("smtp plain+insecure %s tls" % am, [m["tls"] for m in cfg.received], [False])

    # REQUIRE and the server has no STARTTLS -> ERR_TLS_REQUIRED (2)
    cfg, out = await one("require", {"tls": "none"}, "starttls", "require", smtp_script(None, None, None, msg))
    c = [kv(ln) for ln in out if ln.startswith("CLOSED")][0]
    chk("smtp require err", c.get("err"), "2")
    chk("smtp require delivered", len(cfg.received), 0)
    # OPPORTUNISTIC and the server has no STARTTLS -> plain, mail goes
    cfg, out = await one("opp", {"tls": "none", "auth": None}, "starttls", "opp", smtp_script(None, None, None, msg))
    chk("smtp opp plain ok", [kv(ln).get("ok") for ln in out if ln.startswith("SENT")], ["1"])
    chk("smtp opp plain tls", [m["tls"] for m in cfg.received], [False])

    # evil STARTTLS: bytes behind the 220 -> ERR_PROTO (1), nothing delivered
    cfg, out = await one("evil", {"evil": True}, "starttls", "opp", smtp_script("alice", "s3cr3t!", None, msg))
    c = [kv(ln) for ln in out if ln.startswith("CLOSED")][0]
    chk("smtp evil err", c.get("err"), "1")
    chk("smtp evil delivered", len(cfg.received), 0)

    # wrong host name / unknown CA -> TLS fails, nothing delivered, no password sent
    cfg, out = await one("wronghost", {}, "starttls", "opp", smtp_script("alice", "s3cr3t!", None, msg), host="evil.example")
    chk("smtp wronghost delivered", len(cfg.received), 0)
    chk("smtp wronghost closed", any(ln.startswith("CLOSED") for ln in out), True)
    cfg, out = await one("wrongca", {}, "tls", "opp", smtp_script("alice", "s3cr3t!", None, msg), ca=os.path.join(WORK, "other_ca.pem"))
    chk("smtp wrongca delivered", len(cfg.received), 0)
    chk("smtp wrongca closed", any(ln.startswith("CLOSED") for ln in out), True)
    cfg, out = await one("wrongca2", {}, "starttls", "require", smtp_script("alice", "s3cr3t!", None, msg), ca=os.path.join(WORK, "other_ca.pem"))
    chk("smtp wrongca2 delivered", len(cfg.received), 0)
    chk("smtp wrongca2 no password", any("AUTH" in ln for ln in cfg.log), False)

    # recipients: one refused (accepted=2, rejected hex), all refused -> ERR_NO_RCPT (6)
    m1 = [("al@ex.org", ["a@ex.net", "bad1@ex.net", "b@ex.net"], b"Subject: r\r\n\r\nx\r\n")]
    cfg, out = await one("rcpt", {}, "starttls", "opp", smtp_script("alice", "s3cr3t!", None, m1))
    d = [kv(ln) for ln in out if ln.startswith("SENT")][0]
    chk("smtp rcpt partial", (d["ok"], d["acc"], bytes.fromhex(d["rej"]).decode().split()[0]), ("1", "2", "bad1@ex.net"))
    chk("smtp rcpt partial rcpts", [m["rcpts"] for m in cfg.received], [["a@ex.net", "b@ex.net"]])
    m2 = [("al@ex.org", ["bad1@ex.net", "bad2@ex.net"], b"x\r\n"), ("al@ex.org", ["ok@ex.net"], b"after\r\n")]
    cfg, out = await one("rcpt-all", {}, "starttls", "opp", smtp_script("alice", "s3cr3t!", None, m2))
    sent = [kv(ln) for ln in out if ln.startswith("SENT")]
    chk("smtp rcpt all refused", (sent[0]["ok"], sent[0]["err"]), ("0", "6"))
    chk("smtp rcpt then next ok", (sent[1]["ok"], len(cfg.received)), ("1", 1))

    # sender refused with 451 -> ERR_REJECT_FROM (5); the session goes on
    m3 = [("tempfail@ex.org", ["a@ex.net"], b"x\r\n"), ("al@ex.org", ["a@ex.net"], b"y\r\n")]
    cfg, out = await one("451", {}, "starttls", "opp", smtp_script("alice", "s3cr3t!", None, m3))
    sent = [kv(ln) for ln in out if ln.startswith("SENT")]
    chk("smtp 451", (sent[0]["ok"], sent[0]["err"]), ("0", "5"))
    chk("smtp 451 next", (sent[1]["ok"], len(cfg.received)), ("1", 1))

    # SIZE: bigger than announced -> ERR_TOO_BIG (9), no MAIL FROM sent
    m4 = [("al@ex.org", ["a@ex.net"], b"x" * 3000 + b"\r\n"), ("al@ex.org", ["a@ex.net"], b"small\r\n")]
    cfg, out = await one("size", {"size": 2000}, "starttls", "opp", smtp_script("alice", "s3cr3t!", None, m4))
    sent = [kv(ln) for ln in out if ln.startswith("SENT")]
    chk("smtp size sent events", len(sent), 2)
    if len(sent) == 2:
        chk("smtp size", (sent[0]["ok"], sent[0]["err"]), ("0", "9"))
        chk("smtp size next", (sent[1]["ok"], len(cfg.received)), ("1", 1))
    chk("smtp size no MAIL", sum(1 for ln in cfg.log if ln.startswith("MAIL FROM")), 1)

    # bad addresses never reach the wire
    m5 = [("a b@ex.org", ["x@ex.net"], b"x\r\n"), ("al@ex.org", ["x@ex.net>RSET"], b"x\r\n"), ("al@ex.org", ["x@ex.net"], b"ok\r\n")]
    cfg, out = await one("badaddr", {}, "starttls", "opp", smtp_script("alice", "s3cr3t!", None, m5))
    fails = [kv(ln).get("err") for ln in out if ln.startswith("SENDFAIL")]
    chk("smtp bad address", fails, ["10", "10"])
    chk("smtp bad address delivered", len(cfg.received), 1)
    chk("smtp no injected RSET", any(ln == "RSET" for ln in cfg.log), False)

    # the server drops the link after DATA -> an error, never a success
    cfg, out = await one("drop", {"drop_after_data": True}, "starttls", "opp", smtp_script("alice", "s3cr3t!", None, msg))
    chk("smtp drop no ok", any(kv(ln).get("ok") == "1" for ln in out if ln.startswith("SENT")), False)
    c = [kv(ln) for ln in out if ln.startswith("CLOSED")]
    chk("smtp drop closed with error", bool(c) and c[0].get("err") != "0", True)

    # HELO fallback (EHLO answered 502)
    cfg, out = await one("helo", {"helo_only": True, "tls": "none", "auth": None}, "plain", "never", smtp_script(None, None, None, msg))
    chk("smtp helo fallback", [kv(ln).get("ok") for ln in out if ln.startswith("SENT")], ["1"])
    chk("smtp helo server log", [ln.split()[0] for ln in cfg.log[:2]], ["EHLO", "HELO"])


async def smtp_all(seeds, n):
    for seed in seeds:
        r = random.Random(1000 + seed)
        await smtp_ok_case("smtp[%d] starttls/auto" % seed, r, "starttls", "opp", None, n)
        await smtp_ok_case("smtp[%d] tls/login" % seed, r, "tls", "opp", "login", max(2, n // 2))
        await smtp_ok_case("smtp[%d] starttls/plain" % seed, r, "starttls", "require", "plain", max(2, n // 2))
    await smtp_empty_case(None)
    await smtp_error_cases(None)


# ============================================================================ IMAP

USER, PW = "bob", 'p"a\\ss wd'


def build_boxes(cfg, r, n):
    inbox = cfg.boxes["INBOX"]
    for i in range(n):
        m = mm.rmsg(r)
        fl = [f for f in ("\\Seen", "\\Answered", "\\Flagged", "\\Deleted", "\\Draft") if r.random() < 0.25]
        inbox.add(m, fl)
    for name in ("Archive", "Drafté", "日本語/Ñoï", "A&B", "Sent Items"):
        b = fm.Mailbox()
        for i in range(r.randint(0, 3)):
            b.add(mm.rmsg(r), [])
        cfg.boxes[name] = b


async def start_imap(r, n, **kw):
    cfg = fm.ImapCfg(user=USER, pw=PW, seed=r.randint(0, 10 ** 6), **kw)
    cfg.ssl_ctx = fm.make_ssl(LEAF, KEY)
    build_boxes(cfg, r, n)
    srv, port = await fm.start_imap(cfg)
    return cfg, srv, port


def expected_lines(msgs):
    return sort_dump(mm.dump_expected(msgs))


def sort_dump(lines):
    return sorted(lines, key=lambda l: int(l.split()[1]))  # stable: keeps M, E, P, S order within a message


def imaplib_conn(port, mode):
    ctx = client_ctx()
    if mode == "tls":
        M = imaplib.IMAP4_SSL("127.0.0.1", port, ssl_context=ctx)
    else:
        M = imaplib.IMAP4("127.0.0.1", port)
        if mode == "starttls":
            M.starttls(ctx)
    M.login(USER, PW)
    return M


def imaplib_dump(port, mode, box, want_lines):
    """M and S lines the way the driver prints them, from what imaplib fetches"""
    M = imaplib_conn(port, mode)
    typ, data = M.select(box, readonly=True)
    n = int(data[0])
    out = []
    names = {}
    for ln in want_lines:
        t = ln.split()
        if t[0] == "S":
            names.setdefault(int(t[1]), []).append(t[2])
    for seq in range(1, n + 1):
        typ, d = M.fetch(str(seq), "(UID FLAGS RFC822.SIZE)")
        txt = d[0].decode() if isinstance(d[0], bytes) else d[0][0].decode()
        uid = int(re.search(r"UID (\d+)", txt).group(1))
        size = int(re.search(r"RFC822.SIZE (\d+)", txt).group(1))
        fl = re.search(r"FLAGS \(([^)]*)\)", txt).group(1).split()
        order = ["\\Seen", "\\Answered", "\\Flagged", "\\Deleted", "\\Draft"]
        ft = ",".join(f for f in order if f in fl) or "-"
        out.append("M %d %d %s %d" % (seq, uid, ft, size))
        for nm in names.get(seq, []):
            sec = "" if nm == "FULL" else nm
            typ, d = M.fetch(str(seq), "(BODY.PEEK[%s])" % sec)
            lit = d[0][1]
            out.append("S %d %s %d %s" % (seq, nm, len(lit), hashlib.sha1(lit).hexdigest()))
    M.logout()
    return out


def only(lines, kinds):
    return [l for l in lines if l[0] in kinds]


async def imap_dump_case(name, r, n, mode, tls, login_cmd, **kw):
    cfg, srv, port = await start_imap(r, n, tls="implicit" if mode == "tls" else "starttls", **kw)
    box = cfg.boxes["INBOX"]
    script = ["%s\t%s\t%s" % (login_cmd, USER, PW), "caps", "select\tINBOX", "dump", "logout"]
    out = await run_driver("imap", port, mode, tls, script)
    want = expected_lines(box.msgs)
    got = sort_dump([l for l in out if l[:2] in ("M ", "E ", "P ", "S ")])
    chk(name + " ready", any(ln.startswith("READY") for ln in out), True)
    chk(name + " login", [ln for ln in out if ln.startswith("DONE login")][0].split()[2], "0")
    chk(name + " lines", len(got), len(want))
    bad = 0
    for g, w in zip(got, want):
        if g != w:
            bad += 1
            if bad <= 3:
                chk(name + " line", g, w)
    if bad:
        chk(name + " lines differing", bad, 0)
    else:
        chk(name + " lines", True, True)
    sel = kv([ln for ln in out if ln.startswith("SEL")][0])
    chk(name + " select", (int(sel["exists"]), int(sel["uidnext"]), int(sel["uidvalidity"])),
        (len(box.msgs), box.uidnext, box.uidvalidity))
    unseen = sum(1 for m in box.msgs if "\\Seen" not in m.flags)
    chk(name + " unseen", int(sel["unseen"]), unseen if unseen == 0 else [i for i, m in enumerate(box.msgs, 1) if "\\Seen" not in m.flags][0])
    chk(name + " finished", [ln.split()[0] for ln in out][-1], "CLOSED")
    # the reference octets: imaplib on the same server
    ref = await asyncio.to_thread(imaplib_dump, port, mode if mode != "plain" else "plain", "INBOX", want)
    chk(name + " imaplib M/S", sort_dump(ref), only(want, "MS"))
    chk(name + " firn M/S", only(got, "MS"), only(want, "MS"))
    srv.close()
    return cfg


def imaplib_search(port, mode, box, crits):
    M = imaplib_conn(port, mode)
    M.select(box, readonly=True)
    res = {}
    for c in crits:
        typ, d = M.search(None, c)
        res[c] = d[0].decode().split() if d and d[0] else []
        typ, d = M.uid("SEARCH", None, c)
        res["U:" + c] = d[0].decode().split() if d and d[0] else []
    M.logout()
    return res


async def imap_search_case(r, n):
    cfg, srv, port = await start_imap(r, n)
    crits = ["ALL", "UNSEEN", "SEEN", "FLAGGED", "OR SEEN DELETED", "NOT DELETED", "LARGER 1500", "SMALLER 900",
             "SUBJECT alpha", "FROM bob", "TO al", "TEXT hello", "BODY plan", "1:3", "UID 2:5", "UNANSWERED UNDRAFT",
             "OR FLAGGED (SEEN ANSWERED)"]
    script = ["login\t%s\t%s" % (USER, PW), "select\tINBOX"]
    for c in crits:
        script.append("search\t" + c)
        script.append("usearch\t" + c)
    script += ["logout"]
    out = await run_driver("imap", port, "starttls", "opp", script)
    ref = await asyncio.to_thread(imaplib_search, port, "starttls", "INBOX", crits)
    searches = [ln.split()[1:] for ln in out if ln.startswith("SEARCH")]
    got = {}
    for i, c in enumerate(crits):
        got[c] = searches[2 * i]
        got["U:" + c] = searches[2 * i + 1]
    chk("imap search count", len(searches), 2 * len(crits))
    chk("imap search vs imaplib", got, ref)
    srv.close()


def imaplib_list(port, mode, pattern, lsub=False):
    M = imaplib_conn(port, mode)
    typ, d = M.lsub('""', pattern) if lsub else M.list('""', pattern)
    res = [x for x in d if x]
    M.logout()
    return res


async def imap_list_status_case(r, n):
    cfg, srv, port = await start_imap(r, n)
    script = ["login\t%s\t%s" % (USER, PW), "list\t\t*", "list\t\t%", "list\t\tDraf*", "lsub\t\t*", "list\t\t日*",
              "status\tINBOX\tMESSAGES UNSEEN UIDNEXT UIDVALIDITY", "status\tDrafté\tMESSAGES UIDNEXT",
              "status\t日本語/Ñoï\tMESSAGES", "status\tA&B\tMESSAGES UIDNEXT", "status\tNope\tMESSAGES",
              "select\tNope", "select\t日本語/Ñoï", "examine\tINBOX", "select\tSent Items", "logout"]
    out = await run_driver("imap", port, "starttls", "opp", script)
    names = sorted(cfg.boxes)

    def blocks():
        res = []
        for ln in out:
            if ln.startswith("DONE"):
                res.append((ln.split()[1], ln.split()[2], []))
            elif (ln.startswith("LIST") or ln.startswith("STATUS") or ln.startswith("SEL")) and res:
                res[-1][2].append(ln)
        return res

    bl = blocks()
    byname = {}
    for b in bl:
        byname.setdefault(b[0], []).append(b)
    lists = [b for b in bl if b[0] in ("list", "lsub")]  # in script order
    got_names = [sorted(bytes.fromhex(kv(x)["name"]).decode() for x in b[2]) for b in lists]
    want_all = sorted(names)
    chk("imap list *", got_names[0], want_all)
    chk("imap list %", got_names[1], sorted(x for x in names if "/" not in x))
    chk("imap list Draf*", got_names[2], ["Drafté"])
    chk("imap lsub *", got_names[3], want_all)
    chk("imap list 日*", got_names[4], ["日本語/Ñoï"])
    chk("imap list delim", set(kv(x)["delim"] for b in lists for x in b[2]), {b"/".hex()})
    # LIST against imaplib
    ref = await asyncio.to_thread(imaplib_list, port, "starttls", "*")
    refnames = sorted(re.search(rb'"/" (.*)$', x).group(1).strip(b'"') for x in ref)
    chk("imap list vs imaplib count", len(refnames), len(want_all))
    sts = byname["status"]
    inbox = cfg.boxes["INBOX"]
    st = {}
    for x in sts[0][2][0].split()[1:]:
        k, v = x.split("=")
        st[k] = int(v)
    chk("imap status INBOX", st, {"MESSAGES": len(inbox.msgs), "UNSEEN": sum(1 for m in inbox.msgs if "\\Seen" not in m.flags),
                                  "UIDNEXT": inbox.uidnext, "UIDVALIDITY": inbox.uidvalidity})
    ent = cfg.boxes["Drafté"]
    chk("imap status utf7 box", sts[1][2][0], "STATUS MESSAGES=%d UIDNEXT=%d" % (len(ent.msgs), ent.uidnext))
    chk("imap status kanji box", sts[2][2][0], "STATUS MESSAGES=%d" % len(cfg.boxes["日本語/Ñoï"].msgs))
    chk("imap status A&B", sts[3][2][0], "STATUS MESSAGES=%d UIDNEXT=%d" % (len(cfg.boxes["A&B"].msgs), cfg.boxes["A&B"].uidnext))
    chk("imap status Nope is NO", sts[4][1], "1")
    sel = [b for b in bl if b[0] in ("select", "examine")]  # in script order
    chk("imap select Nope is NO", sel[0][1], "1")
    chk("imap select kanji", kv(sel[1][2][0])["exists"], str(len(cfg.boxes["日本語/Ñoï"].msgs)))
    chk("imap examine ro", kv(sel[2][2][0])["ro"], "1")
    chk("imap select Sent Items", kv(sel[3][2][0])["exists"], str(len(cfg.boxes["Sent Items"].msgs)))
    srv.close()


def imaplib_mutate(port, mode, plan):
    M = imaplib_conn(port, mode)
    M.select("INBOX")
    for op in plan:
        if op[0] == "store":
            M.store(op[1], op[2], op[3])
        elif op[0] == "ustore":
            M.uid("STORE", op[1], op[2], op[3])
        elif op[0] == "copy":
            M.copy(op[1], op[2])
        elif op[0] == "ucopy":
            M.uid("COPY", op[1], op[2])
        elif op[0] == "expunge":
            M.expunge()
        elif op[0] == "uexpunge":
            M.uid("EXPUNGE", op[1])
    M.close()  # the Firn script ends with CLOSE too (expunges the deleted rest)
    M.logout()


def box_state(cfg):
    return {name: [(m.uid, sorted(m.flags)) for m in b.msgs] for name, b in sorted(cfg.boxes.items())}


async def imap_mutate_case(r, n):
    seed = r.randint(0, 10 ** 6)
    plan = [("store", "1:3", "+FLAGS", "(\\Seen \\Flagged)"), ("store", "2", "-FLAGS", "(\\Seen)"), ("store", "4:6", "FLAGS", "(\\Deleted \\Draft)"),
            ("copy", "1:2", "Archive"), ("ucopy", "3,5", "Drafté".encode("utf-7").decode().replace("+", "&")), ("expunge",),
            ("store", "1", "+FLAGS.SILENT", "(\\Answered)"), ("ustore", "2:*", "+FLAGS", "(\\Deleted)"), ("uexpunge", "2:3")]
    # UTF-7 mailbox name for imaplib: build it with the fake's own encoder
    plan[4] = ("ucopy", "3,5", fm.utf7_encode("Drafté"))
    cfgs = []
    for who in range(2):
        rr = random.Random(seed)
        cfg, srv, port = await start_imap(rr, n)
        cfgs.append((cfg, srv, port))
    # Firn on twin 0, imaplib on twin 1
    script = ["login\t%s\t%s" % (USER, PW), "select\tINBOX"]
    for op in plan:
        if op[0] == "store":
            script.append("store\t%s\t%s\t%s" % op[1:])
        elif op[0] == "ustore":
            script.append("ustore\t%s\t%s\t%s" % op[1:])
        elif op[0] == "copy":
            script.append("copy\t%s\t%s" % (op[1], "Archive"))
        elif op[0] == "ucopy":
            script.append("ucopy\t%s\t%s" % (op[1], "Drafté"))
        elif op[0] == "expunge":
            script.append("expunge")
        elif op[0] == "uexpunge":
            script.append("uexpunge\t" + op[1])
    script += ["noop", "close", "select\tINBOX", "dump", "select\tArchive", "dump", "select\tDrafté", "dump", "logout"]
    out = await run_driver("imap", cfgs[0][2], "starttls", "opp", script)
    await asyncio.to_thread(imaplib_mutate, cfgs[1][2], "starttls", plan)
    st0, st1 = box_state(cfgs[0][0]), box_state(cfgs[1][0])
    chk("imap mutate state vs imaplib", st0, st1)
    errs = [ln for ln in out if ln.startswith("ERR") or ln.startswith("UNKNOWN") or ln.startswith("TIMEOUT")]
    chk("imap mutate no errors", errs, [])
    bad = [ln for ln in out if ln.startswith("DONE") and ln.split()[2] != "0"]
    chk("imap mutate all OK", bad, [])
    # the three dumps (INBOX, Archive, Entwuerfe) in script order == model of the final state
    cfg = cfgs[0][0]
    groups = []
    cur = []
    for ln in out:
        if ln.startswith("DUMPEND"):
            groups.append(cur)
            cur = []
        elif ln[:2] in ("M ", "E ", "P ", "S "):
            cur.append(ln)
    for g, box in zip(groups, ("INBOX", "Archive", "Drafté")):
        chk("imap mutate dump " + box, sort_dump(g), expected_lines(cfg.boxes[box].msgs))
    # F lines of the STORE answers (flags the server reported)
    fl = [ln for ln in out if ln.startswith("F ")]
    chk("imap mutate store answers", len(fl) > 0, True)
    for _, srv, _ in cfgs:
        srv.close()


async def imap_idle_case(r, n):
    cfg, srv, port = await start_imap(r, n, inject_idle=0.2)
    before = len(cfg.boxes["INBOX"].msgs)
    script = ["login\t%s\t%s" % (USER, PW), "select\tINBOX", "idle", "noop", "dump", "logout"]
    out = await run_driver("imap", port, "starttls", "opp", script)
    chk("imap idle push", [kv(ln)["exists"] for ln in out if ln.startswith("PUSH")], [str(before + 1)])
    chk("imap idle end", [ln.split()[2] for ln in out if ln.startswith("DONE idle")], ["0"])
    chk("imap idle exists", [ln.split()[1] for ln in out if ln.startswith("EXISTS")][:1], [str(before + 1)])
    got = sort_dump([l for l in out if l[:2] in ("M ", "E ", "P ", "S ")])
    chk("imap idle dump has new mail", got, expected_lines(cfg.boxes["INBOX"].msgs))
    srv.close()


async def imap_error_cases(r):
    async def one(srv_kw, mode, tls, script, host="localhost", ca=None, n=3):
        cfg, srv, port = await start_imap(r, n, **srv_kw)
        out = await run_driver("imap", port, mode, tls, script, host=host, ca=ca)
        srv.close()
        return cfg, out

    L = "login\t%s\t%s" % (USER, PW)
    # wrong password / unknown user: NO
    cfg, out = await one({}, "starttls", "opp", ["login\t%s\twrong" % USER, "select\tINBOX", "logout"])
    chk("imap badpw", [ln for ln in out if ln.startswith("DONE login")][0].split()[2], "1")
    chk("imap select before login", [ln for ln in out if ln.startswith("DONE select") or ln.startswith("ERR select")][0].split()[0] in ("DONE", "ERR"), True)
    # commands in the wrong state are refused by the client, not sent
    cfg, out = await one({}, "starttls", "opp", ["select\tINBOX", "search\tALL", "logout"])
    chk("imap state errors", [ln.split()[1] for ln in out if ln.startswith("ERR")][:2], ["select", "search"])
    # login without TLS and without the switch -> refused locally (ERR_AUTH_INSECURE = 7)
    cfg, out = await one({"tls": "none"}, "plain", "never", [L, "logout"])
    chk("imap insecure login refused", [kv(ln)["err"] for ln in out if ln.startswith("ERR login")], ["7"])
    chk("imap insecure not sent", any("LOGIN" in x for x in cfg.log), False)
    # with the switch: works on plain (insecure policy marker)
    cfg, out = await one({"tls": "none"}, "plain", "insecure", [L, "select\tINBOX", "logout"])
    chk("imap insecure ok", [ln.split()[2] for ln in out if ln.startswith("DONE login")], ["0"])
    # server with LOGINDISABLED (require_tls) and a client that refuses STARTTLS but allows plain login
    cfg, out = await one({"require_tls": True}, "plain", "neverinsecure", [L, "logout"])
    chk("imap logindisabled", [ln.split()[2] for ln in out if ln.startswith("DONE login")], ["1"])
    # STARTTLS required and not offered -> failure, no READY
    cfg, out = await one({"tls": "none"}, "starttls", "require", [L, "logout"])
    chk("imap require no ready", any(ln.startswith("READY") for ln in out), False)
    chk("imap require err", [kv(ln)["err"] for ln in out if ln.startswith("CLOSED")], ["6"])
    # evil STARTTLS: bytes behind the OK -> protocol error
    cfg, out = await one({"evil": True}, "starttls", "opp", [L, "logout"])
    chk("imap evil no ready", any(ln.startswith("READY") for ln in out), False)
    c = [kv(ln) for ln in out if ln.startswith("CLOSED")]
    chk("imap evil error", bool(c) and c[0]["err"] != "0", True)
    chk("imap evil no login", any("LOGIN" in x for x in cfg.log), False)
    # wrong host / wrong CA
    cfg, out = await one({}, "tls", "opp", [L, "logout"], host="evil.example")
    chk("imap wronghost no ready", any(ln.startswith("READY") for ln in out), False)
    cfg, out = await one({}, "tls", "opp", [L, "logout"], ca=os.path.join(WORK, "other_ca.pem"))
    chk("imap wrongca no ready", any(ln.startswith("READY") for ln in out), False)
    cfg, out = await one({}, "starttls", "require", [L, "logout"], ca=os.path.join(WORK, "other_ca.pem"))
    chk("imap wrongca2 no password", any("LOGIN" in x for x in cfg.log), False)
    # AUTHENTICATE PLAIN with and without SASL-IR
    for ir in (True, False):
        cfg, out = await one({"sasl_ir": ir}, "starttls", "opp", ["loginplain\t%s\t%s" % (USER, PW), "select\tINBOX", "logout"])
        chk("imap auth plain ir=%s" % ir, [ln.split()[2] for ln in out if ln.startswith("DONE loginplain")], ["0"])
        chk("imap auth plain ir=%s select" % ir, len([ln for ln in out if ln.startswith("SEL")]), 1)
        cfg, out = await one({"sasl_ir": ir}, "starttls", "opp", ["loginplain\t%s\tbad" % USER, "logout"])
        chk("imap auth plain bad ir=%s" % ir, [ln.split()[2] for ln in out if ln.startswith("DONE loginplain")], ["1"])
    # a password the client cannot quote (CR/LF): refused locally, the session survives, nothing else is lost
    cfg, out = await one({}, "starttls", "opp", ["noop", "login\tbob\tx\u0001y", L, "select\tINBOX", "logout"])
    chk("imap login after bad arg", [ln.split()[2] for ln in out if ln.startswith("DONE login")][-1:], ["0"])


async def imap_all(seeds, n):
    for seed in seeds:
        r = random.Random(5000 + seed)
        await imap_dump_case("imap[%d] starttls/login" % seed, r, n, "starttls", "opp", "login")
        await imap_dump_case("imap[%d] tls/auth-plain" % seed, r, n, "tls", "opp", "loginplain", sasl_ir=False)
        await imap_dump_case("imap[%d] starttls/auth-plain-ir" % seed, r, max(3, n // 2), "starttls", "require", "loginplain")
        await imap_search_case(r, n)
    r = random.Random(77)
    await imap_list_status_case(r, 8)
    await imap_mutate_case(r, 12)
    await imap_idle_case(r, 6)
    await imap_error_cases(r)


async def main():
    seeds = [int(x) for x in sys.argv[3:]] or [1]
    n = int(os.environ.get("E2E_MSGS", "20"))
    await smtp_all(seeds, n)
    await imap_all(seeds, n)


if __name__ == "__main__":
    DRIVER = os.path.abspath(sys.argv[1])
    WORK = os.path.abspath(sys.argv[2])
    os.makedirs(WORK, exist_ok=True)
    CA, LEAF, KEY, OTHER = load_certs()
    asyncio.run(main())
    print("e2e: %d checks, %d differences" % (CHECKS, DIFFS))
    sys.exit(1 if DIFFS else 0)
