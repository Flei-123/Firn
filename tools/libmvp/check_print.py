#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/libmvp/check_print.py <print_probe> <workdir> <file.pdf> -- lib/print
# against a real IPP Everywhere printer (CUPS' ippeveprinter: it checks
# every request the way a printer does and spools what it receives) and
# against a stand-in for the CUPS scheduler (CUPS-Get-Printers and
# CUPS-Get-Default, which only CUPS itself answers).
import http.server, os, shutil, socket, struct, subprocess, sys, threading, time
probe, work, pdf = sys.argv[1], sys.argv[2], sys.argv[3]
bad = 0
def fail(m):
    global bad
    bad += 1
    print("  FAIL", m)
def ok(m):
    print("  ok  ", m)
def run(*a):
    return subprocess.run([probe] + list(a), capture_output=True, text=True, timeout=60).stdout.strip()
def free_port():
    s = socket.socket(); s.bind(("127.0.0.1", 0)); p = s.getsockname()[1]; s.close(); return p

# ---- a real IPP printer
if shutil.which("ippeveprinter"):
    spool = os.path.join(work, "spool"); os.makedirs(spool, exist_ok=True)
    port = free_port()
    ipp = subprocess.Popen(["ippeveprinter", "-p", str(port), "-f", "application/pdf,image/jpeg", "-k", "-d", spool,
                            "-r", "off", "-2", "-l", "Werkstatt", "-M", "Firn", "-m", "TestPrinter", "FirnTest"],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    uri = "ipp://127.0.0.1:%d/ipp/print" % port
    for _ in range(50):
        try:
            socket.create_connection(("127.0.0.1", port), 0.2).close(); break
        except OSError:
            time.sleep(0.1)
    try:
        a = run("attrs", uri).split("|")
        if len(a) == 9 and a[0] == "FirnTest" and a[3] == "Werkstatt" and a[4] == "Firn TestPrinter" and a[5] == "3" \
                and a[6][0] == "1" and a[6][2] == "1" and "iso_a4_210x297mm" in a[7] and "application/pdf" in a[8]:
            ok("Get-Printer-Attributes: name, location, model, idle, accepting, duplex, media, formats")
        else:
            fail("attributes %r" % a)
        r = run("print", uri, pdf, "2", "1", "iso_a4_210x297mm")
        job = r.split()[1] if r.startswith("JOB ") else None
        if job:
            ok("Print-Job accepted: job %s" % job)
        else:
            fail("print: %r" % r)
        # follow the job to the end
        state, t0 = None, time.time()
        while job and time.time() - t0 < 30:
            state = run("state", uri, job)
            if state == "STATE 9":
                break
            time.sleep(0.5)
        if state == "STATE 9":
            ok("Get-Job-Attributes: the job reached 'completed'")
        else:
            fail("job state %r" % state)
        spooled = [f for f in os.listdir(spool) if f.endswith(".pdf")]
        if len(spooled) == 1 and open(os.path.join(spool, spooled[0]), "rb").read() == open(pdf, "rb").read():
            ok("the printer received the PDF octet for octet (%d octets)" % os.path.getsize(pdf))
        else:
            fail("spool %r" % spooled)
        # a second job, cancelled while it is being processed
        r = run("print", uri, pdf, "1", "0", "")
        job2 = r.split()[1] if r.startswith("JOB ") else None
        c = run("cancel", uri, job2) if job2 else ""
        s, t0 = "", time.time()
        while job2 and time.time() - t0 < 20:
            s = run("state", uri, job2)  # the printer stops at the next page
            if s == "STATE 7":
                break
            time.sleep(0.3)
        if c == "CANCELED" and s == "STATE 7":
            ok("Cancel-Job: job %s is 'canceled'" % job2)
        else:
            fail("cancel %r %r %r" % (r, c, s))
        r = run("state", uri, "999")
        if r == "ERROR Ipp 1030":
            ok("an unknown job: IPP status 0x0406 client-error-not-found")
        else:
            fail("unknown job %r" % r)
    finally:
        ipp.terminate(); ipp.wait()
else:
    print("  skip: ippeveprinter (cups-ipp-utils) not installed")

# ---- refusals without a printer
r = run("print", "ipp://127.0.0.1:%d/ipp/print" % free_port(), pdf, "1", "0", "")
(ok if r == "ERROR NoServer 0" else fail)("nothing listening: NoServer (%s)" % r)
r = run("print", "ipps://printer.example/ipp/print", pdf, "1", "0", "")
(ok if r == "ERROR BadPrinter 0" else fail)("ipps:// refused: BadPrinter (%s)" % r)

# ---- a stand-in for the CUPS scheduler
def attr(tag, name, value):
    if isinstance(value, int) and tag in (0x21, 0x23):
        value = struct.pack(">i", value)
    elif isinstance(value, bool):
        value = bytes([1 if value else 0])
    elif isinstance(value, str):
        value = value.encode()
    return bytes([tag]) + struct.pack(">H", len(name)) + name.encode() + struct.pack(">H", len(value)) + value
def more(tag, value):
    return bytes([tag]) + struct.pack(">H", 0) + struct.pack(">H", len(value)) + value.encode()
seen = []
class Cups(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_POST(self):
        body = self.rfile.read(int(self.headers["Content-Length"]))
        ver, op, rid = struct.unpack(">HHI", body[:8])
        seen.append((ver, op, self.headers["Content-Type"], b"attributes-charset" in body, b"requested-attributes" in body))
        out = struct.pack(">HHI", 0x0200, 0, rid) + b"\x01" + attr(0x47, "attributes-charset", "utf-8") + attr(0x48, "attributes-natural-language", "en")
        if op == 0x4002:
            for name, loc, color in (("Office", "Buero 2", True), ("Werkstatt-A3", "Halle", False)):
                out += b"\x04" + attr(0x42, "printer-name", name) + attr(0x45, "printer-uri-supported", "ipp://localhost:631/printers/" + name)
                out += attr(0x41, "printer-info", name + " printer") + attr(0x41, "printer-location", loc)
                out += attr(0x23, "printer-state", 3) + attr(0x22, "printer-is-accepting-jobs", True) + attr(0x22, "color-supported", color)
                out += attr(0x44, "media-supported", "iso_a4_210x297mm") + more(0x44, "iso_a3_297x420mm")
        elif op == 0x4001:
            out += b"\x04" + attr(0x42, "printer-name", "Werkstatt-A3")
        out += b"\x03"
        self.send_response(200); self.send_header("Content-Type", "application/ipp"); self.send_header("Content-Length", str(len(out)))
        self.end_headers(); self.wfile.write(out)
srv = http.server.HTTPServer(("127.0.0.1", 0), Cups)
threading.Thread(target=srv.serve_forever, daemon=True).start()
lines = run("list", "http://127.0.0.1:%d/" % srv.server_address[1]).splitlines()
want = ["Office|ipp://localhost:631/printers/Office|Office printer|Buero 2||3|1100|iso_a4_210x297mm,iso_a3_297x420mm|",
        "Werkstatt-A3|ipp://localhost:631/printers/Werkstatt-A3|Werkstatt-A3 printer|Halle||3|1001|iso_a4_210x297mm,iso_a3_297x420mm|",
        "END"]
if lines == want:
    ok("CUPS-Get-Printers: two printers, all fields, the default marked (CUPS-Get-Default)")
else:
    fail("list %r" % lines)
if len(seen) == 2 and all(s[0] == 0x0200 and s[2] == "application/ipp" and s[3] for s in seen) and seen[0][1] == 0x4002 and seen[1][1] == 0x4001:
    ok("requests: IPP/2.0, application/ipp, charset and language first")
else:
    fail("requests %r" % seen)
srv.shutdown()
print("print: %d failed" % bad)
sys.exit(1 if bad else 0)
