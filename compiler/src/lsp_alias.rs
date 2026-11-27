// SPDX-License-Identifier: MPL-2.0
use crate::csalias_table::ALIASES;

const FLAG_BARE: u8 = 1;

pub(crate) struct AliasItem {
    pub label: &'static str,
    pub detail: String,
}

fn describe(entry: &(&'static str, &'static str, &'static str, u8)) -> String {
    format!("C#-style alias of {}{}", entry.2, entry.1)
}

pub(crate) fn global_items() -> Vec<AliasItem> {
    ALIASES
        .iter()
        .filter(|e| e.3 & FLAG_BARE != 0)
        .map(|e| AliasItem { label: e.0, detail: describe(e) })
        .collect()
}

pub(crate) fn member_items<'a>(members: impl Iterator<Item = &'a str>) -> Vec<AliasItem> {
    let names: Vec<&str> = members.collect();
    ALIASES
        .iter()
        .filter(|e| names.contains(&e.1))
        .map(|e| AliasItem { label: e.0, detail: describe(e) })
        .collect()
}

pub(crate) fn hover_text(name: &str) -> Option<String> {
    ALIASES.iter().find(|e| e.0 == name).map(describe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_names_the_target() {
        assert_eq!(hover_text("ToString").as_deref(), Some("C#-style alias of to_str"));
        assert_eq!(hover_text("ReadLine").as_deref(), Some("C#-style alias of io.read_line"));
        assert!(hover_text("to_str").is_none());
    }

    #[test]
    fn members_offer_their_pascal_form() {
        let items = member_items(["to_upper", "trim"].into_iter());
        let labels: Vec<&str> = items.iter().map(|i| i.label).collect();
        assert!(labels.contains(&"ToUpper") && labels.contains(&"Trim"));
        assert!(!labels.contains(&"ToLower"));
    }

    #[test]
    fn globals_are_the_bare_entries() {
        let labels: Vec<&str> = global_items().iter().map(|i| i.label).collect();
        assert!(labels.contains(&"Println") && labels.contains(&"ReadLn"));
        assert!(!labels.contains(&"ToString"));
    }
}
