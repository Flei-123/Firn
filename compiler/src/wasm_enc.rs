// SPDX-License-Identifier: MPL-2.0
//! **ROUND WASM — the WebAssembly module, in memory and on disk.**
//!
//! `codegen_wasm.rs` decides WHAT a Firn program becomes in WebAssembly;
//! this file only knows how a WebAssembly module is WRITTEN. It holds the
//! module as plain data (types, imports, functions, table, memory,
//! globals, exports, data) and turns it into two things:
//!
//!   * the binary format (`Module::to_binary`) — that is what a browser
//!     loads, and it is what `firnc --target=wasm32-browser` writes;
//!   * the text format (`Module::to_text`) — that is what
//!     `--emit=asm` writes, so that a person can read the result the same
//!     way `--emit=asm` shows the x86 assembler.
//!
//! ## Why the binary format directly, and not `.wat` plus `wat2wasm`
//!
//! The native backends write assembler TEXT and hand it to `as`/`ld`,
//! because those two tools belong to the machine: every Linux that runs a
//! Firn program has binutils. WebAssembly has no such platform tool —
//! `wat2wasm` is one project's converter (wabt), not part of any browser
//! and not installed where a browser is. Making it a build dependency would
//! add exactly the kind of foreign tool the compiler has avoided so far
//! (no crates, no LLVM). The binary format itself is small and fully
//! specified: LEB128 numbers, eleven section kinds, one byte per opcode.
//! Writing it here costs about as many lines as writing the text would.
//!
//! The text form is not thrown away, though. It is printed from the SAME
//! instruction list, and `tools/wasm/run.sh` feeds it to `wat2wasm` when
//! that tool is present and compares the result with our own binary, octet
//! for octet. An independent assembler that produces the same octets from
//! our text is the strongest check this encoder can get.
//!
//! Nothing in here knows about Firn. It is deliberately the dullest file of
//! the round.

use std::fmt::Write as _;

/// The four value types of WebAssembly 1.0. (`v128` is SIMD and is refused
/// by `codegen_wasm.rs` before it could ever get here.)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VT {
    I32,
    I64,
    F32,
    F64,
}

impl VT {
    pub fn byte(self) -> u8 {
        match self {
            VT::I32 => 0x7F,
            VT::I64 => 0x7E,
            VT::F32 => 0x7D,
            VT::F64 => 0x7C,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            VT::I32 => "i32",
            VT::I64 => "i64",
            VT::F32 => "f32",
            VT::F64 => "f64",
        }
    }
}

/// A function signature.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FuncType {
    pub params: Vec<VT>,
    pub results: Vec<VT>,
}

/// A numeric instruction without immediates: opcode and text name travel
/// together, so the binary and the text form can never disagree about which
/// instruction was meant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Num(pub u8, pub &'static str);

/// The instructions behind the `0xFC` prefix that this compiler uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NumFc(pub u32, pub &'static str);

/// A memory access: opcode, name, natural alignment (as a power of two).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mem(pub u8, pub &'static str, pub u32);

#[derive(Clone, Debug, PartialEq)]
pub enum Ins {
    Unreachable,
    /// `block` with the empty block type — every value in this compiler
    /// travels through locals, so no block ever yields one.
    Block,
    Loop,
    If,
    Else,
    End,
    Br(u32),
    BrIf(u32),
    BrTable(Vec<u32>, u32),
    Return,
    Call(u32),
    /// type index; the table is always table 0
    CallIndirect(u32),
    Drop,
    Select,
    LocalGet(u32),
    LocalSet(u32),
    LocalTee(u32),
    GlobalGet(u32),
    GlobalSet(u32),
    /// memory access with the offset immediate
    Load(Mem, u32),
    Store(Mem, u32),
    MemorySize,
    MemoryGrow,
    MemoryCopy,
    MemoryFill,
    I32Const(i32),
    I64Const(i64),
    /// the bit pattern
    F32Const(u32),
    /// the bit pattern
    F64Const(u64),
    Num(Num),
    Fc(NumFc),
}

// ------------------------------------------------------------- opcodes

pub const I32_EQZ: Num = Num(0x45, "i32.eqz");
pub const I32_EQ: Num = Num(0x46, "i32.eq");
pub const I32_NE: Num = Num(0x47, "i32.ne");
pub const I32_LT_S: Num = Num(0x48, "i32.lt_s");
pub const I32_LT_U: Num = Num(0x49, "i32.lt_u");
pub const I32_GT_S: Num = Num(0x4A, "i32.gt_s");
pub const I32_GT_U: Num = Num(0x4B, "i32.gt_u");
pub const I32_LE_S: Num = Num(0x4C, "i32.le_s");
pub const I32_LE_U: Num = Num(0x4D, "i32.le_u");
pub const I32_GE_S: Num = Num(0x4E, "i32.ge_s");
pub const I32_GE_U: Num = Num(0x4F, "i32.ge_u");
pub const I64_EQZ: Num = Num(0x50, "i64.eqz");
pub const I64_EQ: Num = Num(0x51, "i64.eq");
pub const I64_NE: Num = Num(0x52, "i64.ne");
pub const I64_LT_S: Num = Num(0x53, "i64.lt_s");
pub const I64_LT_U: Num = Num(0x54, "i64.lt_u");
pub const I64_GT_S: Num = Num(0x55, "i64.gt_s");
pub const I64_GT_U: Num = Num(0x56, "i64.gt_u");
pub const I64_LE_S: Num = Num(0x57, "i64.le_s");
pub const I64_LE_U: Num = Num(0x58, "i64.le_u");
pub const I64_GE_S: Num = Num(0x59, "i64.ge_s");
pub const I64_GE_U: Num = Num(0x5A, "i64.ge_u");
pub const F32_EQ: Num = Num(0x5B, "f32.eq");
pub const F32_NE: Num = Num(0x5C, "f32.ne");
pub const F32_LT: Num = Num(0x5D, "f32.lt");
pub const F32_GT: Num = Num(0x5E, "f32.gt");
pub const F32_LE: Num = Num(0x5F, "f32.le");
pub const F32_GE: Num = Num(0x60, "f32.ge");
pub const F64_EQ: Num = Num(0x61, "f64.eq");
pub const F64_NE: Num = Num(0x62, "f64.ne");
pub const F64_LT: Num = Num(0x63, "f64.lt");
pub const F64_GT: Num = Num(0x64, "f64.gt");
pub const F64_LE: Num = Num(0x65, "f64.le");
pub const F64_GE: Num = Num(0x66, "f64.ge");
pub const I32_ADD: Num = Num(0x6A, "i32.add");
pub const I32_SUB: Num = Num(0x6B, "i32.sub");
pub const I32_MUL: Num = Num(0x6C, "i32.mul");
pub const I32_DIV_S: Num = Num(0x6D, "i32.div_s");
pub const I32_DIV_U: Num = Num(0x6E, "i32.div_u");
pub const I32_REM_S: Num = Num(0x6F, "i32.rem_s");
pub const I32_REM_U: Num = Num(0x70, "i32.rem_u");
pub const I32_AND: Num = Num(0x71, "i32.and");
pub const I32_OR: Num = Num(0x72, "i32.or");
pub const I32_XOR: Num = Num(0x73, "i32.xor");
pub const I32_SHL: Num = Num(0x74, "i32.shl");
pub const I32_SHR_S: Num = Num(0x75, "i32.shr_s");
pub const I32_SHR_U: Num = Num(0x76, "i32.shr_u");
pub const I64_ADD: Num = Num(0x7C, "i64.add");
pub const I64_SUB: Num = Num(0x7D, "i64.sub");
pub const I64_MUL: Num = Num(0x7E, "i64.mul");
pub const I64_DIV_S: Num = Num(0x7F, "i64.div_s");
pub const I64_DIV_U: Num = Num(0x80, "i64.div_u");
pub const I64_REM_S: Num = Num(0x81, "i64.rem_s");
pub const I64_REM_U: Num = Num(0x82, "i64.rem_u");
pub const I64_AND: Num = Num(0x83, "i64.and");
pub const I64_OR: Num = Num(0x84, "i64.or");
pub const I64_XOR: Num = Num(0x85, "i64.xor");
pub const I64_SHL: Num = Num(0x86, "i64.shl");
pub const I64_SHR_S: Num = Num(0x87, "i64.shr_s");
pub const I64_SHR_U: Num = Num(0x88, "i64.shr_u");
pub const F32_NEG: Num = Num(0x8C, "f32.neg");
pub const F32_SQRT: Num = Num(0x91, "f32.sqrt");
pub const F32_ADD: Num = Num(0x92, "f32.add");
pub const F32_SUB: Num = Num(0x93, "f32.sub");
pub const F32_MUL: Num = Num(0x94, "f32.mul");
pub const F32_DIV: Num = Num(0x95, "f32.div");
pub const F64_NEG: Num = Num(0x9A, "f64.neg");
pub const F64_SQRT: Num = Num(0x9F, "f64.sqrt");
pub const F64_ADD: Num = Num(0xA0, "f64.add");
pub const F64_SUB: Num = Num(0xA1, "f64.sub");
pub const F64_MUL: Num = Num(0xA2, "f64.mul");
pub const F64_DIV: Num = Num(0xA3, "f64.div");
pub const I32_WRAP_I64: Num = Num(0xA7, "i32.wrap_i64");
pub const I64_EXTEND_I32_S: Num = Num(0xAC, "i64.extend_i32_s");
pub const I64_EXTEND_I32_U: Num = Num(0xAD, "i64.extend_i32_u");
pub const F32_CONVERT_I64_S: Num = Num(0xB4, "f32.convert_i64_s");
pub const F32_DEMOTE_F64: Num = Num(0xB6, "f32.demote_f64");
pub const F64_CONVERT_I64_S: Num = Num(0xB9, "f64.convert_i64_s");
pub const F64_PROMOTE_F32: Num = Num(0xBB, "f64.promote_f32");
pub const I32_REINTERPRET_F32: Num = Num(0xBC, "i32.reinterpret_f32");
pub const I64_REINTERPRET_F64: Num = Num(0xBD, "i64.reinterpret_f64");
pub const F32_REINTERPRET_I32: Num = Num(0xBE, "f32.reinterpret_i32");
pub const F64_REINTERPRET_I64: Num = Num(0xBF, "f64.reinterpret_i64");
pub const I32_EXTEND8_S: Num = Num(0xC0, "i32.extend8_s");
pub const I32_EXTEND16_S: Num = Num(0xC1, "i32.extend16_s");
pub const I64_EXTEND8_S: Num = Num(0xC2, "i64.extend8_s");
pub const I64_EXTEND16_S: Num = Num(0xC3, "i64.extend16_s");
pub const I64_EXTEND32_S: Num = Num(0xC4, "i64.extend32_s");

pub const I64_TRUNC_SAT_F32_S: NumFc = NumFc(4, "i64.trunc_sat_f32_s");
pub const I64_TRUNC_SAT_F64_S: NumFc = NumFc(6, "i64.trunc_sat_f64_s");

pub const I32_LOAD: Mem = Mem(0x28, "i32.load", 2);
pub const I64_LOAD: Mem = Mem(0x29, "i64.load", 3);
pub const F32_LOAD: Mem = Mem(0x2A, "f32.load", 2);
pub const F64_LOAD: Mem = Mem(0x2B, "f64.load", 3);
pub const I32_LOAD8_S: Mem = Mem(0x2C, "i32.load8_s", 0);
pub const I32_LOAD8_U: Mem = Mem(0x2D, "i32.load8_u", 0);
pub const I32_LOAD16_S: Mem = Mem(0x2E, "i32.load16_s", 1);
pub const I32_LOAD16_U: Mem = Mem(0x2F, "i32.load16_u", 1);
pub const I32_STORE: Mem = Mem(0x36, "i32.store", 2);
pub const I64_STORE: Mem = Mem(0x37, "i64.store", 3);
pub const F32_STORE: Mem = Mem(0x38, "f32.store", 2);
pub const F64_STORE: Mem = Mem(0x39, "f64.store", 3);
pub const I32_STORE8: Mem = Mem(0x3A, "i32.store8", 0);
pub const I32_STORE16: Mem = Mem(0x3B, "i32.store16", 1);

// ------------------------------------------------------------- the module

pub struct Import {
    pub module: String,
    pub name: String,
    pub ty: u32,
    /// the name the text form gives the function (`$...`)
    pub sym: String,
}

pub struct Func {
    pub ty: u32,
    /// the locals AFTER the parameters, in index order
    pub locals: Vec<VT>,
    pub body: Vec<Ins>,
    /// the name the text form and the name section give the function
    pub sym: String,
}

pub struct Global {
    pub vt: VT,
    pub mutable: bool,
    pub init: i64,
    pub sym: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ExportKind {
    Func,
    Memory,
}

pub struct Export {
    pub name: String,
    pub kind: ExportKind,
    pub index: u32,
}

#[derive(Default)]
pub struct Module {
    pub types: Vec<FuncType>,
    pub imports: Vec<Import>,
    pub funcs: Vec<Func>,
    /// function indices in the table, starting at table index `table_base`
    pub table: Vec<u32>,
    /// table index of the first entry of `table`; everything below is null
    pub table_base: u32,
    pub memory_pages: u32,
    pub globals: Vec<Global>,
    pub exports: Vec<Export>,
    /// active data segments: (address, octets)
    pub data: Vec<(u32, Vec<u8>)>,
    /// write the `name` custom section (function names for stack traces)
    pub names: bool,
}

impl Module {
    /// The index of a signature, adding it if it is new. Signatures are
    /// shared: `call_indirect` compares them STRUCTURALLY anyway, and one
    /// entry per shape keeps the type section short.
    pub fn type_index(&mut self, t: FuncType) -> u32 {
        if let Some(i) = self.types.iter().position(|x| *x == t) {
            return i as u32;
        }
        self.types.push(t);
        (self.types.len() - 1) as u32
    }

    /// The function index of the `k`-th defined function.
    pub fn func_index(&self, k: usize) -> u32 {
        (self.imports.len() + k) as u32
    }

    fn table_size(&self) -> u32 {
        self.table_base + self.table.len() as u32
    }

    // ------------------------------------------------------ binary format

    pub fn to_binary(&self) -> Vec<u8> {
        let mut out: Vec<u8> = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        // 1 type
        let mut s = Vec::new();
        uleb(&mut s, self.types.len() as u64);
        for t in &self.types {
            s.push(0x60);
            uleb(&mut s, t.params.len() as u64);
            for p in &t.params {
                s.push(p.byte());
            }
            uleb(&mut s, t.results.len() as u64);
            for r in &t.results {
                s.push(r.byte());
            }
        }
        section(&mut out, 1, &s);
        // 2 import
        if !self.imports.is_empty() {
            let mut s = Vec::new();
            uleb(&mut s, self.imports.len() as u64);
            for im in &self.imports {
                name(&mut s, &im.module);
                name(&mut s, &im.name);
                s.push(0x00);
                uleb(&mut s, im.ty as u64);
            }
            section(&mut out, 2, &s);
        }
        // 3 function
        let mut s = Vec::new();
        uleb(&mut s, self.funcs.len() as u64);
        for f in &self.funcs {
            uleb(&mut s, f.ty as u64);
        }
        section(&mut out, 3, &s);
        // 4 table
        let mut s = Vec::new();
        uleb(&mut s, 1);
        s.push(0x70);
        s.push(0x00);
        uleb(&mut s, self.table_size() as u64);
        section(&mut out, 4, &s);
        // 5 memory
        let mut s = Vec::new();
        uleb(&mut s, 1);
        s.push(0x00);
        uleb(&mut s, self.memory_pages as u64);
        section(&mut out, 5, &s);
        // 6 global
        if !self.globals.is_empty() {
            let mut s = Vec::new();
            uleb(&mut s, self.globals.len() as u64);
            for g in &self.globals {
                s.push(g.vt.byte());
                s.push(if g.mutable { 0x01 } else { 0x00 });
                match g.vt {
                    VT::I64 => {
                        s.push(0x42);
                        sleb(&mut s, g.init);
                    }
                    _ => {
                        s.push(0x41);
                        sleb(&mut s, g.init as i32 as i64);
                    }
                }
                s.push(0x0B);
            }
            section(&mut out, 6, &s);
        }
        // 7 export
        let mut s = Vec::new();
        uleb(&mut s, self.exports.len() as u64);
        for e in &self.exports {
            name(&mut s, &e.name);
            s.push(match e.kind {
                ExportKind::Func => 0x00,
                ExportKind::Memory => 0x02,
            });
            uleb(&mut s, e.index as u64);
        }
        section(&mut out, 7, &s);
        // 9 element
        if !self.table.is_empty() {
            let mut s = Vec::new();
            uleb(&mut s, 1);
            s.push(0x00);
            s.push(0x41);
            sleb(&mut s, self.table_base as i64);
            s.push(0x0B);
            uleb(&mut s, self.table.len() as u64);
            for f in &self.table {
                uleb(&mut s, *f as u64);
            }
            section(&mut out, 9, &s);
        }
        // 10 code
        let mut s = Vec::new();
        uleb(&mut s, self.funcs.len() as u64);
        for f in &self.funcs {
            let mut b = Vec::new();
            let runs = local_runs(&f.locals);
            uleb(&mut b, runs.len() as u64);
            for (n, t) in &runs {
                uleb(&mut b, *n as u64);
                b.push(t.byte());
            }
            for i in &f.body {
                encode(&mut b, i);
            }
            b.push(0x0B);
            uleb(&mut s, b.len() as u64);
            s.extend_from_slice(&b);
        }
        section(&mut out, 10, &s);
        // 11 data
        if !self.data.is_empty() {
            let mut s = Vec::new();
            uleb(&mut s, self.data.len() as u64);
            for (addr, bytes) in &self.data {
                s.push(0x00);
                s.push(0x41);
                sleb(&mut s, *addr as i32 as i64);
                s.push(0x0B);
                uleb(&mut s, bytes.len() as u64);
                s.extend_from_slice(bytes);
            }
            section(&mut out, 11, &s);
        }
        // custom "name": function names, so that a trap in the browser
        // names the Firn function it happened in.
        if self.names {
            let mut sub = Vec::new();
            let n = self.imports.len() + self.funcs.len();
            uleb(&mut sub, n as u64);
            for (i, im) in self.imports.iter().enumerate() {
                uleb(&mut sub, i as u64);
                name(&mut sub, &im.sym);
            }
            for (k, f) in self.funcs.iter().enumerate() {
                uleb(&mut sub, (self.imports.len() + k) as u64);
                name(&mut sub, &f.sym);
            }
            let mut s = Vec::new();
            name(&mut s, "name");
            s.push(1);
            uleb(&mut s, sub.len() as u64);
            s.extend_from_slice(&sub);
            section(&mut out, 0, &s);
        }
        out
    }

    // --------------------------------------------------------- text format

    /// The text format. Flat instructions, one per line, indented by the
    /// nesting depth — the same shape `wasm2wat` prints, so the two can be
    /// compared by eye.
    pub fn to_text(&self) -> String {
        let mut o = String::new();
        let _ = writeln!(o, "(module");
        for (i, t) in self.types.iter().enumerate() {
            let _ = write!(o, "  (type (;{};) (func", i);
            if !t.params.is_empty() {
                let _ = write!(o, " (param");
                for p in &t.params {
                    let _ = write!(o, " {}", p.name());
                }
                let _ = write!(o, ")");
            }
            if !t.results.is_empty() {
                let _ = write!(o, " (result");
                for r in &t.results {
                    let _ = write!(o, " {}", r.name());
                }
                let _ = write!(o, ")");
            }
            let _ = writeln!(o, "))");
        }
        for im in &self.imports {
            let _ = writeln!(
                o,
                "  (import \"{}\" \"{}\" (func ${} (type {})))",
                im.module, im.name, im.sym, im.ty
            );
        }
        let fsym = |idx: u32| -> String {
            let i = idx as usize;
            if i < self.imports.len() {
                self.imports[i].sym.clone()
            } else {
                self.funcs[i - self.imports.len()].sym.clone()
            }
        };
        for f in &self.funcs {
            let _ = writeln!(o, "  (func ${} (type {})", f.sym, f.ty);
            if !f.locals.is_empty() {
                let _ = write!(o, "    (local");
                for l in &f.locals {
                    let _ = write!(o, " {}", l.name());
                }
                let _ = writeln!(o, ")");
            }
            let mut depth = 2usize;
            for i in &f.body {
                if matches!(i, Ins::End | Ins::Else) && depth > 2 {
                    depth -= 1;
                }
                let _ = writeln!(o, "{}{}", "  ".repeat(depth), text_of(i, &self.globals, &fsym));
                if matches!(i, Ins::Block | Ins::Loop | Ins::If | Ins::Else) {
                    depth += 1;
                }
            }
            let _ = writeln!(o, "  )");
        }
        let _ = writeln!(o, "  (table (;0;) {} funcref)", self.table_size());
        let _ = writeln!(o, "  (memory (;0;) {})", self.memory_pages);
        for g in &self.globals {
            let init = match g.vt {
                VT::I64 => format!("(i64.const {})", g.init),
                _ => format!("(i32.const {})", g.init as i32),
            };
            if g.mutable {
                let _ = writeln!(o, "  (global ${} (mut {}) {})", g.sym, g.vt.name(), init);
            } else {
                let _ = writeln!(o, "  (global ${} {} {})", g.sym, g.vt.name(), init);
            }
        }
        for e in &self.exports {
            let what = match e.kind {
                ExportKind::Func => format!("(func ${})", fsym(e.index)),
                ExportKind::Memory => "(memory 0)".to_string(),
            };
            let _ = writeln!(o, "  (export \"{}\" {})", e.name, what);
        }
        if !self.table.is_empty() {
            let _ = write!(o, "  (elem (i32.const {}) func", self.table_base);
            for f in &self.table {
                let _ = write!(o, " ${}", fsym(*f));
            }
            let _ = writeln!(o, ")");
        }
        for (addr, bytes) in &self.data {
            let _ = write!(o, "  (data (i32.const {}) \"", *addr as i32);
            for b in bytes {
                let c = *b;
                if (0x20..0x7F).contains(&c) && c != b'"' && c != b'\\' {
                    o.push(c as char);
                } else {
                    let _ = write!(o, "\\{:02x}", c);
                }
            }
            let _ = writeln!(o, "\")");
        }
        let _ = writeln!(o, ")");
        o
    }
}

/// Consecutive locals of the same type form one entry of the local
/// declaration — `wat2wasm` groups them exactly this way, which is what
/// makes the octet comparison with it possible at all.
fn local_runs(ls: &[VT]) -> Vec<(u32, VT)> {
    let mut runs: Vec<(u32, VT)> = Vec::new();
    for l in ls {
        match runs.last_mut() {
            Some((n, t)) if *t == *l => *n += 1,
            _ => runs.push((1, *l)),
        }
    }
    runs
}

fn text_of(i: &Ins, globals: &[Global], fsym: &dyn Fn(u32) -> String) -> String {
    match i {
        Ins::Unreachable => "unreachable".into(),
        Ins::Block => "block".into(),
        Ins::Loop => "loop".into(),
        Ins::If => "if".into(),
        Ins::Else => "else".into(),
        Ins::End => "end".into(),
        Ins::Br(l) => format!("br {}", l),
        Ins::BrIf(l) => format!("br_if {}", l),
        Ins::BrTable(ls, d) => {
            let mut s = "br_table".to_string();
            for l in ls {
                let _ = write!(s, " {}", l);
            }
            let _ = write!(s, " {}", d);
            s
        }
        Ins::Return => "return".into(),
        Ins::Call(f) => format!("call ${}", fsym(*f)),
        Ins::CallIndirect(t) => format!("call_indirect (type {})", t),
        Ins::Drop => "drop".into(),
        Ins::Select => "select".into(),
        Ins::LocalGet(l) => format!("local.get {}", l),
        Ins::LocalSet(l) => format!("local.set {}", l),
        Ins::LocalTee(l) => format!("local.tee {}", l),
        Ins::GlobalGet(g) => format!("global.get ${}", globals[*g as usize].sym),
        Ins::GlobalSet(g) => format!("global.set ${}", globals[*g as usize].sym),
        Ins::Load(m, off) | Ins::Store(m, off) => {
            if *off == 0 {
                m.1.to_string()
            } else {
                format!("{} offset={}", m.1, off)
            }
        }
        Ins::MemorySize => "memory.size".into(),
        Ins::MemoryGrow => "memory.grow".into(),
        Ins::MemoryCopy => "memory.copy".into(),
        Ins::MemoryFill => "memory.fill".into(),
        Ins::I32Const(v) => format!("i32.const {}", v),
        Ins::I64Const(v) => format!("i64.const {}", v),
        // The text form writes a float constant as its exact bit pattern in
        // hexadecimal float notation, so that no decimal rounding can creep
        // in between our binary and `wat2wasm`'s.
        Ins::F32Const(b) => format!("f32.const {}", hexfloat32(*b)),
        Ins::F64Const(b) => format!("f64.const {}", hexfloat64(*b)),
        Ins::Num(n) => n.1.to_string(),
        Ins::Fc(n) => n.1.to_string(),
    }
}

/// `f64` bit pattern -> exact WAT spelling (hex float, `nan:0x...`, `inf`).
fn hexfloat64(b: u64) -> String {
    let sign = if b >> 63 != 0 { "-" } else { "" };
    let exp = ((b >> 52) & 0x7FF) as i64;
    let man = b & 0x000F_FFFF_FFFF_FFFF;
    if exp == 0x7FF {
        if man == 0 {
            return format!("{}inf", sign);
        }
        return format!("{}nan:0x{:x}", sign, man);
    }
    if exp == 0 {
        if man == 0 {
            return format!("{}0x0p+0", sign);
        }
        return format!("{}0x0.{:013x}p-1022", sign, man);
    }
    format!("{}0x1.{:013x}p{:+}", sign, man, exp - 1023)
}

/// `f32` bit pattern -> exact WAT spelling.
fn hexfloat32(b: u32) -> String {
    let sign = if b >> 31 != 0 { "-" } else { "" };
    let exp = ((b >> 23) & 0xFF) as i64;
    let man = b & 0x007F_FFFF;
    if exp == 0xFF {
        if man == 0 {
            return format!("{}inf", sign);
        }
        return format!("{}nan:0x{:x}", sign, man);
    }
    // 23 mantissa bits, shifted up by one to make six whole hex digits.
    if exp == 0 {
        if man == 0 {
            return format!("{}0x0p+0", sign);
        }
        return format!("{}0x0.{:06x}p-126", sign, man << 1);
    }
    format!("{}0x1.{:06x}p{:+}", sign, man << 1, exp - 127)
}

// ----------------------------------------------------------- encoding

pub fn uleb(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let byte = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

pub fn sleb(out: &mut Vec<u8>, mut v: i64) {
    loop {
        let byte = (v & 0x7F) as u8;
        v >>= 7;
        let done = (v == 0 && byte & 0x40 == 0) || (v == -1 && byte & 0x40 != 0);
        if done {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn name(out: &mut Vec<u8>, s: &str) {
    uleb(out, s.len() as u64);
    out.extend_from_slice(s.as_bytes());
}

fn section(out: &mut Vec<u8>, id: u8, body: &[u8]) {
    out.push(id);
    uleb(out, body.len() as u64);
    out.extend_from_slice(body);
}

fn encode(b: &mut Vec<u8>, i: &Ins) {
    match i {
        Ins::Unreachable => b.push(0x00),
        Ins::Block => b.extend_from_slice(&[0x02, 0x40]),
        Ins::Loop => b.extend_from_slice(&[0x03, 0x40]),
        Ins::If => b.extend_from_slice(&[0x04, 0x40]),
        Ins::Else => b.push(0x05),
        Ins::End => b.push(0x0B),
        Ins::Br(l) => {
            b.push(0x0C);
            uleb(b, *l as u64);
        }
        Ins::BrIf(l) => {
            b.push(0x0D);
            uleb(b, *l as u64);
        }
        Ins::BrTable(ls, d) => {
            b.push(0x0E);
            uleb(b, ls.len() as u64);
            for l in ls {
                uleb(b, *l as u64);
            }
            uleb(b, *d as u64);
        }
        Ins::Return => b.push(0x0F),
        Ins::Call(f) => {
            b.push(0x10);
            uleb(b, *f as u64);
        }
        Ins::CallIndirect(t) => {
            b.push(0x11);
            uleb(b, *t as u64);
            b.push(0x00);
        }
        Ins::Drop => b.push(0x1A),
        Ins::Select => b.push(0x1B),
        Ins::LocalGet(l) => {
            b.push(0x20);
            uleb(b, *l as u64);
        }
        Ins::LocalSet(l) => {
            b.push(0x21);
            uleb(b, *l as u64);
        }
        Ins::LocalTee(l) => {
            b.push(0x22);
            uleb(b, *l as u64);
        }
        Ins::GlobalGet(g) => {
            b.push(0x23);
            uleb(b, *g as u64);
        }
        Ins::GlobalSet(g) => {
            b.push(0x24);
            uleb(b, *g as u64);
        }
        Ins::Load(m, off) | Ins::Store(m, off) => {
            b.push(m.0);
            uleb(b, m.2 as u64);
            uleb(b, *off as u64);
        }
        Ins::MemorySize => b.extend_from_slice(&[0x3F, 0x00]),
        Ins::MemoryGrow => b.extend_from_slice(&[0x40, 0x00]),
        Ins::MemoryCopy => {
            b.push(0xFC);
            uleb(b, 10);
            b.extend_from_slice(&[0x00, 0x00]);
        }
        Ins::MemoryFill => {
            b.push(0xFC);
            uleb(b, 11);
            b.push(0x00);
        }
        Ins::I32Const(v) => {
            b.push(0x41);
            sleb(b, *v as i64);
        }
        Ins::I64Const(v) => {
            b.push(0x42);
            sleb(b, *v);
        }
        Ins::F32Const(bits) => {
            b.push(0x43);
            b.extend_from_slice(&bits.to_le_bytes());
        }
        Ins::F64Const(bits) => {
            b.push(0x44);
            b.extend_from_slice(&bits.to_le_bytes());
        }
        Ins::Num(n) => b.push(n.0),
        Ins::Fc(n) => {
            b.push(0xFC);
            uleb(b, n.0 as u64);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leb128_both_ways_round() {
        let mut v = Vec::new();
        uleb(&mut v, 624485);
        assert_eq!(v, vec![0xE5, 0x8E, 0x26]);
        let mut v = Vec::new();
        sleb(&mut v, -123456);
        assert_eq!(v, vec![0xC0, 0xBB, 0x78]);
        let mut v = Vec::new();
        sleb(&mut v, 63);
        assert_eq!(v, vec![0x3F]);
        let mut v = Vec::new();
        sleb(&mut v, 64);
        assert_eq!(v, vec![0xC0, 0x00]);
        let mut v = Vec::new();
        sleb(&mut v, -64);
        assert_eq!(v, vec![0x40]);
    }

    #[test]
    fn hex_floats_are_exact() {
        assert_eq!(hexfloat64(1.0f64.to_bits()), "0x1.0000000000000p+0");
        assert_eq!(hexfloat64((-2.5f64).to_bits()), "-0x1.4000000000000p+1");
        assert_eq!(hexfloat64(0.0f64.to_bits()), "0x0p+0");
        assert_eq!(hexfloat32(1.0f32.to_bits()), "0x1.000000p+0");
        assert_eq!(hexfloat32(0.5f32.to_bits()), "0x1.000000p-1");
    }

    #[test]
    fn the_smallest_module_has_the_magic_number() {
        let mut m = Module::default();
        let t = m.type_index(FuncType { params: vec![], results: vec![VT::I32] });
        m.funcs.push(Func { ty: t, locals: vec![], body: vec![Ins::I32Const(7)], sym: "f".into() });
        m.memory_pages = 1;
        let b = m.to_binary();
        assert_eq!(&b[0..8], &[0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00]);
    }
}
