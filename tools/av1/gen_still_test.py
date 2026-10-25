import sys

head = '''// expect_exit: 42
import av1.still
import std.fs
import std.rt
import std.deflate

static mut seed: u64 = 987654321

fn rnd() -> u32 {
    seed = (seed * 1103515245 + 12345) & 2147483647
    return ((seed >> 8) & 16777215) as u32
}

fn load(name: str, data: *mut rt.Buf) -> usize {
    let n: usize = fs.read_file(name, data) catch 0
    return n
}

fn crc_of(name: str, want_crc: u32, want_size: usize) -> bool {
    var data: rt.Buf = rt.buf_new()
    let n: usize = load(name, &data)
    if n == 0 {
        return false
    }
    var f: still.Frame = still.frame_new()
    let rc: i32 = still.still_decode(rt.buf_ptr(&data), n, &f)
    if rc != still.STILL_OK {
        return false
    }
    var all: rt.Buf = rt.buf_new()
    rt.buf_push_bytes(&all, f.y, f.y_bytes)
    rt.buf_push_bytes(&all, f.u, f.u_bytes)
    rt.buf_push_bytes(&all, f.v, f.v_bytes)
    let c: u32 = deflate.crc32(rt.buf_ptr(&all), rt.buf_len(&all))
    let sz: usize = rt.buf_len(&all)
    rt.buf_free(&all)
    still.frame_free(&f)
    rt.buf_free(&data)
    return c == want_crc && sz == want_size
}

fn refuses(name: str, want_rc: i32) -> bool {
    var data: rt.Buf = rt.buf_new()
    let n: usize = load(name, &data)
    if n == 0 {
        return false
    }
    var f: still.Frame = still.frame_new()
    let rc: i32 = still.still_decode(rt.buf_ptr(&data), n, &f)
    rt.buf_free(&data)
    if rc == still.STILL_OK {
        still.frame_free(&f)
        return false
    }
    return rc == want_rc
}

fn survives_cuts(name: str, step: usize) -> bool {
    var data: rt.Buf = rt.buf_new()
    let n: usize = load(name, &data)
    if n == 0 {
        return false
    }
    var c: usize = 0
    while c < n {
        var f: still.Frame = still.frame_new()
        let rc: i32 = still.still_decode(rt.buf_ptr(&data), c, &f)
        if rc == still.STILL_OK {
            still.frame_free(&f)
        }
        c = c + step
    }
    rt.buf_free(&data)
    return true
}

fn survives_flips(name: str, copies: u32) -> bool {
    var data: rt.Buf = rt.buf_new()
    let n: usize = load(name, &data)
    if n == 0 {
        return false
    }
    var work: rt.Buf = rt.buf_new()
    var k: u32 = 0
    while k < copies {
        rt.buf_clear(&work)
        rt.buf_push_bytes(&work, rt.buf_ptr(&data), n)
        let hits: u32 = 1 + rnd() % 3
        var j: u32 = 0
        while j < hits {
            let at: usize = (rnd() as usize) % n
            rt.st8(rt.buf_ptr(&work), at, (rnd() % 256) as u8)
            j = j + 1
        }
        var f: still.Frame = still.frame_new()
        let rc: i32 = still.still_decode(rt.buf_ptr(&work), n, &f)
        if rc == still.STILL_OK {
            still.frame_free(&f)
        }
        k = k + 1
    }
    rt.buf_free(&work)
    rt.buf_free(&data)
    return true
}

fn main() -> i32 {
'''
lines = [l.split() for l in open('tests/data/av1/manifest.txt') if l.strip()]
out = [head]
i = 1
for l in lines:
    p = 'tests/data/av1/%s.avif' % l[0]
    if l[1] == 'ok':
        out.append('    if !crc_of("%s", 0x%s as u32, %s) { return %d }\n' % (p, l[2], l[3], i))
    else:
        out.append('    if !refuses("%s", %s) { return %d }\n' % (p, l[2], i))
    i += 1
for name in ('lf_n160_q140_444', 'lr_a', 'cdef_a', 'n64_444_nofilter'):
    p = 'tests/data/av1/%s.avif' % name
    out.append('    if !survives_cuts("%s", 37) { return %d }\n' % (p, i)); i += 1
    out.append('    if !survives_flips("%s", 60 as u32) { return %d }\n' % (p, i)); i += 1
out.append('    return 42\n}\n')
open(sys.argv[1], 'w').write(''.join(out))
