// SPDX-License-Identifier: GPL-2.0-only
//! Type representation and memory layout (SPEC §11).
//!
//! Struct layout: declaration order, natural alignment, no reordering of
//! the fields. The size is rounded up to the alignment of the struct.

use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    Usize,
    Isize,
    Bool,
    /// IEEE-754 binary64 (SPEC §8.6).
    F64,
    /// **ROUND 71** — IEEE-754 binary32, the second floating point type.
    /// Four bytes, four of them in one 128-bit SSE register instead of two.
    /// Without it no WAV, no OBJ, no glTF, no STL and no GPU buffer can even
    /// be READ — 32-bit floats stand in all of them.
    F32,
    /// Pointer; `mutable` = `*mut T`.
    Ptr { mutable: bool, inner: Box<Type> },
    Array(Box<Type>, u64),
    /// Index into `TypeCtx::structs`.
    Struct(usize),
    /// Untyped integer literal, must be derived from the context.
    UntypedInt,
    /// Return type of a function without `-> T`.
    Void,
    /// Only to suppress follow-up errors after a reported error.
    Error,
    /// **ROUND 82** — the 128-bit vector register (SPEC §8.6).
    ///
    /// Sixteen octets, sixteen byte aligned, at home in one `xmm` register.
    /// It carries NO element type: what the sixteen octets mean is decided
    /// by the instruction that is applied to them (`__v128_add32` reads them
    /// as four `u32`, `__aesenc` as one AES state). That is deliberately
    /// different from `f32x4`-style types — the crypto instructions of the
    /// processor have no element type either, and a type per reading would
    /// only produce conversions that generate no code.
    ///
    /// `v128` has NO operators. Everything happens through the `__v128_*`
    /// intrinsics of `simd.rs`; that keeps `+` from silently meaning four
    /// different machine instructions.
    V128,
    /// **Round 58** — a function as a VALUE (`fn(i32, i32) -> i32`).
    ///
    /// One machine word wide: the pointer to a FUNCTION RECORD. Word 0 of
    /// that record is the address of the machine code, the words after it
    /// are the captured values of a closure (`docs/ROUND58.md`). A named
    /// function has a record of exactly one word in `.rodata`; only a
    /// closure that captures something needs the GC heap.
    Fn { params: Vec<Type>, ret: Box<Type> },
}

impl Type {
    pub fn ptr(inner: Type, mutable: bool) -> Type {
        Type::Ptr { mutable, inner: Box::new(inner) }
    }
    pub fn is_int(&self) -> bool {
        matches!(
            self,
            Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::U8 | Type::U16
                | Type::U32 | Type::U64 | Type::Usize | Type::Isize | Type::UntypedInt
        )
    }
    /// Integer type of fixed width (without UntypedInt).
    pub fn is_concrete_int(&self) -> bool {
        self.is_int() && *self != Type::UntypedInt
    }
    pub fn is_signed(&self) -> bool {
        matches!(self, Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Isize | Type::UntypedInt)
    }
    pub fn is_ptr(&self) -> bool {
        matches!(self, Type::Ptr { .. })
    }
    /// **ROUND 71** — one of the two floating point types.
    pub fn is_float(&self) -> bool {
        matches!(self, Type::F32 | Type::F64)
    }
    /// Round 58 — the signature behind a function value, if it is one.
    pub fn fn_sig(&self) -> Option<(&[Type], &Type)> {
        match self {
            Type::Fn { params, ret } => Some((params.as_slice(), ret)),
            _ => None,
        }
    }
    pub fn is_fn(&self) -> bool {
        matches!(self, Type::Fn { .. })
    }
    pub fn is_error(&self) -> bool {
        matches!(self, Type::Error)
    }
    /// Bit width for integers/bool/pointers; 0 for aggregate types.
    pub fn bits(&self) -> u32 {
        match self {
            Type::I8 | Type::U8 => 8,
            Type::I16 | Type::U16 => 16,
            Type::I32 | Type::U32 => 32,
            Type::I64 | Type::U64 | Type::Usize | Type::Isize => 64,
            Type::UntypedInt => 64,
            Type::F32 => 32,
            Type::Bool => 8,
            Type::Ptr { .. } => 64,
            Type::Fn { .. } => 64,
            _ => 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub ty: Type,
    pub offset: u64,
}

#[derive(Clone, Debug)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<Field>,
    pub size: u64,
    pub align: u64,
    /// `#[must_consume]` (attrs.rs): a value of this type must not be
    /// discarded silently.
    pub must_consume: bool,
}

impl StructDef {
    pub fn field(&self, name: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.name == name)
    }
}

/// Table of all known structs.
#[derive(Clone, Debug, Default)]
pub struct TypeCtx {
    pub structs: Vec<StructDef>,
    pub by_name: HashMap<String, usize>,
}

impl TypeCtx {
    pub fn new() -> TypeCtx {
        TypeCtx::default()
    }

    pub fn lookup(&self, name: &str) -> Option<usize> {
        self.by_name.get(name).copied()
    }

    /// Creates a struct (the layout is set by `finish_struct`).
    pub fn declare(&mut self, name: &str) -> usize {
        let idx = self.structs.len();
        self.structs.push(StructDef {
            name: name.to_string(),
            fields: Vec::new(),
            size: 0,
            align: 1,
            must_consume: false,
        });
        self.by_name.insert(name.to_string(), idx);
        idx
    }

    /// Computes offsets/size/alignment from (name, type) pairs.
    pub fn set_fields(&mut self, idx: usize, fields: Vec<(String, Type)>) {
        let mut off: u64 = 0;
        let mut max_align: u64 = 1;
        let mut out = Vec::new();
        for (name, ty) in fields {
            let a = self.align_of(&ty).max(1);
            let s = self.size_of(&ty);
            if a > max_align {
                max_align = a;
            }
            off = round_up(off, a);
            out.push(Field { name, ty, offset: off });
            off += s;
        }
        let size = round_up(off, max_align);
        let d = &mut self.structs[idx];
        d.fields = out;
        d.size = size;
        d.align = max_align;
    }

    pub fn size_of(&self, t: &Type) -> u64 {
        match t {
            Type::I8 | Type::U8 | Type::Bool => 1,
            Type::I16 | Type::U16 => 2,
            Type::I32 | Type::U32 | Type::F32 => 4,
            Type::I64 | Type::U64 | Type::Usize | Type::Isize | Type::UntypedInt => 8,
            Type::F64 => 8,
            // ROUND 82: sixteen octets, and sixteen byte aligned (align_of
            // asks size_of for everything that is not array/struct).
            Type::V128 => 16,
            Type::Ptr { .. } => 8,
            Type::Fn { .. } => 8,
            Type::Array(e, n) => self.size_of(e) * *n,
            Type::Struct(i) => self.structs.get(*i).map(|s| s.size).unwrap_or(0),
            Type::Void | Type::Error => 0,
        }
    }

    pub fn align_of(&self, t: &Type) -> u64 {
        match t {
            Type::Array(e, _) => self.align_of(e),
            Type::Struct(i) => self.structs.get(*i).map(|s| s.align).unwrap_or(1),
            Type::Void | Type::Error => 1,
            other => self.size_of(other).max(1),
        }
    }

    /// Name of the gc class behind a type, if it is one.
    fn gc_class_name(&self, t: &Type) -> Option<&str> {
        match t {
            Type::Struct(i) => self
                .structs
                .get(*i)
                .and_then(|s| s.name.strip_prefix("gc ")),
            _ => None,
        }
    }

    /// Human-readable rendering of a type, for error messages.
    pub fn name_of(&self, t: &Type) -> String {
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
            Type::F32 => "f32".into(),
            Type::V128 => "v128".into(),
            // A pointer to a gc class is spelled `Gc[C]` in the source text
            // (the struct carries "gc C" as its internal name, see gc.rs).
            Type::Ptr { inner, .. } if self.gc_class_name(inner).is_some() => {
                match self.gc_class_name(inner) {
                    Some(n) => format!("Gc[{}]", n),
                    None => "Gc[?]".into(),
                }
            }
            Type::Ptr { mutable, inner } => {
                if *mutable {
                    format!("*mut {}", self.name_of(inner))
                } else {
                    format!("*{}", self.name_of(inner))
                }
            }
            Type::Array(e, n) => format!("[{}; {}]", self.name_of(e), n),
            Type::Struct(i) => self
                .structs
                .get(*i)
                .map(|s| s.name.clone())
                .unwrap_or_else(|| "<struct>".into()),
            Type::Fn { params, ret } => {
                let ps: Vec<String> = params.iter().map(|p| self.name_of(p)).collect();
                match **ret {
                    Type::Void => format!("fn({})", ps.join(", ")),
                    _ => format!("fn({}) -> {}", ps.join(", "), self.name_of(ret)),
                }
            }
            Type::UntypedInt => "{integer}".into(),
            Type::Void => "()".into(),
            Type::Error => "<error>".into(),
        }
    }
}

/// **ROUND 70** -- the second spelling of a primitive type.
///
/// `int` is the SAME type as `i32`, not a new one: the table maps a name
/// onto the canonical name, and everything behind it computes with `i32`
/// exactly as before. That is why there is no conversion, no cast and no
/// second entry in [`Type`] -- an alias that produced its own type would
/// have to be taught to every comparison in the type checker.
///
/// **ROUND 71** — `float` is given out now and it means `f32`, exactly as in
/// C, C++, C#, Java and Go. It was held back in round 70 on purpose so that
/// it would not first mean `f64` and then something else.
///
/// **ROUND 88** — `string` closes the family. The list above is the one of
/// C#, and there the text type is called `string`; it was the only name
/// missing, and `let x: string = "test"` answered with "unknown type".
/// CAUTION, the one difference to all the others: `str` is NOT a primitive
/// type, it is the builtin STRUCT of `strtype.rs`. So `prim_type("string")`
/// stays `None` on purpose — the fold happens one step later, where a name
/// becomes a struct (`sema.rs::resolve_ty`). That is why `string` may not be
/// written into the primitive tables of `layout_canon.rs`/`iface.rs`: they
/// match on the canonical name and let `str` fall through to the struct
/// lookup all by themselves.
pub fn alias_of(name: &str) -> Option<&'static str> {
    Some(match name {
        "sbyte" => "i8",
        "short" => "i16",
        "int" => "i32",
        "long" => "i64",
        "byte" => "u8",
        "ushort" => "u16",
        "uint" => "u32",
        "ulong" => "u64",
        "double" => "f64",
        "float" => "f32",
        // ROUND 88: the text type of the same family. `str` is a struct,
        // not a primitive -- see the note above.
        "string" => "str",
        _ => return None,
    })
}

/// The canonical spelling of a type name (`int` -> `i32`); every other name
/// passes through unchanged.
pub fn canon_name(name: &str) -> &str {
    match alias_of(name) {
        Some(c) => c,
        None => name,
    }
}

pub fn round_up(v: u64, a: u64) -> u64 {
    if a <= 1 {
        v
    } else {
        (v + a - 1) / a * a
    }
}
