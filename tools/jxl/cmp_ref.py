import sys, subprocess, shutil, os, tempfile
import numpy as np

def read_pnm(path):
    d = open(path, 'rb').read()
    toks = []
    i = 0
    magic = d[:2]
    if magic == b'P7':
        hdr = {}
        j = d.index(b'ENDHDR\n') + 7
        for line in d[:j].split(b'\n'):
            p = line.split()
            if len(p) == 2:
                hdr[p[0]] = int(p[1]) if p[1].isdigit() else p[1]
        w, h, ch, mx = hdr[b'WIDTH'], hdr[b'HEIGHT'], hdr[b'DEPTH'], hdr[b'MAXVAL']
        body = d[j:]
    else:
        ch = 3 if magic == b'P6' else 1
        i = 2
        vals = []
        while len(vals) < 3:
            while d[i:i+1].isspace():
                i += 1
            if d[i:i+1] == b'#':
                while d[i:i+1] != b'\n':
                    i += 1
                continue
            s = i
            while not d[i:i+1].isspace():
                i += 1
            vals.append(int(d[s:i]))
        i += 1
        w, h, mx = vals
        body = d[i:]
    if mx > 255:
        a = np.frombuffer(body[:w*h*ch*2], dtype='>u2').astype(np.int64)
    else:
        a = np.frombuffer(body[:w*h*ch], dtype=np.uint8).astype(np.int64)
    return w, h, ch, mx, a

def main():
    probe = sys.argv[1]
    files = sys.argv[2:]
    tot_bad = tot = 0
    for f in files:
        ref = os.path.splitext(f)[0]
        rp = None
        for e in ('.ppm', '.pam', '.pgm'):
            if os.path.exists(ref + e):
                rp = ref + e
        if rp is None:
            continue
        w, h, ch, mx, r = read_pnm(rp)
        d = tempfile.mkdtemp()
        shutil.copy(f, d + '/in.jxl')
        rc = subprocess.run([probe], cwd=d, env=dict(os.environ)).returncode
        if rc != 0:
            print(f, 'rc', rc)
            shutil.rmtree(d)
            continue
        raw = open(d + '/out.raw', 'rb').read()
        shutil.rmtree(d)
        if mx > 255:
            o = np.frombuffer(raw, dtype='<u2').astype(np.int64)
        else:
            o = np.frombuffer(raw, dtype=np.uint8).astype(np.int64)
        if o.size != r.size:
            print(f, 'size', o.size, r.size)
            continue
        e = o - r
        bad = int((e != 0).sum())
        hist = {int(k): int(v) for k, v in zip(*np.unique(e, return_counts=True))}
        tot_bad += bad
        tot += r.size
        print(os.path.basename(f), 'bad %d/%d (%.4f%%)' % (bad, r.size, 100.0 * bad / r.size), hist)
    print('TOTAL', tot_bad, tot, '%.4f%%' % (100.0 * tot_bad / max(tot, 1)))

main()
