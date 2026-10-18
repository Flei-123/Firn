"""Cross-check mail.mail (Firn) against Python's email package.

For every random case a Python SMTP sink (socketserver) takes the mail sent by the Firn tool; the
received message is parsed with email.policy.default and compared with the input (subject, from, to,
body text, attachment bytes / name / media type). Python's own EmailMessage built from the same input
must parse to the same values (so both writers agree). Usage: mail_send_cross.py <sendtool> [cases] [seed]
Prints "cases N mismatches M"; exit 0 when M == 0."""
import email, email.policy, os, random, socketserver, subprocess, sys, tempfile, threading
from email.message import EmailMessage

TOOL = sys.argv[1]
N = int(sys.argv[2]) if len(sys.argv) > 2 else 60
SEED = int(sys.argv[3]) if len(sys.argv) > 3 else 1
rnd = random.Random(SEED)
received = []


class H(socketserver.StreamRequestHandler):
    def handle(self):
        w = lambda t: self.wfile.write(t.encode() + b"\r\n")
        w("220 sink")
        env = {"rcpt": []}
        while True:
            line = self.rfile.readline().decode(errors="replace").strip()
            u = line.upper()
            if not line or u.startswith("QUIT"):
                return w("221 bye")
            if u.startswith("DATA"):
                w("354 go")
                rows = []
                while True:
                    row = self.rfile.readline()
                    if row in (b".\r\n", b""):
                        break
                    rows.append(row[1:] if row.startswith(b"..") else row)
                env["data"] = b"".join(rows)
                received.append(env)
                w("250 ok")
            elif u.startswith("EHLO"):
                w("250-sink\r\n250 8BITMIME")
            else:
                if u.startswith("MAIL FROM:"):
                    env["from"] = line[10:].strip("<> ")
                if u.startswith("RCPT TO:"):
                    env["rcpt"].append(line[8:].strip("<> "))
                w("250 ok")


class S(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True
    request_queue_size = 64


srv = S(("127.0.0.1", 0), H)
port = srv.server_address[1]
threading.Thread(target=srv.serve_forever, daemon=True).start()

WORDS = ["alpha", "beta", ".", "..", ".x", "line", "gamma delta", "café", "über", "x" * 90, "", "tab\there", "-- ", "From me"]  # english: ok


def text(n):
    # trailing newline always present; lines never contain CR
    return "".join(rnd.choice(WORDS) + "\n" for _ in range(n)) or "\n"


mism = 0
tmp = tempfile.mkdtemp()
for case in range(N):
    frm = f"s{case}@example.com"
    to = [f"r{case}a@example.org"] + ([f"r{case}b@example.org"] if rnd.random() < 0.3 else [])
    subject = rnd.choice(["Report", "Hello World", "café über", "x" * 100 + " tail", "Re: a, b; c"]) + f" {case}"  # english: ok
    body = text(rnd.randint(1, 12))
    atts = []
    for k in range(rnd.choice([0, 0, 1, 2])):
        data = bytes(rnd.getrandbits(8) for _ in range(rnd.choice([0, 1, 57, 58, 500, 3000])))
        atts.append((f"file{k}.bin", rnd.choice(["application/octet-stream", "text/csv", "image/png"]), data))
    args = [TOOL, str(port), frm, ",".join(to), subject, os.path.join(tmp, "body")]
    open(args[5], "wb").write(body.encode())
    for i, (name, ct, data) in enumerate(atts):
        pth = os.path.join(tmp, f"att{i}")
        open(pth, "wb").write(data)
        args += [pth, ct, name]
    before = len(received)
    rc = subprocess.run(args, timeout=60).returncode
    errs = []
    if rc != 0 or len(received) != before + 1:
        errs.append(f"send rc={rc} received={len(received) - before}")
    else:
        env = received[-1]
        m = email.message_from_bytes(env["data"], policy=email.policy.default)
        ref = EmailMessage()
        ref["From"], ref["To"], ref["Subject"] = frm, ", ".join(to), subject
        ref.set_content(body)
        for name, ct, data in atts:
            mt, st = ct.split("/")
            ref.add_attachment(data, maintype=mt, subtype=st, filename=name)
        ref = email.message_from_bytes(ref.as_bytes(), policy=email.policy.default)
        for label, msg in (("firn", m), ("ref", ref)):
            if msg["Subject"] != subject: errs.append(f"{label} subject {msg['Subject']!r}")
            if str(msg["From"]) != frm: errs.append(f"{label} from {msg['From']!r}")
            if sorted(a.addr_spec for a in msg["To"].addresses) != sorted(to): errs.append(f"{label} to")
            got_atts = [(p.get_filename(), p.get_content_type(), p.get_payload(decode=True)) for p in msg.iter_attachments()]
            want = [(n, c, d if c != "text/csv" else d) for n, c, d in atts]
            norm = lambda lst: [(n, c, d if isinstance(d, bytes) else d.encode()) for n, c, d in lst]
            if [(n, c) for n, c, _ in got_atts] != [(n, c) for n, c, _ in want]: errs.append(f"{label} attachment names/types")
            elif [d for *_, d in got_atts] != [d for *_, d in want]:
                errs.append(f"{label} attachment bytes")
            b = msg.get_body(("plain",))
            if (b.get_content() if b else "").replace("\r\n", "\n") != body: errs.append(f"{label} body {(b.get_content() if b else None)!r} want {body!r}"[:400])
        if env.get("from") != frm: errs.append("envelope from")
        if sorted(env["rcpt"]) != sorted(to): errs.append("envelope rcpt")
    if errs:
        mism += 1
        print("case", case, errs)
print(f"cases {N} mismatches {mism}")
sys.exit(1 if mism else 0)
