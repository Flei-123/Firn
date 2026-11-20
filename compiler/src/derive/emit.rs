// SPDX-License-Identifier: MPL-2.0
use super::decl::{Body, Decl};
use super::{emit_enum, expr};

pub fn generate(decl: &Decl, names: &[String]) -> Result<String, String> {
    let has = |n: &str| names.iter().any(|x| x == n);
    if has("Default") && (!decl.generics.is_empty() || matches!(decl.body, Body::Enum(_))) {
        return Err("derive(Default) works on non-generic structs only".to_string());
    }
    if !decl.generics.is_empty() && matches!(decl.body, Body::Enum(_)) {
        return Err("derive on a generic enum needs 'match' inside generic templates (roadmap r1573)".to_string());
    }
    let mut methods = String::new();
    let mut frees = String::new();
    for n in names {
        match &decl.body {
            Body::Struct(fields) => match n.as_str() {
                "Default" => frees.push_str(&struct_default(decl, fields)?),
                _ => methods.push_str(&struct_method(n, decl, fields)),
            },
            Body::Enum(variants) => methods.push_str(&emit_enum::method(n, decl, variants)),
        }
    }
    let mut out = format!(" impl {} {{ {}}}", decl.self_type(), methods);
    if methods.is_empty() {
        out.clear();
    }
    out.push_str(&frees);
    Ok(out)
}

fn joined(fields: &[(String, String)], empty: &str, sep: &str, item: impl Fn(&str, &str) -> String) -> String {
    if fields.is_empty() {
        return empty.to_string();
    }
    fields.iter().map(|(f, t)| item(f, t)).collect::<Vec<_>>().join(sep)
}

fn struct_method(name: &str, decl: &Decl, fields: &[(String, String)]) -> String {
    let h = decl.self_type();
    match name {
        "Debug" => struct_debug(decl, fields),
        "Eq" => {
            let body = joined(fields, "true", " && ", |f, _| format!("(*self).{} == o.{}", f, f));
            format!("fn eq(*self, o: {}) -> bool {{ return {} }} ", h, body)
        }
        "Ord" => {
            let steps = joined(fields, "", " ", |f, t| {
                let (a, b) = (format!("(*self).{}", f), format!("o.{}", f));
                format!(
                    "if {} {{ return 0 - 1 }}; if {} {{ return 1 }};",
                    expr::less(t, &a, &b),
                    expr::greater(t, &a, &b)
                )
            });
            format!("fn cmp(*self, o: {}) -> i64 {{ {} return 0 }} ", h, steps)
        }
        "Json" => struct_json(fields),
        "Hash" => {
            let steps = joined(fields, "", " ", |f, t| {
                let v = expr::hash(t, &format!("(*self).{}", f));
                format!("h = derive.hash_mix(h, {});", v)
            });
            format!("fn hash(*self) -> u64 {{ var h: u64 = derive.hash_seed(); {} return h }} ", steps)
        }
        _ => {
            let items = joined(fields, "", ", ", |f, t| {
                format!("{}: {}", f, expr::clone(t, &format!("(*self).{}", f)))
            });
            format!("fn clone(*self) -> {h} {{ return {h} {{ {items} }} }} ")
        }
    }
}

fn struct_debug(decl: &Decl, fields: &[(String, String)]) -> String {
    let mut s = String::from("fn debug(*self) -> str { var c = derive.dbg_new(); ");
    if fields.is_empty() {
        s.push_str(&format!("c = derive.dbg_text(c, \"{}\"); ", decl.name));
    } else {
        s.push_str(&format!("c = derive.dbg_text(c, \"{} {{ \"); ", decl.name));
        for (i, (f, t)) in fields.iter().enumerate() {
            let sep = if i == 0 { "" } else { ", " };
            let v = expr::debug(t, &format!("(*self).{}", f));
            s.push_str(&format!("c = derive.dbg_field(c, \"{}\", \"{}\", {}); ", f, sep, v));
        }
        s.push_str("c = derive.dbg_text(c, \" }\"); ");
    }
    s.push_str("return derive.dbg_done(c) } ");
    s
}

fn struct_json(fields: &[(String, String)]) -> String {
    let mut s = String::from("fn to_json(*self) -> str { var c = derive.dbg_new(); c = derive.dbg_text(c, \"{\"); ");
    for (i, (f, t)) in fields.iter().enumerate() {
        let sep = if i == 0 { "" } else { "," };
        let v = expr::json(t, &format!("(*self).{}", f));
        s.push_str(&format!("c = derive.json_field(c, \"{}\", \"{}\", {}); ", f, sep, v));
    }
    s.push_str("c = derive.dbg_text(c, \"}\"); return derive.dbg_done(c) } ");
    s
}

fn struct_default(decl: &Decl, fields: &[(String, String)]) -> Result<String, String> {
    let mut items = Vec::new();
    for (f, t) in fields {
        items.push(format!("{}: {}", f, expr::default_value(t)?));
    }
    Ok(format!(
        " fn default_{n}() -> {n} {{ return {n} {{ {i} }} }}",
        n = decl.name,
        i = items.join(", ")
    ))
}
