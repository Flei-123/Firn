use crate::ast::{ExprKind, FnDecl, Program, TypeExpr};
use crate::fx::{HashMap, HashSet};

pub(crate) struct Shape {
    pub gc_fields: HashSet<String>,
    pub scalar_fields: HashSet<String>,
}

const SCALARS: [&str; 13] = ["i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "usize", "isize", "bool", "f32", "f64"];

pub(crate) fn class_of(te: &TypeExpr) -> Option<&str> {
    match te {
        TypeExpr::Named(n, _) => n.strip_prefix(crate::gc::P_TY_PUB),
        _ => None,
    }
}

pub(crate) fn alloc_class(name: &str) -> Option<&str> {
    name.strip_prefix("gc ")
}

fn build(class: &str) -> Option<Shape> {
    let fields = crate::gc::class_shape(class)?;
    let mut shape = Shape { gc_fields: HashSet::default(), scalar_fields: HashSet::default() };
    for (name, ty) in fields {
        match (&ty, class_of(&ty)) {
            (_, Some(c)) if c == class => {
                shape.gc_fields.insert(name);
            }
            (TypeExpr::Named(n, _), None) if SCALARS.contains(&n.as_str()) => {
                shape.scalar_fields.insert(name);
            }
            _ => return None,
        }
    }
    Some(shape)
}

fn add(c: &str, seen: &mut HashSet<String>, out: &mut HashMap<String, Shape>) {
    if seen.insert(c.to_string()) {
        if let Some(s) = build(c) {
            out.insert(c.to_string(), s);
        }
    }
}

pub(crate) fn collect(prog: &Program) -> HashMap<String, Shape> {
    let mut out: HashMap<String, Shape> = HashMap::default();
    let mut seen: HashSet<String> = HashSet::default();
    for f in &prog.funcs {
        let tys = f.params.iter().map(|p| &p.ty).chain(f.ret.iter());
        for t in tys {
            if let Some(c) = class_of(t) {
                add(c, &mut seen, &mut out);
            }
        }
        let mut allocs: Vec<String> = Vec::new();
        super::walk::walk_block(&f.body, &mut |e| {
            if let ExprKind::Call(name, _, _) = &e.kind {
                if let Some(c) = alloc_class(name) {
                    allocs.push(c.to_string());
                }
            }
        });
        for c in allocs {
            add(&c, &mut seen, &mut out);
        }
    }
    out
}

pub(crate) fn plain(f: &FnDecl) -> bool {
    f.extern_info.is_none()
        && !f.attrs.iter().any(|a| a.name == "no_gc")
        && !super::walk::has_lambda(&f.body)
        && !super::walk::has_defer(&f.body)
}

pub(crate) fn unique_names(prog: &Program) -> HashMap<String, usize> {
    let mut count: HashMap<&str, usize> = HashMap::default();
    for f in &prog.funcs {
        *count.entry(f.name.as_str()).or_insert(0) += 1;
    }
    let mut out: HashMap<String, usize> = HashMap::default();
    for (i, f) in prog.funcs.iter().enumerate() {
        if count.get(f.name.as_str()) == Some(&1) {
            out.insert(f.name.clone(), i);
        }
    }
    out
}
