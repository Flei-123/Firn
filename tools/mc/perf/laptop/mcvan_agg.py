#!/usr/bin/env python3
"""Aggregates the output of mcvan_report.ps1 (RESULT + METRICS lines on stdin or in a file) into a markdown table.
Median over the repetitions of each (resolution, radius, client) cell; per-run p50 in brackets."""
import re, sys, statistics as st
from collections import defaultdict

text = open(sys.argv[1]).read() if len(sys.argv) > 1 else sys.stdin.read()
cells = defaultdict(lambda: defaultdict(list))
for line in text.splitlines():
    m = re.match(r'RESULT pm_(\w+?)_(\d+x\d+)_r(\d+)_(\w+)\.csv (.*)', line.strip())
    if m:
        client, res, r, tag, rest = m.groups()
        kv = dict(re.findall(r'(\w+)=([\d.]+)', rest))
        key = (res, int(r), client)
        for k, v in kv.items():
            cells[key][k].append(float(v))
        continue
    m = re.search(r'METRICS target=(\w+) res=(\d+x\d+) r=(\d+) tag=(\w+) (.*)', line)
    if m:
        client, res, r, tag, rest = m.groups()
        kv = dict(re.findall(r'(\w+)=([\d.]+)', rest))
        key = (res, int(r), client)
        for k, v in kv.items():
            if v:
                cells[key]['m_' + k].append(float(v))

def med(xs):
    return st.median(xs) if xs else float('nan')

print('| res | r | client | n | p50 | p95 | p99 | 1%low | avg fps | cpu %core | peak WS MB | private MB | t_window ms | t_join ms | p50 per run |')
print('|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|')
order = {'vanilla': 0, 'mcplaygl': 1, 'mcplayvk': 2, 'mcplayglcap': 3, 'mcplayvkcap': 4}
for key in sorted(cells, key=lambda k: (-int(k[0].split('x')[0]), k[1], order.get(k[2], 9))):
    c = cells[key]
    res, r, client = key
    runs = ' / '.join(f'{x:.2f}' for x in c['p50'])
    print(f"| {res} | {r} | {client} | {len(c['p50'])} | {med(c['p50']):.2f} | {med(c['p95']):.2f} | {med(c['p99']):.2f} | {med(c['low1pct_mean']):.2f} | "
          f"{med(c['avg_fps']):.0f} | {med(c['m_cpu_core_pct']):.0f} | {med(c['m_peak_ws_mb']):.0f} | {med(c['m_priv_mb']):.0f} | "
          f"{med(c['m_t_window_ms']):.0f} | {med(c['m_t_join_ms']):.0f} | {runs} |")
