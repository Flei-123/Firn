import sys, os, random, subprocess, glob, tempfile
# usage: fuzz.py <binary> <runs> <seed> <ogg files...>   -- no crash, no hang allowed (exit 0 or 5 only)
binp, runs, seed = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
files = sys.argv[4:]
rnd = random.Random(seed)
def crc_table():
    t = []
    for n in range(256):
        c = n << 24
        for _ in range(8):
            c = ((c << 1) ^ 0x04C11DB7) & 0xFFFFFFFF if c & 0x80000000 else (c << 1) & 0xFFFFFFFF
        t.append(c)
    return t
T = crc_table()
def fix_crc(d):
    # recompute the CRC of every page that still parses (so mutations reach the decoder)
    pos = 0
    while pos + 27 <= len(d) and d[pos:pos+4] == b'OggS':
        n = d[pos+26]
        if pos + 27 + n > len(d): break
        end = pos + 27 + n + sum(d[pos+27:pos+27+n])
        if end > len(d): break
        d[pos+22:pos+26] = b'\0\0\0\0'
        c = 0
        for b in d[pos:end]: c = ((c << 8) & 0xFFFFFFFF) ^ T[((c >> 24) ^ b) & 255]
        d[pos+22:pos+26] = c.to_bytes(4, 'little')
        pos = end
td = tempfile.mkdtemp()
bad = 0; ok = 0; err = 0
for i in range(runs):
    d = bytearray(open(rnd.choice(files), 'rb').read())
    m = rnd.randrange(5)
    if m == 0:
        d = d[:rnd.randrange(len(d))]
    elif m == 1:
        for _ in range(rnd.randrange(1, 8)):
            d[rnd.randrange(len(d))] ^= 1 << rnd.randrange(8)
    elif m == 2:
        for _ in range(rnd.randrange(1, 30)):
            d[rnd.randrange(len(d))] = rnd.randrange(256)
    elif m == 3:
        a = rnd.randrange(len(d)); b = min(len(d), a + rnd.randrange(1, 400))
        del d[a:b]
    else:
        # corrupt inside the first 4 KB (setup header) and fix nothing; CRC breaks pages -> also try with header bits
        for _ in range(rnd.randrange(1, 6)):
            d[rnd.randrange(min(len(d), 4096))] = rnd.randrange(256)
    if rnd.randrange(4) != 0 and m in (1, 2, 4) and len(d) < 400000:
        # mutate the PAYLOAD, then repair CRCs; do it in place
        fix_crc(d)
    p = os.path.join(td, 'f.ogg'); open(p, 'wb').write(d)
    try:
        r = subprocess.run([binp, p, os.path.join(td, 'o.raw')], capture_output=True, timeout=20)
    except subprocess.TimeoutExpired:
        print("HANG run", i, "mode", m); open('/tmp/fuzz_hang_%d.ogg' % i, 'wb').write(d); bad += 1; continue
    if r.returncode in (0, 5): ok += 1 if r.returncode == 0 else 0; err += r.returncode == 5
    else:
        print("CRASH rc=%d run %d mode %d" % (r.returncode, i, m)); open('/root/vorbis_work/crash_%d.ogg' % i, 'wb').write(d); bad += 1
print("runs=%d ok=%d clean_error=%d bad=%d" % (runs, ok, err, bad))
sys.exit(1 if bad else 0)
