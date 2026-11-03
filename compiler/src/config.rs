// SPDX-License-Identifier: MPL-2.0
//! Central configuration; names come from brand.toml via brand.rs.

pub const LANG_NAME: &str = crate::brand::LANG_NAME;
pub const FILE_EXT: &str = crate::brand::EXT;

/// Name of the compiler binary, derived from the language name.
pub fn compiler_name() -> String {
    crate::brand::COMPILER.to_string()
}

/// Version of the prototype (stage 0).
pub const VERSION: &str = "0.1.0 (stage 0)";
