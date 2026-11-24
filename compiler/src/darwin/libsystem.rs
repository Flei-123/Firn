// SPDX-License-Identifier: MPL-2.0
use super::syscalls::imported_symbols;

const INSTALL_NAME: &str = "/usr/lib/libSystem.B.dylib";
const CURRENT_VERSION: u32 = 1311;

pub fn tbd_text() -> String {
    let symbols: Vec<&str> = imported_symbols();
    format!(
        "--- !tapi-tbd\ntbd-version: 4\ntargets: [ arm64-macos, x86_64-macos ]\ninstall-name: '{}'\ncurrent-version: {}\nexports:\n  - targets: [ arm64-macos, x86_64-macos ]\n    symbols: [ {} ]\n...\n",
        INSTALL_NAME,
        CURRENT_VERSION,
        symbols.join(", ")
    )
}
