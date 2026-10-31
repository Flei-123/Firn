import re
import sys

template_path, mappings_path, out_path = sys.argv[1], sys.argv[2], sys.argv[3]

classes = {}
members = {}
current = None
for line in open(mappings_path):
    if line.startswith('#'):
        continue
    if not line.startswith(' '):
        m = re.match(r'(\S+) -> (\S+):', line)
        current = m.group(1)
        classes[current] = m.group(2)
        members[current] = {}
        continue
    s = re.sub(r'^\s*(\d+:\d+:)?', '', line.rstrip())
    m = re.match(r'(\S+) (\S+?)(\(.*\))? -> (\S+)$', s)
    if not m:
        continue
    kind_sig = m.group(3)
    key = m.group(2) + (kind_sig if kind_sig is not None else '#field')
    members[current].setdefault(key, m.group(4))


def cls(name):
    return classes[name].replace('$', '.')


def meth(arg):
    c, sig = arg.split('#', 1)
    return members[c][sig]


def fld(arg):
    c, name = arg.split('#', 1)
    return members[c][name + '#field']


src = open(template_path).read()
src = re.sub(r'\$\{N:([^}]+)\}', lambda m: classes[m.group(1)], src)
src = re.sub(r'\$\{C:([^}]+)\}', lambda m: cls(m.group(1)), src)
src = re.sub(r'\$\{M:([^}]+)\}', lambda m: meth(m.group(1)), src)
src = re.sub(r'\$\{F:([^}]+)\}', lambda m: fld(m.group(1)), src)
open(out_path, 'w').write(src)
