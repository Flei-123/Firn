#!/usr/bin/env python3
"""stats_check.py <bench-binary>: mc_perf percentiles / mean / 1 % low against numpy on random frame-time samples."""
import subprocess, sys, tempfile, os, random
import numpy as np
exe = sys.argv[1]; bad = 0
random.seed(11)
cases = [("lognormal", [int(random.lognormvariate(16, .4)) for _ in range(997)]),
         ("small", [int(random.uniform(1e6, 2e7)) for _ in range(7)]),
         ("spiky", [16000000] * 480 + [90000000] * 20), ("one", [5000000]), ("hundred", [random.randrange(1, 10**8) for _ in range(100)])]
for name, v in cases:
    with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False) as f: f.write(" ".join(map(str, v))); p = f.name
    try:
        got = dict(l.split() for l in subprocess.run([exe, "--stats", p], capture_output=True, text=True, check=True).stdout.splitlines())
    finally: os.unlink(p)
    a = np.array(sorted(v), dtype=np.float64); k = max(1, -(-len(v) // 100))
    want = {"p50": np.percentile(a, 50), "p95": np.percentile(a, 95), "p99": np.percentile(a, 99), "p999": np.percentile(a, 99.9),
            "mean": a.mean(), "low1_mean": a[-k:].mean(), "max": a.max(), "min": a.min()}
    for key, w in want.items():
        if abs(int(got[key]) - w) > 1.0:
            print(f"  FAIL  {name} {key}: firn {got[key]} numpy {w}"); bad += 1
    print(f"  ok    {name}: {len(want)} statistics == numpy") if not bad else None
sys.exit(1 if bad else 0)
