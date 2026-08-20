//! Canonical rendering of **memory layout and calling convention** — the
//! yardstick for `lib/firnc1/types.fi`.
//!
//! ## Why a rendering of its own
//!
//! Layout and ABI are the spots where a compiler turns quietly wrong: one
//! field offset off by a bit, one aggregate held by registers rather than by
//! memory — and the program runs, just wrongly. Comparing two independent
//! implementations is worth more here than any test case somebody dreams up.
//!
//! ## Resolution just like the Firn version
//!
//! Resolved gets **the root file only**: base types, pointers, arrays and the
//! structs declared by this file. A type spelling that is missing here
//! (say `rt.Buf` from another module) turns into the placeholder `?` with
//! size 0 and alignment 1 — both implementations do the same, so that the
//! comparison stays exact rather than failing over artificial uncertainty.

use crate::abi::{self, ArgClass};
use crate::ast::{Program, TypeExpr};
use crate::types::{Type, TypeCtx};
use std::collections::HashMap;

pub fn render(p: &Program) -> String {
    // 1. Register the structs (order = declaration order).
    let mut tcx = TypeCtx::new();
    let mut idx: HashMap<String, usize> = HashMap::new();
    for s in &p.structs {
        let i = tcx.declare(&s.name);
        idx.insert(s.name.clone(), i);
    }
    // 2. Resolve the fields. Order matters: a struct may contain one declared
    //    earlier, and that layout must already be settled by then.
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
