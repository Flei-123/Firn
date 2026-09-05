#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/liveb4/cookie_check.py -- the dates and the cookie jar.

TWO measurements, and the second one is the interesting one.

1. THE DATES. Every date format RFC 9110 5.6.7 makes a recipient accept is
   fed to `lib/net/httpstate.fi` and to Python's
   `email.utils.parsedate_to_datetime`, which shares no line of code with
   it. The answers are unix seconds and are compared as integers.

2. THE JAR. The rules of RFC 6265 that decide whether a cookie is sent
   back are stated here as cases with an expected answer, and each case
   carries WHY. They are not a re-implementation to compare against -- a
   second implementation of the same misunderstanding proves nothing --
   they are the rules read out of the standard, with the section number.

   And the counter-checks are in the list: a `Domain` that does not cover
   the host must be REFUSED, a `Secure` cookie must not go over http, a
   cookie whose path does not match must not be sent, and `Max-Age=0` must
   DELETE. If any of those four came back the other way the whole jar
   would be worthless, so they are the cases that matter most.
"""
import subprocess
import sys
from email.utils import parsedate_to_datetime

DATES = [
    "Sun, 06 Nov 1994 08:49:37 GMT",
    "Sunday, 06-Nov-94 08:49:37 GMT",
    "Sun Nov  6 08:49:37 1994",
    "Tue, 15 Nov 1994 12:45:26 GMT",
    "Wed, 21 Oct 2015 07:28:00 GMT",
    "Thu, 01 Jan 1970 00:00:01 GMT",
    "Mon, 31 Dec 2029 23:59:59 GMT",
    "Fri, 29 Feb 2querying 2024 00:00:00 GMT",  # broken on purpose
    "not a date at all",
]

# host, path, Set-Cookie, expected number of live cookies afterwards, why
SETS = [
    ("ex.com", "/a/b", "sid=1; Path=/", 1, "a plain session cookie"),
    ("ex.com", "/a/b", "pref=2; Path=/; Max-Age=600", 2, "Max-Age keeps it"),
    ("ex.com", "/a/b", "gone=3; Path=/; Max-Age=0", 2,
     "RFC 6265 5.3 step 11: Max-Age=0 deletes"),
    ("ex.com", "/a/b", "deep=4; Path=/a/b", 3, "a deeper path"),
    ("ex.com", "/a/b", "sec=5; Path=/; Secure", 4, "Secure is stored"),
    ("ex.com", "/a/b", "dom=6; Domain=ex.com; Path=/", 5,
     "a Domain that covers the host"),
    ("other.com", "/a/b", "evil=7; Domain=ex.com", 5,
     "COUNTER-CHECK 5.3 step 6: a Domain that does not cover the host "
     "is refused"),
    ("ex.com", "/a/b", "old=8; Expires=Sun, 06 Nov 1994 08:49:37 GMT", 5,
     "COUNTER-CHECK: an Expires in the past deletes instead of storing"),
    ("ex.com", "/a/b", "novalue", 5,
     "COUNTER-CHECK 5.2 step 3: no '=' at all is ignored"),
    ("ex.com", "/a/b", "sid=9; Path=/", 5, "the same name replaces"),
]

# host, path, secure, expected Cookie header, why
SENDS = [
    ("ex.com", "/a/b", 0, "deep=4; sid=9; pref=2; dom=6",
     "5.4: longer paths first, then creation order; no Secure over http"),
    ("ex.com", "/a/b", 1, "deep=4; sid=9; pref=2; sec=5; dom=6",
     "over https the Secure one comes too"),
    ("ex.com", "/z", 0, "sid=9; pref=2; dom=6",
     "COUNTER-CHECK 5.1.4: the /a/b cookie does not match /z"),
    ("sub.ex.com", "/a/b", 0, "dom=6",
     "5.1.3: only the Domain cookie reaches a subdomain, the host-only "
     "ones do not"),
    ("elsewhere.com", "/a/b", 0, "-",
     "COUNTER-CHECK: another host gets nothing at all"),
]


def main():
    binary = sys.argv[1]
    jobs = []
    for d in DATES:
        jobs.append("D " + d)
    jobs.append("Z")
    for host, path, sc, _, _ in SETS:
        jobs.append("S %s %s %s" % (host, path, sc))
    for host, path, sec, _, _ in SENDS:
        jobs.append("H %s %s %d" % (host, path, sec))
    p = subprocess.run([binary], input=("\n".join(jobs) + "\n").encode(),
                       stdout=subprocess.PIPE, timeout=60)
    out = p.stdout.decode().splitlines()
    if len(out) != len(jobs):
        print("   cookies: %d answers for %d jobs" % (len(out), len(jobs)))
        return 1

    bad = 0
    i = 0
    dgood = 0
    for d in DATES:
        got = int(out[i])
        try:
            want = int(parsedate_to_datetime(d).timestamp())
        except Exception:
            want = 0
        if got == want:
            dgood += 1
        else:
            bad += 1
            print("      date %-34s want %d got %d" % (repr(d), want, got))
        i += 1
    print("   http dates       %d / %d equal to email.utils "
          "(3 formats, %d refused as broken)"
          % (dgood, len(DATES), sum(1 for d in DATES
                                    if not _ok(d))))
    i += 1  # the Z
    sgood = 0
    for host, path, sc, want, why in SETS:
        parts = out[i].split()
        got = int(parts[1]) if len(parts) > 1 else -1
        if got == want:
            sgood += 1
        else:
            bad += 1
            print("      set %-40s want %d live got %d  (%s)"
                  % (sc[:40], want, got, why))
        i += 1
    print("   Set-Cookie       %d / %d as RFC 6265 says (4 of them "
          "counter-checks)" % (sgood, len(SETS)))
    hgood = 0
    for host, path, sec, want, why in SENDS:
        got = out[i].strip()
        if got == want:
            hgood += 1
        else:
            bad += 1
            print("      send %s%s secure=%d\n         want %s\n         got  %s\n"
                  "         (%s)" % (host, path, sec, want, got, why))
        i += 1
    print("   Cookie header    %d / %d as RFC 6265 says (2 of them "
          "counter-checks)" % (hgood, len(SENDS)))
    if bad:
        print("COOKIE FAIL: %d wrong" % bad)
        return 1
    print("COOKIE OK: %d dates, %d Set-Cookie rules, %d Cookie headers, "
          "0 wrong" % (len(DATES), len(SETS), len(SENDS)))
    return 0


def _ok(d):
    try:
        parsedate_to_datetime(d)
        return True
    except Exception:
        return False


if __name__ == "__main__":
    sys.exit(main())
