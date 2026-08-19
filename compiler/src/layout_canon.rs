//! Kanonische Ausgabe von **Speicherlayout und Aufrufkonvention** — der
//! Maßstab für `lib/firnc1/types.fi`.
//!
//! ## Warum eine eigene Ausgabe
//!
//! Layout und ABI sind die Stellen, an denen ein Compiler still falsch wird:
//! ein Feldversatz daneben, ein Aggregat in Registern statt im Speicher — und
//! das Programm läuft, nur eben falsch. Ein Vergleich zweier unabhängiger
//! Umsetzungen ist hier mehr wert als jeder Testfall, den man sich ausdenkt.
//!
//! ## Auflösung wie in der Firn-Fassung
//!
//! Aufgelöst wird **nur die Wurzeldatei**: Grundtypen, Zeiger, Arrays und die
//! in dieser Datei deklarierten Structs. Ein Name, den es hier nicht gibt
//! (etwa `rt.Buf` aus einem anderen Modul), wird zu `?name` mit Größe 0 und
//! Ausrichtung 1 — beide Umsetzungen tun dasselbe, damit der Vergleich exakt
//! bleibt, statt an einer künstlichen Unsicherheit zu scheitern.

use crate::abi::{self, ArgClass};
use crate::ast::{Program, TypeExpr};
use crate::types::{Type, TypeCtx};
use std::collections::HashMap;

pub fn render(p: &Program) -> String {
    // 1. Structs anmelden (Reihenfolge = Deklarationsreihenfolge).
    let mut tcx = TypeCtx::new();
    let mut idx: HashMap<String, usize> = HashMap::new();
    for s in &p.structs {
        let i = tcx.declare(&s.name);
        idx.insert(s.name.clone(), i);
    }
    // 2. Felder auflösen. Reihenfolge zählt: ein Struct kann einen früher
    //    deklarierten enthalten, und dessen Layout muss dann schon stehen.
    for s in &p.structs {
        let i = match idx.get(&s.name) {
            Some(i) => *i,
            None => continue,
        };
        let fs: Vec<(String, Type)> = s
            .fields
            .iter()
            .map(|(n, t, _)| (n.clone(), resolve(t, &idx)))
            .collect();
        tcx.set_fields(i, fs);
    }

    let mut o = String::from("(layout\n");
    for s in &p.structs {
        let i = match idx.get(&s.name) {
            Some(i) => *i,
            None => continue,
        };
        let d = &tcx.structs[i];
        o.push_str(&format!(
            "  (struct {} size {} align {}",
            d.name, d.size, d.align
        ));
        for f in &d.fields {
            o.push_str(&format!(
                " (field {} offset {} size {} align {} type {})",
                f.name,
                f.offset,
                tcx.size_of(&f.ty),
                tcx.align_of(&f.ty),
                tyname(&f.ty, &tcx)
            ));
        }
        o.push_str(")\n");
    }
    for f in &p.funcs {
        o.push_str(&format!("  (fn {}", f.name));
        for pa in &f.params {
            let t = resolve(&pa.ty, &idx);
            o.push_str(&format!(
                " (arg {} size {} class {})",
                tyname(&t, &tcx),
                tcx.size_of(&t),
                class(abi::classify(&t, &tcx))
            ));
        }
        let rt = match &f.ret {
            Some(t) => resolve(t, &idx),
            None => Type::Void,
        };
        o.push_str(&format!(
            " (ret {} size {} class {} sret {})",
            tyname(&rt, &tcx),
            tcx.size_of(&rt),
            class(abi::classify(&rt, &tcx)),
            if abi::ret_needs_sret(&rt, &tcx) { 1 } else { 0 }
        ));
        o.push_str(")\n");
    }
    o.push_str(")\n");
    o
}

fn class(c: ArgClass) -> String {
    match c {
        ArgClass::Integer(n) => format!("int{}", n),
        ArgClass::Memory => "mem".to_string(),
    }
}

fn resolve(t: &TypeExpr, idx: &HashMap<String, usize>) -> Type {
    match t {
        TypeExpr::Ptr { mutable, inner, .. } => Type::ptr(resolve(inner, idx), *mutable),
        TypeExpr::Array { elem, len, .. } => Type::Array(Box::new(resolve(elem, idx)), *len),
        TypeExpr::Named(n, _) => match n.as_str() {
            "i8" => Type::I8,
            "i16" => Type::I16,
            "i32" => Type::I32,
            "i64" => Type::I64,
            "u8" => Type::U8,
            "u16" => Type::U16,
            "u32" => Type::U32,
            "u64" => Type::U64,
            "usize" => Type::Usize,
            "isize" => Type::Isize,
            "bool" => Type::Bool,
            "f64" => Type::F64,
            other => match idx.get(other) {
                Some(i) => Type::Struct(*i),
                None => Type::Error,
            },
        },
    }
}

fn tyname(t: &Type, tcx: &TypeCtx) -> String {
    match t {
        Type::I8 => "i8".into(),
        Type::I16 => "i16".into(),
        Type::I32 => "i32".into(),
        Type::I64 => "i64".into(),
        Type::U8 => "u8".into(),
        Type::U16 => "u16".into(),
        Type::U32 => "u32".into(),
        Type::U64 => "u64".into(),
        Type::Usize => "usize".into(),
        Type::Isize => "isize".into(),
        Type::Bool => "bool".into(),
        Type::F64 => "f64".into(),
        Type::Void => "void".into(),
        Type::UntypedInt => "untyped".into(),
        Type::Error => "?".into(),
        Type::Ptr { mutable, inner } => format!(
            "(ptr {} {})",
            if *mutable { "mut" } else { "const" },
            tyname(inner, tcx)
        ),
        Type::Array(e, n) => format!("(arr {} {})", n, tyname(e, tcx)),
        Type::Struct(i) => tcx
            .structs
            .get(*i)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| "?".to_string()),
    }
}
