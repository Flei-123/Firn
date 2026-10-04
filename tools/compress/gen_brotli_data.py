#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/compress/gen_brotli_data.py -- builds lib/compress/brotli_data.bin, the constant data
of the Brotli format (RFC 7932) that is too large or too irregular to type:

  offset  size    what
  0       2048    context lookup tables: 4 modes (LSB6, MSB6, UTF8, SIGNED) x [lut0 256][lut1 256]
  2048    217     the prefix/suffix strings of the 121 word transforms (length-prefixed)
  2265    363     the 121 transforms as (prefix id, type, suffix id) triples
  2628    122784  the static dictionary (RFC 7932 appendix A)

All of it is the data of the Brotli reference implementation (Google, MIT license, see
THIRD_PARTY.md); this script lifts it out of an installed libbrotlicommon and checks it against
the digests of the published dictionary and the structure of the tables, so the repository does
not depend on retyping 122 KB.

    gen_brotli_data.py [libbrotlicommon.so] > lib/compress/brotli_data.bin
"""
import sys, hashlib, glob

lib = sys.argv[1] if len(sys.argv) > 1 else sorted(glob.glob("/usr/lib/*/libbrotlicommon.so.1*"))[0]
d = open(lib, "rb").read()

# the dictionary
k = d.find(b"timedownlifeleft")
assert k >= 0, "dictionary not found"
dic = d[k:k + 122784]
assert hashlib.sha256(dic).hexdigest() == "20e42eb1b511c21806d4d227d07e5dd06877d8ce7b3a817f378f313653f35c70", "dictionary digest"

# prefix/suffix strings: length-prefixed, 50 of them, 217 octets
ps_at = d.find(b"\x01 \x02, \x08 of the \x04 of \x02s \x01.\x05 and ")
assert ps_at >= 0
ps = d[ps_at:ps_at + 217]
p = 0
n = 0
while p < len(ps):
    p += 1 + ps[p]
    n += 1
assert p == 217 and n == 50, (p, n)

# the transforms: (prefix id, type, suffix id) x 121; the first four are known from the RFC
tr_at = d.find(bytes([49, 0, 49, 49, 0, 0, 0, 0, 0, 49, 12, 49]))
assert tr_at >= 0
tr = d[tr_at:tr_at + 363]
trip = [(tr[i], tr[i + 1], tr[i + 2]) for i in range(0, 363, 3)]
assert all(a <= 49 and c <= 49 and 0 <= t <= 20 for a, t, c in trip)
assert trip[0] == (49, 0, 49) and trip[2] == (0, 0, 0) and trip[9] == (49, 10, 49)

# context lookup: 4 modes x 512
c_at = d.find(bytes(list(range(64)) * 4))
ctx = d[c_at:c_at + 2048]
assert ctx[0:256] == bytes(i & 63 for i in range(256)), "LSB6"
assert ctx[512:768] == bytes(i >> 2 for i in range(256)), "MSB6"
assert ctx[256:512] == bytes(256), "lut1 of LSB6"

out = ctx + ps + tr + dic
assert len(out) == 2048 + 217 + 363 + 122784
sys.stdout.buffer.write(out)
