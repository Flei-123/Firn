//! Calling convention: System V AMD64 (SPEC §13, §14.1).
//!
//! This file is the **single truth** about how a value crosses a function
//! boundary. `sema`, `lower`, `codegen_x86` and the modules `types`/`opt`
//! ask here rather than inventing rules of their own.
//!
//! Classification per System V AMD64 (§3.2.3 of the ABI):
//!   * integers, `bool`, pointers -> INTEGER, one word
//!   * aggregates up to 16 bytes  -> INTEGER, one or two words
//!   * aggregates over 16 bytes   -> MEMORY
//!   * floating point             -> SSE (stage 0 has no float types)
//!
//! IMPLEMENTATION (see SPEC §14.1 point 1): INTEGER words are passed as the
//! ABI says, through `rdi, rsi, rdx, rcx, r8, r9` and after that on the stack.
//! MEMORY arguments are passed **as a hidden pointer to a copy owned by the
//! caller** rather than as a stack copy; returns over 8 bytes always travel
//! through the hidden pointer in `rdi` (`rax` hands it back).
//! Both are recorded in SPEC §14.1 as a deliberate deviation.

use crate::types::{Type, TypeCtx};

/// Class of an argument/return value at the function boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgClass {
    /// Passed through integer registers; `u8` is the number of 8-byte words
    /// (0 for `()`), at most 2 per System V.
    Integer(u8),
    /// Through memory (stage 0: hidden pointer to a copy).
    Memory,
    // A class `Sse` (SSE/SSEUP per System V) deliberately does NOT exist here:
    // stage 0 knows no float types, `classify` could never yield it, and a
    // variant that nobody produces would be dead code carrying a suppression
    // attribute. It arrives together with `f32`/`f64` — then the compiler
    // itself forces every case split to handle it.
}

/// Size of the largest structure still passed through registers.
pub const MAX_INTEGER_AGGREGATE: u64 = 16;

/// System V classification of a source type.
pub fn classify(ty: &Type, tcx: &TypeCtx) -> ArgClass {
    match ty {
        Type::Void | Type::Error => ArgClass::Integer(0),
        Type::Array(..) | Type::Struct(_) => {
            let size = tcx.size_of(ty);
            if size == 0 {
                ArgClass::Integer(0)
            } else if size > MAX_INTEGER_AGGREGATE {
                ArgClass::Memory
            } else {
                ArgClass::Integer(((size + 7) / 8) as u8)
            }
        }
        // Every scalar type of stage 0 is at most 8 bytes wide.
        _ => ArgClass::Integer(1),
    }
}

/// Is `ty` an aggregate (struct/array)?
pub fn is_aggregate(ty: &Type) -> bool {
    matches!(ty, Type::Array(..) | Type::Struct(_))
}

/// Does the return type need the hidden pointer (`sret`) in `rdi`?
/// That holds for every aggregate over 8 bytes (SPEC §14.1: deviation from
/// System V, which returns 9..16 bytes through `rax:rdx`).
pub fn ret_needs_sret(ty: &Type, tcx: &TypeCtx) -> bool {
    is_aggregate(ty) && tcx.size_of(ty) > 8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Type, TypeCtx};

    fn ctx_with(fields: Vec<(String, Type)>) -> (TypeCtx, Type) {
        let mut tcx = TypeCtx::new();
        let i = tcx.declare("S");
        tcx.set_fields(i, fields);
        (tcx, Type::Struct(i))
    }

    #[test]
    fn scalars_occupy_in_word() {
        let tcx = TypeCtx::new();
        for t in [Type::I8, Type::U64, Type::Bool, Type::ptr(Type::I32, true)] {
            assert_eq!(classify(&t, &tcx), ArgClass::Integer(1), "{:?}", t);
        }
        assert_eq!(classify(&Type::Void, &tcx), ArgClass::Integer(0));
    }

    #[test]
    fn small_structs_in_registers() {
        let (tcx, s) = ctx_with(vec![("a".into(), Type::I32), ("b".into(), Type::I32)]);
        assert_eq!(tcx.size_of(&s), 8);
        assert_eq!(classify(&s, &tcx), ArgClass::Integer(1));
        assert!(!ret_needs_sret(&s, &tcx));

        let (tcx2, s2) = ctx_with(vec![("a".into(), Type::I64), ("b".into(), Type::I64)]);
        assert_eq!(tcx2.size_of(&s2), 16);
        assert_eq!(classify(&s2, &tcx2), ArgClass::Integer(2));
        assert!(ret_needs_sret(&s2, &tcx2));
    }

    #[test]
    fn big_structs_over_memory() {
        let (tcx, s) = ctx_with(vec![
            ("a".into(), Type::I64),
            ("b".into(), Type::I64),
            ("c".into(), Type::I64),
        ]);
        assert_eq!(tcx.size_of(&s), 24);
        assert_eq!(classify(&s, &tcx), ArgClass::Memory);
        assert!(ret_needs_sret(&s, &tcx));
    }

    #[test]
    fn arrays_count_how_structs() {
        let tcx = TypeCtx::new();
        let a = Type::Array(Box::new(Type::U8), 12);
        assert_eq!(classify(&a, &tcx), ArgClass::Integer(2));
        let b = Type::Array(Box::new(Type::U8), 17);
        assert_eq!(classify(&b, &tcx), ArgClass::Memory);
        let c = Type::Array(Box::new(Type::U8), 3);
        assert_eq!(classify(&c, &tcx), ArgClass::Integer(1));
    }
}
