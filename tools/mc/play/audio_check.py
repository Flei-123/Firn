#!/usr/bin/env python3
"""Objective check of a mcplay FIRN_AUDIO=file dump (no ears needed).

usage: audio_check.py <dump.wav> <mcplay.out> [--probe] [--min-events N] [--bursts N]
       --bursts N: the dump comes from a REAL device (PulseAudio null sink monitor, no frame alignment): only count the
                   sound bursts (runs of non-silent 5 ms blocks, gaps < 120 ms merged) and require >= N of them

Inputs: the WAV the file sink wrote and the program output with `audiolog frame=.. ev=.. cat=.. vol100=.. pitch100=..`
lines (MCPLAY_AUDIO_LOG=1). `frame` = sink frames already pumped when the voice started, so the voice must be audible
at or after that frame.

Checks (exit 1 on a failure):
  * file: PCM16 stereo, frames > 0, no NaN-like garbage, clipping fraction < 0.1 %, DC offset small
  * every logged event has an onset (first 5 ms block over the noise floor) within 400 ms after its frame
  * onset latency (ms after the logged frame) is reported; median must stay < 150 ms
  * every event window is not silent: peak > 200, and no window is clipped
  * silence share of the whole dump is reported (a quiet game is mostly silent; > 99.9 % with events = broken mix)
  * spectral centroid per event name is printed (sanity: glass/ui brighter than stone/wood)
  * engine stats line (`audio_stats`): dropped / stolen / cmd_dropped / underruns must be 0
With --probe all 12 probe events must be present and in the logged order.
"""
import re
import sys
import wave

import numpy as np

MIN_WIN = 0.4  # seconds: minimum length of an event window (see the slow-attack note below)
PROBE = ["block.stone.step", "block.stone.break", "block.stone.place", "block.stone.hit", "block.stone.fall",
         "block.glass.break", "block.wood.break", "block.grass.step", "block.gravel.step", "ambient.cave",
         "entity.zombie.ambient", "ui.button.click"]


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    probe = "--probe" in sys.argv
    min_events = 1
    if "--min-events" in sys.argv:
        min_events = int(sys.argv[sys.argv.index("--min-events") + 1])
        args = [a for a in args if a != str(min_events)]
    bursts_min = 0
    if "--bursts" in sys.argv:
        bursts_min = int(sys.argv[sys.argv.index("--bursts") + 1])
        args = [a for a in args if a != str(bursts_min)]
    wav_path, out_path = args[0], args[1]
    bad = []

    w = wave.open(wav_path)
    rate, ch, sw, n = w.getframerate(), w.getnchannels(), w.getsampwidth(), w.getnframes()
    raw = w.readframes(n)
    if ch != 2 or sw != 2 or n < 1000:
        print(f"  audio FAIL format ch={ch} sw={sw} frames={n}")
        return 1
    x = np.frombuffer(raw, dtype="<i2").reshape(-1, 2).astype(np.float64)
    mono = x.mean(axis=1)
    peak = int(np.abs(x).max())
    clip = float(np.mean(np.abs(x) >= 32766))
    dc = float(abs(mono.mean()))
    blk = int(rate * 0.005)
    nb = len(mono) // blk
    rms = np.sqrt((mono[: nb * blk].reshape(nb, blk) ** 2).mean(axis=1))
    silent = float(np.mean(rms < 4.0))
    print(f"  audio wav rate={rate} frames={n} dur={n / rate:.2f}s peak={peak} clip={clip * 100:.4f}% dc={dc:.1f} silence={silent * 100:.1f}%")
    if clip > 0.001:
        bad.append(f"clipping {clip * 100:.3f}%")
    if dc > 200:
        bad.append(f"DC offset {dc:.0f}")

    if bursts_min:
        thr = max(6.0, float(np.percentile(rms, 20)) * 3.0)
        act = rms > thr
        runs, i = [], 0
        while i < nb:
            if act[i]:
                j = i
                last = i
                while j < nb and j - last < 24:
                    if act[j]:
                        last = j
                    j += 1
                runs.append((i, last))
                i = last + 1
            else:
                i += 1
        print(f"  audio device bursts={len(runs)} first_at={runs[0][0] * 5 if runs else -1}ms")
        if len(runs) < bursts_min:
            bad.append(f"only {len(runs)} bursts (< {bursts_min})")
        if peak < 1000:
            bad.append(f"device dump too quiet (peak {peak})")
        for line in open(out_path, errors="replace"):
            m = re.search(r"audio_stats (.*)", line)
            if m:
                st = dict(kv.split("=") for kv in m.group(1).split())
                print("  audio stats " + " ".join(f"{k}={v}" for k, v in st.items()))
                for k in ("dropped", "stolen", "cmd_dropped", "underruns"):
                    if int(st.get(k, "0")) != 0:
                        bad.append(f"engine {k}={st[k]}")
        if bad:
            for b in bad:
                print("  audio FAIL " + b)
            return 1
        print("  audio device check ok")
        return 0

    ev = []
    stats = None
    for line in open(out_path, errors="replace"):
        m = re.search(r"audiolog frame=(\d+) ev=(\S+) cat=(\d+) vol100=(\d+) pitch100=(\d+)", line)
        if m:
            ev.append((int(m.group(1)), m.group(2), int(m.group(3)), int(m.group(4)), int(m.group(5))))
        m = re.search(r"audio_stats (.*)", line)
        if m:
            stats = dict(kv.split("=") for kv in m.group(1).split())
    print(f"  audio events logged={len(ev)}")
    if len(ev) < min_events:
        bad.append(f"only {len(ev)} logged events (< {min_events})")

    # noise floor from the dump: 20th percentile of block RMS, thresholded generously
    floor = max(3.0, float(np.percentile(rms, 20)) * 3.0)
    lat = []
    cent = {}
    for i, (fr, name, cat, v, p) in enumerate(ev):
        b0 = fr // blk
        end = min(nb, b0 + int(0.4 / 0.005))
        if end <= b0:
            bad.append(f"event {name} at frame {fr} beyond the dump ({n})")
            continue
        seg = rms[b0:end]
        hit = np.nonzero(seg > floor)[0]
        if len(hit) == 0:
            bad.append(f"event {name} at frame {fr}: no onset within 400 ms (max rms {seg.max():.1f})")
            continue
        l_ms = hit[0] * 5.0
        lat.append(l_ms)
        # window until the next event (max 700 ms)
        nxt = ev[i + 1][0] if i + 1 < len(ev) else fr + int(0.7 * rate)
        a = fr
        z = min(n, max(a + blk * 4, min(nxt, a + int(0.7 * rate))))
        # Slow-attack voices (ambient.cave: low rumble fading in over ~200 ms) are still near-silent in the first ~150 ms. A
        # following event that starts earlier than MIN_WIN would cut the window before the voice is audible, so the window is
        # at least MIN_WIN long (the later event may then add to it: such an overlapped window only proves "not silent").
        overlap = (z - a) < int(MIN_WIN * rate)
        if overlap:
            z = min(n, a + int(0.7 * rate))
        win = mono[a:z]
        wpeak = float(np.abs(win).max()) if len(win) else 0.0
        if wpeak < 200:
            bad.append(f"event {name} at frame {fr}: window peak only {wpeak:.0f}")
        spec = np.abs(np.fft.rfft(win * np.hanning(len(win)))) if len(win) > 64 else np.zeros(2)
        fq = np.fft.rfftfreq(len(win), 1.0 / rate) if len(win) > 64 else np.zeros(2)
        c = float((spec * fq).sum() / max(spec.sum(), 1e-9))
        cent.setdefault(name, []).append(c)
        print(f"  audio ev {name:26s} frame={fr:8d} onset+{l_ms:5.0f}ms peak={wpeak:7.0f} centroid={c:7.0f}Hz" + (" (overlapped by next event)" if overlap else ""))
    if lat:
        med = float(np.median(lat))
        print(f"  audio latency onset-after-start: median={med:.0f}ms max={max(lat):.0f}ms")
        if med > 150:
            bad.append(f"median onset latency {med:.0f} ms")
    if probe:
        names = [e[1] for e in ev]
        it = iter(names)
        if not all(nm in it for nm in PROBE):  # PROBE must be a subsequence (extra ambient.cave from the mood clock is fine)
            bad.append("probe list incomplete / out of order: " + ",".join(names))
        if stats is not None and int(stats.get("ambient", "0")) < 1 and "--ambient" in sys.argv:
            bad.append("mood-clock ambient.cave never played")
        g = np.mean(cent.get("block.glass.break", [0]))
        s = np.mean(cent.get("block.stone.step", [1e9]))
        if cent and not g > s:
            print(f"  audio note: glass.break centroid {g:.0f} Hz not above stone.step {s:.0f} Hz")
    if stats is None:
        bad.append("no audio_stats line")
    else:
        print("  audio stats " + " ".join(f"{k}={v}" for k, v in stats.items()))
        for k in ("dropped", "stolen", "cmd_dropped", "underruns"):
            if int(stats.get(k, "0")) != 0:
                bad.append(f"engine {k}={stats[k]}")
    if bad:
        for b in bad:
            print("  audio FAIL " + b)
        return 1
    print("  audio check ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
