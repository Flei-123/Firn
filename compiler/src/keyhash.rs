// SPDX-License-Identifier: MPL-2.0
const K1: u64 = 0xA076_1D64_78BD_642F;
const K2: u64 = 0xE703_7ED1_A0B4_28DB;
const K3: u64 = 0x8EBC_6AF0_9C88_C6E3;
const K4: u64 = 0x5899_65CC_7537_4CC3;

pub struct KeyHasher {
    a: u64,
    b: u64,
    total: u64,
}

fn mix(x: u64, y: u64) -> u64 {
    let r = (x as u128).wrapping_mul(y as u128);
    (r as u64) ^ ((r >> 64) as u64)
}

fn word(bytes: &[u8]) -> u64 {
    let mut w = [0u8; 8];
    w.copy_from_slice(bytes);
    u64::from_le_bytes(w)
}

impl KeyHasher {
    pub fn new() -> KeyHasher {
        KeyHasher { a: K1 ^ K3, b: K2 ^ K4, total: 0 }
    }

    pub fn push(&mut self, data: &[u8]) {
        let mut chunks = data.chunks_exact(16);
        for c in chunks.by_ref() {
            let w0 = word(&c[..8]);
            let w1 = word(&c[8..]);
            self.a = mix(w0 ^ K1 ^ self.a, w1 ^ K2);
            self.b = mix(w1 ^ K3 ^ self.b, w0 ^ K4);
        }
        let rest = chunks.remainder();
        if !rest.is_empty() {
            let mut pad = [0u8; 16];
            pad[..rest.len()].copy_from_slice(rest);
            let w0 = word(&pad[..8]);
            let w1 = word(&pad[8..]);
            let n = rest.len() as u64;
            self.a = mix(w0 ^ K1 ^ self.a, w1 ^ K2 ^ n);
            self.b = mix(w1 ^ K3 ^ self.b, w0 ^ K4 ^ n);
        }
        self.total = self.total.wrapping_add(data.len() as u64);
    }

    pub fn hex(&self) -> String {
        let x = mix(self.a ^ self.total, K3);
        let y = mix(self.b ^ self.total.rotate_left(32), K1);
        let a = mix(x ^ K2, y ^ K4);
        let b = mix(y ^ K3, x ^ K1);
        format!("{:016x}{:016x}", a, b)
    }
}

pub fn digest_hex(data: &[u8]) -> String {
    let mut h = KeyHasher::new();
    h.push(data);
    h.hex()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_is_deterministic_and_input_sensitive() {
        assert_eq!(digest_hex(b"abc"), digest_hex(b"abc"));
        assert_ne!(digest_hex(b"abc"), digest_hex(b"abd"));
        assert_ne!(digest_hex(b""), digest_hex(b"\0"));
        assert_ne!(digest_hex(b"a"), digest_hex(b"a\0"));
        assert_ne!(digest_hex(&[0u8; 16]), digest_hex(&[0u8; 32]));
        assert_eq!(digest_hex(b"abc").len(), 32);
    }

    #[test]
    fn single_bit_flips_change_the_digest() {
        let base = vec![7u8; 1000];
        let d = digest_hex(&base);
        for i in [0usize, 1, 15, 16, 500, 999] {
            let mut v = base.clone();
            v[i] ^= 1;
            assert_ne!(digest_hex(&v), d);
        }
    }
}
