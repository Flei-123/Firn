import re, sys, subprocess, os, json

def split(text):
    parts = re.split(r'(?m)^(?=\.globl )', text)
    out = {}
    order = []
    for p in parts:
        m = re.match(r'\.globl (\S+)\n', p)
        key = m.group(1) if m else '__head%d' % len(order)
        if key in out:
            key = key + '#%d' % len(order)
        out[key] = p
        order.append(key)
    return out, order

a = open(sys.argv[1]).read()
b = open(sys.argv[2]).read()
fa, oa = split(a)
fb, ob = split(b)
assert oa == ob, "block order differs"
diff = [k for k in oa if fa[k] != fb[k]]
print("blocks", len(oa), "differing", len(diff))

def build(use_b, name):
    text = ''.join(fb[k] if k in use_b else fa[k] for k in oa)
    s = '/tmp/r109w/probe/%s.s' % name
    open(s, 'w').write(text)
    r = subprocess.run(['aarch64-linux-gnu-as', s, '-o', s + '.o'], capture_output=True, text=True)
    if r.returncode:
        print("as failed", r.stderr[:300]); return None
    r = subprocess.run(['aarch64-linux-gnu-ld', '-n', '-o', s + '.bin', s + '.o'], capture_output=True, text=True)
    if r.returncode:
        print("ld failed", r.stderr[:300]); return None
    try:
        p = subprocess.run(['qemu-aarch64', s + '.bin'], capture_output=True, timeout=200)
        return p.returncode
    except subprocess.TimeoutExpired:
        return 'timeout'

if len(sys.argv) > 3 and sys.argv[3] == 'list':
    print('\n'.join(diff)); sys.exit()

cur = list(diff)
print("all-b rc", build(set(diff), 'allb'))
print("all-a rc", build(set(), 'alla'))
while len(cur) > 1:
    half = cur[:len(cur)//2]
    rc = build(set(half), 'bis')
    print(len(cur), "first half rc", rc, flush=True)
    if rc == 7:
        cur = half
    else:
        rest = cur[len(cur)//2:]
        rc2 = build(set(rest), 'bis')
        print(len(cur), "second half rc", rc2, flush=True)
        if rc2 == 7:
            cur = rest
        else:
            print("needs a combination", cur); break
print("culprit candidates", cur)
