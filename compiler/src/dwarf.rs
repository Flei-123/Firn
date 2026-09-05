// SPDX-License-Identifier: GPL-2.0-only
//! DWARF basics: line numbers (`.debug_line`) for the debugger.
//!
//! FIR carries no source positions (`fir.rs` is frozen). That is why lowering
//! collects the mapping *instruction -> source line* here in a table that the
//! code generator queries while writing the assembler. The line numbers are
//! emitted as `.file`/`.loc` directives; from those `as` produces the sections
//! `.debug_line`, `.debug_info` and `.debug_abbrev`.
//!
//! Accuracy:
//!   * **always**: line of the `fn` declaration (a breakpoint on a function
//!     shows the right `.fi` file and line)
//!   * **without the optimizer**: additionally instruction-exact lines. With
//!     the optimizer they are suppressed, because the optimizer removes and
//!     moves instructions and renumbers blocks — wrong lines would be worse
//!     than none.
//!
//! ROUND 64 — `.debug_info` OF OUR OWN. Up to now the assembler wrote the
//! sections; from `.file`/`.loc` it can only produce lines, no names, no
//! types, no variables. `gdb` therefore could not `print` anything. Now the
//! compiler writes `.debug_abbrev` and `.debug_info` ITSELF (DWARF 4), with
//!
//!   * one `DW_TAG_subprogram` per function, with the address range,
//!     the return type and `DW_AT_frame_base = DW_OP_reg6` (rbp)
//!   * `DW_TAG_formal_parameter` and `DW_TAG_variable` for every declared
//!     name, with `DW_AT_location = DW_OP_fbreg <offset>` — the frame
//!     offsets come out of `codegen_x86.rs::Frame`
//!   * a type graph out of `DW_TAG_base_type`, `DW_TAG_pointer_type`,
//!     `DW_TAG_array_type` and `DW_TAG_structure_type` with members
//!
//! The line table stays with the assembler: it can already do that and it
//! knows the addresses. `DW_AT_stmt_list` therefore points at offset 0 of
//! `.debug_line` — there is exactly one line program per object file.
//!
//! VARIABLES ONLY WITHOUT THE OPTIMIZER, for the same reason as the lines:
//! `mem2reg` pulls an `alloca` into a register, and then the frame offset
//! recorded here points at storage that is no longer written to. A wrong
//! value in the debugger is worse than none, so `--no-opt` is the condition
//! (`ACCEPTANCE.md`, `docs/DEBUGGER.md`).

use std::collections::HashMap;
use std::sync::Mutex;

/// A type, reduced to what DWARF needs. Deliberately its own enum and not
/// `types::Type`: the debug information must not depend on the internals of
/// the type checker, and a struct has to carry its members WITH offsets.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DType {
    /// `DW_TAG_base_type`: name, size in octets, `DW_ATE_*`.
    Base(String, u64, u8),
    Ptr(Box<DType>),
    Array(Box<DType>, u64),
    /// name, size, members (name, offset, type)
    Struct(String, u64, Vec<(String, u64, DType)>),
    /// Something the debugger cannot look into (function value, error union):
    /// name and size, no members.
    Opaque(String, u64),
    Void,
}

/// `DW_ATE_*` (DWARF 4, table 7.11)
pub const ATE_ADDRESS: u8 = 0x01;
pub const ATE_BOOLEAN: u8 = 0x02;
pub const ATE_FLOAT: u8 = 0x04;
pub const ATE_SIGNED: u8 = 0x05;
pub const ATE_UNSIGNED: u8 = 0x07;

/// One declared name inside a function.
#[derive(Clone, Debug)]
pub struct VarNote {
    pub name: String,
    /// FIR value of the `alloca` that holds the storage.
    pub val: u32,
    pub ty: DType,
    pub file: u32,
    pub line: u32,
    pub param: bool,
}

#[derive(Default)]
struct FuncLines {
    /// Position of the `fn` line: (file number, line)
    decl: Option<(u32, u32)>,
    /// Round 64: the declared names, in the order of their declaration.
    vars: Vec<VarNote>,
    /// Round 64: the result type of the function.
    ret: Option<DType>,
}

#[derive(Default)]
struct Table {
    /// Source files ordered by their numbers (0-based).
    files: Vec<String>,
    funcs: HashMap<String, FuncLines>,
    /// Emit VARIABLE information (names, types, frame offsets)? Since round
    /// 94 the line table no longer hangs on this flag -- lines travel on the
    /// instructions themselves (`fir::Loc`) and are therefore right at every
    /// build level. Variables still need the frame, so they stay tied to
    /// `--no-opt`.
    variables: bool,
}

static TABLE: Mutex<Option<Table>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Table) -> R) -> R {
    // A poisoned mutex is possible only after a panic; the inner value then
    // stays usable rather than triggering a second panic.
    let mut guard = match TABLE.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    let t = guard.get_or_insert_with(Table::default);
    f(t)
}

/// Resets the table and enters the source files.
pub fn reset(files: Vec<String>, variables: bool) {
    with(|t| {
        t.files = files.clone();
        t.funcs.clear();
        t.variables = variables;
    });
}

/// Appends one more source file — needed for the source text produced by
/// `comptime`, which appears only after `reset`. The order must agree with
/// `Diags::add_file`, otherwise the `.loc` directives point to a number that
/// `as` does not know.
pub fn add_file(name: &str) {
    with(|t| {
        if !t.files.is_empty() {
            t.files.push(name.to_string());
        }
    });
}

/// Source files ordered by their numbers; empty = no debug info.
pub fn files() -> Vec<String> {
    with(|t| t.files.clone())
}

/// Remember the position of the `fn` declaration.
pub fn set_fn(name: &str, file: u32, line: u32) {
    if line == 0 {
        return;
    }
    with(|t| {
        t.funcs.entry(name.to_string()).or_default().decl = Some((file, line));
    });
}

/// Line of the `fn` declaration.
pub fn fn_line(name: &str) -> Option<(u32, u32)> {
    with(|t| t.funcs.get(name).and_then(|f| f.decl))
}

/// Round 64: records a declared name. `lower.rs::declare_ty` is the ONE
/// place where a source name is bound to storage — everything the debugger
/// shows comes from there.
pub fn declare_var(name: &str, var: &str, val: u32, ty: DType, file: u32, line: u32, param: bool) {
    with(|t| {
        if !t.variables {
            return;
        }
        let f = t.funcs.entry(name.to_string()).or_default();
        // A name may be declared twice (an inner scope shadows an outer
        // one). The debugger gets both; the one that comes later wins in
        // gdb, which matches the language.
        f.vars.push(VarNote {
            name: var.to_string(),
            val,
            ty,
            file,
            line,
            param,
        });
    });
}

/// Round 64: the result type of a function.
pub fn set_fn_type(name: &str, ret: DType) {
    with(|t| {
        if !t.variables {
            return;
        }
        t.funcs.entry(name.to_string()).or_default().ret = Some(ret);
    });
}

/// Round 64: the declared names of a function, in declaration order.
pub fn vars_of(name: &str) -> Vec<VarNote> {
    with(|t| t.funcs.get(name).map(|f| f.vars.clone()).unwrap_or_default())
}

/// Round 64: the result type of a function.
pub fn ret_of(name: &str) -> Option<DType> {
    with(|t| t.funcs.get(name).and_then(|f| f.ret.clone()))
}

/// Round 64: is debug information for variables being produced at all?
pub fn with_variables() -> bool {
    // ROUND WINDOWS: `.debug_info`/`.debug_abbrev` are emitted with ELF
    // section flags (`dwarf_info.rs`: `,"",@progbits`), which the COFF
    // assembler does not take. Debug information for the Windows target is
    // therefore OFF, and that is written down as an open point rather than
    // patched around here.
    if crate::target::windows() {
        return false;
    }
    with(|t| t.variables && !t.files.is_empty())
}

/// ROUND 94: is a line table being produced at all? True as soon as there
/// are source files -- at EVERY build level, because the positions sit on
/// the instructions and survive the optimizer (`fir::Loc`).
pub fn with_lines() -> bool {
    if crate::target::windows() {
        return false;
    }
    with(|t| !t.files.is_empty())
}

/// `.file` directives for all source files (numbers are 1-based).
pub fn file_directives() -> String {
    let mut out = String::new();
    if crate::target::windows() {
        return out;
    }
    for (i, f) in files().iter().enumerate() {
        out.push_str(&format!(".file {} \"{}\"\n", i + 1, f.replace('"', "\\\"")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table is global state — hence ONE test that checks all of it
    /// (parallel tests would otherwise reset each other).
    ///
    /// ROUND 94: the line NOTES are gone from here; they sit on the
    /// instructions (`fir::Loc`) and are tested in `fir.rs`. What is left in
    /// this table is what really is per function and not per instruction: the
    /// `fn` line, the declared names, the file list.
    #[test]
    fn the_table_carries_function_and_variables() {
        reset(vec!["a.fi".to_string()], true);
        set_fn("f", 0, 3);
        assert_eq!(fn_line("f"), Some((0, 3)));
        assert!(with_variables());
        assert!(with_lines());
        declare_var("f", "x", 7, DType::Base("i32".into(), 4, ATE_SIGNED), 0, 4, false);
        assert_eq!(vars_of("f").len(), 1);
        assert!(file_directives().contains(".file 1 \"a.fi\""));

        // Without variable information the names stay away -- the lines do not.
        reset(vec!["a.fi".to_string()], false);
        set_fn("g", 0, 9);
        declare_var("g", "y", 1, DType::Void, 0, 9, false);
        assert!(vars_of("g").is_empty());
        assert!(!with_variables());
        assert!(with_lines());
        assert_eq!(fn_line("g"), Some((0, 9)));

        // No files at all = no debug information at all.
        reset(Vec::new(), false);
        assert!(!with_lines());
    }
}
