// SPDX-License-Identifier: MPL-2.0

const PIN_FILE: &str = ".firn/pin";

pub fn read_pin(root: &str) -> Option<String> {
    let text = std::fs::read_to_string(format!("{}/{}", root, PIN_FILE)).ok()?;
    let first = text.lines().next()?.trim();
    if first.is_empty() {
        None
    } else {
        Some(first.to_string())
    }
}

pub fn same_pin(root_a: &str, root_b: &str) -> bool {
    match (read_pin(root_a), read_pin(root_b)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_package(tag: &str, pin: Option<&str>) -> String {
        let dir = std::env::temp_dir().join(format!("firn-pin-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".firn")).unwrap();
        if let Some(text) = pin {
            std::fs::write(dir.join(PIN_FILE), text).unwrap();
        }
        dir.display().to_string()
    }

    #[test]
    fn equal_pins_match_and_missing_or_different_do_not() {
        let a = temp_package("a", Some("abc\n"));
        let b = temp_package("b", Some("abc\n"));
        let c = temp_package("c", Some("abd\n"));
        let d = temp_package("d", None);
        let e = temp_package("e", Some("\n"));
        assert!(same_pin(&a, &b));
        assert!(!same_pin(&a, &c));
        assert!(!same_pin(&a, &d));
        assert!(!same_pin(&d, &d));
        assert!(!same_pin(&e, &e));
        for p in [a, b, c, d, e] {
            let _ = std::fs::remove_dir_all(p);
        }
    }
}
