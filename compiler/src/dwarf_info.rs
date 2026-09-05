// SPDX-License-Identifier: GPL-2.0-only
//! Round 64 — `.debug_abbrev` and `.debug_info` in DWARF 4, written by the
//! compiler itself.
//!
//! WHY BY HAND. The assembler can produce a line table out of `.file`/`.loc`,
//! nothing more. Names, types and variables have to be described by whoever
//! knows them, and that is the compiler. So this module builds the two
//! sections as a byte stream and writes them into the assembly as `.byte`
//! lines — with one exception: addresses of functions and labels cannot be
//! octets, they need a relocation. That is why the stream is not a `Vec<u8>`
//! but a list of PIECES: literal octets, or an eight-octet address that the
//! assembler fills in (`.quad label`). The length is known for both, so
//! every offset inside `.debug_info` can be computed exactly — and DWARF is
//! nothing but offsets.
//!
//! THE SHAPE OF THE INFORMATION:
//!
//! ```text
//! DW_TAG_compile_unit                 name, comp_dir, producer, language,
//!                                     low_pc/high_pc, stmt_list
//!   DW_TAG_base_type ...              i32, u64, bool, f64, ...
//!   DW_TAG_pointer_type ...           *mut T
//!   DW_TAG_array_type ...             [T; N]
//!   DW_TAG_structure_type ...         struct with members and offsets
//!   DW_TAG_subprogram                 name, decl_file/line, low_pc/high_pc,
//!     DW_TAG_formal_parameter         frame_base = rbp, type
//!     DW_TAG_variable                 name, decl_file/line, type,
//!                                     location = fbreg <offset>
//! ```
//!
//! The types come first, so that a `DW_AT_type` can point BACKWARDS at an
//! offset that is already known. That saves a second pass.

use std::collections::HashMap;

use crate::dwarf::{DType, VarNote};

// ---------------------------------------------------------------- DWARF 4
const DW_TAG_ARRAY_TYPE: u8 = 0x01;
const DW_TAG_FORMAL_PARAMETER: u8 = 0x05;
const DW_TAG_MEMBER: u8 = 0x0d;
const DW_TAG_POINTER_TYPE: u8 = 0x0f;
const DW_TAG_COMPILE_UNIT: u8 = 0x11;
const DW_TAG_STRUCTURE_TYPE: u8 = 0x13;
const DW_TAG_SUBRANGE_TYPE: u8 = 0x21;
const DW_TAG_BASE_TYPE: u8 = 0x24;
const DW_TAG_SUBPROGRAM: u8 = 0x2e;
const DW_TAG_VARIABLE: u8 = 0x34;

const DW_CHILDREN_NO: u8 = 0x00;
const DW_CHILDREN_YES: u8 = 0x01;

const DW_AT_LOCATION: u8 = 0x02;
const DW_AT_NAME: u8 = 0x03;
const DW_AT_BYTE_SIZE: u8 = 0x0b;
const DW_AT_STMT_LIST: u8 = 0x10;
const DW_AT_LOW_PC: u8 = 0x11;
const DW_AT_HIGH_PC: u8 = 0x12;
const DW_AT_LANGUAGE: u8 = 0x13;
const DW_AT_COMP_DIR: u8 = 0x1b;
const DW_AT_UPPER_BOUND: u8 = 0x2f;
const DW_AT_PRODUCER: u8 = 0x25;
const DW_AT_PROTOTYPED: u8 = 0x27;
const DW_AT_DATA_MEMBER_LOCATION: u8 = 0x38;
const DW_AT_DECL_FILE: u8 = 0x3a;
const DW_AT_DECL_LINE: u8 = 0x3b;
const DW_AT_DECLARATION: u8 = 0x3c;
const DW_AT_ENCODING: u8 = 0x3e;
const DW_AT_EXTERNAL: u8 = 0x3f;
const DW_AT_FRAME_BASE: u8 = 0x40;
const DW_AT_TYPE: u8 = 0x49;

const DW_FORM_ADDR: u8 = 0x01;
const DW_FORM_DATA2: u8 = 0x05;
const DW_FORM_DATA4: u8 = 0x06;
const DW_FORM_DATA1: u8 = 0x0b;
const DW_FORM_STRING: u8 = 0x08;
const DW_FORM_REF4: u8 = 0x13;
const DW_FORM_SEC_OFFSET: u8 = 0x17;
const DW_FORM_EXPRLOC: u8 = 0x18;
const DW_FORM_FLAG_PRESENT: u8 = 0x19;

/// `DW_LANG_C99`. Firn is no C, but the debugger only reads the way
/// expressions and array indexing are written out of this — and there Firn
/// follows C. A language number of its own would only make `gdb` fall back
/// to its default.
const DW_LANG_C99: u16 = 0x000c;

const DW_OP_FBREG: u8 = 0x91;
const DW_OP_REG6: u8 = 0x56; // rbp

/// Abbreviation numbers, fixed, so that `.debug_abbrev` stays a constant.
const AB_CU: u64 = 1;
const AB_BASE: u64 = 2;
const AB_PTR: u64 = 3;
const AB_ARRAY: u64 = 4;
const AB_SUBRANGE: u64 = 5;
const AB_STRUCT: u64 = 6;
const AB_MEMBER: u64 = 7;
const AB_SUBPROGRAM: u64 = 8;
const AB_SUBPROGRAM_VOID: u64 = 9;
const AB_PARAM: u64 = 10;
const AB_VAR: u64 = 11;
const AB_OPAQUE: u64 = 12;

// ------------------------------------------------------------ the pieces

enum Piece {
    /// Literal octets.
    Raw(Vec<u8>),
    /// Eight octets that the assembler fills in: the address of a label.
    Addr(String),
}

/// A section under construction. `len` is always the number of octets
/// written so far — that is the offset every `DW_FORM_ref4` needs.
struct Section {
    pieces: Vec<Piece>,
    len: usize,
}

impl Section {
    fn new() -> Section {
        Section { pieces: Vec::new(), len: 0 }
    }
    fn u8(&mut self, v: u8) {
        self.raw(&[v]);
    }
    fn u16(&mut self, v: u16) {
        self.raw(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.raw(&v.to_le_bytes());
    }
    fn raw(&mut self, bytes: &[u8]) {
        self.len += bytes.len();
        match self.pieces.last_mut() {
            Some(Piece::Raw(b)) => b.extend_from_slice(bytes),
            _ => self.pieces.push(Piece::Raw(bytes.to_vec())),
        }
    }
    fn uleb(&mut self, mut v: u64) {
        loop {
            let mut b = (v & 0x7f) as u8;
            v >>= 7;
            if v != 0 {
                b |= 0x80;
            }
            self.u8(b);
            if v == 0 {
                break;
            }
        }
    }
    fn sleb(&mut self, mut v: i64) {
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            let sign = b & 0x40 != 0;
            if (v == 0 && !sign) || (v == -1 && sign) {
                self.u8(b);
                break;
            }
            self.u8(b | 0x80);
        }
    }
    fn text(&mut self, s: &str) {
        let mut b = s.as_bytes().to_vec();
        b.push(0);
        self.raw(&b);
    }
    fn addr(&mut self, label: &str) {
        self.len += 8;
        self.pieces.push(Piece::Addr(label.to_string()));
    }

    /// The section as assembly text.
    fn render(&self, name: &str) -> String {
        let mut out = String::new();
        out.push_str(&format!(".section {},\"\",@progbits\n", name));
        for p in &self.pieces {
            match p {
                Piece::Raw(b) => {
                    for chunk in b.chunks(16) {
                        let list: Vec<String> = chunk.iter().map(|x| x.to_string()).collect();
                        out.push_str(&format!("    .byte {}\n", list.join(",")));
                    }
                }
                Piece::Addr(l) => out.push_str(&format!("    .quad {}\n", l)),
            }
        }
        out
    }
}

// --------------------------------------------------------- .debug_abbrev
//
// One entry per shape of a DIE. The list is a CONSTANT: which attributes a
// subprogram has does not depend on the program.

fn abbrev(sec: &mut Section, code: u64, tag: u8, children: u8, attrs: &[(u8, u8)]) {
    sec.uleb(code);
    sec.uleb(tag as u64);
    sec.u8(children);
    for (at, form) in attrs {
        sec.uleb(*at as u64);
        sec.uleb(*form as u64);
    }
    sec.uleb(0);
    sec.uleb(0);
}

fn abbrev_section() -> Section {
    let mut s = Section::new();
    abbrev(
        &mut s,
        AB_CU,
        DW_TAG_COMPILE_UNIT,
        DW_CHILDREN_YES,
        &[
            (DW_AT_PRODUCER, DW_FORM_STRING),
            (DW_AT_LANGUAGE, DW_FORM_DATA2),
            (DW_AT_NAME, DW_FORM_STRING),
            (DW_AT_COMP_DIR, DW_FORM_STRING),
            (DW_AT_LOW_PC, DW_FORM_ADDR),
            (DW_AT_HIGH_PC, DW_FORM_ADDR),
            (DW_AT_STMT_LIST, DW_FORM_SEC_OFFSET),
        ],
    );
    abbrev(
        &mut s,
        AB_BASE,
        DW_TAG_BASE_TYPE,
        DW_CHILDREN_NO,
        &[
            (DW_AT_NAME, DW_FORM_STRING),
            (DW_AT_ENCODING, DW_FORM_DATA1),
            (DW_AT_BYTE_SIZE, DW_FORM_DATA1),
        ],
    );
    abbrev(
        &mut s,
        AB_PTR,
        DW_TAG_POINTER_TYPE,
        DW_CHILDREN_NO,
        &[(DW_AT_BYTE_SIZE, DW_FORM_DATA1), (DW_AT_TYPE, DW_FORM_REF4)],
    );
    abbrev(
        &mut s,
        AB_ARRAY,
        DW_TAG_ARRAY_TYPE,
        DW_CHILDREN_YES,
        &[(DW_AT_TYPE, DW_FORM_REF4)],
    );
    abbrev(
        &mut s,
        AB_SUBRANGE,
        DW_TAG_SUBRANGE_TYPE,
        DW_CHILDREN_NO,
        &[(DW_AT_UPPER_BOUND, DW_FORM_DATA4)],
    );
    abbrev(
        &mut s,
        AB_STRUCT,
        DW_TAG_STRUCTURE_TYPE,
        DW_CHILDREN_YES,
        &[(DW_AT_NAME, DW_FORM_STRING), (DW_AT_BYTE_SIZE, DW_FORM_DATA4)],
    );
    abbrev(
        &mut s,
        AB_MEMBER,
        DW_TAG_MEMBER,
        DW_CHILDREN_NO,
        &[
            (DW_AT_NAME, DW_FORM_STRING),
            (DW_AT_TYPE, DW_FORM_REF4),
            (DW_AT_DATA_MEMBER_LOCATION, DW_FORM_DATA4),
        ],
    );
    abbrev(
        &mut s,
        AB_SUBPROGRAM,
        DW_TAG_SUBPROGRAM,
        DW_CHILDREN_YES,
        &[
            (DW_AT_EXTERNAL, DW_FORM_FLAG_PRESENT),
            (DW_AT_NAME, DW_FORM_STRING),
            (DW_AT_DECL_FILE, DW_FORM_DATA4),
            (DW_AT_DECL_LINE, DW_FORM_DATA4),
            (DW_AT_PROTOTYPED, DW_FORM_FLAG_PRESENT),
            (DW_AT_TYPE, DW_FORM_REF4),
            (DW_AT_LOW_PC, DW_FORM_ADDR),
            (DW_AT_HIGH_PC, DW_FORM_ADDR),
            (DW_AT_FRAME_BASE, DW_FORM_EXPRLOC),
        ],
    );
    abbrev(
        &mut s,
        AB_SUBPROGRAM_VOID,
        DW_TAG_SUBPROGRAM,
        DW_CHILDREN_YES,
        &[
            (DW_AT_EXTERNAL, DW_FORM_FLAG_PRESENT),
            (DW_AT_NAME, DW_FORM_STRING),
            (DW_AT_DECL_FILE, DW_FORM_DATA4),
            (DW_AT_DECL_LINE, DW_FORM_DATA4),
            (DW_AT_PROTOTYPED, DW_FORM_FLAG_PRESENT),
            (DW_AT_LOW_PC, DW_FORM_ADDR),
            (DW_AT_HIGH_PC, DW_FORM_ADDR),
            (DW_AT_FRAME_BASE, DW_FORM_EXPRLOC),
        ],
    );
    abbrev(
        &mut s,
        AB_PARAM,
        DW_TAG_FORMAL_PARAMETER,
        DW_CHILDREN_NO,
        &[
            (DW_AT_NAME, DW_FORM_STRING),
            (DW_AT_DECL_FILE, DW_FORM_DATA4),
            (DW_AT_DECL_LINE, DW_FORM_DATA4),
            (DW_AT_TYPE, DW_FORM_REF4),
            (DW_AT_LOCATION, DW_FORM_EXPRLOC),
        ],
    );
    abbrev(
        &mut s,
        AB_VAR,
        DW_TAG_VARIABLE,
        DW_CHILDREN_NO,
        &[
            (DW_AT_NAME, DW_FORM_STRING),
            (DW_AT_DECL_FILE, DW_FORM_DATA4),
            (DW_AT_DECL_LINE, DW_FORM_DATA4),
            (DW_AT_TYPE, DW_FORM_REF4),
            (DW_AT_LOCATION, DW_FORM_EXPRLOC),
        ],
    );
    abbrev(
        &mut s,
        AB_OPAQUE,
        DW_TAG_STRUCTURE_TYPE,
        DW_CHILDREN_NO,
        &[
            (DW_AT_NAME, DW_FORM_STRING),
            (DW_AT_BYTE_SIZE, DW_FORM_DATA4),
            (DW_AT_DECLARATION, DW_FORM_FLAG_PRESENT),
        ],
    );
    s.uleb(0);
    s
}

// ----------------------------------------------------------- .debug_info

/// One function with everything the debugger needs about it.
pub struct FnInfo {
    pub name: String,
    /// Symbol of the function (`low_pc`) and label at its end (`high_pc`).
    pub start_label: String,
    pub end_label: String,
    pub file: u32,
    pub line: u32,
    pub ret: DType,
    /// The declared names WITH their frame offset (address = rbp - off).
    pub vars: Vec<(VarNote, u64)>,
}

/// Collects the types and hands out their offsets in `.debug_info`.
struct Types {
    at: HashMap<String, u32>,
    order: Vec<DType>,
}

/// A key that identifies a type. `DType` is not `Hash`, and the text form is
/// unambiguous enough: it contains name, size and the members.
fn key(t: &DType) -> String {
    match t {
        DType::Base(n, s, e) => format!("b:{}:{}:{}", n, s, e),
        DType::Ptr(i) => format!("p:{}", key(i)),
        DType::Array(i, n) => format!("a:{}:{}", key(i), n),
        DType::Struct(n, s, m) => {
            let mut k = format!("s:{}:{}", n, s);
            for (mn, mo, mt) in m {
                k.push_str(&format!("|{}:{}:{}", mn, mo, key(mt)));
            }
            k
        }
        DType::Opaque(n, s) => format!("o:{}:{}", n, s),
        DType::Void => "v".to_string(),
    }
}

impl Types {
    fn new() -> Types {
        Types { at: HashMap::new(), order: Vec::new() }
    }
    /// Enters a type and everything it is made of, deepest first.
    fn collect(&mut self, t: &DType) {
        if matches!(t, DType::Void) {
            return;
        }
        let k = key(t);
        if self.at.contains_key(&k) {
            return;
        }
        match t {
            DType::Ptr(i) => self.collect(i),
            DType::Array(i, _) => self.collect(i),
            DType::Struct(_, _, m) => {
                for (_, _, mt) in m {
                    self.collect(mt);
                }
            }
            _ => {}
        }
        // The recursion may have entered the type itself (a struct that
        // points at itself); then it must not go in a second time.
        if self.at.contains_key(&k) {
            return;
        }
        self.at.insert(k, 0);
        self.order.push(t.clone());
    }
    fn offset(&self, t: &DType) -> u32 {
        *self.at.get(&key(t)).unwrap_or(&0)
    }
}

/// Builds both sections. Returns (abbrev, info) as assembly text.
pub fn build(
    producer: &str,
    unit_name: &str,
    comp_dir: &str,
    text_start: &str,
    text_end: &str,
    funcs: &[FnInfo],
) -> (String, String) {
    // --- collect the types -------------------------------------------------
    let mut types = Types::new();
    for f in funcs {
        types.collect(&f.ret);
        for (v, _) in &f.vars {
            types.collect(&v.ty);
        }
    }

    // --- the header, so that the offsets are right -------------------------
    let mut s = Section::new();
    // unit_length (filled in at the end as a constant: everything is known)
    s.u32(0); // placeholder, corrected below
    let after_length = s.len; // = 4
    s.u16(4); // version
    s.u32(0); // abbrev offset
    s.u8(8); // address size

    // --- DW_TAG_compile_unit ----------------------------------------------
    s.uleb(AB_CU);
    s.text(producer);
    s.u16(DW_LANG_C99);
    s.text(unit_name);
    s.text(comp_dir);
    s.addr(text_start);
    s.addr(text_end);
    s.u32(0); // stmt_list: the only line program starts at 0

    // --- the types ---------------------------------------------------------
    // Two passes: the first one only works out the offsets (nothing is
    // written), the second one writes. That is necessary because a struct
    // member points forward at nothing but backwards at a lot, and because
    // an array names its element type.
    let mut probe = Section::new();
    probe.len = s.len;
    let mut offsets: Vec<u32> = Vec::new();
    for t in &types.order {
        offsets.push(probe.len as u32);
        write_type(&mut probe, t, &Types::new(), true);
    }
    for (i, t) in types.order.iter().enumerate() {
        types.at.insert(key(t), offsets[i]);
    }
    for t in &types.order {
        write_type(&mut s, t, &types, false);
    }

    // --- the functions -----------------------------------------------------
    for f in funcs {
        let void = matches!(f.ret, DType::Void);
        s.uleb(if void { AB_SUBPROGRAM_VOID } else { AB_SUBPROGRAM });
        // DW_AT_external is DW_FORM_flag_present: no octets
        s.text(&f.name);
        s.u32(f.file + 1);
        s.u32(f.line);
        // DW_AT_prototyped: no octets
        if !void {
            s.u32(types.offset(&f.ret));
        }
        s.addr(&f.start_label);
        s.addr(&f.end_label);
        // DW_AT_frame_base: one octet of expression, DW_OP_reg6 = rbp
        s.uleb(1);
        s.u8(DW_OP_REG6);
        for (v, off) in &f.vars {
            s.uleb(if v.param { AB_PARAM } else { AB_VAR });
            s.text(&v.name);
            s.u32(v.file + 1);
            s.u32(v.line);
            s.u32(types.offset(&v.ty));
            // location: DW_OP_fbreg <sleb>. The frame base is rbp, the
            // storage lies at rbp - off, so the offset is negative.
            let mut expr = Section::new();
            expr.u8(DW_OP_FBREG);
            expr.sleb(-(*off as i64));
            s.uleb(expr.len as u64);
            for p in expr.pieces {
                if let Piece::Raw(b) = p {
                    s.raw(&b);
                }
            }
        }
        s.uleb(0); // end of the children of the subprogram
    }
    s.uleb(0); // end of the children of the compile unit

    // --- correct the length ------------------------------------------------
    let total = (s.len - after_length) as u32;
    if let Some(Piece::Raw(b)) = s.pieces.first_mut() {
        b[0..4].copy_from_slice(&total.to_le_bytes());
    }

    (
        abbrev_section().render(".debug_abbrev"),
        s.render(".debug_info"),
    )
}

/// Writes one type DIE. With `probe` only the length matters.
fn write_type(s: &mut Section, t: &DType, types: &Types, probe: bool) {
    let refoff = |x: &DType| -> u32 {
        if probe {
            0
        } else {
            types.offset(x)
        }
    };
    match t {
        DType::Base(n, size, enc) => {
            s.uleb(AB_BASE);
            s.text(n);
            s.u8(*enc);
            s.u8(*size as u8);
        }
        DType::Ptr(inner) => {
            s.uleb(AB_PTR);
            s.u8(8);
            s.u32(refoff(inner));
        }
        DType::Array(inner, n) => {
            s.uleb(AB_ARRAY);
            s.u32(refoff(inner));
            s.uleb(AB_SUBRANGE);
            s.u32(if *n == 0 { 0 } else { (*n - 1) as u32 });
            s.uleb(0);
        }
        DType::Struct(n, size, members) => {
            s.uleb(AB_STRUCT);
            s.text(n);
            s.u32(*size as u32);
            for (mn, mo, mt) in members {
                s.uleb(AB_MEMBER);
                s.text(mn);
                s.u32(refoff(mt));
                s.u32(*mo as u32);
            }
            s.uleb(0);
        }
        DType::Opaque(n, size) => {
            s.uleb(AB_OPAQUE);
            s.text(n);
            s.u32(*size as u32);
            // DW_AT_declaration: flag_present, no octets
        }
        DType::Void => {}
    }
}

// ------------------------------------------------- from Type to DType
//
// The debug information must not depend on the internals of the type
// checker, and it needs things the type checker does not have to know: the
// SIZE of everything and the OFFSET of every member. Both come from
// `TypeCtx`, and this is the only place where they meet.
pub fn dtype_of(tcx: &crate::types::TypeCtx, t: &crate::types::Type) -> DType {
    let mut open: Vec<usize> = Vec::new();
    dtype_rec(tcx, t, &mut open)
}

/// THE CYCLE. A `gc class Node { next: Gc[Node] }` points at itself, and the
/// straightforward recursion runs into the stack (round 64, found by
/// `tests/500_gc_grundlagen.fi` with `--no-opt`). `open` carries the structs
/// currently being unfolded; whoever meets himself again becomes an
/// INCOMPLETE type -- name and size, no members. That is what
/// `DW_AT_declaration` is for, and `gdb` joins it back to the complete DIE
/// of the same name.
fn dtype_rec(
    tcx: &crate::types::TypeCtx,
    t: &crate::types::Type,
    open: &mut Vec<usize>,
) -> DType {
    use crate::types::Type;
    match t {
        Type::I8 => DType::Base("i8".into(), 1, ATE_SIGNED),
        Type::I16 => DType::Base("i16".into(), 2, ATE_SIGNED),
        Type::I32 => DType::Base("i32".into(), 4, ATE_SIGNED),
        Type::I64 => DType::Base("i64".into(), 8, ATE_SIGNED),
        Type::Isize => DType::Base("isize".into(), 8, ATE_SIGNED),
        Type::U8 => DType::Base("u8".into(), 1, ATE_UNSIGNED),
        Type::U16 => DType::Base("u16".into(), 2, ATE_UNSIGNED),
        Type::U32 => DType::Base("u32".into(), 4, ATE_UNSIGNED),
        Type::U64 => DType::Base("u64".into(), 8, ATE_UNSIGNED),
        Type::Usize => DType::Base("usize".into(), 8, ATE_UNSIGNED),
        Type::Bool => DType::Base("bool".into(), 1, ATE_BOOLEAN),
        Type::F64 => DType::Base("f64".into(), 8, ATE_FLOAT),
        Type::F32 => DType::Base("f32".into(), 4, ATE_FLOAT),
        // An untyped literal never reaches storage; if it does, it is an
        // i32 -- the same rule the type checker uses.
        Type::UntypedInt => DType::Base("i32".into(), 4, ATE_SIGNED),
        Type::Ptr { inner, .. } => DType::Ptr(Box::new(dtype_rec(tcx, inner, open))),
        Type::Array(inner, n) => DType::Array(Box::new(dtype_rec(tcx, inner, open)), *n),
        Type::Struct(i) => match tcx.structs.get(*i) {
            Some(sd) => {
                if open.contains(i) || open.len() > 16 {
                    return DType::Opaque(sd.name.clone(), tcx.size_of(t).max(1));
                }
                open.push(*i);
                let members: Vec<(String, u64, DType)> = sd
                    .fields
                    .iter()
                    .map(|f| (f.name.clone(), f.offset, dtype_rec(tcx, &f.ty, open)))
                    .collect();
                open.pop();
                DType::Struct(sd.name.clone(), tcx.size_of(t), members)
            }
            None => DType::Opaque("<struct>".into(), 8),
        },
        Type::Void => DType::Void,
        // Everything the debugger cannot look into anyway: a function value
        // is a pointer to a record, an error union a tagged word. They get a
        // name and a size, no members -- a lie about the shape would be
        // worse than an honest "opaque".
        other => DType::Opaque(tcx.name_of(other), tcx.size_of(other).max(1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The encodings of LEB128 -- the one place where an off-by-one would
    /// destroy the whole section without any error message.
    #[test]
    fn leb128_encodings() {
        let mut s = Section::new();
        s.uleb(0);
        s.uleb(127);
        s.uleb(128);
        s.uleb(624485);
        s.sleb(0);
        s.sleb(-1);
        s.sleb(-8);
        s.sleb(-129);
        let mut got = Vec::new();
        for p in &s.pieces {
            if let Piece::Raw(b) = p {
                got.extend_from_slice(b);
            }
        }
        assert_eq!(
            got,
            vec![
                0x00, // 0
                0x7f, // 127
                0x80, 0x01, // 128
                0xe5, 0x8e, 0x26, // 624485
                0x00, // 0
                0x7f, // -1
                0x78, // -8
                0xff, 0x7e, // -129
            ]
        );
        assert_eq!(s.len, got.len());
    }

    /// Every type has to get an offset, and one that is really its own.
    #[test]
    fn types_get_their_own_offsets() {
        let i32t = DType::Base("i32".into(), 4, ATE_SIGNED);
        let p = DType::Ptr(Box::new(i32t.clone()));
        let f = FnInfo {
            name: "f".into(),
            start_label: "f".into(),
            end_label: ".Lend_f".into(),
            file: 0,
            line: 1,
            ret: i32t.clone(),
            vars: vec![(
                VarNote {
                    name: "p".into(),
                    val: 0,
                    ty: p.clone(),
                    file: 0,
                    line: 2,
                    param: true,
                },
                16,
            )],
        };
        let (abbrev, info) = build("firnc", "a.fi", "/tmp", "__start", ".Ltext_end", &[f]);
        assert!(abbrev.contains(".debug_abbrev"));
        assert!(info.contains(".debug_info"));
        assert!(info.contains(".quad f"));
        assert!(info.contains(".quad .Lend_f"));
    }
}

/// `DW_ATE_*` re-exported, so that `codegen_x86.rs` does not have to import
/// `dwarf.rs` a second time.
pub use crate::dwarf::{ATE_ADDRESS, ATE_BOOLEAN, ATE_FLOAT, ATE_SIGNED, ATE_UNSIGNED};
