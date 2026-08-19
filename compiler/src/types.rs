//! Type representation and memory layout (SPEC §11).
//!
//! Struct layout: declaration order, natural alignment, no reordering of
//! the fields. The size gets rounded up to the alignment of the struct.

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
            Type::Bool => 8,
            Type::Ptr { .. } => 64,
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

    /// Creates a struct (the layout gets set by `finish_struct`).
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

    /// Computes offsets/size/alignment from (identifier, type) pairs.
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
            Type::I32 | Type::U32 => 4,
            Type::I64 | Type::U64 | Type::Usize | Type::Isize | Type::UntypedInt => 8,
            Type::F64 => 8,
            Type::Ptr { .. } => 8,
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

    /// Identifier of the gc class behind a type, if it is one.
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
            // A pointer to a gc class is spelled `Gc[C]` within the source text
            // (the struct carries "gc C" as its internal label, see gc.rs).
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
            Type::UntypedInt => "{integer}".into(),
            Type::Void => "()".into(),
            Type::Error => "<error>".into(),
        }
    }
}

pub fn round_up(v: u64, a: u64) -> u64 {
    if a <= 1 {
        v
    } else {
        (v + a - 1) / a * a
    }
}
