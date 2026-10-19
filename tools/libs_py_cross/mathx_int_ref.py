# Reference for tests/4522_std_mathx_int.fi: prints "NAME ck" lines (mirrors the test).
import math
st = [99]
def nxt():
    st[0] = (st[0] * 1103515245 + 12345) & 0x7fffffff
    return st[0]
def r62():
    hi = nxt(); lo = nxt()
    return (hi << 31) | lo
H = [7]
def feed(x):
    H[0] = (H[0] * 31 + (x & 0xffffffff)) & 0xffffffff
    H[0] = (H[0] * 31 + ((x >> 32) & 0xffffffff)) & 0xffffffff
def done(name):
    print(name, H[0]); H[0] = 7
M = 1 << 64
def fv(v):
    feed(0xdead if v >= M else v)
for n in range(71):
    for k in range(n + 3):
        fv(math.comb(n, k))
done("comb")
for n in range(71):
    for k in range(n + 3):
        fv(math.perm(n, k))
done("perm")
for n in range(0, 26):
    fv(math.factorial(n))
done("factorial")
for _ in range(500):
    a = nxt() % 2000001 - 1000000; b = nxt() % 2000001 - 1000000
    if _ % 50 == 0: a = 0
    if _ % 70 == 0: b = 0
    feed(math.gcd(a, b)); feed(math.lcm(a, b))
done("gcd_lcm")
for _ in range(500):
    a = nxt() % 2000001 - 1000000; b = nxt() % 2000 - 1000
    if b == 0: b = 7
    feed((a // b) & 0xffffffffffffffff); feed((a % b) & 0xffffffffffffffff)
done("floordiv_mod")
for _ in range(300):
    a = r62(); b = r62(); m = (r62() << 1 | (nxt() & 1)) | 1
    if _ % 40 == 0: m = 1
    if _ % 41 == 0: m = 2 + nxt() % 100
    feed(a * b % m)
    feed(pow(a, b, m))
done("mul_pow_mod")
inv_ok = 0
for _ in range(400):
    m = 2 + r62() % (10 ** 18); a = r62() % (3 * m)
    if _ % 3 == 0:
        m = 2 + nxt() % 1000; a = nxt() % 3000
    try:
        v = pow(a, -1, m); feed(v); inv_ok += 1
    except ValueError:
        feed(0xbeef)
done("mod_inv")
print("inv_ok", inv_ok)
