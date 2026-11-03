use std::sync::{Condvar, Mutex};

pub(crate) struct Budget {
    used: Mutex<usize>,
    freed: Condvar,
    cap: usize,
}

pub(crate) struct Lease<'a> {
    budget: &'a Budget,
    weight: usize,
}

impl Budget {
    pub fn new(cap: usize) -> Budget {
        Budget { used: Mutex::new(0), freed: Condvar::new(), cap }
    }

    pub fn acquire(&self, weight: usize) -> Lease<'_> {
        let weight = weight.min(self.cap);
        let mut used = self.used.lock().unwrap();
        while *used != 0 && *used + weight > self.cap {
            used = self.freed.wait(used).unwrap();
        }
        *used += weight;
        Lease { budget: self, weight }
    }
}

impl Drop for Lease<'_> {
    fn drop(&mut self) {
        *self.budget.used.lock().unwrap() -= self.weight;
        self.budget.freed.notify_all();
    }
}
