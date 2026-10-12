#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/audio/wasapi/check.py -- driven by run.sh: WASAPI under Wine + ALSA file plugin.
import os, subprocess, sys
import numpy as np

W = sys.argv[1]
FAILED = []
def check(name, cond, extra=""):
    print(("  ok    " if cond else "  FAIL  ") + name + ((" -- " + str(extra)[:300]) if (extra and not cond) else ""))
    if not cond:
        FAILED.append(name)

raw = os.path.join(W, "alsa-out.raw")
conf = os.path.join(W, "asound.conf")
def set_conf(device):
    open(conf, "w").write(('pcm.null { type null }\npcm.!default {\n    type file\n    slave.pcm "null"\n    file "%s"\n    format "raw"\n}\n' % raw) if device else "")
env = dict(os.environ, ALSA_CONFIG_PATH=conf)
env.pop("PULSE_SERVER", None)

def run(exe, *args, **extra):
    if os.path.exists(raw):
        os.unlink(raw)
    e = dict(env, **extra)
    r = subprocess.run(["wine", os.path.join(W, exe)] + list(args), capture_output=True, text=True, env=e, timeout=150)
    return r, (open(raw, "rb").read() if os.path.exists(raw) else b"")

def pattern(seconds):
    n = np.arange(seconds * 44100, dtype=np.int64)
    l = ((n * 7 + 1000) & 65535).astype("<u2")
    r = (l ^ 65535).astype("<u2")
    return np.stack([l, r], axis=1).astype("<u2").tobytes()

def kv(out):
    d = {}
    for ln in out.splitlines():
        p = ln.split(None, 1)
        if p:
            d[p[0]] = p[1] if len(p) > 1 else ""
    return d

def delivered(data, want):
    # everything the program wrote, in order; Wine/ALSA may add silence in front and drain a last period of zeros
    i = data.find(want[:400])
    return i >= 0 and i <= 4410 * 4 and data[i:i + len(want)] == want and not any(data[i + len(want):]) and len(data) - i - len(want) <= 4410 * 4

try:
    set_conf(True)
    for block in (1152, 4410, 77):
        r, data = run("probe.exe", "2", str(block))
        k = kv(r.stdout)
        want = pattern(2)
        check("WASAPI, blocks of %d frames: Initialize + GetBuffer + ReleaseBuffer path, exit 0, 'WASAPI OK'" % block, r.returncode == 0 and "WASAPI" in k, (r.returncode, r.stdout, r.stderr[-200:]))
        check("WASAPI, blocks of %d: the sound system received the pattern bit for bit (%d octets)" % (block, len(want)), delivered(data, want), (len(data), len(want)))
        if block == 1152:
            check("endpoint list: at least one active render endpoint, named", int(k.get("LIST", "-1").split()[0]) >= 1 and k.get("DEV", "").strip() != "", r.stdout)
            check("open: step 0 (everything worked), buffer of >= 20 ms engine buffer", k.get("OPEN", "").split()[:1] == ["0"] and int(k.get("BUF", "0")) >= 882, r.stdout)
            check("latency reported (GetStreamLatency + padding) between 1 and 200 ms", 1000 <= int(k.get("LAT", "-1")) <= 200000, k.get("LAT"))
            check("drained", k.get("DRAINED", "").strip() == "1", r.stdout)
    # through audio.sink / audio.dev: FIRN_WIN_AUDIO=wasapi, and the default (waveOut) unchanged
    want = pattern(2)
    r, data = run("sink.exe", "2", "1152", FIRN_WIN_AUDIO="wasapi")
    check("audio.sink with FIRN_WIN_AUDIO=wasapi: pattern bit for bit", r.returncode == 0 and delivered(data, want), (r.returncode, r.stdout, len(data)))
    check("audio.sink with FIRN_WIN_AUDIO=wasapi: the program's CRC-32 is that of the delivered octets",
          r.stdout.split()[:2] == ["SINK", "88200"] and int(r.stdout.split()[2]) == __import__("zlib").crc32(want), r.stdout)
    r, data = run("sink.exe", "2", "1152")
    check("audio.sink default (waveOut) unchanged: pattern bit for bit, exact length", r.returncode == 0 and data == want, (r.returncode, len(data)))
    # no device: ALSA without any PCM
    set_conf(False)
    r, data = run("probe.exe", "1", "1152")
    k = kv(r.stdout)
    check("no sound device: NOSINK, exit 1, an error step is reported, no crash", r.returncode == 1 and "NOSINK" in r.stdout and k.get("OPEN", "0").split()[0] != "0", (r.returncode, r.stdout, r.stderr[-200:]))
    print("      (no device: %s)" % " | ".join(x for x in r.stdout.splitlines() if x.startswith(("LIST", "OPEN"))))
    r, data = run("sink.exe", "1", "1152", FIRN_WIN_AUDIO="wasapi")
    check("audio.sink, wasapi requested but no device: waveOut fallback also fails cleanly (NOSINK, exit 1)", r.returncode == 1 and "NOSINK" in r.stdout, (r.returncode, r.stdout))
finally:
    pass
print("wasapi: %d failed" % len(FAILED) if FAILED else "wasapi: all checks passed")
sys.exit(1 if FAILED else 0)
