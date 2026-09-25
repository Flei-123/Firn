#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/libmvp/check_time.py -- compares the output of time_probe with
# Python's datetime (UTC text) and zoneinfo (offsets), read from stdin.
import sys, datetime as dt, zoneinfo
ZONES = ["Europe/Vienna", "America/New_York", "Australia/Lord_Howe", "Asia/Kathmandu", "America/Sao_Paulo"]
zi = [zoneinfo.ZoneInfo(z) for z in ZONES]
bad = n = 0
for line in sys.stdin:
    f = line.split()
    ms = int(f[0])
    t = dt.datetime(1970, 1, 1, tzinfo=dt.timezone.utc) + dt.timedelta(milliseconds=ms)
    want = [t.strftime("%Y-%m-%dT%H:%M:%S.") + "%03d" % (t.microsecond // 1000) + "Z"]
    for z in zi:
        off = t.astimezone(z).utcoffset().total_seconds()
        want.append(str(int(off // 60)))
    n += 1
    if f[1:] != want:
        bad += 1
        if bad <= 10:
            print("DIFF", ms, "firn:", " ".join(f[1:]), "python:", " ".join(want))
print("time: %d instants x (1 text + %d zones), %d differ" % (n, len(ZONES), bad))
sys.exit(1 if bad or n < 20000 else 0)
