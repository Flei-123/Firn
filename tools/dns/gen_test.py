#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/dns/gen_test.py -- writes tests/2090_dns_wire.fi.

The expected values of every fixture come from an INDEPENDENT decoder in
this file (a few lines of Python, written separately from lib/net/dns.fi),
not from the library under test. The live fixtures are real answers from
1.1.1.1 (fixtures_live.py); the hostile ones are built here.

    python3 tools/dns/gen_test.py > tests/2090_dns_wire.fi
"""
import struct, sys, os
sys.path.insert(0, os.path.dirname(__file__))
from fixtures_live import LIVE

# ----------------------------------------------------------- reference decoder
def rd_name(m, off):
    labels, jumped, nxt, hops = [], False, None, 0
    while True:
        b = m[off]
        if b == 0:
            if not jumped: nxt = off + 1
            break
        if b & 0xC0 == 0xC0:
            ptr = ((b & 0x3F) << 8) | m[off + 1]
            if not jumped: nxt = off + 2; jumped = True
            hops += 1
            assert hops < 20
            off = ptr
        else:
            labels.append(m[off + 1:off + 1 + b].decode().lower()); off += 1 + b
    return '.'.join(labels), nxt

def reference(m, qname, qtype):
    """The chain-following answer a careful reader expects."""
    flags = struct.unpack('>H', m[2:4])[0]
    qd, an = struct.unpack('>HH', m[4:8])
    off = 12
    for _ in range(qd):
        _, off = rd_name(m, off); off += 4
    rrs = []
    for _ in range(an):
        n, off = rd_name(m, off)
        t, c, ttl, rdl = struct.unpack('>HHIH', m[off:off + 10]); off += 10
        rrs.append((n, t, c, ttl, off, rdl)); off += rdl
    cur, ncname, ttls, looped = qname.lower(), 0, [], False
    addrs, addrs6 = [], []
    if (flags & 0x200) or (flags & 15):
        rrs = []
    changed = True
    while changed:
        changed = False
        for (n, t, c, ttl, o, rdl) in rrs:
            if t == 5 and n == cur:
                if ncname >= 8:
                    looped = True
                    break
                cur, _ = rd_name(m, o); ncname += 1; ttls.append(ttl); changed = True
                break
        if looped:
            rrs = []
    for (n, t, c, ttl, o, rdl) in rrs:
        if n == cur and t == 1 and rdl == 4 and len(addrs) < 8:
            addrs.append(struct.unpack('>I', m[o:o + 4])[0]); ttls.append(ttl)
        if n == cur and t == 28 and rdl == 16 and len(addrs6) < 4:
            addrs6.append(m[o:o + 16]); ttls.append(ttl)
    return dict(rcode=flags & 15, tc=bool(flags & 0x200), addrs=addrs,
                addrs6=addrs6, ncname=ncname, cur=cur, looped=looped,
                ttl=min(ttls) if ttls else 0xFFFFFFFF)

# ------------------------------------------------------- hostile message builder
def name_bytes(n):
    out = b''
    for l in n.split('.'):
        if l: out += bytes([len(l)]) + l.encode()
    return out + b'\0'

def msg(qname, qtype, rrs, flags=0x8180, qd=1, an=None, qclass=1, ident=0x4242):
    """rrs: list of (owner_bytes, type, ttl, rdata_bytes)."""
    m = struct.pack('>HHHHHH', ident, flags, qd, len(rrs) if an is None else an, 0, 0)
    if qd: m += name_bytes(qname) + struct.pack('>HH', qtype, qclass)
    for (o, t, ttl, rd) in rrs:
        m += o + struct.pack('>HHIH', t, 1, ttl, len(rd)) + rd
    return m

def ip(a): return bytes(int(x) for x in a.split('.'))
PTR_Q = b'\xc0\x0c'                       # pointer to the question name

cases = []   # (title, message, qname, qtype)

for (n, t, d) in LIVE:
    cases.append(('live: %s type %d' % (n, t), d, n, t))

cases.append(('chain in REVERSED record order: the address first, the CNAMEs after',
    msg('a.test', 1, [
        (name_bytes('c.test'), 1, 60, ip('192.0.2.7')),
        (name_bytes('b.test'), 5, 300, name_bytes('c.test')),
        (PTR_Q, 5, 120, name_bytes('b.test'))]), 'a.test', 1))
cases.append(('a CNAME with no address behind it: the chain ends in another name',
    msg('a.test', 1, [(PTR_Q, 5, 77, name_bytes('b.test'))]), 'a.test', 1))
cases.append(('a CNAME cycle a -> b -> a',
    msg('a.test', 1, [
        (PTR_Q, 5, 60, name_bytes('b.test')),
        (name_bytes('b.test'), 5, 60, name_bytes('a.test'))]), 'a.test', 1))
chain = [(name_bytes('h%d.test' % i), 5, 50, name_bytes('h%d.test' % (i + 1))) for i in range(1, 12)]
chain.insert(0, (PTR_Q, 5, 50, name_bytes('h1.test')))
cases.append(('a chain of 12 CNAMEs (limit 8)', msg('a.test', 1, chain), 'a.test', 1))
cases.append(('records that do NOT belong to the chain are dropped (poisoning attempt)',
    msg('a.test', 1, [
        (name_bytes('evil.test'), 1, 60, ip('6.6.6.6')),
        (PTR_Q, 1, 60, ip('192.0.2.1')),
        (name_bytes('www.victim.test'), 1, 60, ip('6.6.6.7'))]), 'a.test', 1))
cases.append(('an A record with RDLENGTH 5 is ignored, the next one is used',
    msg('a.test', 1, [
        (PTR_Q, 1, 60, b'\x01\x02\x03\x04\x05'),
        (PTR_Q, 1, 90, ip('192.0.2.2'))]), 'a.test', 1))
cases.append(('owner name in UPPER CASE matches the lower case question',
    msg('a.test', 1, [(name_bytes('A.TEST'), 1, 60, ip('192.0.2.3'))]), 'a.test', 1))
cases.append(('question in upper case, answer in lower case',
    msg('A.Test', 1, [(PTR_Q, 1, 60, ip('192.0.2.4'))]), 'a.test', 1))
cases.append(('NOERROR with no records (NODATA)', msg('a.test', 1, []), 'a.test', 1))
cases.append(('NXDOMAIN', msg('a.test', 1, [], flags=0x8183), 'a.test', 1))
cases.append(('SERVFAIL', msg('a.test', 1, [], flags=0x8182), 'a.test', 1))
cases.append(('truncated (TC): the answer part is not trusted',
    msg('a.test', 1, [(PTR_Q, 1, 60, ip('192.0.2.5'))], flags=0x8380), 'a.test', 1))
cases.append(('eight A records, the ninth is dropped',
    msg('a.test', 1, [(PTR_Q, 1, 10 + i, ip('192.0.2.%d' % (10 + i))) for i in range(9)]), 'a.test', 1))
cases.append(('two AAAA records',
    msg('a.test', 28, [(PTR_Q, 28, 100, bytes(range(16))), (PTR_Q, 28, 50, bytes(range(16, 32)))]), 'a.test', 28))

# --- messages that are NOT an answer to the question: the parser says "no"
bad = []
bad.append(('a QUERY (QR clear)', msg('a.test', 1, [(PTR_Q, 1, 60, ip('1.2.3.4'))], flags=0x0100), 'a.test', 1))
bad.append(('opcode 2', msg('a.test', 1, [], flags=0x9180), 'a.test', 1))
bad.append(('another question name', msg('b.test', 1, [(PTR_Q, 1, 60, ip('1.2.3.4'))]), 'a.test', 1))
bad.append(('another question type', msg('a.test', 28, []), 'a.test', 1))
bad.append(('another question class', msg('a.test', 1, [], qclass=3), 'a.test', 1))
bad.append(('QDCOUNT 0', msg('a.test', 1, [], qd=0), 'a.test', 1))
bad.append(('QDCOUNT 2', struct.pack('>HHHHHH', 1, 0x8180, 2, 0, 0, 0) + name_bytes('a.test') + b'\0\1\0\1', 'a.test', 1))
bad.append(('shorter than a header', b'\x42\x42\x81\x80\x00\x01', 'a.test', 1))
bad.append(('empty', b'', 'a.test', 1))
good = msg('a.test', 1, [(PTR_Q, 1, 60, ip('1.2.3.4'))])
bad.append(('cut in the middle of the RDATA', good[:-2], 'a.test', 1))
bad.append(('cut in the middle of the record header', good[:-9], 'a.test', 1))
bad.append(('ANCOUNT claims 3, one record present', msg('a.test', 1, [(PTR_Q, 1, 60, ip('1.2.3.4'))], an=3), 'a.test', 1))
pre = struct.pack('>HHHHHH', 0x4242, 0x8180, 1, 1, 0, 0) + name_bytes('a.test') + b'\0\1\0\1'
loop = pre + bytes([0xC0, len(pre)]) + struct.pack('>HHIH', 1, 1, 60, 4) + ip('1.2.3.4')
# the owner pointer c0 1c points at itself (offset 28 = start of the record)
bad.append(('compression pointer pointing at itself', loop, 'a.test', 1))
far = msg('a.test', 1, [(b'\xc3\xff', 1, 60, ip('1.2.3.4'))])
bad.append(('compression pointer beyond the message', far, 'a.test', 1))
ext = msg('a.test', 1, [(b'\x40' + b'a' * 64 + b'\0', 1, 60, ip('1.2.3.4'))])
bad.append(('extended label type 01', ext, 'a.test', 1))
# pointer ping-pong: two pointers that point at each other
pp = struct.pack('>HHHHHH', 0x4242, 0x8180, 1, 1, 0, 0) + name_bytes('a.test') + b'\0\1\0\1'
base = len(pp)
pp += bytes([0xC0, base + 2]) + bytes([0xC0, base]) + struct.pack('>HHIH', 1, 1, 60, 4) + ip('1.2.3.4')
bad.append(('two pointers pointing at each other', pp, 'a.test', 1))
bad.append(('a label that runs past the end', msg('a.test', 1, [(b'\x3fabc', 1, 60, ip('1.2.3.4'))]), 'a.test', 1))
long_label = msg('a.test', 1, [(bytes([63]) + b'x' * 63 + bytes([63]) + b'y' * 63 + bytes([63]) + b'z' * 63 + bytes([63]) + b'w' * 63 + b'\0', 1, 60, ip('1.2.3.4'))])
bad.append(('an owner name of 256 octets', long_label, 'a.test', 1))

# ------------------------------------------------------------------ emit
out = []
w = out.append
def arr(name, data):
    w('    var %s: [u8; %d] = [%s]' % (name, len(data), ', '.join(str(b) for b in data)) if data else
      '    var %s: [u8; 1] = [0]' % name)

def bytes_lit(name, s):
    w('    var %s: [u8; %d] = "%s"' % (name, len(s), s))

w('''// expect_exit: 0
// SPDX-License-Identifier: MPL-2.0
// tests/2090_dns_wire.fi -- lib/net/dns.fi: the wire format, the parsers,
// and the refusals. GENERATED by tools/dns/gen_test.py; do not edit.
//
// WHAT IS CHECKED
//   A  seven REAL answers from 1.1.1.1 (plain A, CNAME chains of one and two
//      links, AAAA, NXDOMAIN with a SOA) -- expected values from an
//      independent Python decoder, not from the library.
//   B  hostile and odd messages: a chain in reversed order, a CNAME with no
//      address, a CNAME cycle, a 12-link chain, records off the chain,
//      a wrong RDLENGTH, mixed case, TC, SERVFAIL, nine A records.
//   C  messages that must be REFUSED ("this is not an answer to what was
//      asked"): QR clear, other opcode, other question name/type/class,
//      QDCOUNT 0 and 2, short, cut, a lying ANCOUNT, compression pointers
//      that loop or lie, an extended label type, a name of 256 octets.
//   D  the query bytes, the name rules, strict dotted quads, resolv.conf and
//      hosts parsing.
import std.rt
import std.net
import net.dns

fn expect(m: u64, n: usize, qname: u64, qn: usize, qtype: u32, rcode: u32,
    tc: bool, want: u64, nwant: usize, want6: u64, nwant6: usize,
    ncname: u32, ttl: u32, looped: bool, cur: u64, curn: usize) -> bool {
    var r: dns.Reply = dns.reply_new()
    if !dns.dns_parse_reply(m, n, qname, qn, qtype, &r) {
        return false
    }
    if r.rcode != rcode || r.truncated != tc {
        return false
    }
    if r.naddr != nwant || r.naddr6 != nwant6 || r.ncname != ncname {
        return false
    }
    var i: usize = 0
    while i < nwant {
        if r.addr[i] != *((want + (i * 4) as u64) as *mut u32) {
            return false
        }
        i = i + 1
    }
    var k: usize = 0
    while k < nwant6 * 16 {
        if r.addr6[k] != rt.ld8(want6, k) {
            return false
        }
        k = k + 1
    }
    if r.ttl != ttl || r.looped != looped {
        return false
    }
    if r.ncur != curn {
        return false
    }
    var j: usize = 0
    while j < curn {
        if r.cur[j] != rt.ld8(cur, j) {
            return false
        }
        j = j + 1
    }
    return true
}

fn refused(m: u64, n: usize, qname: u64, qn: usize, qtype: u32) -> bool {
    var r: dns.Reply = dns.reply_new()
    return !dns.dns_parse_reply(m, n, qname, qn, qtype, &r)
}
''')

idx = 0
good_cases = []
for (title, m, qn, qt) in cases:
    idx += 1
    ref = reference(m, qn, qt)
    w('// %s' % title)
    w('fn case_%d() -> i32 {' % idx)
    arr('m', m)
    bytes_lit('qn', qn)
    w('    var want: [u32; 8] = [%s]' % ', '.join([str(a) for a in ref['addrs']] + ['0'] * (8 - len(ref['addrs']))))
    a6 = b''.join(ref['addrs6']) or b'\0'
    w('    var want6: [u8; %d] = [%s]' % (len(a6), ', '.join(str(b) for b in a6)))
    bytes_lit('cur', ref['cur'])
    w('    if !expect((&m[0]) as u64, %d, (&qn[0]) as u64, %d, %d as u32, %d as u32, %s,'
      % (len(m), len(qn), qt, ref['rcode'], 'true' if ref['tc'] else 'false'))
    w('        (&want[0]) as u64, %d, (&want6[0]) as u64, %d, %d as u32, %d as u32,'
      % (len(ref['addrs']), len(ref['addrs6']), ref['ncname'], ref['ttl']))
    w('        %s, (&cur[0]) as u64, %d) {' % ('true' if ref['looped'] else 'false', len(ref['cur'])))
    w('        return %d' % idx)
    w('    }')
    w('    return 0')
    w('}\n')
    good_cases.append(idx)

# the cases whose reference expectations differ from what the library does on purpose:
# - 9 A records: the library keeps 8 (reference decoder returns 9) -> handled in the
#   expectation above by clamping the count; the 8 first addresses are the same.
# - 12-link chain: the library stops at 8 links and says `looped`; those cases are
#   emitted separately below by hand.

# hostile: refusals
for (title, m, qn, qt) in bad:
    idx += 1
    w('// refused: %s' % title)
    w('fn case_%d() -> i32 {' % idx)
    arr('m', m)
    bytes_lit('qn', qn)
    w('    if !refused((&m[0]) as u64, %d, (&qn[0]) as u64, %d, %d as u32) {' % (len(m), len(qn), qt))
    w('        return %d' % idx)
    w('    }')
    w('    return 0')
    w('}\n')
last_case = idx

w('''// ------------------------------------------------------------------ D: the rest

fn query_bytes() -> i32 {
    // id 0x1234, "example.com", A -- written out by hand
    var want: [u8; 29] = [18, 52, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 7, 101, 120, 97, 109, 112, 108, 101, 3, 99, 111, 109, 0, 0, 1, 0, 1]
    var nm: [u8; 11] = "example.com"
    var q: rt.Buf = rt.buf_new()
    if !dns.dns_build_query(&q, 4660 as u32, (&nm[0]) as u64, 11, 1 as u32) { return 1 }
    if q.len != 29 { return 2 }
    var i: usize = 0
    while i < 29 {
        if rt.ld8(q.ptr, i) != want[i] { return 3 }
        i = i + 1
    }
    // a trailing dot gives the same bytes
    var nd: [u8; 12] = "example.com."
    var q2: rt.Buf = rt.buf_new()
    if !dns.dns_build_query(&q2, 4660 as u32, (&nd[0]) as u64, 12, 1 as u32) { return 4 }
    if q2.len != 29 { return 5 }
    var k: usize = 0
    while k < 29 {
        if rt.ld8(q2.ptr, k) != want[k] { return 6 }
        k = k + 1
    }
    // AAAA: only the type changes
    var q3: rt.Buf = rt.buf_new()
    if !dns.dns_build_query(&q3, 4660 as u32, (&nm[0]) as u64, 11, 28 as u32) { return 7 }
    if rt.ld8(q3.ptr, 26) != 28 as u8 { return 8 }
    rt.buf_free(&q)
    rt.buf_free(&q2)
    rt.buf_free(&q3)
    return 0
}

fn name_ok(s: u64, n: usize) -> bool { return dns.dns_name_valid(s, n) }

fn names() -> i32 {
    var a: [u8; 11] = "example.com"
    if !name_ok((&a[0]) as u64, 11) { return 1 }
    var b: [u8; 9] = "a-b_c.d1e"
    if !name_ok((&b[0]) as u64, 9) { return 2 }
    var c: [u8; 6] = "a..bcd"
    if name_ok((&c[0]) as u64, 6) { return 3 }
    var d: [u8; 4] = ".abc"
    if name_ok((&d[0]) as u64, 4) { return 4 }
    var e: [u8; 11] = "ex ample.co"
    if name_ok((&e[0]) as u64, 11) { return 5 }
    if name_ok((&a[0]) as u64, 0) { return 6 }
    var f: [u8; 5] = [97, 195, 164, 46, 98]
    if name_ok((&f[0]) as u64, 5) { return 7 }
    // a label of 63 octets is fine, of 64 is not
    var l63: [u8; 63] = [97; 63]
    if !name_ok((&l63[0]) as u64, 63) { return 8 }
    var l64: [u8; 64] = [97; 64]
    if name_ok((&l64[0]) as u64, 64) { return 9 }
    // 253 octets in all is fine (four labels of 62 + three dots + 2 = 253), 254 is not
    var t: [u8; 254] = [97; 254]
    var i: usize = 0
    while i < 254 {
        if i % 63 == 62 { t[i] = 46 as u8 }
        i = i + 1
    }
    t[253] = 97 as u8
    if !name_ok((&t[0]) as u64, 253) { return 10 }
    if name_ok((&t[0]) as u64, 254) { return 11 }
    return 0
}

fn quad(s: u64, n: usize, want: u32) -> bool {
    var v: u32 = 0
    if !dns.dns_parse_ipv4(s, n, &v) { return false }
    return v == want
}

fn noquad(s: u64, n: usize) -> bool {
    var v: u32 = 0
    return !dns.dns_parse_ipv4(s, n, &v)
}

fn quads() -> i32 {
    var a: [u8; 9] = "127.0.0.1"
    if !quad((&a[0]) as u64, 9, net.ADDR_LOCAL) { return 1 }
    var b: [u8; 15] = "255.255.255.255"
    if !quad((&b[0]) as u64, 15, 4294967295 as u32) { return 2 }
    var c: [u8; 7] = "0.0.0.0"
    if !quad((&c[0]) as u64, 7, 0 as u32) { return 3 }
    var d: [u8; 11] = "192.168.3.7"
    if !quad((&d[0]) as u64, 11, 3232236295 as u32) { return 4 }
    var e: [u8; 9] = "1.2.3.256"
    if !noquad((&e[0]) as u64, 9) { return 5 }
    var f: [u8; 8] = "1.2.3.4."
    if !noquad((&f[0]) as u64, 8) { return 6 }
    var g: [u8; 5] = "1.2.3"
    if !noquad((&g[0]) as u64, 5) { return 7 }
    var h: [u8; 9] = "1.2.3.4.5"
    if !noquad((&h[0]) as u64, 9) { return 8 }
    var i: [u8; 8] = "01.2.3.4"
    if !noquad((&i[0]) as u64, 8) { return 9 }
    var j: [u8; 8] = "1.2.3.04"
    if !noquad((&j[0]) as u64, 8) { return 10 }
    var k: [u8; 7] = "1..3.4."
    if !noquad((&k[0]) as u64, 7) { return 11 }
    var l: [u8; 7] = "a.b.c.d"
    if !noquad((&l[0]) as u64, 7) { return 12 }
    var m: [u8; 8] = "1.2.3.-4"
    if !noquad((&m[0]) as u64, 8) { return 13 }
    var n: [u8; 8] = "1.2.3.4 "
    if !noquad((&n[0]) as u64, 8) { return 14 }
    var o: [u8; 7] = "1.2.3.4"
    if !quad((&o[0]) as u64, 7, 16909060 as u32) { return 15 }
    return 0
}

fn resolv_conf() -> i32 {
    @@T_RESOLV@@
    var s: [u32; 4] = [0; 4]
    let n: usize = dns.dns_parse_resolv_conf((&t[0]) as u64, @@N_T_RESOLV@@, &s)
    // 192.168.1.1, 10.0.0.2, (IPv6 skipped, duplicate skipped, comment skipped), 8.8.4.4, 1.1.1.1 -- the fifth is over the limit
    if n != 4 { return 1 }
    if s[0] != net.ipv4(192 as u8, 168 as u8, 1 as u8, 1 as u8) { return 2 }
    if s[1] != net.ipv4(10 as u8, 0 as u8, 0 as u8, 2 as u8) { return 3 }
    if s[2] != net.ipv4(8 as u8, 8 as u8, 4 as u8, 4 as u8) { return 4 }
    if s[3] != net.ipv4(1 as u8, 1 as u8, 1 as u8, 1 as u8) { return 5 }
    // nothing usable
    @@U_RESOLV@@
    var s2: [u32; 4] = [0; 4]
    if dns.dns_parse_resolv_conf((&u[0]) as u64, @@N_U_RESOLV@@, &s2) != 0 { return 6 }
    // the keyword is not a prefix match
    @@V_RESOLV@@
    var s3: [u32; 4] = [0; 4]
    if dns.dns_parse_resolv_conf((&v[0]) as u64, @@N_V_RESOLV@@, &s3) != 0 { return 7 }
    return 0
}

fn host_is(t: u64, n: usize, name: u64, nn: usize, want: u32) -> bool {
    var v: u32 = 0
    if !dns.dns_hosts_lookup(t, n, name, nn, &v) { return false }
    return v == want
}

fn host_not(t: u64, n: usize, name: u64, nn: usize) -> bool {
    var v: u32 = 0
    return !dns.dns_hosts_lookup(t, n, name, nn, &v)
}

fn hosts() -> i32 {
    @@T_HOSTS@@
    let tp: u64 = (&t[0]) as u64
    var a: [u8; 9] = "localhost"
    if !host_is(tp, @@N_T_HOSTS@@, (&a[0]) as u64, 9, net.ADDR_LOCAL) { return 1 }
    var b: [u8; 8] = "loopback"
    if !host_is(tp, @@N_T_HOSTS@@, (&b[0]) as u64, 8, net.ADDR_LOCAL) { return 2 }
    var c: [u8; 7] = "nas.lan"
    if !host_is(tp, @@N_T_HOSTS@@, (&c[0]) as u64, 7, net.ipv4(192 as u8, 168 as u8, 1 as u8, 10 as u8)) { return 3 }
    var d: [u8; 3] = "nas"
    if !host_is(tp, @@N_T_HOSTS@@, (&d[0]) as u64, 3, net.ipv4(192 as u8, 168 as u8, 1 as u8, 10 as u8)) { return 4 }
    // case-insensitive
    var e: [u8; 9] = "BUILD.LAN"
    if !host_is(tp, @@N_T_HOSTS@@, (&e[0]) as u64, 9, net.ipv4(10 as u8, 0 as u8, 0 as u8, 9 as u8)) { return 5 }
    // IPv6 lines are skipped
    var f: [u8; 10] = "localhost6"
    if !host_not(tp, @@N_T_HOSTS@@, (&f[0]) as u64, 10) { return 6 }
    // a name that only appears in a comment, a line with a bad address
    var g: [u8; 8] = "nas2.lan"
    if !host_not(tp, @@N_T_HOSTS@@, (&g[0]) as u64, 8) { return 7 }
    var h: [u8; 7] = "bad.lan"
    if !host_not(tp, @@N_T_HOSTS@@, (&h[0]) as u64, 7) { return 8 }
    // a prefix is not the name
    var i: [u8; 3] = "loc"
    if !host_not(tp, @@N_T_HOSTS@@, (&i[0]) as u64, 3) { return 9 }
    return 0
}

fn main() -> i32 {''')
for i in range(1, last_case + 1):
    w('    if case_%d() != 0 { return %d }' % (i, i))
w('''    let q: i32 = query_bytes()
    if q != 0 { return 1000 + q }
    let nm: i32 = names()
    if nm != 0 { return 1100 + nm }
    let qd: i32 = quads()
    if qd != 0 { return 1200 + qd }
    let rc: i32 = resolv_conf()
    if rc != 0 { return 1300 + rc }
    let hs: i32 = hosts()
    if hs != 0 { return 1400 + hs }
    return 0
}''')

T_RESOLV = b"# generated by NetworkManager\nsearch example.org\nnameserver 192.168.1.1\n  nameserver\t10.0.0.2   # the second\r\nnameserver 2001:4860:4860::8888\nnameserver 192.168.1.1\n; nameserver 9.9.9.9\noptions timeout:2\nnameserver 8.8.4.4\nnameserver 1.1.1.1\nnameserver 4.4.4.4\n"
U_RESOLV = b"nameserver ::1\nsearch a.b\n#x\n"
V_RESOLV = b"nameservers 192.168.1.1\n"
T_HOSTS = b"# hosts\n127.0.0.1\tlocalhost loopback\n::1 localhost6 ip6-localhost\n192.168.1.10   nas.lan  NAS  # comment nas2.lan\r\n10.0.0.9 build.lan\n999.1.1.1 bad.lan\n10.0.0.10 build.lan\n  \n192.168.1.11 \n"
text = '\n'.join(out) + '\n'
for key, val in (('T_RESOLV', T_RESOLV), ('U_RESOLV', U_RESOLV), ('V_RESOLV', V_RESOLV), ('T_HOSTS', T_HOSTS)):
    var = {'T_RESOLV': 't', 'U_RESOLV': 'u', 'V_RESOLV': 'v', 'T_HOSTS': 't'}[key]
    text = text.replace('@@%s@@' % key, 'var %s: [u8; %d] = [%s]' % (var, len(val), ', '.join(str(b) for b in val)))
    text = text.replace('@@N_%s@@' % key, str(len(val)))
sys.stdout.write(text)

