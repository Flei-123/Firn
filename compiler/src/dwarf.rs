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
    /// (block, index of the instruction in the block) -> (file number, line)
    notes: HashMap<(u32, u32), (u32, u32)>,
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
    /// Emit instruction-exact lines?
    statements: bool,
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
pub fn reset(files: Vec<String>, statements: bool) {
    with(|t| {
        t.files = files.clone();
        t.funcs.clear();
        t.statements = statements;
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

/// Remember the source line of instruction `idx` in block `block`.
pub fn note(name: &str, block: u32, idx: u32, file: u32, line: u32) {
    if line == 0 {
        return;
    }
    with(|t| {
        if !t.statements {
            return;
        }
        t.funcs
            .entry(name.to_string())
            .or_default()
            .notes
            .entry((block, idx))
            .or_insert((file, line));
    });
}

/// An `alloca` was INSERTED into block `block` at position `at`: every
/// note from that position onwards slides one step back.
pub fn shift_after_insert(name: &str, block: u32, at: u32) {
    with(|t| {
        if !t.statements {
            return;
        }
        if let Some(f) = t.funcs.get_mut(name) {
            let old = std::mem::take(&mut f.notes);
            for ((b, i), v) in old {
                let i2 = if b == block && i >= at { i + 1 } else { i };
                f.notes.insert((b, i2), v);
            }
        }
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
        if !t.statements {
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
        if !t.statements {
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
    with(|t| t.statements && !t.files.is_empty())
}

/// Line of instruction `idx` in block `block`, if noted.
pub fn line_at(name: &str, block: u32, idx: u32) -> Option<(u32, u32)> {
    with(|t| {
        t.funcs
            .get(name)
            .and_then(|f| f.notes.get(&(block, idx)).copied())
    })
}

/// `.file` directives for all source files (numbers are 1-based).
pub fn file_directives() -> String {
    let mut out = String::new();
    for (i, f) in files().iter().enumerate() {
        out.push_str(&format!(".file {} \"{}\"\n", i + 1, f.replace('"', "\\\"")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table is global state — hence ONE test that checks both
    /// (parallel tests would otherwise reset each other).
    #[test]
    fn notes_move_itself_and_let_itself_disable() {
        reset(vec!["a.fi".to_string()], true);
        set_fn("f", 0, 3);
        note("f", 0, 2, 0, 10);
        shift_after_insert("f", 0, 1);
        assert_eq!(line_at("f", 0, 3), Some((0, 10)));
        assert_eq!(line_at("f", 0, 2), None);
        assert_eq!(fn_line("f"), Some((0, 3)));
        assert!(file_directives().contains(".file 1 \"a.fi\""));

        reset(vec!["a.fi".to_string()], false);
        note("g", 0, 0, 0, 7);
        assert_eq!(line_at("g", 0, 0), None);
        reset(Vec::new(), false);
    }
}
