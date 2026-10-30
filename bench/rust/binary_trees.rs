use std::hint::black_box;

struct Tree {
    left: Option<Box<Tree>>,
    right: Option<Box<Tree>>,
}

fn build(d: u32) -> Box<Tree> {
    if d == 0 {
        return Box::new(Tree { left: None, right: None });
    }
    let l = build(d - 1);
    let r = build(d - 1);
    Box::new(Tree { left: Some(l), right: Some(r) })
}

fn check(t: &Tree) -> u64 {
    match (&t.left, &t.right) {
        (Some(l), Some(r)) => 1 + check(l) + check(r),
        _ => 1,
    }
}

fn main() {
    let min_depth: u32 = black_box(4);
    let max_depth: u32 = black_box(16);
    let mut total: u64 = 0;
    let stretch = build(max_depth + 1);
    total = total.wrapping_add(check(&stretch));
    drop(stretch);
    let long_lived = build(max_depth);
    let mut d = min_depth;
    while d <= max_depth {
        let iters: u64 = 1u64 << (max_depth - d + min_depth);
        let mut chk: u64 = 0;
        let mut i: u64 = 0;
        while i < iters {
            let t = build(d);
            chk = chk.wrapping_add(check(&t));
            drop(t);
            i += 1;
        }
        total = total.wrapping_mul(1000003).wrapping_add(chk);
        d += 4;
    }
    total = total.wrapping_mul(1000003).wrapping_add(check(&long_lived));
    drop(long_lived);
    println!("{}", black_box(total));
}
