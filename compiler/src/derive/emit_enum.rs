// SPDX-License-Identifier: MPL-2.0
use super::decl::{Decl, Variant};
use super::expr;

fn pattern(decl: &Decl, v: &Variant, prefix: &str) -> String {
    if v.types.is_empty() {
        return format!("{}::{}", decl.name, v.name);
    }
    format!("{}::{}({})", decl.name, v.name, binds(v, prefix).join(", "))
}

fn binds(v: &Variant, prefix: &str) -> Vec<String> {
    (0..v.types.len()).map(|i| format!("{}{}", prefix, i)).collect()
}

fn arms(variants: &[Variant], arm: impl Fn(usize, &Variant) -> String) -> String {
    variants.iter().enumerate().map(|(i, v)| arm(i, v)).collect::<Vec<_>>().join(" ")
}

pub fn method(name: &str, decl: &Decl, variants: &[Variant]) -> String {
    let h = decl.self_type();
    match name {
        "Debug" => format!("fn debug(*self) -> str {{ match *self {{ {} }} }} ", arms(variants, |_, v| debug_arm(decl, v))),
        "Eq" => format!(
            "fn eq(*self, o: {}) -> bool {{ match *self {{ {} }} }} ",
            h,
            arms(variants, |_, v| eq_arm(decl, v))
        ),
        "Hash" => format!("fn hash(*self) -> u64 {{ match *self {{ {} }} }} ", arms(variants, |i, v| hash_arm(decl, i, v))),
        "Ord" => ord_methods(decl, variants),
        "Json" => format!("fn to_json(*self) -> str {{ match *self {{ {} }} }} ", arms(variants, |_, v| json_arm(decl, v))),
        _ => format!("fn clone(*self) -> {} {{ match *self {{ {} }} }} ", h, arms(variants, |_, v| clone_arm(decl, v))),
    }
}

fn json_arm(decl: &Decl, v: &Variant) -> String {
    let head = pattern(decl, v, "a");
    if v.types.is_empty() {
        return format!("{} => {{ return derive.json_str(\"{}\") }}", head, v.name);
    }
    let mut s = format!("{} => {{ var c = derive.dbg_new(); c = derive.dbg_text(c, \"{{\\\"{}\\\":[\"); ", head, v.name);
    for (i, (b, t)) in binds(v, "a").iter().zip(&v.types).enumerate() {
        if i > 0 {
            s.push_str("c = derive.dbg_text(c, \",\"); ");
        }
        s.push_str(&format!("c = derive.dbg_text(c, {}); ", expr::json(t, b)));
    }
    s.push_str("c = derive.dbg_text(c, \"]}\"); return derive.dbg_done(c) }");
    s
}

fn debug_arm(decl: &Decl, v: &Variant) -> String {
    let mut s = format!("{} => {{ var c = derive.dbg_new(); c = derive.dbg_text(c, \"{}\"); ", pattern(decl, v, "a"), v.name);
    if !v.types.is_empty() {
        s.push_str("c = derive.dbg_text(c, \"(\"); ");
        for (i, (b, t)) in binds(v, "a").iter().zip(&v.types).enumerate() {
            if i > 0 {
                s.push_str("c = derive.dbg_text(c, \", \"); ");
            }
            s.push_str(&format!("c = derive.dbg_text(c, {}); ", expr::debug(t, b)));
        }
        s.push_str("c = derive.dbg_text(c, \")\"); ");
    }
    s.push_str("return derive.dbg_done(c) }");
    s
}

fn eq_arm(decl: &Decl, v: &Variant) -> String {
    let same: Vec<String> = binds(v, "a").iter().zip(binds(v, "b")).map(|(a, b)| format!("{} == {}", a, b)).collect();
    let test = if same.is_empty() { "true".to_string() } else { same.join(" && ") };
    format!(
        "{} => {{ match o {{ {} => {{ return {} }} _ => {{ return false }} }} }}",
        pattern(decl, v, "a"),
        pattern(decl, v, "b"),
        test
    )
}

fn clone_arm(decl: &Decl, v: &Variant) -> String {
    let items: Vec<String> = binds(v, "a").iter().zip(&v.types).map(|(b, t)| expr::clone(t, b)).collect();
    let made = if items.is_empty() {
        format!("{}::{}", decl.name, v.name)
    } else {
        format!("{}::{}({})", decl.name, v.name, items.join(", "))
    };
    format!("{} => {{ return {} }}", pattern(decl, v, "a"), made)
}

fn hash_arm(decl: &Decl, index: usize, v: &Variant) -> String {
    let mut s = format!(
        "{} => {{ var h: u64 = derive.hash_mix(derive.hash_seed(), {} as u64); ",
        pattern(decl, v, "a"),
        index
    );
    for (b, t) in binds(v, "a").iter().zip(&v.types) {
        s.push_str(&format!("h = derive.hash_mix(h, {}); ", expr::hash(t, b)));
    }
    s.push_str("return h }");
    s
}

fn ord_methods(decl: &Decl, variants: &[Variant]) -> String {
    let h = decl.self_type();
    let tags = arms(variants, |i, v| format!("{} => {{ return {} }}", pattern(decl, v, "a"), i));
    let inner = arms(variants, |_, v| ord_arm(decl, v));
    format!(
        "fn derive_tag(*self) -> i64 {{ match *self {{ {tags} }} }} \
         fn cmp(*self, o: {h}) -> i64 {{ let ta = (*self).derive_tag(); let tb = o.derive_tag(); \
         if ta < tb {{ return 0 - 1 }}; if ta > tb {{ return 1 }}; \
         match *self {{ {inner} }} }} "
    )
}

fn ord_arm(decl: &Decl, v: &Variant) -> String {
    if v.types.is_empty() {
        return format!("{} => {{ return 0 }}", pattern(decl, v, "a"));
    }
    let mut steps = String::new();
    for ((a, b), t) in binds(v, "a").iter().zip(binds(v, "b")).zip(&v.types) {
        steps.push_str(&format!(
            "if {} {{ return 0 - 1 }}; if {} {{ return 1 }}; ",
            expr::less(t, a, &b),
            expr::greater(t, a, &b)
        ));
    }
    format!(
        "{} => {{ match o {{ {} => {{ {} return 0 }} _ => {{ return 0 }} }} }}",
        pattern(decl, v, "a"),
        pattern(decl, v, "b"),
        steps
    )
}
