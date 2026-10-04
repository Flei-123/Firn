#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/desktop/audio_check_win.py -- the Windows sound path (audio.dev.windows: waveOut) under Wine.
#
#   usage: audio_check_win.py <sink_main.exe> <audio_main.exe> <mp3_main (Linux decoder)> <repo root>
#
# Wine's ALSA driver is pointed at ALSA's `file` plugin, so everything that waveOut hands to the sound
# system lands in a raw file: no sound card, no sound server in the way (Wine's PulseAudio driver was
# tried first and loses samples under load -- see docs/DESKTOP.md). The file is then compared with:
#   * a generated pattern (every frame distinct, bit for bit, length exactly right);
#   * the PCM of the plain MP3 decoder, and CRC-32 from zlib, through the player at 100 % and at
#     other volumes (the formula of audio.pcm, written again here).
import array, os, shutil, subprocess, sys, tempfile, zlib

FAILED = []
def check(name, cond, extra=""):
    print(("  OK    " if cond else "  FAIL  ") + name + ((" -- " + str(extra)[:300]) if (extra and not cond) else ""))
    if not cond:
        FAILED.append(name)

if shutil.which("wine") is None:
    print("  SKIP  wine not installed")
    sys.exit(0)
sink_exe, audio_exe, mp3_ref, root = [os.path.abspath(a) for a in sys.argv[1:5]]
td = tempfile.mkdtemp(prefix="audio-win-")
raw = os.path.join(td, "alsa-out.raw")
conf = os.path.join(td, "asound.conf")
open(conf, "w").write('pcm.null { type null }\npcm.!default {\n    type file\n    slave.pcm "null"\n    file "%s"\n    format "raw"\n}\n' % raw)
env = dict(os.environ, WINEPREFIX=os.environ.get("WINEPREFIX", os.path.expanduser("~/.wine-firn")), WINEDEBUG="-all",
           ALSA_CONFIG_PATH=conf)
env.pop("PULSE_SERVER", None)
env.setdefault("DISPLAY", ":0")

def z(p):
    return "Z:" + p.replace("/", "\\")

def run(exe, *args):
    if os.path.exists(raw):
        os.unlink(raw)
    r = subprocess.run(["wine", exe] + list(args), capture_output=True, text=True, env=env, timeout=120)
    data = open(raw, "rb").read() if os.path.exists(raw) else b""
    return r, data

def padded(data, ref):
    """The song as the sound system received it. Every song of ours starts with silence (the MP3 decoder's
    delay: 1336 frames of zeros), and a program that is still decoding its second frame while the device
    has already played the first (a build without optimisation on a loaded machine) leaves a few
    periods of silence THERE and nowhere else; Wine's ALSA driver may also fill its last 10 ms period.
    So: what comes after the leading zeros must equal the song's, octet for octet."""
    def lead(b):
        n = 0
        while n < len(b) and b[n] == 0:
            n += 1
        return n
    ld, lr = lead(data), lead(ref)
    body_d = data[ld:]
    body_r = ref[lr:]
    tail = len(body_d) - len(body_r)
    return ld >= lr and ld - lr <= 4410 * 4 and body_d[:len(body_r)] == body_r and 0 <= tail <= 441 * 4 and not any(body_d[len(body_r):])

def scale(pcm, percent):
    g = min(percent, 400) * 65536 // 100
    a = array.array("h"); a.frombytes(pcm)
    o = array.array("h")
    for v in a:
        x = abs(v) * g // 65536
        x = x if v >= 0 else -x
        o.append(max(-32768, min(32767, x)))
    return o.tobytes()

try:
    # 1. a pattern: frame n = (low16(7n + 1000), its complement), 3 s at 44.1 kHz
    for block in (1152, 4410, 77):
        r, data = run(sink_exe, "3", str(block))
        frames = 3 * 44100
        want = bytearray()
        import numpy as np
        n = np.arange(frames, dtype=np.int64)
        left = ((n * 7 + 1000) & 65535).astype("<u2")
        right = (left ^ 65535).astype("<u2")
        want = np.stack([left, right], axis=1).astype("<u2").tobytes()
        check("pattern, blocks of %d frames: waveOut delivered exactly 3 s, every frame right (%d octets)" % (block, len(want)),
              data == want, (len(data), len(want), r.stdout, r.stderr[-200:]))
        check("pattern, blocks of %d: the program's CRC-32 is that of the delivered octets" % block,
              r.stdout.split()[:2] == ["SINK", str(frames)] and int(r.stdout.split()[2]) == zlib.crc32(data), r.stdout)
    # 2. the MP3 player
    mp3 = os.path.join(root, "testdata", "ton", "stereo44.mp3")
    refpcm_path = os.path.join(td, "ref.pcm")
    subprocess.run([mp3_ref, mp3, refpcm_path], capture_output=True, check=True)
    ref = open(refpcm_path, "rb").read()
    r, data = run(audio_exe, z(mp3), "100", "play")
    check("MP3 through waveOut: the delivered PCM equals the decoder's, bit for bit (%d octets)" % len(ref), padded(data, ref), (len(data), len(ref), r.stdout, r.stderr[-200:]))
    check("MP3: reported CRC equals zlib's over the song's PCM", len(r.stdout.split()) > 3 and int(r.stdout.split()[3]) == zlib.crc32(ref), r.stdout)
    for pc in (50, 150):
        r, data = run(audio_exe, z(mp3), str(pc), "play")
        check("MP3 at %d %% through waveOut: samples scaled by the formula" % pc, padded(data, scale(ref, pc)), (len(data), len(ref)))
    r, data = run(audio_exe, z(mp3), "100", "stop")
    check("stop: a proper prefix of the song reached the device", 0 < len(data) < len(ref) and ref.startswith(data), (len(data), len(ref)))
    r, data = run(audio_exe, z(mp3), "100", "pause")
    check("pause: nothing lost, nothing repeated, no gap (waveOutPause)", padded(data, ref) and "PAUSE ok" in r.stdout, (len(data), len(ref), r.stdout))
    # no device at all (an ALSA configuration without any PCM): the sink says so, the program does not crash
    open(conf, "w").write("")
    r, data = run(sink_exe, "1", "1152")
    check("no sound device: NOSINK, exit 1, no crash", "NOSINK" in r.stdout and r.returncode == 1, (r.stdout, r.returncode, r.stderr[-200:]))
finally:
    subprocess.run(["wineserver", "-k"], env=env)
    shutil.rmtree(td, ignore_errors=True)
print("audio (Windows): %d failed" % len(FAILED) if FAILED else "audio (Windows): all checks passed")
sys.exit(1 if FAILED else 0)
