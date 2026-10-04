#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/desktop/audio_check.py -- lib/audio (player, sink, pulse client).
#
#   usage: audio_check.py <audio_main binary> <mp3_main binary (the plain decoder)> <repo root>
#
#  * FILE sinks: the PCM that the player hands over is byte for byte what the
#    plain decoder (lib/ton/mp3_main.fi) writes; CRC-32 compared with zlib's;
#    WAVE header read back with Python's `wave`; volume against the written
#    formula; pause, stop, junk input; the decoder against ffmpeg (SNR).
#  * a REAL PulseAudio daemon (module-null-sink; `parec`, PulseAudio's own
#    recorder, records the sink's monitor, so the samples that came out of the
#    sound server can be compared) and `pactl` reading the stream back.
import array, os, shutil, struct, subprocess, sys, tempfile, threading, time, wave, zlib

FAILED = []
def check(name, cond, extra=""):
    print(("  OK    " if cond else "  FAIL  ") + name + ((" -- " + str(extra)[:300]) if (extra and not cond) else ""))
    if not cond:
        FAILED.append(name)

audio_main, mp3_ref, root = sys.argv[1], sys.argv[2], sys.argv[3]
td = tempfile.mkdtemp(prefix="audio-check-")
songs = {n: os.path.join(root, "testdata", "ton", n + ".mp3") for n in ("stereo44", "mono24", "shortblocks", "mono8")}

def run(song, percent, mode, env_extra, timeout=60):
    env = dict(os.environ)
    env.pop("PULSE_SERVER", None)
    env.update(env_extra)
    r = subprocess.run([audio_main, songs[song], str(percent), mode], capture_output=True, text=True, env=env, timeout=timeout)
    res = [l for l in r.stdout.splitlines() if l.startswith("RESULT")]
    return r, (res[0].split() if res else None)

def ref_pcm(song):
    out = os.path.join(td, "ref_%s.pcm" % song)
    if not os.path.exists(out):
        subprocess.run([mp3_ref, songs[song], out], capture_output=True, check=True)
    return open(out, "rb").read()

def scale(pcm, percent):
    g = min(percent, 400) * 65536 // 100
    a = array.array("h"); a.frombytes(pcm)
    o = array.array("h")
    for v in a:
        x = abs(v) * g // 65536
        x = x if v >= 0 else -x           # truncation toward zero, like the library
        o.append(max(-32768, min(32767, x)))
    return o.tobytes()

try:
    # ---------------------------------------------------------------- file sinks
    for song in ("stereo44", "mono24", "shortblocks", "mono8"):
        out = os.path.join(td, song + ".pcm")
        r, res = run(song, 100, "play", {"FIRN_AUDIO": "raw:" + out})
        ref = ref_pcm(song)
        got = open(out, "rb").read() if os.path.exists(out) else b""
        check("%s: raw sink equals the plain decoder, byte for byte (%d octets)" % (song, len(ref)), got == ref, (len(got), len(ref), r.stdout, r.stderr))
        check("%s: reported CRC-32 equals zlib's over the output" % song, res is not None and int(res[3]) == zlib.crc32(got), res)
        check("%s: state FINISHED, sink kind raw" % song, res is not None and res[1] == "3" and res[4] == "2", res)
    # WAVE
    out = os.path.join(td, "s44.wav")
    r, res = run("stereo44", 100, "play", {"FIRN_AUDIO": "file:" + out})
    w = wave.open(out, "rb")
    check("WAVE: 2 channels, 44100 Hz, 16 bit", (w.getnchannels(), w.getframerate(), w.getsampwidth()) == (2, 44100, 2), (w.getnchannels(), w.getframerate(), w.getsampwidth()))
    data = w.readframes(w.getnframes())
    check("WAVE: data equals the decoder's PCM", data == ref_pcm("stereo44"))
    check("WAVE: frame count is right", w.getnframes() == len(ref_pcm("stereo44")) // 4, w.getnframes())
    # volume
    for pc in (50, 25, 150, 0):
        out = os.path.join(td, "v%d.pcm" % pc)
        r, res = run("stereo44", pc, "play", {"FIRN_AUDIO": "raw:" + out})
        got = open(out, "rb").read()
        check("volume %d %%: every sample as the formula says" % pc, got == scale(ref_pcm("stereo44"), pc), r.stdout)
        check("volume %d %%: CRC is that of the scaled octets" % pc, res is not None and int(res[3]) == zlib.crc32(got), res)
    # pause: nothing advances while paused; the result is the same
    out = os.path.join(td, "pause.pcm")
    r, res = run("stereo44", 100, "pause", {"FIRN_AUDIO": "raw:" + out})
    check("pause: no progress while paused", "PAUSE ok" in r.stdout, r.stdout + r.stderr)
    check("pause: the output is complete afterwards", open(out, "rb").read() == ref_pcm("stereo44"))
    # stop
    out = os.path.join(td, "stop.pcm")
    r, res = run("stereo44", 100, "stop", {"FIRN_AUDIO": "raw:" + out})
    got = open(out, "rb").read()
    ref = ref_pcm("stereo44")
    check("stop: output is a proper prefix of the song", 0 < len(got) < len(ref) and ref.startswith(got), (len(got), len(ref)))
    check("stop: at least 300 ms were played, not much more", len(got) >= 300 * 44100 * 4 // 1000 and len(got) < 700 * 44100 * 4 // 1000, len(got))
    # junk
    r, res = run("stereo44", 100, "bad", {"FIRN_AUDIO": "null"})
    check("junk input is refused, no crash", "BADOK" in r.stdout and r.returncode == 0, r.stdout + r.stderr)
    # null sink
    r, res = run("stereo44", 100, "play", {"FIRN_AUDIO": "null"})
    check("null sink counts the same CRC", res is not None and int(res[3]) == zlib.crc32(ref_pcm("stereo44")), res)

    # ------------------------------------------------ the decoder against ffmpeg
    if shutil.which("ffmpeg"):
        import numpy as np
        for song in ("stereo44", "mono24"):
            ff = os.path.join(td, song + "_ffmpeg.pcm")
            subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", songs[song], "-f", "s16le", "-acodec", "pcm_s16le", ff], check=True)
            a = np.frombuffer(open(ff, "rb").read(), dtype="<i2").astype(np.float64)
            b = np.frombuffer(ref_pcm(song), dtype="<i2").astype(np.float64)
            chans = 2 if song == "stereo44" else 1
            worst_snr, worst_err = 999.0, 0.0
            for ch in range(chans):
                x, y = a[ch::chans], b[ch::chans]
                n = 1 << int(np.ceil(np.log2(len(x) + len(y))))
                c = np.fft.irfft(np.fft.rfft(x, n) * np.conj(np.fft.rfft(y, n)), n)
                k = int(np.argmax(c)); k = k if k < n // 2 else k - n   # ffmpeg trims the encoder delay: k < 0
                if k >= 0:
                    xs = x[k:k + len(y)]; ys = y[:len(xs)]
                else:
                    ys = y[-k:]; xs = x[:len(ys)]
                m = min(len(xs), len(ys)) - 3000
                e = xs[:m] - ys[:m]
                worst_snr = min(worst_snr, 10 * np.log10((ys[:m] ** 2).sum() / max((e ** 2).sum(), 1e-9)))
                worst_err = max(worst_err, abs(e).max())
            check("%s: decoder vs ffmpeg (aligned over the encoder delay): SNR %.0f dB, largest error %d LSB" % (song, worst_snr, worst_err),
                  worst_snr >= 80 and worst_err <= 2, (worst_snr, worst_err))
    else:
        print("  SKIP  ffmpeg not installed")

    # ----------------------------------------------------------- a real PulseAudio
    if shutil.which("pulseaudio") and shutil.which("pactl"):
        pa = os.path.join(td, "pa"); os.makedirs(pa)
        sock = os.path.join(pa, "native")
        open(os.path.join(pa, "rc.pa"), "w").write(
            "load-module module-native-protocol-unix socket=%s auth-anonymous=1\n"
            "load-module module-null-sink sink_name=nul format=s16le rate=44100 channels=2\n"
            "set-default-sink nul\n" % sock)
        penv = dict(os.environ, HOME=pa, XDG_RUNTIME_DIR=pa)
        penv.pop("PULSE_SERVER", None)
        daemon = subprocess.Popen(["pulseaudio", "-n", "--daemonize=no", "--exit-idle-time=-1", "--disallow-exit",
                                   "--file=" + os.path.join(pa, "rc.pa"), "--log-target=stderr"], env=penv,
                                  stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for _ in range(100):
            if os.path.exists(sock) and "nul" in subprocess.run(["pactl", "--server=unix:" + sock, "list", "short", "sinks"],
                                                                 capture_output=True, text=True).stdout:
                break
            time.sleep(0.1)
        time.sleep(0.5)
        class Cap:
            def __init__(self):
                self.p = None
            def start(self):
                self.f = open(os.path.join(pa, "cap.pcm"), "wb")
                self.p = subprocess.Popen(["parec", "-d", "nul.monitor", "--raw", "--format=s16le", "--rate=44100", "--channels=2"],
                                          stdout=self.f, stderr=subprocess.DEVNULL, env=dict(os.environ, PULSE_SERVER="unix:" + sock))
                # wait until parec really records (silence is flowing), so no sample is missed
                for _ in range(100):
                    if os.path.getsize(os.path.join(pa, "cap.pcm")) > 0:
                        break
                    time.sleep(0.1)
                time.sleep(0.3)
            def stop(self):
                time.sleep(0.8)
                self.p.terminate(); self.p.wait(); self.f.close()
                return open(os.path.join(pa, "cap.pcm"), "rb").read()
        def locate(capture, ref):
            # the song starts with silence: look for the first loud stretch of the reference
            rn = next(k for k, b in enumerate(ref) if b)
            i = capture.find(ref[rn:rn + 4000])
            return i - rn if i >= 0 else -1
        pactl = ["pactl", "--server=unix:" + sock]
        senv = {"PULSE_SERVER": "unix:" + sock}
        try:
            # the stream while it plays
            cap = Cap(); cap.start()
            drv = subprocess.Popen([audio_main, songs["stereo44"], "100", "play"], stdout=subprocess.PIPE, text=True,
                                   env=dict(os.environ, **senv))
            time.sleep(1.0)
            li = subprocess.run(pactl + ["list", "sink-inputs"], capture_output=True, text=True).stdout
            check("pactl sees our stream by name", "Firn audio test" in li, li[:400])
            check("pactl: sample specification s16le 2ch 44100Hz", "s16le 2ch 44100Hz" in li, li[:600])
            check("pactl: media role music", "music" in li, li[:600])
            out = drv.communicate(timeout=30)[0]
            res = [l for l in out.splitlines() if l.startswith("RESULT")]
            check("player finished through PulseAudio", len(res) == 1 and res[0].split()[1] == "3" and res[0].split()[4] == "4", out)
            capture = cap.stop()
            ref = ref_pcm("stereo44")
            off = locate(capture, ref)
            if off < 0 and os.environ.get("AUDIO_CHECK_KEEP"):
                open(os.environ["AUDIO_CHECK_KEEP"], "wb").write(capture)
            check("the sound server delivered our PCM bit for bit (%d octets)" % len(ref), off >= 0 and capture[off:off + len(ref)] == ref, (off, len(capture)))
            check("CRC reported by the program is the CRC of that PCM", len(res) == 1 and int(res[0].split()[3]) == zlib.crc32(ref), res)
            # volume
            cap = Cap(); cap.start()
            r2 = subprocess.run([audio_main, songs["stereo44"], "50", "play"], capture_output=True, text=True, env=dict(os.environ, **senv), timeout=30)
            capture = cap.stop()
            half = scale(ref, 50)
            off = locate(capture, half)
            check("volume 50 percent through PulseAudio: samples scaled bit for bit", off >= 0 and capture[off:off + len(half)] == half, (off, r2.stdout))
            # mono, other rate: the server converts; we only check format and duration
            cap = Cap(); cap.start()
            drv = subprocess.Popen([audio_main, songs["mono24"], "100", "play"], stdout=subprocess.PIPE, text=True,
                                   env=dict(os.environ, **senv))
            time.sleep(0.8)
            li = subprocess.run(pactl + ["list", "sink-inputs"], capture_output=True, text=True).stdout
            check("pactl: mono 24 kHz stream announced as s16le 1ch 24000Hz", "s16le 1ch 24000Hz" in li, li[:600])
            out = drv.communicate(timeout=30)[0]
            capture = cap.stop()
            a = array.array("h"); a.frombytes(bytes(capture[:len(capture) // 4 * 4]))
            nz = sum(1 for v in a[::2] if abs(v) > 200)
            expected = 50112 * 44100 // 24000      # frames of the song after the server resamples 24 kHz to 44.1 kHz
            check("mono through the server: audible samples about as long as the song (%d of ~%d)" % (nz, expected), 0.8 * expected < nz < 1.1 * expected, nz)
            # stop: the stream is deleted and the server goes quiet
            drv = subprocess.Popen([audio_main, songs["stereo44"], "100", "stop"], stdout=subprocess.PIPE, text=True,
                                   env=dict(os.environ, **senv))
            out = drv.communicate(timeout=30)[0]
            time.sleep(0.3)
            li = subprocess.run(pactl + ["list", "short", "sink-inputs"], capture_output=True, text=True).stdout
            check("after stop no stream is left on the server", li.strip() == "", li)
            # no server at all: a clean failure
            r3 = subprocess.run([audio_main, songs["stereo44"], "100", "play"], capture_output=True, text=True,
                                env=dict(os.environ, PULSE_SERVER="unix:" + os.path.join(pa, "nothing-here")), timeout=30)
            check("no server: reported as an error (state 4), no crash", "play failed" not in r3.stdout and "RESULT 4" in r3.stdout, r3.stdout + r3.stderr)
        finally:
            daemon.terminate()
    else:
        print("  SKIP  pulseaudio/pactl not installed")
finally:
    shutil.rmtree(td, ignore_errors=True)

print("audio: %d failed" % len(FAILED) if FAILED else "audio: all checks passed")
sys.exit(1 if FAILED else 0)
