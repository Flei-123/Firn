import sys
sys.path.insert(0, sys.argv[1])
from hpack.huffman_constants import REQUEST_CODES as C, REQUEST_CODES_LENGTH as L
from hpack.table import HeaderTable

static = HeaderTable.STATIC_TABLE
syms = sorted(range(257), key=lambda s: (L[s], s))
first = [0] * 32
cnt = [0] * 32
idx = [0] * 32
for i, s in enumerate(syms):
    n = L[s]
    if cnt[n] == 0:
        first[n] = C[s]
        idx[n] = i
    cnt[n] += 1

def arr(name, ty, vals):
    return "static %s: [%s; %d] = [%s]\n" % (name, ty, len(vals), ", ".join(str(v) for v in vals))

blob = ""
offs = [0]
nlens = []
for name, value in static:
    blob += name.decode() + value.decode()
    nlens.append(len(name))
    offs.append(len(blob))
out = "export {\n    HUFF_CODE, HUFF_LEN, HUFF_FIRST, HUFF_COUNT, HUFF_INDEX, HUFF_SORTED,\n    STATIC_BLOB, STATIC_OFF, STATIC_NAME_LEN, STATIC_COUNT,\n}\n\n"
out += "const STATIC_COUNT: usize = %d\n\n" % len(static)
out += arr("HUFF_CODE", "u32", C)
out += arr("HUFF_LEN", "u8", L)
out += arr("HUFF_FIRST", "u32", first)
out += arr("HUFF_COUNT", "u16", cnt)
out += arr("HUFF_INDEX", "u16", idx)
out += arr("HUFF_SORTED", "u16", syms)
out += arr("STATIC_OFF", "u16", offs)
out += arr("STATIC_NAME_LEN", "u8", nlens)
out += 'static STATIC_BLOB: [u8; %d] = "%s"\n' % (len(blob), blob)
sys.stdout.write(out)
