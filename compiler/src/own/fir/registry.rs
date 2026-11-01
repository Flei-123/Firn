use crate::own::Mode;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Shape {
    SretPair,
    SlotsOwner,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Drop {
    Block,
    SlotsOwner,
}

pub(crate) struct Producer {
    pub name: &'static str,
    pub shape: Shape,
}

pub(crate) const PRODUCERS: &[Producer] = &[
    Producer { name: "__own_str_concat2", shape: Shape::SretPair },
    Producer { name: "__str_concat3", shape: Shape::SretPair },
    Producer { name: "__str_concat4", shape: Shape::SretPair },
    Producer { name: "gcvec_new", shape: Shape::SlotsOwner },
    Producer { name: "gcvec_new_raw", shape: Shape::SlotsOwner },
    Producer { name: "gcmap_new", shape: Shape::SlotsOwner },
];

const DROP_FNS: &[(Drop, &str, &str)] = &[
    (Drop::Block, "__gc_free_block", "__gc_poison_block"),
    (Drop::SlotsOwner, "__own_drop_slots_owner", "__own_drop_slots_owner_check"),
];

pub(crate) const RENAMES: &[(&str, &str)] = &[("__str_concat", "__own_str_concat2")];

pub(crate) const CONSUMES: &[&str] = &[
    "__gc_free_block",
    "__gc_poison_block",
    "__own_drop_slots_owner",
    "__own_drop_slots_owner_check",
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
    drop_fn(Drop::Block, mode)
}

pub(crate) fn drop_fn(drop: Drop, mode: Mode) -> &'static str {
    let (_, free, poison) = DROP_FNS.iter().find(|d| d.0 == drop).copied().unwrap_or(DROP_FNS[0]);
    match mode {
        Mode::Check => poison,
        _ => free,
    }
}

pub(crate) fn root_names() -> Vec<String> {
    let mut v: Vec<String> = PRODUCERS.iter().map(|p| p.name.to_string()).collect();
    for (_, free, poison) in DROP_FNS.iter() {
        v.push(free.to_string());
        v.push(poison.to_string());
    }
    v
}
