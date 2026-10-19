import os
import re

M, A, C = 0xFFFFFFFFFFFFFFFF, 6364136223846793005, 1442695040888963407
count = 350_000 // int(os.environ.get("PYN_SCALE", "1") or 1)
fmt = '{"id":%d,"name":"it\\"em%d\\"","ok":%s,"v":[%d,-%d,%d],"o":{"p":%d}},'
recs, s = [], 21
for i in range(count):
    s = (s * A + C) & M
    recs.append(fmt % (i, (s >> 40) % 100000, "true" if (s >> 20) & 1 else "false",
                       (s >> 8) % 1000, (s >> 30) % 1000, (s >> 50) % 100000, (s >> 12) % 100))
text = "".join(recs)
objects = arrays = numbers = strings = total = 0
for m in re.finditer(r'"(?:\\.|[^"\\])*"|(-?)(\d+)|([{\[])', text):
    if m[2]:
        numbers += 1
        total += -int(m[2]) if m[1] else int(m[2])
    elif m[3]:
        objects, arrays = objects + (m[3] == "{"), arrays + (m[3] == "[")
    else:
        strings += 1
r = (((objects * 1000003 + arrays) * 1000003 + numbers) * 1000003 + strings) * 1000003 + total
t = 0
for _ in range(4):      # the 4 scan passes are identical (pure function of the text)
    t = (t * 1000003 + r) & M
print(t)
