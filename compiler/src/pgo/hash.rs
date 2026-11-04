// SPDX-License-Identifier: MPL-2.0

use crate::fir::{Func, Module};
use crate::fx::HashMap;
use std::sync::RwLock;

static FUNC_HASHES: RwLock<Option<HashMap<String, u64>>> = RwLock::new(None);

pub fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

pub fn func_hash(f: &Func) -> u64 {
    let text = format!("{}|{:?}|{:?}|{:?}", f.name, f.params, f.ret, f.blocks);
    fnv(text.as_bytes())
}

pub fn record(m: &Module) {
    if !super::active() {
        return;
    }
    let map: HashMap<String, u64> = m.funcs.iter().map(|f| (f.name.clone(), func_hash(f))).collect();
    *FUNC_HASHES.write().unwrap_or_else(|e| e.into_inner()) = Some(map);
}

pub fn of(name: &str) -> Option<u64> {
    FUNC_HASHES.read().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(name).copied())
}

pub fn file_hash(path: &str) -> u64 {
    match std::fs::read(path) {
        Ok(b) => fnv(&b),
        Err(_) => 0,
    }
}
