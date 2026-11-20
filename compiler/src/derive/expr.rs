// SPDX-License-Identifier: MPL-2.0
const STR: &str = "str";

pub fn debug(ty: &str, value: &str) -> String {
    if ty == STR {
        return format!("derive.str_debug({})", value);
    }
    format!("{}.debug()", value)
}

pub fn clone(ty: &str, value: &str) -> String {
    if ty == STR {
        return value.to_string();
    }
    format!("{}.clone()", value)
}

pub fn hash(ty: &str, value: &str) -> String {
    if ty == STR {
        return format!("derive.str_hash({})", value);
    }
    format!("{}.hash()", value)
}

pub fn default_value(ty: &str) -> Result<String, String> {
    let text = match ty {
        "i8" | "i16" | "i32" | "i64" | "isize" | "u8" | "u16" | "u32" | "u64" | "usize" | "f32" => {
            format!("0 as {}", ty)
        }
        "f64" => "0.0".to_string(),
        "bool" => "false".to_string(),
        STR => "\"\"".to_string(),
        _ => return default_named(ty),
    };
    Ok(text)
}

fn default_named(ty: &str) -> Result<String, String> {
    if let Some(inner) = ty.strip_prefix("Vec[").and_then(|r| r.strip_suffix(']')) {
        return Ok(format!("vec_new[{}]()", inner));
    }
    if ty.contains('[') || ty.contains('(') {
        return Err(format!("derive(Default): the type '{}' has no default", ty));
    }
    Ok(format!("default_{}()", ty))
}

pub fn less(ty: &str, a: &str, b: &str) -> String {
    match ty {
        STR => format!("derive.str_cmp({}, {}) < 0", a, b),
        "bool" => format!("derive.bool_cmp({}, {}) < 0", a, b),
        _ => format!("{} < {}", a, b),
    }
}

pub fn greater(ty: &str, a: &str, b: &str) -> String {
    match ty {
        STR => format!("derive.str_cmp({}, {}) > 0", a, b),
        "bool" => format!("derive.bool_cmp({}, {}) > 0", a, b),
        _ => format!("{} > {}", a, b),
    }
}
