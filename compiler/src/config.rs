// SPDX-License-Identifier: GPL-2.0-only
//! Central configuration: the language name and the file suffix live EXCLUSIVELY here.
//! Renaming the language = adjust these three constants, nothing else.

pub const LANG_NAME: &str = "Firn";
pub const LANG_NAME_LOWER: &str = "firn";
pub const FILE_EXT: &str = "fi";

/// Name of the compiler binary, derived from the language name.
pub fn compiler_name() -> String {
    format!("{}c", LANG_NAME_LOWER)
}

/// Version of the prototype (stage 0).
pub const VERSION: &str = "0.1.0 (stage 0)";
