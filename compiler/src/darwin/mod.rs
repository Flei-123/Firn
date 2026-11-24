// SPDX-License-Identifier: MPL-2.0
pub mod entry;
mod libsystem;
mod syscalls;
mod text;
pub mod tools;

pub fn translate(asm: &str) -> String {
    let mut out = text::translate(asm);
    out.push_str(&syscalls::stub_asm());
    out
}
