#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Richtet die LAENGENANGABEN an den Aufrufstellen `(&x[0]) as u64, N`.

Die Umbenennung aendert die Byte-Laenge der Zeichenkettenliterale. Die
Deklaration `var x: [u8; N] = "..."` zieht lengths.py mit; die zweite Stelle,
an der dieselbe Laenge NOCH EINMAL als Zahl steht (der Aufruf), nicht.
Fehlt sie, laeuft alles scheinbar weiter und bricht spaeter.

Verfahren: die Aufrufstellen werden mit dem Stand VOR der Umstellung
(Basis-Commit) der Reihe nach abgeglichen. War die Zahl dort GLEICH der
deklarierten Laenge (also "ganze Zeichenkette"), muss sie es hier auch sein.
Teilzugriffe (`w(c, (&m[0]) as u64, 4)` auf einen laengeren Puffer) bleiben
unangetastet.
"""
import re, subprocess, os, glob, sys

BASIS = sys.argv[1] if len(sys.argv) > 1 else '370e6b9'

def alt_pfade():
    ns = subprocess.run(['git','diff','-M','--name-status',BASIS],
                        capture_output=True,text=True).stdout
    m = {}
    for line in ns.strip().split('\n'):
        p = line.split('\t')
        if p[0].startswith('R'): m[p[2]] = p[1]
        elif p[0] in ('M','A'):  m[p[1]] = p[1]
    return m

def stellen(text):
    decls, out = {}, []
    for i, l in enumerate(text.split('\n'), 1):
        for m in re.finditer(r'var\s+(\w+)\s*:\s*\[u8;\s*(\d+)\s*\]\s*=\s*"', l):
            decls[m.group(1)] = int(m.group(2))
        for m in re.finditer(r'\(&(\w+)\[0\]\)\s*as\s*u64,\s*(\d+)', l):
            if m.group(1) in decls:
                out.append((i, int(m.group(2)), decls[m.group(1)], m.start(2), m.end(2)))
    return out

def main():
    alt = alt_pfade()
    fixed, skipped = 0, []
    files = set()
    for pat in ('bin/*.fi','lib/**/*.fi','tests/**/*.fi','demos/**/*.fi',
                'tools/**/*.fi','examples/**/*.fi'):
        files |= set(glob.glob(pat, recursive=True))
    for f in sorted(files):
        if os.path.islink(f) or f.endswith('gctext.fi'): continue
        o = alt.get(f)
        if not o: continue
        r = subprocess.run(['git','show',BASIS+':'+o],capture_output=True,text=True)
        if r.returncode: continue
        sn, so = stellen(open(f,encoding='utf-8').read()), stellen(r.stdout)
        if len(sn) != len(so):
            skipped.append((f,len(sn),len(so))); continue
        lines = open(f,encoding='utf-8').read().split('\n')
        edits = [a for a,b in zip(sn,so) if b[1]==b[2] and a[1]!=a[2]]
        for e in sorted(edits, key=lambda x:(-x[0],-x[3])):
            s = lines[e[0]-1]
            lines[e[0]-1] = s[:e[3]] + str(e[2]) + s[e[4]:]
            print('LAENGE', f, e[0], e[1], '->', e[2])
            fixed += 1
        if edits: open(f,'w',encoding='utf-8').write('\n'.join(lines))
    print('gerichtet:', fixed, ' uebersprungen(Struktur geaendert):', len(skipped))
    for s in skipped: print('  ?', s)

main()
