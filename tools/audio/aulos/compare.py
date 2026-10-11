#!/usr/bin/env python3
"""compare.py AULOS.f32 FIRN.f32 NAME [--max-abs X] -- peak/RMS/max deviation of two renders."""
import sys, numpy as np
a = np.fromfile(sys.argv[1], dtype='<f4')
b = np.fromfile(sys.argv[2], dtype='<f4')
name = sys.argv[3]
tol = 1e-5
if '--max-abs' in sys.argv:
    tol = float(sys.argv[sys.argv.index('--max-abs') + 1])
if a.size != b.size or a.size == 0:
    print(f"FAIL {name}: size aulos={a.size} firn={b.size}")
    sys.exit(1)
d = np.abs(a.astype(np.float64) - b.astype(np.float64))
rms = lambda x: float(np.sqrt(np.mean(x.astype(np.float64) ** 2)))
same = float(np.mean(a.view('<u4') == b.view('<u4'))) * 100
print(f"{name}: {a.size // 2} frames | peak aulos {np.abs(a).max():.6f} firn {np.abs(b).max():.6f} | "
      f"rms aulos {rms(a):.6f} firn {rms(b):.6f} | max|diff| {d.max():.3e} | bit-equal {same:.2f} % | "
      f"rms diff {rms(d):.3e}")
ok = d.max() <= tol and np.isfinite(b).all() and rms(a) > 0
print(("OK   " if ok else "FAIL ") + name + f" (limit {tol:g})")
sys.exit(0 if ok else 1)
