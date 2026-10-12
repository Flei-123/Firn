import sys, numpy as np, soundfile as sf
# usage: compare.py <ogg> <f32 raw from firn>
ref, sr = sf.read(sys.argv[1], dtype='float32', always_2d=True)
got = np.fromfile(sys.argv[2], dtype='<f4').reshape(-1, ref.shape[1]) if ref.shape[0] else np.zeros((0, ref.shape[1]), 'f4')
if got.shape != ref.shape:
    print("SHAPE", got.shape, ref.shape); sys.exit(1)
d = np.abs(got - ref)
lsb = d.max() * 32768.0
rms = float(np.sqrt(np.mean(d.astype('f8') ** 2))) if d.size else 0.0
print("max_lsb16=%.4f rms=%.3e" % (lsb, rms))
sys.exit(0 if lsb <= 1.0 else 1)
