// expect_exit: 0
import media.video.vp9.vp9mem
import media.video.av1.av1ipred
import media.video.av1.av1ipcfl

static EXPECT: [u32; @COUNT@] = [
@EXPECT@
]

static WL: [i64; 19] = [2, 3, 4, 5, 6, 2, 3, 3, 4, 4, 5, 5, 6, 2, 4, 3, 5, 4, 6]
static HL: [i64; 19] = [2, 3, 4, 5, 6, 3, 2, 4, 3, 5, 4, 6, 5, 4, 2, 5, 3, 6, 4]

const PW: i64 = 144
const PH: i64 = 144

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

fn fill_plane(r: *mut Rng, p: u64, bd: i64) {
    var i: i64 = 0
    while i < PW * PH {
        vp9mem.sti16(p, i as usize, big(r, 1 << (bd as u64)))
        i = i + 1
    }
}

fn hash_plane(p: u64) -> i64 {
    var h: i64 = 2166136261
    var i: i64 = 0
    while i < PW * PH {
        h = fnv(h, vp9mem.ld16(p, i as usize))
        i = i + 1
    }
    return h
}

fn edge_dist(c: i64, n: i64, bigv: i64, mid: i64) -> i64 {
    if c < 4 {
        return n + bigv
    }
    if c < 6 {
        return mid - (n - 1)
    }
    if c == 6 {
        return 0
    }
    return 1 + (bigv % 4)
}

fn plane_seed(seed: i64) -> i64 {
    return seed * 7919 + 13
}

fn run_pred(seed: i64, ts: i64, bd: i64, slot: i64, ad: i64, plane: u64, scr: u64) -> i64 {
    let w: i64 = 1 << (WL[ts as usize] as u64)
    let h: i64 = 1 << (HL[ts as usize] as u64)
    var r: Rng = Rng { s: seed }
    let hl: bool = rnd(&r, 5) != 0
    let ha: bool = rnd(&r, 5) != 0
    let t1: i64 = rnd(&r, 2)
    let t2: i64 = rnd(&r, 2)
    let htr: bool = ha && t1 == 1
    let hbl: bool = hl && t2 == 1
    let cx: i64 = rnd(&r, 8)
    let rx: i64 = rnd(&r, 32)
    let ry: i64 = rnd(&r, 32)
    let sx: i64 = rnd(&r, 2 * w - 1)
    let sy: i64 = rnd(&r, 2 * h - 1)
    let cy: i64 = rnd(&r, 8)
    let xr: i64 = edge_dist(cx, w, rx, sx)
    let yd: i64 = edge_dist(cy, h, ry, sy)
    let et: i64 = rnd(&r, 2)
    let ef: bool = rnd(&r, 5) != 0
    let x: i64 = 1 + rnd(&r, 8)
    let y: i64 = 1 + rnd(&r, 8)
    var pr: Rng = Rng { s: plane_seed(seed) }
    fill_plane(&pr, plane, bd)
    var mode: i64 = slot
    var fm: i64 = 0 - 1
    if slot > 12 {
        mode = 0
        fm = slot - 13
    }
    av1ipred.predict_intra(plane, PW, x, y, w, h, mode, ad, fm, hl, ha, htr, hbl, xr, yd, et, ef, bd, scr)
    return hash_plane(plane)
}

fn run_cfl(seed: i64, ts: i64, bd: i64, si: i64, luma: u64, chroma: u64, tmp: u64) -> i64 {
    let w: i64 = 1 << (WL[ts as usize] as u64)
    let h: i64 = 1 << (HL[ts as usize] as u64)
    var sx: i64 = 1
    var sy: i64 = 1
    if si == 1 {
        sy = 0
    }
    if si == 2 {
        sx = 0
        sy = 0
    }
    var r: Rng = Rng { s: seed }
    let alpha: i64 = rnd(&r, 33) - 16
    let c1: i64 = rnd(&r, 3)
    let r1: i64 = rnd(&r, w)
    let c2: i64 = rnd(&r, 3)
    let r2: i64 = rnd(&r, h)
    var vw: i64 = 1 + r1
    var vh: i64 = 1 + r2
    if c1 == 0 {
        vw = w
    }
    if c2 == 0 {
        vh = h
    }
    let lx: i64 = 1 + rnd(&r, 8)
    let ly: i64 = 1 + rnd(&r, 8)
    var pr: Rng = Rng { s: plane_seed(seed) }
    fill_plane(&pr, luma, bd)
    fill_plane(&pr, chroma, bd)
    let dst: u64 = chroma + ((4 * PW + 4) * 2) as u64
    av1ipcfl.pred_cfl(luma, PW, lx, ly, lx + (vw << (sx as u64)), ly + (vh << (sy as u64)), dst, PW, w, h, sx, sy, alpha, bd, tmp)
    return (hash_plane(chroma) ^ ((hash_plane(luma) * 31) & 4294967295))
}

fn bd_of(bi: i64) -> i64 {
    if bi == 1 {
        return 10
    }
    if bi == 2 {
        return 12
    }
    return 8
}

fn main() -> i32 {
    let plane: u64 = vp9mem.alloc((PW * PH * 2) as usize)
    let plane2: u64 = vp9mem.alloc((PW * PH * 2) as usize)
    let scr: u64 = vp9mem.alloc(av1ipred.IPRED_SCRATCH)
    let tmp: u64 = vp9mem.alloc(4096)
    var idx: usize = 0
    var seed: i64 = 0
    var bad: i32 = 0
    var ts: i64 = 0
    while ts < 19 {
        let w: i64 = 1 << (WL[ts as usize] as u64)
        let h: i64 = 1 << (HL[ts as usize] as u64)
        var bi: i64 = 0
        while bi < 3 {
            var slot: i64 = 0
            while slot < 18 {
                if slot < 13 || (w <= 32 && h <= 32) {
                    var lo: i64 = 0
                    var hi: i64 = 0
                    if slot >= 1 && slot <= 8 {
                        lo = 0 - 3
                        hi = 3
                    }
                    var ad: i64 = lo
                    while ad <= hi {
                        var k: i64 = 0
                        while k < 2 {
                            seed = seed + 1
                            if run_pred(seed, ts, bd_of(bi), slot, ad, plane, scr) != (EXPECT[idx] as i64) {
                                bad = bad + 1
                            }
                            idx = idx + 1
                            k = k + 1
                        }
                        ad = ad + 1
                    }
                }
                slot = slot + 1
            }
            bi = bi + 1
        }
        ts = ts + 1
    }
    ts = 0
    while ts < 19 {
        if WL[ts as usize] <= 5 && HL[ts as usize] <= 5 {
            var bi: i64 = 0
            while bi < 3 {
                var si: i64 = 0
                while si < 3 {
                    var k: i64 = 0
                    while k < 3 {
                        seed = seed + 1
                        if run_cfl(seed, ts, bd_of(bi), si, plane, plane2, tmp) != (EXPECT[idx] as i64) {
                            bad = bad + 1
                        }
                        idx = idx + 1
                        k = k + 1
                    }
                    si = si + 1
                }
                bi = bi + 1
            }
        }
        ts = ts + 1
    }
    if idx != @COUNT@ {
        return 99
    }
    return bad
}
