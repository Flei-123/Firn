//! Central configuration: the language label and the file suffix live EXCLUSIVELY here.
//! Renaming the language = adjust these three constants, nothing else.

pub const LANG_NAME: &str = "Firn";
pub const LANG_NAME_LOWER: &str = "firn";
pub const FILE_EXT: &str = "fi";

/// Label of the compiler binary, derived from the language label.
pub fn compiler_name() -> String {
    format!("{}c", LANG_NAME_LOWER)
}

/// Version of the prototype (stage 0).
pub const VERSION: &str = "0.1.0 (stage 0)";
