// SPDX-License-Identifier: MPL-2.0
use crate::prelude::{is_disabled, skip_block, skip_line, skip_quoted};
use crate::std_modules::STD_MODULES;

const KERNEL_MARK: &str = "profile kernel";
const MODULE_COUNT: usize = STD_MODULES.len();
const TYPE_MODULES: [(&str, &str); 3] = [("Vec", "vec"), ("Map", "map"), ("Set", "map")];

#[derive(Clone, Copy, Default)]
struct Usage {
    dotted: u32,
    other: u32,
    declared: u32,
    imported: bool,
    types_used: u8,
    types_declared: u8,
}

fn is_ident_byte(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

fn module_index(name: &str) -> Option<usize> {
    STD_MODULES.binary_search(&name).ok()
}

fn skip_spaces(b: &[u8], mut i: usize, end: usize) -> usize {
    while i < end && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    i
}

fn is_dotted_use(b: &[u8], after: usize, end: usize) -> bool {
    let i = skip_spaces(b, after, end);
    i + 1 < end && b[i] == b'.' && b[i + 1] != b'.'
}

fn read_import(b: &[u8], mut i: usize, end: usize, usage: &mut [Usage]) -> usize {
    let mut last = (0, 0);
    loop {
        i = skip_spaces(b, i, end);
        let start = i;
        while i < end && is_ident_byte(b[i]) {
            i += 1;
        }
        if start == i {
            break;
        }
        last = (start, i);
        if i >= end || b[i] != b'.' {
            break;
        }
        i += 1;
    }
    if let Some(m) = std::str::from_utf8(&b[last.0..last.1]).ok().and_then(module_index) {
        usage[m].imported = true;
    }
    i
}

fn expression_end(b: &[u8], open: usize, end: usize) -> usize {
    let mut depth = 1;
    let mut k = open + 1;
    while k < end && depth > 0 {
        match b[k] {
            b'"' => {
                k = skip_quoted(b, k, b'"');
                continue;
            }
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        k += 1;
    }
    k
}

fn spec_start(b: &[u8], from: usize, to: usize) -> usize {
    let mut i = from;
    while i < to {
        if b[i] == b':' {
            if i + 1 < to && b[i + 1] == b':' {
                i += 2;
                continue;
            }
            return i;
        }
        i += 1;
    }
    to
}

fn scan_interpolation(b: &[u8], quote: usize, end: usize, usage: &mut [Usage]) -> usize {
    let mut j = quote + 1;
    while j < end && b[j] != b'"' && b[j] != b'\n' {
        if b[j] == b'\\' {
            j += 2;
        } else if b[j] == b'{' && b.get(j + 1) == Some(&b'{') {
            j += 2;
        } else if b[j] == b'{' {
            let close = expression_end(b, j, end);
            let stop = close.saturating_sub(1).max(j + 1);
            scan_range(b, j + 1, spec_start(b, j + 1, stop), usage);
            j = close;
        } else {
            j += 1;
        }
    }
    (j + 1).min(end)
}

fn is_fstring_quote(b: &[u8], i: usize) -> bool {
    i >= 1 && b[i - 1] == b'f' && (i < 2 || !is_ident_byte(b[i - 2]))
}

fn note_type(b: &[u8], word: &str, after: usize, end: usize, declaration: bool, usage: &mut [Usage]) {
    let Some(bit) = TYPE_MODULES.iter().position(|(name, _)| *name == word) else {
        return;
    };
    let Some(m) = module_index(TYPE_MODULES[bit].1) else {
        return;
    };
    if declaration {
        usage[m].types_declared |= 1 << bit;
    } else if b.get(skip_spaces(b, after, end)) == Some(&b'[') {
        usage[m].types_used |= 1 << bit;
    }
}

fn scan_range(b: &[u8], from: usize, end: usize, usage: &mut [Usage]) {
    let mut i = from;
    let mut prev_dot = false;
    let mut prev_fn = false;
    let mut prev_decl = false;
    while i < end {
        let c = b[i];
        if c == b'/' && i + 1 < end && b[i + 1] == b'/' {
            i = skip_line(b, i);
        } else if c == b'/' && i + 1 < end && b[i + 1] == b'*' {
            i = skip_block(b, i + 2);
        } else if c == b'"' && is_fstring_quote(b, i) {
            i = scan_interpolation(b, i, end, usage);
            prev_dot = false;
            prev_fn = false;
        } else if c == b'"' || c == b'\'' {
            i = skip_quoted(b, i, c);
            prev_dot = false;
            prev_fn = false;
        } else if c == b'.' {
            prev_dot = true;
            i += 1;
        } else if is_ident_byte(c) && !c.is_ascii_digit() {
            let start = i;
            while i < end && is_ident_byte(b[i]) {
                i += 1;
            }
            let word = std::str::from_utf8(&b[start..i]).unwrap_or("");
            if word == "import" {
                i = read_import(b, i, end, usage);
                prev_fn = false;
                prev_decl = false;
            } else {
                if !prev_dot {
                    note_type(b, word, i, end, prev_decl, usage);
                }
                prev_decl = matches!(word, "struct" | "enum" | "interface" | "type");
                if let (false, Some(m)) = (prev_dot, module_index(word)) {
                    if prev_fn {
                        usage[m].declared += 1;
                    } else if is_dotted_use(b, i, end) {
                        usage[m].dotted += 1;
                    } else {
                        usage[m].other += 1;
                    }
                }
                prev_fn = word == "fn";
            }
            prev_dot = false;
        } else {
            if !c.is_ascii_whitespace() {
                prev_dot = false;
            }
            i += 1;
        }
    }
}

pub fn expand(src: &str) -> Option<String> {
    if is_disabled() || src.contains(KERNEL_MARK) {
        return None;
    }
    let mut usage = [Usage::default(); MODULE_COUNT];
    scan_range(src.as_bytes(), 0, src.len(), &mut usage);
    let mut out = String::new();
    for (name, u) in STD_MODULES.iter().zip(usage.iter()) {
        let dotted_use = u.dotted > 0 && u.other == 0 && u.declared == 0;
        let type_use = u.types_used & !u.types_declared != 0;
        if (dotted_use || type_use) && !u.imported {
            out.push_str("import std.");
            out.push_str(name);
            out.push_str("; ");
        }
    }
    if out.is_empty() {
        return None;
    }
    out.push_str(src);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn added(src: &str) -> String {
        expand(src).map(|t| t.replace(src, "")).unwrap_or_default()
    }

    #[test]
    fn module_list_matches_lib_std() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../lib/std");
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.ends_with(".fi"))
            .map(|n| n.split('.').next().unwrap().to_string())
            .collect();
        names.sort();
        names.dedup();
        assert_eq!(names, STD_MODULES.to_vec());
    }

    #[test]
    fn dotted_only_names_are_imported() {
        assert_eq!(added("fn main() -> i32 { io.print_line(\"x\")\n return 0 }"), "import std.io; ");
        assert_eq!(added("fn main() -> i32 { let t = time.millis()\n return 0 }"), "import std.time; ");
    }

    #[test]
    fn own_names_win() {
        assert_eq!(added("fn main() -> i32 { let io = 1\n return io.x }"), "");
        assert_eq!(added("fn f(time: i64) -> i64 { return time.x }"), "");
        assert_eq!(added("fn math() {}\nfn main() -> i32 { math.f()\n return 0 }"), "");
        assert_eq!(added("import std.io\nfn main() -> i32 { io.print_line(\"x\")\n return 0 }"), "");
        assert_eq!(added("import app.json\nfn main() -> i32 { json.f()\n return 0 }"), "");
    }

    #[test]
    fn strings_and_comments_are_ignored_but_fstrings_count() {
        assert_eq!(added("fn main() -> i32 { // io.x\n let s = \"io.x\"\n return 0 }"), "");
        assert_eq!(added("fn main() -> i32 { let s = f\"{time.millis()}\"\n return 0 }"), "import std.time; ");
        assert_eq!(added("fn main() -> i32 { let s = f\"{{time.x}} {n:>5}\"\n return 0 }"), "");
    }

    #[test]
    fn collection_types_import_their_module() {
        assert_eq!(added("fn main() -> i32 { var v = Vec[int].new()\n return 0 }"), "import std.vec; ");
        assert_eq!(added("fn main() -> i32 { var m: Map[u32, int] = Map[u32, int].new()\n return 0 }"), "import std.map; ");
        assert_eq!(added("fn main() -> i32 { var s = Set [u32].new()\n return 0 }"), "import std.map; ");
        assert_eq!(added("import std.vec\nfn main() -> i32 { var v = Vec[int].new()\n return 0 }"), "");
        assert_eq!(added("struct Set[K] { n: K }\nfn main() -> i32 { var s = Set[u32] { n: 1 }\n return 0 }"), "");
        assert_eq!(added("fn main() -> i32 { let n = Vec\n return 0 }"), "");
    }

    #[test]
    fn fields_and_kernel_are_untouched() {
        assert_eq!(added("fn main() -> i32 { return a.io.x }"), "");
        assert_eq!(added("profile kernel\nfn main() -> i32 { io.x()\n return 0 }"), "");
    }
}
