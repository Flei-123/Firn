// SPDX-License-Identifier: MPL-2.0
use std::collections::HashSet;
use std::sync::OnceLock;

const NAMES_TEXT: &str = include_str!("../../lib/gc/runtime_names.txt");

fn table() -> &'static HashSet<&'static str> {
    static TABLE: OnceLock<HashSet<&'static str>> = OnceLock::new();
    TABLE.get_or_init(|| NAMES_TEXT.lines().map(str::trim).filter(|l| !l.is_empty()).collect())
}

pub(crate) fn is_runtime_name(name: &str) -> bool {
    table().contains(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_name_is_a_runtime_function() {
        let runtime = crate::gc::runtime_source(true, true, true, true);
        assert!(!table().is_empty());
        for n in table() {
            assert!(runtime.contains(&format!("fn {}(", n)), "runtime without 'fn {}'", n);
        }
    }

    #[test]
    fn names_are_unique_and_plain() {
        let lines: Vec<&str> = NAMES_TEXT.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), table().len());
        for l in lines {
            assert!(l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'), "bad name '{}'", l);
        }
    }

    #[test]
    fn lookup_is_exact() {
        assert!(is_runtime_name("gc_collect"));
        assert!(!is_runtime_name("gc_collectx"));
        assert!(!is_runtime_name("dom__gc_collect"));
    }
}
