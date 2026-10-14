# Cross-check of the Firn PSNR / SSIM against an independent numpy implementation (scikit-image's SSIM
# definition: 7x7 uniform window, sample covariance, K1 0.01, K2 0.03, valid region only, mean over channels).
# Usage: python3 -I crosscheck.py <dir with metrics.txt and *.ppm written by eval>
import sys, os
import numpy as np

def read_ppm(p):
    with open(p, 'rb') as f:
        data = f.read()
    parts = data.split(b'\n', 3)
    w, h = map(int, parts[1].split())
    return np.frombuffer(parts[3], dtype=np.uint8).reshape(h, w, 3).astype(np.float64) / 255.0

def box(a, n):
    c = np.cumsum(np.cumsum(np.pad(a, ((1, 0), (1, 0))), 0), 1)
    return (c[n:, n:] - c[:-n, n:] - c[n:, :-n] + c[:-n, :-n]) / (n * n)

def ssim1(a, b, rng=1.0):
    n = 7
    ma, mb = box(a, n), box(b, n)
    cf = n * n / (n * n - 1.0)
    va = cf * (box(a * a, n) - ma * ma)
    vb = cf * (box(b * b, n) - mb * mb)
    vab = cf * (box(a * b, n) - ma * mb)
    c1, c2 = (0.01 * rng) ** 2, (0.03 * rng) ** 2
    s = ((2 * ma * mb + c1) * (2 * vab + c2)) / ((ma * ma + mb * mb + c1) * (va + vb + c2))
    return s.mean()

def ssim(a, b):
    return float(np.mean([ssim1(a[:, :, c], b[:, :, c]) for c in range(3)]))

def psnr(a, b):
    mse = np.mean((a - b) ** 2)
    return 99.0 if mse < 1e-12 else float(10 * np.log10(1.0 / mse))

d = sys.argv[1]
m = {}
for line in open(os.path.join(d, 'metrics.txt')):
    k, v = line.split()
    m[k] = float(v)
ideal = read_ppm(os.path.join(d, 'ideal_final.ppm'))
bad = 0
for name, key in (('single_final', 'final_single'), ('burst8_final', 'final_burst8'), ('burst8_bracket_final', 'final_burst8_bracket')):
    img = read_ppm(os.path.join(d, name + '.ppm'))
    p, s = psnr(img, ideal), ssim(img, ideal)
    dp, ds = abs(p - m[key + '_psnr']), abs(s - m[key + '_ssim'])
    ok = dp < 0.15 and ds < 0.005
    bad += 0 if ok else 1
    print('crosscheck %-22s psnr firn %.3f numpy %.3f | ssim firn %.4f numpy %.4f | %s' % (name, m[key + '_psnr'], p, m[key + '_ssim'], s, 'OK' if ok else 'DIFF'))
sys.exit(1 if bad else 0)
