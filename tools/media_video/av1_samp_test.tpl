// expect_exit: 0
import media.video.vp9.vp9mem
import media.video.av1.av1ipm
import media.video.av1.av1warpsamp

static CASES: [i32; @FLAT_COUNT@] = [
@FLAT@
]

static EXPECT: [u32; @CASES@] = [
@EXPECT@
]

static BW4: [i32; 22] = [@BW4@]
static BH4: [i32; 22] = [@BH4@]

fn fnv(h: i64, b: i64) -> i64 {
    return ((h ^ b) * 16777619) & 4294967295
}

fn pv(i: usize) -> i64 {
    return CASES[i] as i64
}

fn get_field(ctx: u64, field: i64, row: i64, col: i64) -> i64 {
    let bw4: i64 = av1ipm.ld64(ctx, 0)
    var idx: i64 = 0
    if row < 0 {
        idx = col + 1
    } else {
        idx = bw4 + 2 + row
    }
    return av1ipm.ld64(ctx, 1 + idx * 6 + field)
}

fn run_case(pos: usize, ctx: u64, pts: u64, inref: u64) -> i64 {
    let bw4: i64 = pv(pos + 2)
    let bh4: i64 = pv(pos + 3)
    let ncell: i64 = bw4 + 2 + bh4
    av1ipm.st64(ctx, 0, bw4)
    var i: i64 = 0
    while i < ncell {
        let c: usize = pos + 13 + (i * 5) as usize
        let bs: usize = pv(c + 2) as usize
        av1ipm.st64(ctx, 1 + i * 6, pv(c))
        av1ipm.st64(ctx, 2 + i * 6, pv(c + 1))
        av1ipm.st64(ctx, 3 + i * 6, BW4[bs] as i64)
        av1ipm.st64(ctx, 4 + i * 6, BH4[bs] as i64)
        av1ipm.st64(ctx, 5 + i * 6, pv(c + 3))
        av1ipm.st64(ctx, 6 + i * 6, pv(c + 4))
        i = i + 1
    }
    let getter: fn(u64, i64, i64, i64) -> i64 = get_field
    var np: i64 = av1warpsamp.find_samples(getter, ctx, pv(pos), pv(pos + 1), bw4, bh4, pv(pos + 4), pv(pos + 5), pv(pos + 6), pv(pos + 7), pv(pos + 8), pv(pos + 9), pts, inref)
    var hsh: i64 = 2166136261
    hsh = fnv(hsh, np)
    var np2: i64 = np
    if np > 1 {
        let cb: usize = pv(pos + 12) as usize
        np2 = av1warpsamp.select_samples(pv(pos + 10), pv(pos + 11), pts, inref, np, (BW4[cb] * 4) as i64, (BH4[cb] * 4) as i64)
    }
    hsh = fnv(hsh, np2)
    i = 0
    while i < np2 * 2 {
        hsh = fnv(hsh, av1ipm.ld64(pts, i) & 4294967295)
        hsh = fnv(hsh, av1ipm.ld64(inref, i) & 4294967295)
        i = i + 1
    }
    return hsh
}

fn main() -> i32 {
    let ctx: u64 = vp9mem.alloc(4096)
    let pts: u64 = vp9mem.alloc(256)
    let inref: u64 = vp9mem.alloc(256)
    var pos: usize = 0
    var idx: usize = 0
    var bad: i32 = 0
    while idx < @CASES@ {
        let got: i64 = run_case(pos, ctx, pts, inref)
        if got != (EXPECT[idx] as i64) {
            bad = bad + 1
        }
        pos = pos + 13 + ((pv(pos + 2) + 2 + pv(pos + 3)) * 5) as usize
        idx = idx + 1
    }
    if bad > 200 {
        return 200
    }
    return bad
}
