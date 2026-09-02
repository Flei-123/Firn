#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/tempo/ab.py -- A/B measurement of two firnc binaries on the SAME work.

The machine this repository is developed on is SHARED: other builds run on it
at the same time and the load average moves between 8 and 16.  Two numbers
taken one after the other therefore measure the neighbours as much as the
compiler.  This runner does two things about it:

  * it INTERLEAVES: A B A B A B ..., so a load spike hits both sides,
  * it reports the MINIMUM of the passes, not the mean.  The minimum is the
    run in which the machine got out of the way; it is the only number of a
    shared machine that is about the program.

Usage:
    python3 tools/tempo/ab.py RUNS A=<path> B=<path> -- NAME:CWD:FIRNLIB:ARGS ...

%OUT% in ARGS is replaced by a temporary output path.
"""
import subprocess, sys, os, time, tempfile, statistics


def parse_job(spec):
    name, cwd, lib, args = spec.split(':', 3)
    return name, cwd, lib, args


def run_once(binary, cwd, lib, args, timings=False):
    extra = []
    if '::' in binary:
        binary, e = binary.split('::', 1)
        extra = e.split()
    out = tempfile.mktemp(prefix='tempo_')
    env = dict(os.environ)
    if lib:
        env['FIRNLIB'] = lib
    argv = [binary] + extra + (['--timings'] if timings else []) + args.replace('%OUT%', out).split()
    t = time.time()
    r = subprocess.run(argv, cwd=cwd, env=env, capture_output=True, text=True)
    dt = time.time() - t
    for suffix in ('', '.s', '.o'):
        try:
            os.unlink(out + suffix)
        except OSError:
            pass
    if r.returncode != 0:
        return None, r.stderr[-800:]
    return dt, r.stderr


def phases(stderr):
    rows = {}
    for line in stderr.split('\n'):
        p = line.split()
        # 'as + ld' of the old compiler is one phase with blanks in its name
        if line.startswith('  as + ld') and len(p) >= 6:
            rows['as+ld'] = float(p[3])
        elif len(p) >= 4 and p[2] == 'ms':
            rows[p[0]] = float(p[1])
        elif line.startswith('total ') and len(p) >= 2:
            rows['TOTAL'] = float(p[1])
    return rows


def main():
    runs = int(sys.argv[1])
    bins, jobs, mode = {}, [], 'bins'
    for a in sys.argv[2:]:
        if a == '--':
            mode = 'jobs'
            continue
        if mode == 'bins':
            k, v = a.split('=', 1)
            bins[k] = v
        else:
            jobs.append(parse_job(a))

    for name, cwd, lib, args in jobs:
        res = {k: [] for k in bins}
        ph = {k: None for k in bins}
        ph = {k: {} for k in bins}
        for i in range(runs):
            for k, b in bins.items():
                dt, err = run_once(b, cwd, lib, args, timings=True)
                if dt is None:
                    print(f'{name}: {k} FAILED\n{err}')
                    return 1
                res[k].append(dt)
                for n, v in phases(err).items():
                    ph[k].setdefault(n, []).append(v)
        print(f'== {name} ==')
        for k in sorted(bins):
            print(f'  {k}: min {min(res[k]):7.3f}s   median {statistics.median(res[k]):7.3f}s'
                  f'   all {[round(x, 3) for x in res[k]]}')
        ks = sorted(bins)
        if len(ks) == 2:
            a, b = min(res[ks[0]]), min(res[ks[1]])
            print(f'  {ks[1]} / {ks[0]} = {b / a:.3f}x   ({(1 - b / a) * 100:+.1f} %)')
        names = sorted({n for k in ks for n in ph[k]},
                       key=lambda n: -min(ph[ks[0]].get(n, [0])))
        print('  phase minima (ms):')
        for n in names:
            row = '  '.join(f'{k} {min(ph[k][n]):8.1f}' for k in ks if n in ph[k])
            extra = ''
            if len(ks) == 2 and all(n in ph[k] for k in ks):
                a, b = min(ph[ks[0]][n]), min(ph[ks[1]][n])
                if a > 0:
                    extra = f'   {b / a:.3f}x'
            print(f'    {n:<14} {row}{extra}')
    return 0


if __name__ == '__main__':
    sys.exit(main())
