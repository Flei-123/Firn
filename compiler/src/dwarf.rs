//! DWARF basics: line numbers (`.debug_line`) for the debugger.
//!
//! FIR carries no source positions (`fir.rs` is frozen). That is why lowering
//! collects the mapping *instruction -> source line* here as a table that the
//! code generator queries while writing the assembler. The line numbers get
//! emitted as `.file`/`.loc` directives; from those `as` produces the sections
//! `.debug_line`, `.debug_info` and `.debug_abbrev`.
//!
//! Accuracy:
//!   * **always**: line of the `fn` declaration (a breakpoint on a function
//!     shows the right `.fi` file and line)
//!   * **without the optimizer**: additionally instruction-exact lines. With
//!     the optimizer they get suppressed, because the optimizer removes and
//!     moves instructions and renumbers blocks — wrong lines would be worse
//!     than none.

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
struct FuncLines {
    /// Position of the `fn` line: (file number, line)
    decl: Option<(u32, u32)>,
    /// (block, index of the instruction within the block) -> (file number, line)
    notes: HashMap<(u32, u32), (u32, u32)>,
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

/// Remember the source line of instruction `idx` within block `block`.
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

/// One `alloca` got INSERTED into block `block` at position `at`: every
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

/// Line of instruction `idx` within block `block`, if noted.
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
