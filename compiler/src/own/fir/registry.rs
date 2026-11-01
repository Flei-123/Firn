use crate::own::Mode;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Shape {
    SretPair,
}

pub(crate) struct Producer {
    pub name: &'static str,
    pub shape: Shape,
}

pub(crate) const PRODUCERS: &[Producer] = &[
    Producer { name: "__own_str_concat2", shape: Shape::SretPair },
    Producer { name: "__own_str_concat3", shape: Shape::SretPair },
    Producer { name: "__own_str_concat4", shape: Shape::SretPair },
];

pub(crate) const RENAMES: &[(&str, &str)] = &[
    ("__str_concat", "__own_str_concat2"),
    ("__str_concat3", "__own_str_concat3"),
    ("__str_concat4", "__own_str_concat4"),
];

pub(crate) const CONSUMES: &[&str] = &[
    "__gc_free_block",
    "__gc_poison_block",
    "__own_poison_quiet",
    "__own_drop_tree",
    "__own_drop_tree_check",
    "__own_drop_run",
    "__own_drop_loop",
    "__str_concat4_tail",
    "__str_concat4_tail_check",
];

pub(crate) const GC_ALLOC: &str = crate::gc::FN_ALLOC;

pub(crate) const ESCAPES: &[&str] = &[
    crate::gc::FN_WEAK,
    crate::gc::FN_AS,
    "gc_finalizer_set",
    "gc_finalizer_kind",
    "gc_finalizer_delete",
    "gc_root_register",
    "gc_root_register_r",
    "gc_root_unregister",
];

pub(crate) const BORROW_ALL: &[&str] = &["__own_blit"];

pub(crate) fn producer(name: &str) -> Option<&'static Producer> {
    PRODUCERS.iter().find(|p| p.name == name)
}

pub(crate) fn consumes(name: &str) -> bool {
    CONSUMES.contains(&name) || ESCAPES.contains(&name)
}

pub(crate) fn is_gc_alloc(name: &str) -> bool {
    name == GC_ALLOC
}

pub(crate) fn borrows_all(name: &str) -> bool {
    BORROW_ALL.contains(&name)
}

pub(crate) fn free_fn(mode: Mode) -> &'static str {
    match mode {
        Mode::Check => "__gc_poison_block",
        _ => "__gc_free_block",
    }
}

pub(crate) fn root_names() -> Vec<String> {
    let mut v: Vec<String> = PRODUCERS.iter().map(|p| p.name.to_string()).collect();
    v.extend(RENAMES.iter().map(|(from, _)| from.to_string()));
    v.push("__gc_free_block".to_string());
    v.push("__gc_poison_block".to_string());
    v
}
