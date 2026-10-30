use std::hint::black_box;

fn lcg(s: u64) -> u64 {
    s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)
}

fn make_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i: u32 = 0;
    while i < 256 {
        let mut c = i;
        let mut k = 0;
        while k < 8 {
            if c & 1 != 0 {
                c = 0xEDB88320u32 ^ (c >> 1);
            } else {
                c >>= 1;
            }
            k += 1;
        }
        table[i as usize] = c;
        i += 1;
    }
    table
}

fn main() {
    let table = make_table();
    let n: usize = black_box(67108864);
    let mut buf = vec![0u8; n];
    let mut s: u64 = 1;
    for b in buf.iter_mut() {
        s = lcg(s);
        *b = (s >> 56) as u8;
    }
    let mut crc: u32 = 0xFFFFFFFF;
    for _ in 0..3 {
        for i in 0..n {
            let idx = (crc ^ buf[i] as u32) & 255;
            crc = table[idx as usize] ^ (crc >> 8);
        }
    }
    println!("{}", black_box(crc ^ 0xFFFFFFFF));
}
