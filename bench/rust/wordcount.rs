use std::hint::black_box;

fn lcg(s: u64) -> u64 {
    s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)
}

fn add_word(keys: &mut [u64], counts: &mut [u64], h: u64) {
    let mut slot = ((h >> 20) & 16383) as usize;
    loop {
        let k = keys[slot];
        if k == h {
            counts[slot] += 1;
            return;
        }
        if k == 0 {
            keys[slot] = h;
            counts[slot] = 1;
            return;
        }
        slot = (slot + 1) & 16383;
    }
}

fn main() {
    let nwords: usize = black_box(3000000);
    let mut keys = vec![0u64; 16384];
    let mut counts = vec![0u64; 16384];
    let mut text = vec![0u8; nwords * 7 + 8];
    let mut s: u64 = 99;
    let mut at = 0;
    for _ in 0..nwords {
        s = lcg(s);
        let len = 1 + ((s >> 59) % 6);
        for _ in 0..len {
            s = lcg(s);
            text[at] = (97 + (s >> 62)) as u8;
            at += 1;
        }
        text[at] = 32;
        at += 1;
    }
    for _ in 0..3 {
        let mut h: u64 = 14695981039346656037;
        for i in 0..at {
            let b = text[i];
            if b == 32 {
                add_word(&mut keys, &mut counts, h);
                h = 14695981039346656037;
            } else {
                h = (h ^ b as u64).wrapping_mul(1099511628211);
            }
        }
    }
    let mut acc: u64 = 0;
    let mut distinct: u64 = 0;
    for j in 0..16384 {
        if keys[j] != 0 {
            distinct += 1;
        }
        acc = acc.wrapping_mul(31).wrapping_add(keys[j] ^ counts[j].wrapping_mul(1000003));
    }
    println!("{}", black_box(acc.wrapping_mul(1000003).wrapping_add(distinct)));
}
