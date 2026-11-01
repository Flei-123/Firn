// expect_exit: 0
import media.video.vp9.vp9mem
import media.video.av1.av1ipm
import media.video.av1.av1mc
import media.video.av1.av1mcmask
import media.video.av1.av1wedge
import media.video.av1.av1obmc
import media.video.av1.av1warp
import media.video.av1.av1warpest

static CASES: [i32; @FLAT_COUNT@] = [
@FLAT@
]

static EXPECT: [u32; @CASES@] = [
@EXPECT@
]

const BIG: usize = 131072

struct Rng {
    s: i64,
}

struct Ctx {
    refp: u64,
    scratch: u64,
    out0: u64,
    out1: u64,
    dst: u64,
    mask: u64,
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
    wp: u64,
    t4: u64,
    tmp: u64,
    pts1: u64,
    pts2: u64,
    obl: u64,
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

fn pv(b: usize, i: usize) -> i64 {
    return CASES[b + i] as i64
}

fn hash_px(p: u64, n: i64) -> i64 {
    var h: i64 = 2166136261
    var i: i64 = 0
    while i < n {
        h = fnv(h, av1ipm.ld(p, i))
        i = i + 1
    }
    return h
}

fn hash_mask(h0: i64, m: u64, n: i64) -> i64 {
    var h: i64 = h0
    var i: i64 = 0
    while i < n {
        h = fnv(h, vp9mem.ldi16(m, i as usize))
        i = i + 1
    }
    return h
}

fn fill_ref(r: *mut Rng, p: u64, n: i64, bd: i64) {
    var i: i64 = 0
    while i < n {
        av1ipm.st(p, i, big(r, 1 << (bd as u64)))
        i = i + 1
    }
}

fn fill_comp(r: *mut Rng, p: u64, n: i64, bd: i64, post: i64) {
    var i: i64 = 0
    let half: i64 = (1 << (post as u64)) / 2
    while i < n {
        let pix: i64 = big(r, 1 << (bd as u64))
        let v: i64 = (pix << (post as u64)) + rnd(r, 1 << (post as u64)) - half
        av1ipm.st64(p, i, v)
        i = i + 1
    }
}

fn fill_pix64(r: *mut Rng, p: u64, n: i64, bd: i64) {
    var i: i64 = 0
    while i < n {
        av1ipm.st64(p, i, big(r, 1 << (bd as u64)))
        i = i + 1
    }
}

fn combine(cx: *mut Ctx, mode: i64, w: i64, h: i64, fwd: i64, bck: i64, bd: i64) {
    if mode == 0 {
        av1mcmask.single_to_plane((*cx).out0, w, h, (*cx).dst, w, bd)
    }
    if mode == 1 {
        av1mcmask.avg_to_plane((*cx).out0, (*cx).out1, w, h, (*cx).dst, w, bd)
    }
    if mode == 2 {
        av1mcmask.dist_to_plane((*cx).out0, (*cx).out1, w, h, fwd, bck, (*cx).dst, w, bd)
    }
}

fn run_mc(cx: *mut Ctx, b: usize) -> i64 {
    let bd: i64 = pv(b, 0)
    let w: i64 = pv(b, 1)
    let h: i64 = pv(b, 2)
    let fxt: i64 = pv(b, 3)
    let fyt: i64 = pv(b, 4)
    let rw: i64 = pv(b, 5)
    let rh: i64 = pv(b, 6)
    let fw: i64 = pv(b, 7)
    let fh: i64 = pv(b, 8)
    let x: i64 = pv(b, 9)
    let y: i64 = pv(b, 10)
    let ssx: i64 = pv(b, 11)
    let ssy: i64 = pv(b, 12)
    let mode: i64 = pv(b, 17)
    var r: Rng = Rng { s: pv(b, 20) }
    fill_ref(&r, (*cx).refp, rw * rh, bd)
    let scx: i64 = av1mc.scale_factor(rw, fw)
    let scy: i64 = av1mc.scale_factor(rh, fh)
    var comp: i64 = 0
    if mode != 0 {
        comp = 1
    }
    let stepx: i64 = av1mc.step_size(scx)
    let stepy: i64 = av1mc.step_size(scy)
    av1mc.predict_block((*cx).refp, rw, rw - 1, rh - 1, av1mc.start_pos(x, pv(b, 13), ssx, scx), av1mc.start_pos(y, pv(b, 14), ssy, scy), stepx, stepy, w, h, fxt, fyt, bd, comp, (*cx).scratch, (*cx).out0)
    if mode != 0 {
        av1mc.predict_block((*cx).refp, rw, rw - 1, rh - 1, av1mc.start_pos(x, pv(b, 15), ssx, scx), av1mc.start_pos(y, pv(b, 16), ssy, scy), stepx, stepy, w, h, fxt, fyt, bd, comp, (*cx).scratch, (*cx).out1)
    }
    combine(cx, mode, w, h, pv(b, 18), pv(b, 19), bd)
    return hash_px((*cx).dst, w * h)
}

fn run_mask(cx: *mut Ctx, b: usize) -> i64 {
    let bd: i64 = pv(b, 0)
    let w: i64 = pv(b, 1)
    let h: i64 = pv(b, 2)
    let ssx: i64 = pv(b, 3)
    let ssy: i64 = pv(b, 4)
    let typ: i64 = pv(b, 5)
    let bs: i64 = pv(b, 6)
    let widx: i64 = pv(b, 7)
    let sg: i64 = pv(b, 8)
    let inv: i64 = pv(b, 9)
    var r: Rng = Rng { s: pv(b, 10) }
    let post: i64 = av1mc.post_round_of(bd)
    let bwl: i64 = w << (ssx as u64)
    let bhl: i64 = h << (ssy as u64)
    var hsh: i64 = 2166136261
    if typ == 1 {
        fill_comp(&r, (*cx).a0, bwl * bhl, bd, post)
        fill_comp(&r, (*cx).a1, bwl * bhl, bd, post)
        fill_comp(&r, (*cx).a2, w * h, bd, post)
        fill_comp(&r, (*cx).a3, w * h, bd, post)
        av1mcmask.diffwtd_mask((*cx).a0, (*cx).a1, bwl, bhl, bd, inv, (*cx).mask)
        hsh = hash_mask(hsh, (*cx).mask, bwl * bhl)
        av1mcmask.mask_to_plane((*cx).a2, (*cx).a3, (*cx).mask, bwl, ssx, ssy, w, h, post, (*cx).dst, w, bd)
    }
    if typ == 0 {
        fill_comp(&r, (*cx).a2, w * h, bd, post)
        fill_comp(&r, (*cx).a3, w * h, bd, post)
        av1wedge.wedge_mask((*cx).obl, bs, widx, sg, (*cx).mask)
        hsh = hash_mask(hsh, (*cx).mask, bwl * bhl)
        av1mcmask.mask_to_plane((*cx).a2, (*cx).a3, (*cx).mask, bwl, ssx, ssy, w, h, post, (*cx).dst, w, bd)
    }
    if typ >= 2 {
        fill_pix64(&r, (*cx).a2, w * h, bd)
        fill_pix64(&r, (*cx).a3, w * h, bd)
        if typ == 3 {
            av1wedge.wedge_mask((*cx).obl, bs, widx, 0, (*cx).mask)
        }
        if typ == 2 {
            av1wedge.smooth_mask(bs, sg, (*cx).mask)
        }
        hsh = hash_mask(hsh, (*cx).mask, bwl * bhl)
        av1mcmask.mask_to_plane((*cx).a2, (*cx).a3, (*cx).mask, bwl, ssx, ssy, w, h, 0, (*cx).dst, w, bd)
    }
    var i: i64 = 0
    while i < w * h {
        hsh = fnv(hsh, av1ipm.ld((*cx).dst, i))
        i = i + 1
    }
    return hsh
}

fn run_obmc(cx: *mut Ctx, b: usize) -> i64 {
    let bd: i64 = pv(b, 0)
    let w: i64 = pv(b, 1)
    let h: i64 = pv(b, 2)
    let ov: i64 = pv(b, 3)
    let dir: i64 = pv(b, 4)
    var r: Rng = Rng { s: pv(b, 5) }
    fill_ref(&r, (*cx).a0, w * h, bd)
    fill_ref(&r, (*cx).a1, w * h, bd)
    if dir == 0 {
        av1obmc.blend_above((*cx).a0, w, (*cx).a1, w, w, ov)
    }
    if dir != 0 {
        av1obmc.blend_left((*cx).a0, w, (*cx).a1, w, h, ov)
    }
    return hash_px((*cx).a0, w * h)
}

fn setup_wp(cx: *mut Ctx, b: usize) -> bool {
    var i: i64 = 0
    while i < 6 {
        av1ipm.st64((*cx).wp, i, pv(b, i as usize))
        i = i + 1
    }
    let ok: i64 = av1warp.setup_shear((*cx).wp, (*cx).t4)
    if ok == 0 {
        return false
    }
    i = 0
    while i < 4 {
        av1ipm.st64((*cx).wp, 6 + i, av1ipm.ld64((*cx).t4, i))
        i = i + 1
    }
    return true
}

fn run_warp(cx: *mut Ctx, b: usize) -> i64 {
    let bd: i64 = pv(b, 0)
    let w: i64 = pv(b, 1)
    let h: i64 = pv(b, 2)
    let pc: i64 = pv(b, 3)
    let pr: i64 = pv(b, 4)
    let ssx: i64 = pv(b, 5)
    let ssy: i64 = pv(b, 6)
    let rw: i64 = pv(b, 7)
    let rh: i64 = pv(b, 8)
    let mode: i64 = pv(b, 9)
    var r: Rng = Rng { s: pv(b, 12) }
    fill_ref(&r, (*cx).refp, rw * rh, bd)
    var comp: i64 = 0
    if mode != 0 {
        comp = 1
    }
    if !setup_wp(cx, b + 13) {
        return 0
    }
    av1warp.warp_block((*cx).refp, rw, rw, rh, (*cx).wp, pc, pr, w, h, ssx, ssy, bd, comp, (*cx).tmp, (*cx).out0)
    if mode != 0 {
        if !setup_wp(cx, b + 19) {
            return 0
        }
        av1warp.warp_block((*cx).refp, rw, rw, rh, (*cx).wp, pc, pr, w, h, ssx, ssy, bd, comp, (*cx).tmp, (*cx).out1)
    }
    combine(cx, mode, w, h, pv(b, 10), pv(b, 11), bd)
    return hash_px((*cx).dst, w * h)
}

fn run_warpest(cx: *mut Ctx, b: usize) -> i64 {
    var i: i64 = 0
    while i < 16 {
        av1ipm.st64((*cx).pts1, i, pv(b, (7 + i) as usize))
        av1ipm.st64((*cx).pts2, i, pv(b, (23 + i) as usize))
        i = i + 1
    }
    let ok: i64 = av1warpest.warp_estimate(pv(b, 0), (*cx).pts1, (*cx).pts2, pv(b, 1), pv(b, 2), pv(b, 3), pv(b, 4), pv(b, 5), pv(b, 6), (*cx).wp)
    var hsh: i64 = 2166136261
    if ok != 0 {
        hsh = fnv(hsh, 1)
        i = 0
        while i < 6 {
            hsh = fnv(hsh, av1ipm.ld64((*cx).wp, i) & 4294967295)
            i = i + 1
        }
        while i < 10 {
            hsh = fnv(hsh, av1ipm.ld64((*cx).wp, i) & 65535)
            i = i + 1
        }
    }
    if ok == 0 {
        hsh = fnv(hsh, 0)
    }
    return hsh
}

fn main() -> i32 {
    var cx: Ctx = Ctx {
        refp: vp9mem.alloc(BIG),
        scratch: vp9mem.alloc((av1mc.SCRATCH_ELEMS * 8) as usize),
        out0: vp9mem.alloc(BIG),
        out1: vp9mem.alloc(BIG),
        dst: vp9mem.alloc(BIG),
        mask: vp9mem.alloc(BIG),
        a0: vp9mem.alloc(BIG),
        a1: vp9mem.alloc(BIG),
        a2: vp9mem.alloc(BIG),
        a3: vp9mem.alloc(BIG),
        wp: vp9mem.alloc(128),
        t4: vp9mem.alloc(128),
        tmp: vp9mem.alloc(1024),
        pts1: vp9mem.alloc(256),
        pts2: vp9mem.alloc(256),
        obl: av1wedge.wedge_obl_new(),
    }
    let counts: [i64; 6] = [21, 2, 11, 6, 25, 39]
    var pos: usize = 0
    var idx: usize = 0
    var bad: i32 = 0
    while idx < @CASES@ {
        let kind: i64 = pv(pos, 0)
        let b: usize = pos + 1
        var got: i64 = 0 - 1
        if kind == 0 {
            got = run_mc(&cx, b)
        }
        if kind == 1 {
            got = av1mcmask.dist_weights(pv(b, 0), pv(b, 1))
        }
        if kind == 2 {
            got = run_mask(&cx, b)
        }
        if kind == 3 {
            got = run_obmc(&cx, b)
        }
        if kind == 4 {
            got = run_warp(&cx, b)
        }
        if kind == 5 {
            got = run_warpest(&cx, b)
        }
        if got != (EXPECT[idx] as i64) {
            bad = bad + 1
        }
        pos = b + (counts[kind as usize] as usize)
        idx = idx + 1
    }
    if bad > 200 {
        return 200
    }
    return bad
}
