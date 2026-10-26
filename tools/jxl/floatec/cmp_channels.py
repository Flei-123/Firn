import sys, os, shutil, subprocess, tempfile
import numpy as np
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))

def read_pam(path):
    d = open(path, 'rb').read()
    j = d.index(b'ENDHDR\n') + 7
    hdr = {}
    for line in d[:j].split(b'\n'):
        p = line.split()
        if len(p) == 2 and p[1].isdigit():
            hdr[p[0]] = int(p[1])
    w, h, ch, mx = hdr[b'WIDTH'], hdr[b'HEIGHT'], hdr[b'DEPTH'], hdr[b'MAXVAL']
    dt = '>u2' if mx > 255 else np.uint8
    return w, h, ch, mx, np.frombuffer(d[j:], dtype=dt).astype(np.int64).reshape(h, w, ch)

def run(probe, f):
    d = tempfile.mkdtemp()
    shutil.copy(f, d + '/in.jxl')
    rc = subprocess.run([probe], cwd=d).returncode
    raw = open(d + '/out.raw', 'rb').read() if rc == 0 else b''
    shutil.rmtree(d)
    return rc, raw

def main():
    probe = sys.argv[1]
    for f in sys.argv[2:]:
        w, h, ch, mx, r = read_pam(os.path.splitext(f)[0] + '.pam')
        rc, raw = run(probe, f)
        if rc != 0:
            print(os.path.basename(f), 'rc', rc)
            continue
        o = np.frombuffer(raw, dtype='<u2' if mx > 255 else np.uint8).astype(np.int64).reshape(h, w, ch)
        parts = []
        for c in range(ch):
            e = o[:, :, c] - r[:, :, c]
            parts.append('c%d bad %.3f%% max %d' % (c, 100.0 * (e != 0).mean(), int(abs(e).max())))
        print(os.path.basename(f), ' | '.join(parts))

main()
