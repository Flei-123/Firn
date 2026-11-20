// SPDX-License-Identifier: MPL-2.0
pub const STD_MODULES: &[&str] = &[
    "args", "base64", "bisect", "bytes", "csv", "deflate", "deque", "dict", "dir", "env", "extract",
    "fs", "glob", "hash", "io", "itertools", "json", "log", "map", "math", "md5", "net", "num",
    "option", "process", "rc", "rng", "rt", "setops", "shell", "stats", "str", "tar", "testkit",
    "text", "time", "vec", "xml",
];

const NUMERIC: &[&str] = &[
    "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "usize", "isize", "f32", "f64",
];

pub fn import_hint(name: &str) -> Option<String> {
    if STD_MODULES.contains(&name) {
        Some(format!("add 'import std.{}' at the top of the file", name))
    } else {
        None
    }
}

pub fn mismatch_hint(expected: &str, found: &str) -> Option<String> {
    let want_number = NUMERIC.contains(&expected);
    let have_number = NUMERIC.contains(&found);
    if want_number && found == "str" {
        return Some("a str holds text; parse it with std.num before using it as a number".to_string());
    }
    if expected == "str" && (have_number || found == "bool") {
        return Some("format the value as text: io.fmt_string(f\"{value}\")".to_string());
    }
    if want_number && have_number {
        return Some(format!("convert it explicitly: write '<value> as {}'", expected));
    }
    None
}
