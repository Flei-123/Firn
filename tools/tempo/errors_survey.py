#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/tempo/errors_survey.py -- what do the error messages of firnc look like?

Runs every file under tests/neg through the compiler and scores each message
against the four questions of the round:

  1. Does it name file, LINE and COLUMN?
  2. Is the offending source line printed with a marker underneath?
  3. Is there a suggestion (`= help:`) or at least an explanation (`= note:`)?
  4. Does one mistake produce ONE message or an avalanche?

Usage:  python3 tools/tempo/errors_survey.py [firnc] [--json out.json]
"""
import subprocess, sys, os, re, json, glob

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
FIRNC = sys.argv[1] if len(sys.argv) > 1 and not sys.argv[1].startswith('--') \
    else os.path.join(ROOT, 'compiler/target/release/firnc')


def compile_one(path):
    env = dict(os.environ)
    env['FIRNLIB'] = os.path.join(ROOT, 'lib')
    r = subprocess.run([FIRNC, '-o', '/tmp/errsurvey.out', path],
                       cwd=ROOT, env=env, capture_output=True, text=True, timeout=120)
    return r.returncode, r.stderr


def analyse(text):
    """Split the stderr into single diagnostics and score each."""
    lines = text.split('\n')
    diags = []
    cur = None
    for l in lines:
        if l.startswith('error:') or l.startswith('warning:'):
            if cur:
                diags.append(cur)
            cur = {'head': l, 'body': []}
        elif cur is not None:
            cur['body'].append(l)
    if cur:
        diags.append(cur)
    out = []
    for d in diags:
        body = '\n'.join(d['body'])
        arrow = re.search(r'-->\s*(\S+)', body)
        loc = arrow.group(1) if arrow else None
        has_col = bool(loc and re.search(r':\d+:\d+$', loc))
        has_line_no = bool(loc and re.search(r':\d+', loc))
        unknown_file = bool(loc and loc.startswith('<unknown>'))
        # the printed source line is the one that starts with " N | "
        srcline = None
        for b in d['body']:
            m = re.match(r'\s*(\d+) \| (.*)$', b)
            if m:
                srcline = m.group(2)
                break
        has_caret = any('^' in b for b in d['body'])
        out.append({
            'head': d['head'],
            'loc': loc,
            'has_col': has_col,
            'has_line': has_line_no,
            'unknown_file': unknown_file,
            'src_shown': srcline is not None,
            'src_empty': (srcline is not None and srcline.strip() == ''),
            'caret': has_caret,
            'help': '= help:' in body,
            'note': '= note:' in body,
        })
    return out


def main():
    files = sorted(glob.glob(os.path.join(ROOT, 'tests/neg/*.fi')))
    rows = []
    for f in files:
        rc, err = compile_one(f)
        ds = analyse(err)
        rows.append({'file': os.path.relpath(f, ROOT), 'rc': rc,
                     'n': len(ds), 'diags': ds, 'raw': err})
    tot = sum(r['n'] for r in rows)
    def cnt(key):
        return sum(1 for r in rows for d in r['diags'] if d[key])
    print(f'files: {len(rows)}   diagnostics: {tot}')
    print(f'  with file:line:col          {cnt("has_col"):4d}  ({cnt("has_col")/tot*100:5.1f} %)')
    print(f'  with a line number at all   {cnt("has_line"):4d}  ({cnt("has_line")/tot*100:5.1f} %)')
    print(f'  file name is <unknown>      {cnt("unknown_file"):4d}')
    print(f'  source line printed         {cnt("src_shown"):4d}  ({cnt("src_shown")/tot*100:5.1f} %)')
    print(f'  ... but the line was EMPTY  {cnt("src_empty"):4d}')
    print(f'  marker (^) under the place  {cnt("caret"):4d}  ({cnt("caret")/tot*100:5.1f} %)')
    print(f'  with "= help:" suggestion   {cnt("help"):4d}  ({cnt("help")/tot*100:5.1f} %)')
    print(f'  with "= note:" explanation  {cnt("note"):4d}  ({cnt("note")/tot*100:5.1f} %)')
    many = [r for r in rows if r['n'] > 1]
    print(f'  files with MORE than one diagnostic: {len(many)}')
    for r in sorted(many, key=lambda r: -r['n'])[:12]:
        print(f'      {r["n"]:3d}  {r["file"]}')
    broken = [r for r in rows if r['rc'] == 0]
    if broken:
        print(f'  !! files that compiled without error: {[r["file"] for r in broken]}')
    if '--json' in sys.argv:
        out = sys.argv[sys.argv.index('--json') + 1]
        json.dump(rows, open(out, 'w'), indent=1)
        print(f'  written: {out}')


if __name__ == '__main__':
    main()
