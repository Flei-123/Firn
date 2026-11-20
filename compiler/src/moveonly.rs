use crate::types::{Type, TypeCtx};
use std::sync::atomic::{AtomicBool, Ordering};

const MOVE_ONLY_PREFIXES: &[&str] = &["Vec__"];

pub(crate) const ALLOWED: &str = "off, vec";

static VEC_MOVES: AtomicBool = AtomicBool::new(true);

pub(crate) fn parse(v: &str) -> Option<bool> {
    match v {
        "off" => Some(false),
        "vec" => Some(true),
        _ => None,
    }
}

pub(crate) fn set_enabled(on: bool) {
    VEC_MOVES.store(on, Ordering::Relaxed);
}

pub(crate) fn enabled() -> bool {
    VEC_MOVES.load(Ordering::Relaxed)
}

pub(crate) fn is_move_only(tcx: &TypeCtx, t: &Type, depth: u32) -> bool {
    if depth > 32 {
        return false;
    }
    match t {
        Type::Struct(i) => match tcx.structs.get(*i) {
            Some(s) => {
                MOVE_ONLY_PREFIXES.iter().any(|p| s.name.starts_with(p))
                    || s.fields.iter().any(|f| is_move_only(tcx, &f.ty, depth + 1))
            }
            None => false,
        },
        Type::Array(inner, _) => is_move_only(tcx, inner, depth + 1),
        _ => false,
    }
}
