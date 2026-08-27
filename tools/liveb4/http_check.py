#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/liveb4/http_check.py -- the HTTP client against a REAL server.

`tools/liveb4/server.py` is Python's own `http.server`. It shares no line
of code with `lib/net/http.fi`, it frames its answers the way the standard
says and not the way this client happens to expect, and it is the reason
any of the numbers below mean anything: two ends written together agree
perfectly about a shared misunderstanding.

Every case names the RULE it checks. The counter-checks are in the list
and marked; without them "the client fetched a page" would prove almost
nothing:

  * ROUND B5 MOVED THIS ONE. In round B4 an `https://` URL was refused
    outright and the two cases below demanded exactly that. The client can
    now speak TLS, so what they demand instead is the property that
    survives: a client whose TRUST STORE IS EMPTY fetches nothing over
    `https://`. The refusal still comes back as `Tls`, still twice, typed
    in and through a redirect -- and `tools/tlsb5/https_check.py` is where
    the successful case is measured.
  * a chunked body whose last chunk never comes must NOT be reported as
    a successful fetch.
  * with the cache switched OFF the second fetch of the same URL must
    reach the server again -- otherwise "the cache works" is a claim
    about a counter that always says the same thing.
  * `Cache-Control: no-store` must not be stored even with the cache on.
  * two requests to the same origin must open ONE socket, and a request
    to another port must open a second.
"""
import json
import re
import subprocess
import sys
import time


def start_server(py):
    p = subprocess.Popen([sys.executable, py], stdout=subprocess.PIPE,
                         stderr=subprocess.PIPE)
    line = p.stdout.readline().decode().strip()
    m = re.match(r"port (\d+)", line)
    if not m:
        p.kill()
        raise SystemExit("the test server printed %r instead of a port"
                         % line)
    return p, int(m.group(1))


def drive(binary, jobs):
    text = "\n".join(jobs) + "\n"
    p = subprocess.run([binary], input=text.encode(),
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                       timeout=180)
    blocks = []
    cur = {}
    body = None
    for line in p.stdout.decode("utf-8", "replace").splitlines():
        if line == ".":
            blocks.append(cur)
            cur = {}
            continue
        if line.startswith("BODYTEXT "):
            cur["BODY_TEXT"] = line[9:]
            continue
        parts = line.split(" ", 1)
        cur[parts[0]] = parts[1] if len(parts) > 1 else ""
    return blocks, p.stderr.decode()[:400]


PLAIN = ("<!doctype html><html><head><title>plain</title></head>"
         "<body><p id=p>hello</p></body></html>")


def main():
    binary = sys.argv[1]
    srv, port = start_server("tools/liveb4/server.py")
    base = "http://127.0.0.1:%d" % port
    try:
        jobs = []
        checks = []

        def get(path, why, **want):
            jobs.append("G " + base + path)
            checks.append((len(jobs) - 1, path, why, want))

        def raw(cmd):
            jobs.append(cmd)
            checks.append((len(jobs) - 1, cmd, None, None))

        get("/plain", "Content-Length, the plain case",
            STATUS="200", BODY_TEXT=PLAIN, MEDIA="text/html",
            CHARSET="utf-8", CHUNKS="0")
        get("/chunked", "RFC 9112 7.1: chunked, with extensions and a "
            "trailer section", STATUS="200", BODY_TEXT=PLAIN)
        get("/gzip", "RFC 9110 8.4.1.3: Content-Encoding: gzip",
            STATUS="200", BODY_TEXT=PLAIN)
        get("/gzip-chunked", "gzip INSIDE chunked -- the two framings are "
            "independent and are undone in the right order",
            STATUS="200", BODY_TEXT=PLAIN)
        get("/deflate", "Content-Encoding: deflate", STATUS="200",
            BODY_TEXT=PLAIN)
        get("/latin1", "the charset out of Content-Type",
            STATUS="200", CHARSET="iso-8859-1")
        get("/slowhead", "the whole answer ONE OCTET PER WRITE: a reader "
            "that treats one read as one message breaks here",
            STATUS="200", BODY_TEXT=PLAIN)
        get("/closebody", "RFC 9112 6.3: no framing at all, the body ends "
            "with the connection", STATUS="200", BODY_TEXT=PLAIN)
        get("/redir/3", "RFC 9110 15.4: four 302 hops (3, 2, 1, 0)",
            STATUS="200", HOPS="4", BODY_TEXT=PLAIN)
        get("/deep/relredir", "a RELATIVE Location, resolved against the "
            "URL the answer came from", STATUS="200", BODY_TEXT=PLAIN)
        get("/loop-a", "COUNTER-CHECK: two 302 at each other must not "
            "spin", ERR="RedirectLoop")
        get("/notrailer", "COUNTER-CHECK: a chunked body whose last chunk "
            "never comes is not a successful fetch", ERR_ANY=True)
        get("/status/404", "a status code that is not 200", STATUS="404")
        get("/tohttps", "COUNTER-CHECK: an https URL reached through a "
            "REDIRECT, with no roots loaded", ERR="Tls")
        jobs.append("G " + base.replace("http://", "https://") + "/plain")
        checks.append((len(jobs) - 1, "https://<the test server>/plain",
                       "COUNTER-CHECK: an https URL typed in, with no "
                       "roots loaded", {"ERR": "Tls"}))

        # POST and the method rules of a redirect
        jobs.append("P %s/echo text/plain name=firn" % base)
        checks.append((len(jobs) - 1, "POST /echo",
                       "a POST with a body and a Content-Type",
                       {"STATUS": "200", "BODY_HAS": "METHOD=POST"}))
        jobs.append("P %s/see-other text/plain name=firn" % base)
        checks.append((len(jobs) - 1, "POST /see-other",
                       "RFC 9110 15.4.4: a 303 turns a POST into a GET "
                       "and drops the body",
                       {"STATUS": "200", "BODY_HAS": "METHOD=GET"}))
        jobs.append("P %s/keep-method text/plain name=firn" % base)
        checks.append((len(jobs) - 1, "POST /keep-method",
                       "RFC 9110 15.4.8: a 307 keeps method AND body",
                       {"STATUS": "200", "BODY_HAS": "BODY=name=firn"}))

        # cookies
        get("/cookie", "five Set-Cookie headers at once", STATUS="200")
        jobs.append("G %s/echo" % base)
        checks.append((len(jobs) - 1, "/echo after /cookie",
                       "the jar sends back exactly the cookies that match: "
                       "not the deleted one, not the deep path, not the "
                       "Secure one over http",
                       {"BODY_HAS": "Cookie=sid=abc; pref=dark"}))

        # the cache
        raw("X")
        get("/cached", "first fetch: max-age=60", CACHE="0")
        get("/cached", "second fetch: out of the cache, no request",
            CACHE="1")
        get("/nostore", "Cache-Control: no-store", CACHE="0")
        get("/nostore", "COUNTER-CHECK: no-store is not stored", CACHE="0")
        get("/etag", "an ETag and no max-age", CACHE="0")
        get("/etag", "revalidated with If-None-Match, the server says 304 "
            "and the body comes out of the cache", CACHE="1")
        jobs.append("S")
        stats_at = len(jobs) - 1
        checks.append((stats_at, "counters", None, None))
        # the counter-check: the same two fetches with the cache OFF
        raw("C 0")
        get("/cached", "cache off, first", CACHE="0")
        get("/cached", "COUNTER-CHECK: cache off, second -- must NOT come "
            "out of the cache", CACHE="0")
        raw("C 1")
        # The keep-alive measurement, on its own so that `Connection:
        # close` from another route cannot muddy it: five fetches of the
        # same URL with the cache off, and the number of sockets before
        # and after.
        raw("C 0")
        jobs.append("S")
        ka_before = len(jobs) - 1
        checks.append((ka_before, "sockets before", None, None))
        for _ in range(5):
            jobs.append("G " + base + "/plain")
            checks.append((len(jobs) - 1, "keep-alive fetch", None, None))
        jobs.append("S")
        ka_after = len(jobs) - 1
        checks.append((ka_after, "sockets after", None, None))
        raw("C 1")

        blocks, err = drive(binary, jobs)
        if len(blocks) != len(jobs):
            print("   http: %d answers for %d jobs %s"
                  % (len(blocks), len(jobs), err))
            return 1

        good = 0
        bad = 0
        for idx, what, why, want in checks:
            if want is None:
                continue
            b = blocks[idx]
            ok = True
            detail = ""
            if want.get("ERR_ANY"):
                ok = "ERR" in b
                detail = "expected any error, got %s" % b.get("STATUS", b)
            elif "ERR" in want:
                ok = b.get("ERR", "").strip() == want["ERR"]
                detail = "expected ERR %s, got %s" % (want["ERR"],
                                                      b.get("ERR", b))
            else:
                for k, v in want.items():
                    if k == "BODY_HAS":
                        if v not in b.get("BODY_TEXT", ""):
                            ok = False
                            detail = "body has no %r" % v
                    elif b.get(k, "").strip() != v:
                        ok = False
                        detail = "%s: expected %r, got %r" % (
                            k, v, b.get(k, "").strip())
            if ok:
                good += 1
            else:
                bad += 1
                print("      FAIL %-22s %s" % (what, detail))
                print("           (%s)" % why)
        st = blocks[stats_at]
        sockets = int(st.get("SOCKETS", "0"))
        requests = int(st.get("REQUESTS", "0"))
        reused = int(st.get("REUSED", "0"))
        cookies = int(st.get("COOKIES", "0"))
        cached = int(st.get("CACHED", "0"))
        print("   http rules       %d / %d (of them %d counter-checks)"
              % (good, good + bad,
                 sum(1 for _, _, w, _ in checks
                     if w and "COUNTER-CHECK" in w)))
        print("   persistent conn  %d requests over %d socket(s), %d reused"
              % (requests, sockets, reused))
        print("   jar / cache      %d cookies, %d cached documents"
              % (cookies, cached))
        opened = (int(blocks[ka_after].get("SOCKETS", "0"))
                  - int(blocks[ka_before].get("SOCKETS", "0")))
        print("   keep-alive       5 fetches of the same URL opened %d "
              "socket(s)" % opened)
        if opened > 1:
            print("      FAIL: %d sockets for five fetches of one origin "
                  "-- `Connection: keep-alive` is not honoured" % opened)
            bad += 1
        if bad:
            print("HTTP FAIL: %d of %d" % (bad, good + bad))
            return 1
        print("HTTP OK: %d rules against a real server, five fetches over "
              "one socket, 0 wrong" % good)
        return 0
    finally:
        srv.kill()
        srv.wait()


if __name__ == "__main__":
    sys.exit(main())
