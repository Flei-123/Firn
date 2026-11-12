// SPDX-License-Identifier: GPL-2.0-only
use crate::codegen_x86::Emitter;

const MAX_PAIR_OFFSET: u64 = 504;

pub(crate) fn emit_saved_registers(e: &mut Emitter, saved: &[&'static str], base: u64, store: bool) {
    let (single, pair) = if store { ("str", "stp") } else { ("ldr", "ldp") };
    let mut k = 0;
    while k < saved.len() {
        let off = base + 8 * k as u64;
        if k + 1 < saved.len() && off <= MAX_PAIR_OFFSET {
            e.line(&format!("{} {}, {}, [sp, #{}]", pair, saved[k], saved[k + 1], off));
            k += 2;
        } else {
            let m = crate::codegen_a64::at_base(e, "sp", off, 8);
            e.line(&format!("{} {}, {}", single, saved[k], m));
            k += 1;
        }
    }
}
