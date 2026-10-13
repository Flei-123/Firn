"""tools/vorbis/stream_check.py -- ton.vorbis_stream against libsndfile.
usage: stream_check.py <stream_bin> <workdir> <ogg files...>
Checks: file source, pipe source (stdin, not seekable, odd chunk sizes), chained links (concatenated files, different
rates / channel counts), seek to chosen frames (<= 1 LSB of 16 bit vs a full reference decode)."""
import subprocess, sys, os, random, zlib
import numpy as np
import soundfile as sf

binp, work = sys.argv[1], sys.argv[2]
files = sys.argv[3:]
fails = 0


def ref(path):
    x, sr = sf.read(path, dtype='float32', always_2d=True)
    return x, sr


def run(args, stdin_file=None):
    if stdin_file:
        with open(stdin_file, 'rb') as f:
            r = subprocess.run([binp] + args, stdin=f, capture_output=True, text=True, timeout=120)
    else:
        r = subprocess.run([binp] + args, capture_output=True, text=True, timeout=120)
    if r.returncode != 0:
        raise RuntimeError('stream_main %s -> %d %s' % (args, r.returncode, r.stdout))
    return r.stdout


def close(got, exp):
    if got.shape != exp.shape:
        return 'shape %s vs %s' % (got.shape, exp.shape)
    d = float(np.abs(got - exp).max()) * 32768.0 if exp.size else 0.0
    return None if d <= 1.0 else 'max_lsb16=%.3f' % d


out = os.path.join(work, 'o.f32')
n_ok = 0
for f in files:
    x, sr = ref(f)
    ch = x.shape[1]
    for mode, args, stdin in (('file', [f, out, '777'], None), ('pipe', ['-', out, '1531'], f)):
        log = run(args, stdin)
        got = np.fromfile(out, dtype='<f4').reshape(-1, ch)
        e = close(got, x)
        if e:
            print('FAIL', mode, os.path.basename(f), e); fails += 1
        else:
            n_ok += 1
    rnd = random.Random(zlib.crc32(os.path.basename(f).encode()))
    targets = [0, 1, len(x) // 3, len(x) // 2 + 17, max(0, len(x) - 700), rnd.randrange(len(x))]
    for t in targets:
        cnt = min(3000, len(x) - t)
        if cnt <= 0:
            continue
        log = run([f, out, '1000', str(t), str(cnt)])
        got = np.fromfile(out, dtype='<f4').reshape(-1, ch)
        e = close(got, x[t:t + cnt])
        tell = int(log.split()[0])
        if e or tell != t + cnt:
            print('FAIL seek', os.path.basename(f), t, e, 'tell', tell, 'want', t + cnt); fails += 1
        else:
            n_ok += 1

if len(files) >= 3:
    pick = [f for f in files if 'sine_3ch' not in f][:3]
    chain = os.path.join(work, 'chain.ogg')
    with open(chain, 'wb') as o:
        for f in pick:
            o.write(open(f, 'rb').read())
    log = run([chain, out, '999']).strip().split('\n')
    exp = [ref(f) for f in pick]
    raw = np.fromfile(out, dtype='<f4')
    pos = 0
    for i, (x, sr) in enumerate(exp):
        want = '%d %d %d %d' % (i, sr, x.shape[1], len(x))
        if log[i] != want:
            print('FAIL chain log', log[i], 'want', want); fails += 1
        n = len(x) * x.shape[1]
        e = close(raw[pos:pos + n].reshape(-1, x.shape[1]), x)
        if e:
            print('FAIL chain link', i, e); fails += 1
        else:
            n_ok += 1
        pos += n
    if pos != len(raw):
        print('FAIL chain total', pos, len(raw)); fails += 1
    run(['-', out, '1234'], chain)
    raw2 = np.fromfile(out, dtype='<f4')
    if not np.array_equal(raw, raw2):
        print('FAIL chain pipe differs'); fails += 1
    else:
        n_ok += 1
    x0, _ = exp[0]
    t = len(x0) // 2
    run([chain, out, '1000', str(t), '2000'])
    got = np.fromfile(out, dtype='<f4').reshape(-1, x0.shape[1])
    e = close(got, x0[t:t + 2000])
    if e:
        print('FAIL chain seek', e); fails += 1
    else:
        n_ok += 1
print('stream: %d checks ok, %d failed' % (n_ok, fails))
sys.exit(1 if fails else 0)
