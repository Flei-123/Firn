// expect_exit: 0
import media.video.vp9.vp9mem
import media.video.vp9.vp9state
import media.video.vp9.vp9scale
import media.video.vp9.vp9mc
import media.video.vp9.vp9mcblk

@EXPECT1@
@EXPECT2@
static REFW: [u8; 4] = [37, 63, 19, 74]
static REFH: [u8; 4] = [29, 45, 15, 58]
static BW4: [u8; 13] = [1, 1, 2, 2, 2, 4, 4, 4, 8, 8, 8, 16, 16]
static BH4: [u8; 13] = [1, 2, 1, 2, 4, 2, 4, 8, 4, 8, 16, 8, 16]

struct Rng {
    s: i64,
}

fn rnd(r: *mut Rng, n: i64) -> i64 {
    (*r).s = ((*r).s * 1103515245 + 12345) & 2147483647
    return (((*r).s >> 16) & 32767) % n
}

fn fill(p: u64, n: usize, r: *mut Rng) {
    var i: usize = 0
    while i < n {
        vp9mem.st8(p, i, rnd(r, 256))
        i = i + 1
    }
}

fn fill_buf(b: *vp9state.Buf9, r: *mut Rng) {
    fill((*b).y, (*b).cap_y, r)
    fill((*b).u, (*b).cap_uv, r)
    fill((*b).v, (*b).cap_uv, r)
}

fn plane_of(b: *vp9state.Buf9, plane: i64) -> u64 {
    if plane == 0 {
        return (*b).y
    }
    if plane == 1 {
        return (*b).u
    }
    return (*b).v
}

fn stride_of(b: *vp9state.Buf9, plane: i64) -> i64 {
    if plane == 0 {
        return (*b).ys
    }
    return (*b).uvs
}

fn fnv(h: i64, b: i64) -> i64 {
    return ((h ^ b) * 16777619) & 4294967295
}

fn hash_rect(h0: i64, org: u64, stride: i64, w: i64, h: i64) -> i64 {
    var hh: i64 = h0
    var j: i64 = 0
    while j < h {
        var i: i64 = 0
        while i < w {
            hh = fnv(hh, vp9mem.ld8(org, (j * stride + i) as usize))
            i = i + 1
        }
        j = j + 1
    }
    return hh
}

fn case1(f: *mut vp9state.Vp9, mc: *vp9mc.Mc, c: i64) -> i64 {
    var rng: Rng = Rng { s: c * 7919 + 1 }
    let kd: usize = (c % 4) as usize
    let rw: i64 = REFW[kd] as i64
    let rh: i64 = REFH[kd] as i64
    let refb: *mut vp9state.Buf9 = &(*f).pool[0]
    let cur: *mut vp9state.Buf9 = &(*f).pool[2]
    vp9state.buf_alloc(refb, rw, rh)
    vp9state.buf_alloc(cur, 37, 29)
    fill_buf(refb, &rng)
    fill_buf(cur, &rng)
    let plane: i64 = rnd(&rng, 3)
    var ss: i64 = 0
    if plane > 0 {
        ss = 1
    }
    let bw8: i64 = 1 << (rnd(&rng, 4) as u64)
    let bh8: i64 = 1 << (rnd(&rng, 4) as u64)
    let mi_col: i64 = rnd(&rng, 5) / bw8 * bw8
    let mi_row: i64 = rnd(&rng, 4) / bh8 * bh8
    let n4w: i64 = (bw8 * 2) >> (ss as u64)
    let n4h: i64 = (bh8 * 2) >> (ss as u64)
    let bw: i64 = n4w * 4
    let bh: i64 = n4h * 4
    var x: i64 = 0
    var y: i64 = 0
    var w: i64 = bw
    var h: i64 = bh
    if bw8 == 1 && bh8 == 1 {
        if rnd(&rng, 2) == 0 {
            x = 4 * rnd(&rng, n4w)
            y = 4 * rnd(&rng, n4h)
            w = 4
            h = 4
        }
    }
    var mv_row: i64 = rnd(&rng, 801) - 400
    var mv_col: i64 = rnd(&rng, 801) - 400
    if rnd(&rng, 4) == 0 {
        mv_row = rnd(&rng, 4001) - 2000
        mv_col = rnd(&rng, 4001) - 2000
    }
    let k: i64 = rnd(&rng, 4)
    let avg: bool = rnd(&rng, 2) == 1
    var sf: vp9scale.Scale = vp9scale.scale_new()
    if !vp9scale.scale_setup(&sf, rw, rh, 37, 29) {
        return 0 - 1
    }
    var pos: vp9mc.McPos = vp9mc.McPos { mi_x: 0, mi_y: 0, to_left: 0, to_right: 0, to_top: 0, to_bottom: 0, kernel: 0 }
    vp9mc.mcpos_set(&pos, mi_row, mi_col, bw8, bh8, 4, 5, k)
    var pcw: i64 = rw
    var pch: i64 = rh
    if plane > 0 {
        pcw = (rw + 1) >> 1
        pch = (rh + 1) >> 1
    }
    let rp: vp9mc.RefPlane = vp9mc.RefPlane { p: plane_of(refb, plane), stride: stride_of(refb, plane), cw: pcw, ch: pch }
    let dstride: i64 = stride_of(cur, plane)
    let dorg: u64 = plane_of(cur, plane) + (((mi_row * 8) >> (ss as u64)) * dstride + ((mi_col * 8) >> (ss as u64))) as u64
    vp9mc.mc_build(mc, &sf, &rp, &pos, dorg, dstride, ss, ss, bw, bh, x, y, w, h, mv_row, mv_col, avg)
    return hash_rect(2166136261, dorg + (y * dstride + x) as u64, dstride, w, h)
}

fn case2(f: *mut vp9state.Vp9, mc: *vp9mc.Mc, c: i64) -> i64 {
    var rng: Rng = Rng { s: c * 104729 + 3 }
    let ka: usize = rnd(&rng, 4) as usize
    let kb: usize = rnd(&rng, 4) as usize
    vp9state.buf_alloc(&(*f).pool[0], REFW[ka] as i64, REFH[ka] as i64)
    vp9state.buf_alloc(&(*f).pool[1], REFW[kb] as i64, REFH[kb] as i64)
    vp9state.buf_alloc(&(*f).pool[2], 37, 29)
    fill_buf(&(*f).pool[0], &rng)
    fill_buf(&(*f).pool[1], &rng)
    fill_buf(&(*f).pool[2], &rng)
    let bsize: i64 = rnd(&rng, 13)
    let bw8: i64 = vp9mem.imax(1, (BW4[bsize as usize] as i64) / 2)
    let bh8: i64 = vp9mem.imax(1, (BH4[bsize as usize] as i64) / 2)
    let mi_col: i64 = rnd(&rng, 5) / bw8 * bw8
    let mi_row: i64 = rnd(&rng, 4) / bh8 * bh8
    let ref0: i64 = 1 + rnd(&rng, 3)
    var ref1: i64 = 0
    if rnd(&rng, 2) == 1 {
        ref1 = 1 + rnd(&rng, 3)
    }
    let k: i64 = rnd(&rng, 4)
    let mi: u64 = vp9state.mi_ptr(f, mi_row, mi_col)
    vp9mem.st8(mi, vp9state.MI_BSIZE, bsize)
    vp9mem.st8(mi, vp9state.MI_REF0, ref0)
    vp9mem.st8(mi, vp9state.MI_REF1, ref1)
    vp9mem.st8(mi, vp9state.MI_FILTER, k)
    var r: i64 = 0
    while r < 2 {
        let row: i64 = rnd(&rng, 801) - 400
        let col: i64 = rnd(&rng, 801) - 400
        vp9mem.sti16(mi, (8 + r * 2) as usize, row)
        vp9mem.sti16(mi, (9 + r * 2) as usize, col)
        r = r + 1
    }
    var b: i64 = 0
    while b < 4 {
        var q: i64 = 0
        while q < 2 {
            let row: i64 = rnd(&rng, 801) - 400
            let col: i64 = rnd(&rng, 801) - 400
            vp9mem.sti16(mi, (12 + b * 4 + q * 2) as usize, row)
            vp9mem.sti16(mi, (13 + b * 4 + q * 2) as usize, col)
            q = q + 1
        }
        b = b + 1
    }
    if !vp9mcblk.mc_inter_block(mc, f, mi_row, mi_col) {
        return 0 - 1
    }
    let cur: *mut vp9state.Buf9 = &(*f).pool[2]
    var hh: i64 = 2166136261
    var plane: i64 = 0
    while plane < 3 {
        var ss: i64 = 0
        if plane > 0 {
            ss = 1
        }
        let dstride: i64 = stride_of(cur, plane)
        let dorg: u64 = plane_of(cur, plane) + (((mi_row * 8) >> (ss as u64)) * dstride + ((mi_col * 8) >> (ss as u64))) as u64
        hh = hash_rect(hh, dorg, dstride, ((bw8 * 2) >> (ss as u64)) * 4, ((bh8 * 2) >> (ss as u64)) * 4)
        plane = plane + 1
    }
    return hh
}

fn hand_checks(f: *mut vp9state.Vp9, mc: *vp9mc.Mc) -> i32 {
    let refb: *mut vp9state.Buf9 = &(*f).pool[0]
    let cur: *mut vp9state.Buf9 = &(*f).pool[2]
    vp9state.buf_alloc(refb, 37, 29)
    vp9state.buf_alloc(cur, 37, 29)
    var i: usize = 0
    while i < (*refb).cap_y {
        vp9mem.st8((*refb).y, i, 100)
        i = i + 1
    }
    let rp: vp9mc.RefPlane = vp9mc.RefPlane { p: (*refb).y, stride: (*refb).ys, cw: 37, ch: 29 }
    var k: i64 = 0
    while k < 4 {
        vp9mc.mc_predict(mc, &rp, 5, 5, 3, 11, 16, 16, 8, 8, k, (*cur).y, (*cur).ys, false)
        if hash_rect(7, (*cur).y, (*cur).ys, 8, 8) != hash_rect(7, (*refb).y, (*refb).ys, 8, 8) {
            return 241
        }
        k = k + 1
    }
    var yy: usize = 0
    while yy < 29 {
        var xx: usize = 0
        while xx < 24 {
            vp9mem.st8((*refb).y, yy * ((*refb).ys as usize) + xx, 10 * xx as i64)
            xx = xx + 1
        }
        yy = yy + 1
    }
    vp9mc.mc_predict(mc, &rp, 5, 5, 8, 0, 16, 16, 8, 8, 3, (*cur).y, (*cur).ys, false)
    var d: usize = 0
    while d < 8 {
        if vp9mem.ld8((*cur).y, d) != 10 * (5 + d as i64) + 5 {
            return 242
        }
        d = d + 1
    }
    vp9mem.st8((*refb).y, 0, 77)
    vp9mc.mc_predict(mc, &rp, 0 - 20, 0 - 20, 0, 0, 16, 16, 4, 4, 0, (*cur).y, (*cur).ys, false)
    if vp9mem.ld8((*cur).y, 0) != 77 || vp9mem.ld8((*cur).y, 3) != 77 || vp9mem.ld8((*cur).y, (3 * (*cur).ys + 3) as usize) != 77 {
        return 243
    }
    var sf: vp9scale.Scale = vp9scale.scale_new()
    if vp9scale.scale_setup(&sf, 100, 100, 40, 40) {
        return 244
    }
    if !vp9scale.scale_setup(&sf, 80, 60, 40, 30) || sf.xstep != 32 || sf.ystep != 32 {
        return 245
    }
    if !vp9scale.scale_setup(&sf, 63, 45, 37, 29) || sf.xfp != 27897 || sf.xstep != 27 {
        return 246
    }
    if vp9scale.round_mv_q4(0 - 5) != 0 - 1 || vp9scale.round_mv_q4(6) != 2 || vp9scale.round_mv_q4(0 - 6) != 0 - 2 {
        return 247
    }
    return 0
}

fn main() -> i32 {
    var f: vp9state.Vp9 = vp9state.vp9_new()
    if !vp9state.vp9_setup(&f, 37, 29) {
        return 250
    }
    var mc: vp9mc.Mc = vp9mc.mc_new()
    if !vp9mc.mc_ready(&mc) {
        return 251
    }
    f.slot[0] = 0
    f.slot[1] = 1
    f.ref_idx[0] = 0
    f.ref_idx[1] = 1
    f.ref_idx[2] = 0
    f.cur = 2
    let hc: i32 = hand_checks(&f, &mc)
    if hc != 0 {
        return hc
    }
    var c: i64 = 0
    while c < @N1@ {
        if case1(&f, &mc, c) != (EXPECT1[c as usize] as i64) {
            return (c + 1) as i32
        }
        c = c + 1
    }
    c = 0
    while c < @N2@ {
        if case2(&f, &mc, c) != (EXPECT2[c as usize] as i64) {
            return (c + 1 + @N1@) as i32
        }
        c = c + 1
    }
    return 0
}
