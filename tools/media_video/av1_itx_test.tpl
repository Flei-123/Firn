// expect_exit: 0
import media.video.vp9.vp9mem
import media.video.av1.av1itx

static EXPECT: [u32; @COUNT@] = [
@EXPECT@
]

static WL: [i64; 19] = [2, 3, 4, 5, 6, 2, 3, 3, 4, 4, 5, 5, 6, 2, 4, 3, 5, 4, 6]
static HL: [i64; 19] = [2, 3, 4, 5, 6, 3, 2, 4, 3, 5, 4, 6, 5, 4, 2, 5, 3, 6, 4]
static VT: [i64; 16] = [0, 1, 0, 1, 2, 0, 2, 1, 2, 3, 0, 3, 1, 3, 2, 3]
static HT: [i64; 16] = [0, 0, 1, 1, 0, 2, 2, 2, 1, 3, 3, 0, 3, 1, 3, 2]

struct Rng {
    s: i64,
}

fn rnd(r: *mut Rng, n: i64) -> i64 {
    (*r).s = ((*r).s * 1103515245 + 12345) & 2147483647
    return (((*r).s >> 16) & 32767) % n
}

fn big(r: *mut Rng, n: i64) -> i64 {
    let a: i64 = rnd(r, 32768)
    let b: i64 = rnd(r, 32768)
    return (a * 32768 + b) % n
}

fn fnv(h: i64, b: i64) -> i64 {
    return ((h ^ b) * 16777619) & 4294967295
}

fn kind_valid(kind: i64, lg: i64) -> bool {
    if kind == 0 {
        return true
    }
    if kind == 3 {
        return lg <= 5
    }
    return lg <= 4
}

fn run_case(seed: i64, ts: i64, tt: i64, bd: i64, coefs: u64, dest: u64, scr: u64) -> i64 {
    var w: i64 = 4
    var h: i64 = 4
    if ts >= 0 {
        w = 1 << (WL[ts as usize] as u64)
        h = 1 << (HL[ts as usize] as u64)
    }
    var r: Rng = Rng { s: seed }
    var i: i64 = 0
    while i < w * h {
        vp9mem.sti(coefs, i as usize, 0)
        i = i + 1
    }
    var cw: i64 = w
    var ch: i64 = h
    if cw > 32 {
        cw = 32
    }
    if ch > 32 {
        ch = 32
    }
    var cls: i64 = rnd(&r, 3)
    if ts < 0 {
        cls = 0
    }
    var mag: i64 = 200
    if cls == 1 {
        mag = 1 << ((bd + 3) as u64)
    }
    if cls == 2 {
        mag = 1 << ((bd + 6) as u64)
    }
    if rnd(&r, 4) == 0 {
        var y: i64 = 0
        while y < ch {
            var x: i64 = 0
            while x < cw {
                vp9mem.sti(coefs, (y * w + x) as usize, big(&r, 81) - 40)
                x = x + 1
            }
            y = y + 1
        }
    } else {
        let n: i64 = 1 + rnd(&r, 12)
        var k: i64 = 0
        while k < n {
            let x: i64 = rnd(&r, cw)
            let y: i64 = rnd(&r, ch)
            vp9mem.sti(coefs, (y * w + x) as usize, big(&r, 2 * mag + 1) - mag)
            k = k + 1
        }
    }
    i = 0
    while i < w * h {
        vp9mem.sti16(dest, i as usize, big(&r, 1 << (bd as u64)))
        i = i + 1
    }
    if ts < 0 {
        av1itx.iwht_add(coefs, dest, w, bd, scr)
    } else {
        av1itx.itx_add(coefs, dest, w, ts, tt, bd, scr)
    }
    var hh: i64 = 2166136261
    i = 0
    while i < w * h {
        hh = fnv(hh, vp9mem.ld16(dest, i as usize))
        i = i + 1
    }
    return hh
}

fn main() -> i32 {
    let coefs: u64 = vp9mem.alloc(16384)
    let dest: u64 = vp9mem.alloc(8192)
    let scr: u64 = vp9mem.alloc(av1itx.ITX_SCRATCH)
    var idx: usize = 0
    var bad: i32 = 0
    var ts: i64 = 0
    while ts < 19 {
        var tt: i64 = 0
        while tt < 16 {
            if kind_valid(VT[tt as usize], HL[ts as usize]) && kind_valid(HT[tt as usize], WL[ts as usize]) {
                var bi: i64 = 0
                while bi < 3 {
                    var bd: i64 = 8
                    if bi == 1 {
                        bd = 10
                    }
                    if bi == 2 {
                        bd = 12
                    }
                    var k: i64 = 0
                    while k < 2 {
                        let seed: i64 = ((ts * 16 + tt) * 3 + bi) * 4 + k + 1
                        if run_case(seed, ts, tt, bd, coefs, dest, scr) != (EXPECT[idx] as i64) {
                            bad = bad + 1
                        }
                        idx = idx + 1
                        k = k + 1
                    }
                    bi = bi + 1
                }
            }
            tt = tt + 1
        }
        ts = ts + 1
    }
    var bi: i64 = 0
    while bi < 3 {
        var bd: i64 = 8
        if bi == 1 {
            bd = 10
        }
        if bi == 2 {
            bd = 12
        }
        var k: i64 = 0
        while k < 8 {
            if run_case(100000 + bd * 10 + k, 0 - 1, 0, bd, coefs, dest, scr) != (EXPECT[idx] as i64) {
                bad = bad + 1
            }
            idx = idx + 1
            k = k + 1
        }
        bi = bi + 1
    }
    if idx != @COUNT@ {
        return 99
    }
    return bad
}
