// SPDX-License-Identifier: GPL-2.0-only
//! Calling convention: System V AMD64 (SPEC §13, §14.1).
//!
//! This file is the **single truth** about how a value crosses a function
//! boundary. `sema`, `lower`, `codegen_x86` and the modules `types`/`opt`
//! ask here rather than inventing rules of their own.
//!
//! Classification per System V AMD64 (§3.2.3 of the ABI):
//!   * integers, `bool`, pointers -> INTEGER, one word
//!   * `f32`/`f64`                -> SSE, one word (round 71)
//!   * aggregates up to 16 bytes  -> one or two words, class PER EIGHTBYTE
//!   * aggregates over 16 bytes   -> MEMORY
//!
//! **ROUND 71 — the SSE class really exists now.** Up to round 70 an `f64`
//! travelled as a bit pattern in an INTEGER register; SPEC §14.1.f64 named
//! that as deviation F2 and as a debt. With `f32` the debt came due: without
//! the SSE class a struct `{ f32, f32 }` cannot be handed over the way every
//! other tool chain on this platform hands it over, and reading a WAV or a
//! glTF file means talking to code that was translated by somebody else.
//! Since then floating point arguments travel in `xmm0`-`xmm7`, floating
//! point results in `xmm0`, and an aggregate is classified eightbyte by
//! eightbyte exactly as the ABI document prescribes.
//!
//! IMPLEMENTATION (see SPEC §14.1 point 1): INTEGER words are passed as the
//! ABI says, through `rdi, rsi, rdx, rcx, r8, r9` and after that on the stack.
//! MEMORY arguments are passed **as a hidden pointer to a copy owned by the
//! caller** rather than as a stack copy; returns over 8 bytes always travel
//! through the hidden pointer in `rdi` (`rax` hands it back).
//! Both are recorded in SPEC §14.1 as a deliberate deviation.

use crate::types::{Type, TypeCtx};

/// Register class of ONE eightbyte (System V §3.2.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Word {
    /// `rdi, rsi, rdx, rcx, r8, r9`
    Int,
    /// `xmm0`-`xmm7`
    Sse,
}

/// Class of an argument/return value at the function boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgClass {
    /// Passed through registers: `n` eightbytes (0 for `()`), at most 2 per
    /// System V, each with its own class in `w[..n]`.
    Regs { n: u8, w: [Word; 2] },
    /// Through memory (stage 0: hidden pointer to a copy).
    Memory,
}

impl ArgClass {
    /// `n` eightbytes, all of them INTEGER.
    pub fn ints(n: u8) -> ArgClass {
        ArgClass::Regs { n, w: [Word::Int, Word::Int] }
    }
    /// The classes of the eightbytes, in order (empty for `()` and MEMORY).
    pub fn words(&self) -> &[Word] {
        match self {
            ArgClass::Regs { n, w } => &w[..(*n as usize).min(2)],
            ArgClass::Memory => &[],
        }
    }
}

/// Size of the largest structure still passed through registers.
pub const MAX_INTEGER_AGGREGATE: u64 = 16;

/// System V classification of a source type.
pub fn classify(ty: &Type, tcx: &TypeCtx) -> ArgClass {
    match ty {
        Type::Void | Type::Error => ArgClass::ints(0),
        Type::Array(..) | Type::Struct(_) => {
            let size = tcx.size_of(ty);
            if size == 0 {
                return ArgClass::ints(0);
            }
            if size > MAX_INTEGER_AGGREGATE {
                return ArgClass::Memory;
            }
            let n = ((size + 7) / 8) as u8;
            let mut cls: [Option<Word>; 2] = [None, None];
            collect(ty, tcx, 0, &mut cls);
            let w = [
                cls[0].unwrap_or(Word::Int),
                cls[1].unwrap_or(Word::Int),
            ];
            ArgClass::Regs { n, w }
        }
        // ROUND 71: floating point is its own register class.
        Type::F32 | Type::F64 => ArgClass::Regs { n: 1, w: [Word::Sse, Word::Sse] },
        // Every other scalar type of stage 0 is at most 8 bytes wide.
        _ => ArgClass::ints(1),
    }
}

/// Merge rule of the ABI (§3.2.3, "post merger cleanup"): as soon as ONE
/// integer field lies in an eightbyte, the whole eightbyte counts as INTEGER.
/// SSE therefore only survives where nothing but floating point sits.
fn merge(cur: Option<Word>, add: Word) -> Option<Word> {
    match cur {
        None => Some(add),
        Some(Word::Sse) if add == Word::Sse => Some(Word::Sse),
        _ => Some(Word::Int),
    }
}

/// Walks every SCALAR leaf of an aggregate and enters its class into the
/// eightbyte it lies in. A leaf that straddles the boundary (only possible
/// with `#[packed]`) marks BOTH eightbytes, as the ABI prescribes.
fn collect(ty: &Type, tcx: &TypeCtx, base: u64, cls: &mut [Option<Word>; 2]) {
    match ty {
        Type::Struct(i) => {
            let fields = match tcx.structs.get(*i) {
                Some(s) => s.fields.clone(),
                None => return,
            };
            for f in &fields {
                collect(&f.ty, tcx, base + f.offset, cls);
            }
        }
        Type::Array(e, n) => {
            let step = tcx.size_of(e);
            for k in 0..*n {
                let off = base + k * step;
                if off >= MAX_INTEGER_AGGREGATE {
                    break;
                }
                collect(e, tcx, off, cls);
            }
        }
        _ => {
            let w = if ty.is_float() { Word::Sse } else { Word::Int };
            let size = tcx.size_of(ty).max(1);
            let first = (base / 8) as usize;
            let last = ((base + size - 1) / 8) as usize;
            for k in first..=last {
                if k < 2 {
                    cls[k] = merge(cls[k], w);
                }
            }
        }
    }
}

/// Is `ty` an aggregate (struct/array)?
pub fn is_aggregate(ty: &Type) -> bool {
    matches!(ty, Type::Array(..) | Type::Struct(_))
}

/// Does the return type need the hidden pointer (`sret`) in `rdi`?
/// That holds for every aggregate over 8 bytes (SPEC §14.1: deviation from
/// System V, which returns 9..16 bytes through `rax:rdx` / `xmm0:xmm1`).
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
            assert_eq!(classify(&t, &tcx), ArgClass::ints(1), "{:?}", t);
        }
        assert_eq!(classify(&Type::Void, &tcx), ArgClass::ints(0));
    }

    #[test]
    fn floating_point_goes_to_sse() {
        let tcx = TypeCtx::new();
        for t in [Type::F32, Type::F64] {
            assert_eq!(classify(&t, &tcx).words(), &[Word::Sse], "{:?}", t);
        }
    }

    #[test]
    fn small_structs_in_registers() {
        let (tcx, s) = ctx_with(vec![("a".into(), Type::I32), ("b".into(), Type::I32)]);
        assert_eq!(tcx.size_of(&s), 8);
        assert_eq!(classify(&s, &tcx), ArgClass::ints(1));
        assert!(!ret_needs_sret(&s, &tcx));

        let (tcx2, s2) = ctx_with(vec![("a".into(), Type::I64), ("b".into(), Type::I64)]);
        assert_eq!(tcx2.size_of(&s2), 16);
        assert_eq!(classify(&s2, &tcx2), ArgClass::ints(2));
        assert!(ret_needs_sret(&s2, &tcx2));
    }

    /// The point of round 71: the class is decided PER EIGHTBYTE.
    #[test]
    fn eightbytes_are_classified_separately() {
        // { f32, f32 } -> one eightbyte, pure SSE
        let (tcx, s) = ctx_with(vec![("x".into(), Type::F32), ("y".into(), Type::F32)]);
        assert_eq!(tcx.size_of(&s), 8);
        assert_eq!(classify(&s, &tcx).words(), &[Word::Sse]);

        // { f64, f64 } -> two eightbytes, both SSE (xmm0, xmm1)
        let (tcx, s) = ctx_with(vec![("x".into(), Type::F64), ("y".into(), Type::F64)]);
        assert_eq!(classify(&s, &tcx).words(), &[Word::Sse, Word::Sse]);

        // { i64, f64 } -> INTEGER then SSE (rdi, xmm0)
        let (tcx, s) = ctx_with(vec![("a".into(), Type::I64), ("x".into(), Type::F64)]);
        assert_eq!(classify(&s, &tcx).words(), &[Word::Int, Word::Sse]);

        // { f64, i64 } -> SSE then INTEGER (xmm0, rdi)
        let (tcx, s) = ctx_with(vec![("x".into(), Type::F64), ("a".into(), Type::I64)]);
        assert_eq!(classify(&s, &tcx).words(), &[Word::Sse, Word::Int]);

        // { i32, f32 } -> ONE eightbyte, mixed, and mixed means INTEGER
        let (tcx, s) = ctx_with(vec![("a".into(), Type::I32), ("x".into(), Type::F32)]);
        assert_eq!(tcx.size_of(&s), 8);
        assert_eq!(classify(&s, &tcx).words(), &[Word::Int]);

        // { f32, f32, f32, f32 } -> two SSE eightbytes
        let (tcx, s) = ctx_with(vec![
            ("a".into(), Type::F32),
            ("b".into(), Type::F32),
            ("c".into(), Type::F32),
            ("d".into(), Type::F32),
        ]);
        assert_eq!(tcx.size_of(&s), 16);
        assert_eq!(classify(&s, &tcx).words(), &[Word::Sse, Word::Sse]);
    }

    #[test]
    fn array_of_floats_is_sse() {
        let tcx = TypeCtx::new();
        let a = Type::Array(Box::new(Type::F32), 4);
        assert_eq!(classify(&a, &tcx).words(), &[Word::Sse, Word::Sse]);
        let b = Type::Array(Box::new(Type::F32), 5); // 20 bytes
        assert_eq!(classify(&b, &tcx), ArgClass::Memory);
    }

    #[test]
    fn nested_structs_are_looked_through() {
        let mut tcx = TypeCtx::new();
        let inner = tcx.declare("Inner");
        tcx.set_fields(inner, vec![("x".into(), Type::F32), ("y".into(), Type::F32)]);
        let outer = tcx.declare("Outer");
        tcx.set_fields(
            outer,
            vec![("p".into(), Type::Struct(inner)), ("q".into(), Type::F64)],
        );
        let t = Type::Struct(outer);
        assert_eq!(tcx.size_of(&t), 16);
        assert_eq!(classify(&t, &tcx).words(), &[Word::Sse, Word::Sse]);
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
        assert_eq!(classify(&a, &tcx), ArgClass::ints(2));
        let b = Type::Array(Box::new(Type::U8), 17);
        assert_eq!(classify(&b, &tcx), ArgClass::Memory);
        let c = Type::Array(Box::new(Type::U8), 3);
        assert_eq!(classify(&c, &tcx), ArgClass::ints(1));
    }
}
