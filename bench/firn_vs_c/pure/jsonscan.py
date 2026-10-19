import os

S = int(os.environ.get("PYN_SCALE", "1") or 1)
M64 = 0xFFFFFFFFFFFFFFFF
A = 6364136223846793005
C = 1442695040888963407
F0 = b'{"id":'
F1 = b',"name":"it\\"em'
F2 = b'\\"","ok":'
F3 = b"true"
F4 = b"false"
F5 = b',"v":['
F6 = b'],"o":{"p":'
F7 = b"}},"
buf = bytearray()


def push_dec(v):
    tmp = []
    x = v
    if x == 0:
        tmp.append(48)
    while x > 0:
        tmp.append(x % 10 + 48)
        x //= 10
    while tmp:
        buf.append(tmp.pop())


def scan(p, length):
    i = 0
    objects = arrays = numbers = strings = total = 0
    while i < length:
        c = p[i]
        if c == 123:
            objects += 1
            i += 1
        elif c == 91:
            arrays += 1
            i += 1
        elif c == 34:
            strings += 1
            i += 1
            while i < length:
                d = p[i]
                if d == 92:
                    i += 2
                elif d == 34:
                    i += 1
                    break
                else:
                    i += 1
        elif 48 <= c <= 57 or c == 45:
            neg = False
            if c == 45:
                neg = True
                i += 1
            v = 0
            while i < length:
                d = p[i]
                if 48 <= d <= 57:
                    v = v * 10 + (d - 48)
                    i += 1
                else:
                    break
            numbers += 1
            if neg:
                total = (total - v) & M64
            else:
                total = (total + v) & M64
        else:
            i += 1
    r = objects
    r = (r * 1000003 + arrays) & M64
    r = (r * 1000003 + numbers) & M64
    r = (r * 1000003 + strings) & M64
    r = (r * 1000003 + total) & M64
    return r


count = 350_000 // S
s = 21
for i in range(count):
    s = (s * A + C) & M64
    buf += F0
    push_dec(i)
    buf += F1
    push_dec((s >> 40) % 100000)
    buf += F2
    buf += F3 if (s >> 20) & 1 == 1 else F4
    buf += F5
    push_dec((s >> 8) % 1000)
    buf.append(44)
    buf.append(45)
    push_dec((s >> 30) % 1000)
    buf.append(44)
    push_dec((s >> 50) % 100000)
    buf += F6
    push_dec((s >> 12) % 100)
    buf += F7
total = 0
for _ in range(4):
    total = (total * 1000003 + scan(buf, len(buf))) & M64
print(total)
