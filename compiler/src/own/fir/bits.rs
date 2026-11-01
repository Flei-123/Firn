#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub(crate) struct Bits {
    lo: u64,
    hi: Vec<u64>,
}

impl Bits {
    pub fn new(n: usize) -> Bits {
        let words = (n + 63) / 64;
        Bits { lo: 0, hi: vec![0; words.saturating_sub(1)] }
    }
    pub fn single(n: usize, i: usize) -> Bits {
        let mut b = Bits::new(n);
        b.set(i);
        b
    }
    fn word(&self, k: usize) -> Option<u64> {
        if k == 0 {
            Some(self.lo)
        } else {
            self.hi.get(k - 1).copied()
        }
    }
    fn word_mut(&mut self, k: usize) -> &mut u64 {
        if k == 0 {
            &mut self.lo
        } else {
            &mut self.hi[k - 1]
        }
    }
    pub fn set(&mut self, i: usize) {
        *self.word_mut(i / 64) |= 1 << (i % 64);
    }
    pub fn clear(&mut self, i: usize) {
        *self.word_mut(i / 64) &= !(1 << (i % 64));
    }
    pub fn get(&self, i: usize) -> bool {
        self.word(i / 64).map_or(false, |x| x & (1 << (i % 64)) != 0)
    }
    pub fn reset(&mut self) {
        self.lo = 0;
        self.hi.iter_mut().for_each(|x| *x = 0);
    }
    pub fn intersects(&self, o: &Bits) -> bool {
        self.lo & o.lo != 0 || self.hi.iter().zip(o.hi.iter()).any(|(a, b)| a & b != 0)
    }
    pub fn assign(&mut self, o: &Bits) {
        self.lo = o.lo;
        self.hi.clear();
        self.hi.extend_from_slice(&o.hi);
    }
    pub fn union(&mut self, o: &Bits) -> bool {
        let n = self.lo | o.lo;
        let mut ch = n != self.lo;
        self.lo = n;
        for (a, b) in self.hi.iter_mut().zip(o.hi.iter()) {
            let n = *a | *b;
            ch |= n != *a;
            *a = n;
        }
        ch
    }
    pub fn count(&self) -> u32 {
        self.lo.count_ones() + self.hi.iter().map(|x| x.count_ones()).sum::<u32>()
    }
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        std::iter::once(self.lo).chain(self.hi.iter().copied()).enumerate().flat_map(|(k, x)| (0..64).filter(move |b| x & (1u64 << b) != 0).map(move |b| k * 64 + b))
    }
}
