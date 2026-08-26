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

/// **ROUND 96** — WHERE a variable lies once the optimizer has had it.
///
/// Round 64 knew exactly one answer, `rbp - off`, and it was only ever true
/// without the optimizer — which is why variable information was tied to
/// `--no-opt` at all. The three answers here are the ones that can be GIVEN
/// TRUTHFULLY, and everything that cannot be answered truthfully gets no
/// entry at all: a wrong value in a debugger is worse than none
/// (`docs/DEBUGGER.md`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VarPlace {
    /// `rbp - off`, for the whole function: the storage is still there.
    Frame(u64),
    /// A machine register, for the whole function: the register allocator
    /// keeps this cell there and never moves it (`regalloc::Alloc::cells`).
    Reg(&'static str),
    /// A LOCATION LIST (`.debug_loc`): the value comes into being at the
    /// label and lives from there to the end of the function. Before the
    /// label the debugger says "optimized out" instead of reading a register
    /// that still holds something else.
    ListFrom(String, Box<VarPlace>),
    /// The value is a CONSTANT that the code generator folded into the
    /// instructions that use it. It has no place at all, and yet the
    /// debugger can print it: `DW_OP_consts <n> DW_OP_stack_value` says
    /// "this is the value, not the address of the value".
    Const(i64),
}

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
    /// **ROUND 96**: `alloca` value -> the value it was promoted to
    /// (`mem2reg`). Without this trail a variable whose storage the
    /// optimizer removed cannot be found again at all.
    promoted: HashMap<u32, u32>,
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
/// **ROUND 96** — `// HOOK dwarf` in `mem2reg`: the storage of this
/// `alloca` is gone, the value now sits in `val`.
///
/// Only ever called where the promotion is UNAMBIGUOUS (one store, or a
/// load that reads exactly one store). Where mem2reg cannot say which value
/// carries the variable, nothing is written down and the variable stays
/// invisible — which is the honest of the two answers.
pub fn note_promoted(func: &str, alloca: u32, val: u32) {
    if !with_variables() {
        return;
    }
    with(|t| {
        let e = t.funcs.entry(func.to_string()).or_default();
        e.promoted.insert(alloca, val);
    });
}

/// **ROUND 96** — the trail of `note_promoted`.
pub fn promoted_of(func: &str, alloca: u32) -> Option<u32> {
    with(|t| t.funcs.get(func).and_then(|f| f.promoted.get(&alloca).copied()))
}

pub fn vars_of(name: &str) -> Vec<VarNote> {
    with(|t| t.funcs.get(name).map(|f| f.vars.clone()).unwrap_or_default())
}

/// Round 64: the result type of a function.
pub fn ret_of(name: &str) -> Option<DType> {
    with(|t| t.funcs.get(name).and_then(|f| f.ret.clone()))
}

/// Round 64: is debug information for variables being produced at all?
pub fn with_variables() -> bool {
    with(|t| t.variables && !t.files.is_empty())
}

/// ROUND 94: is a line table being produced at all? True as soon as there
/// are source files -- at EVERY build level, because the positions sit on
/// the instructions and survive the optimizer (`fir::Loc`).
pub fn with_lines() -> bool {
    with(|t| !t.files.is_empty())
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
