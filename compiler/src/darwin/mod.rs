// SPDX-License-Identifier: MPL-2.0
pub mod entry;
mod libsystem;
mod stub_a64;
mod stub_x86;
mod syscalls;
mod text;
mod x86_abs;
pub mod tools;

pub fn translate(asm: &str) -> String {
    let mut out = text::translate(asm);
    out.push_str(&stub_asm());
    out
}

fn stub_asm() -> String {
    if crate::target::active() == crate::target::Target::Aarch64 {
        stub_a64::stub_asm()
    } else {
        stub_x86::stub_asm()
    }
}
