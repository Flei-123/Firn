#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import os
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time
import urllib.parse
import xml.etree.ElementTree as ET

binary = sys.argv[1]
passed = failed = 0
IFACE = "127.0.0.1"
GROUP = "239.255.255.250"
PORT = 19000 + os.getpid() % 2000


def check(label, got, want):
    global passed, failed
    if got == want:
        passed += 1
        print("  ok    %s" % label)
    else:
        failed += 1
        print("  FAIL  %s: got %r, want %r" % (label, got, want))


def group_socket():
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.IPPROTO_UDP)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEPORT, 1)
    s.bind(("", PORT))
    s.setsockopt(socket.IPPROTO_IP, socket.IP_ADD_MEMBERSHIP, socket.inet_aton(GROUP) + socket.inet_aton(IFACE))
    s.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_IF, socket.inet_aton(IFACE))
    s.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_LOOP, 1)
    s.settimeout(0.3)
    return s


def headers(data):
    lines = data.decode("latin-1").split("\r\n")
    h = {}
    for l in lines[1:]:
        if ":" in l:
            k, v = l.split(":", 1)
            h[k.strip().upper()] = v.strip()
    return lines[0], h


def test_search():
    st = "urn:schemas-upnp-org:device:Basic:1"
    s = group_socket()
    seen = []
    stop = threading.Event()

    def serve():
        while not stop.is_set():
            try:
                data, addr = s.recvfrom(2048)
            except socket.timeout:
                continue
            first, h = headers(data)
            if first.startswith("M-SEARCH"):
                seen.append(h)
                resp = ("HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age=1800\r\nEXT:\r\nLOCATION: http://127.0.0.1:8080/desc.xml\r\n"
                        "SERVER: py/1 UPnP/1.1 ref/1\r\nST: %s\r\nUSN: uuid:py-1::%s\r\n\r\n" % (st, st))
                s.sendto(resp.encode(), addr)
                s.sendto(resp.encode(), addr)

    t = threading.Thread(target=serve)
    t.start()
    out = subprocess.run([binary, "search", st, "--ms", "1500", "--port", str(PORT), "--iface", IFACE],
                         capture_output=True, text=True, timeout=30).stdout
    stop.set()
    t.join()
    s.close()
    rows = [l.split("\t") for l in out.splitlines()]
    check("search: M-SEARCH seen", len(seen) >= 1, True)
    check("search: MAN", seen[0].get("MAN") if seen else None, '"ssdp:discover"')
    check("search: ST sent", seen[0].get("ST") if seen else None, st)
    check("search: MX sent", seen[0].get("MX") if seen else None, "1")
    check("search: HOST", seen[0].get("HOST") if seen else None, "239.255.255.250:1900")
    check("search: rows (dedup)", rows, [["ST", st], ["USN", "uuid:py-1::" + st], ["LOCATION", "http://127.0.0.1:8080/desc.xml"]])


def test_serve():
    nt = "urn:schemas-upnp-org:device:Basic:1"
    usn = "uuid:firn-1::" + nt
    loc = "http://127.0.0.1:9090/d.xml"
    s = group_socket()
    p = subprocess.Popen([binary, "serve", nt, usn, loc, "--ms", "2500", "--port", str(PORT), "--iface", IFACE],
                         stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    check("serve: ready", p.stdout.readline().strip(), "READY")
    time.sleep(0.2)
    q = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    q.settimeout(1.5)
    q.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_IF, socket.inet_aton(IFACE))
    msg = ("M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 1\r\nST: ssdp:all\r\n\r\n").encode()
    q.sendto(msg, (GROUP, PORT))
    data, _ = q.recvfrom(2048)
    first, h = headers(data)
    check("serve: status", first, "HTTP/1.1 200 OK")
    check("serve: ST", h.get("ST"), nt)
    check("serve: USN", h.get("USN"), usn)
    check("serve: LOCATION", h.get("LOCATION"), loc)
    check("serve: max-age", h.get("CACHE-CONTROL"), "max-age=1800")
    q.sendto(msg.replace(b"ssdp:all", b"urn:other:device:X:1"), (GROUP, PORT))
    got_other = True
    try:
        q.recvfrom(2048)
    except socket.timeout:
        got_other = False
    check("serve: non-matching ST ignored", got_other, False)
    nts = []
    deadline = time.time() + 4
    p.wait(timeout=10)
    while time.time() < deadline:
        try:
            data, _ = s.recvfrom(2048)
        except socket.timeout:
            break
        first, h = headers(data)
        if first.startswith("NOTIFY"):
            nts.append((h.get("NTS"), h.get("NT"), h.get("USN")))
    check("serve: NOTIFY alive", ("ssdp:alive", nt, usn) in nts, True)
    check("serve: NOTIFY byebye", ("ssdp:byebye", nt, usn) in nts, True)
    s.close()
    q.close()


DESC = """<?xml version="1.0"?>
<root xmlns="urn:schemas-upnp-org:device-1-0"><specVersion><major>1</major><minor>0</minor></specVersion>
<device><deviceType>urn:schemas-upnp-org:device:MediaServer:1</deviceType><friendlyName>Tom &amp; Jerry</friendlyName>
<manufacturer>Acme</manufacturer><modelName>M1</modelName><UDN>uuid:abc-123</UDN>
<serviceList>
<service><serviceType>urn:schemas-upnp-org:service:ContentDirectory:1</serviceType><SCPDURL>/cd.xml</SCPDURL><controlURL>ctl/cd</controlURL><eventSubURL>http://other.host:81/ev</eventSubURL></service>
<service><serviceType>urn:schemas-upnp-org:service:ConnectionManager:1</serviceType><SCPDURL>cm.xml</SCPDURL><controlURL>../ctl/cm</controlURL><eventSubURL>ev/cm</eventSubURL></service>
</serviceList></device></root>"""


def test_desc():
    loc = "http://10.1.2.3:49152/upnp/dev/desc.xml"
    with tempfile.TemporaryDirectory() as td:
        for with_base in (False, True):
            text = DESC.replace("<device>", "<URLBase>http://base.example:7000/root/</URLBase><device>", 1) if with_base else DESC
            path = os.path.join(td, "d.xml")
            with open(path, "w") as f:
                f.write(text)
            out = subprocess.run([binary, "desc", loc, path], capture_output=True, text=True, timeout=30).stdout
            rows = [l.split("\t") for l in out.splitlines()]
            ns = {"u": "urn:schemas-upnp-org:device-1-0"}
            root = ET.fromstring(text)
            dev = root.find("u:device", ns)
            base = root.findtext("u:URLBase", default=loc, namespaces=ns)
            want = [["FRIENDLY", dev.findtext("u:friendlyName", namespaces=ns)],
                    ["MANUFACTURER", dev.findtext("u:manufacturer", namespaces=ns)],
                    ["MODEL", dev.findtext("u:modelName", namespaces=ns)],
                    ["UDN", dev.findtext("u:UDN", namespaces=ns)],
                    ["TYPE", dev.findtext("u:deviceType", namespaces=ns)]]
            for sv in dev.find("u:serviceList", ns):
                want.append(["SERVICE", sv.findtext("u:serviceType", namespaces=ns)])
                for tag, key in (("CONTROL", "controlURL"), ("SCPD", "SCPDURL"), ("EVENTS", "eventSubURL")):
                    want.append([tag, urllib.parse.urljoin(base, sv.findtext("u:" + key, namespaces=ns))])
            check("desc: fields and URL resolution (URLBase=%s)" % with_base, rows, want)
        bad = os.path.join(td, "bad.xml")
        with open(bad, "w") as f:
            f.write("<root><device></root>")
        r = subprocess.run([binary, "desc", loc, bad], capture_output=True, text=True, timeout=30)
        check("desc: malformed XML rejected", r.returncode, 65)


test_desc()
test_search()
test_serve()
print("%d passed, %d failed" % (passed, failed))
sys.exit(1 if failed else 0)
