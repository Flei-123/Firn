// SPDX-License-Identifier: GPL-2.0-only
//! `extern fn` and `#[export_c]` — the C ABI boundary (SPEC §14.5, round 75).
//!
//! Up to round 74 `extern fn` was recognized syntactically and rejected
//! outright (SPEC §14.1 point 7). This file is what makes it real: a
//! declaration WITHOUT a Firn body that names a function defined
//! elsewhere, with a linker symbol that is **not** mangled through
//! `modules::symbol` (no `_F0.` prefix) — because the whole point is that
//! `gcc`/`ld` have to find it under the name THEY know.
//!
//! ## Why a table here rather than a field on `fir::Func`
//!
//! `fir::Module::funcs` holds functions that HAVE a body — every one of
//! them gets emitted as a real `label:` block with a prologue/epilogue.
//! An `extern fn` has none of that: nothing is emitted for it at all, only
//! *references TO it* (`Op::Call { name, .. }`) exist, from callers that
//! DO have a body. So the information "this call name is extern, and its
//! real symbol is X" cannot live on a `Func` that is never created — it
//! has to live somewhere `codegen_x86.rs::label()` can ask, exactly where
//! `prof::is_kernel()` already answers a parallel question. Same shape,
//! same reason: `thread_local!`, filled once after the type check, read
//! by lowering and codegen without threading a new parameter through
//! every call site between `main.rs` and `emit_inst`.
//!
//! ## What is registered, and when
//!
//! `register(prog)` runs right after `collect_fns` inside
//! `sema::Checker::run` (the `// HOOK extfn` line) — at that point every
//! function of the compilation unit, imported modules included, is known
//! under its FINAL internal name (`modules.rs` has already rewritten
//! `module__name`). For every `FnDecl` with `extern_info: Some(_)`:
//!
//!   * `link_name` = the `#[link_name(...)]` argument if present,
//!     otherwise the bare Firn name — UNMANGLED. A name from an imported
//!     module (`helper__square`) would carry its module prefix into the
//!     symbol table, which is wrong for a `extern fn` declared inside a
//!     module — so `link_name` always uses the SOURCE name
//!     (`FnDecl.name`, before `modules.rs` touches it) unless overridden.
//!     Concretely: `modules.rs` calls `mark` with BOTH names, so the
//!     rewritten internal name is what call sites use to look it up and
//!     the source name is what ends up in the assembly.
//!
//! `#[export_c]` works the other way: the function KEEPS its normal FIR
//! body (nothing changes about how it is compiled), but its **linker
//! symbol** becomes the bare Firn name instead of `_F0.name` — so `gcc`
//! can call it. `codegen_x86.rs::label()` asks `is_exported` for that.

use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    /// internal call name (post module-rewrite) -> C link name
    static EXTERNS: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
    /// internal names of functions marked `#[export_c]`
    static EXPORTED: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

/// Only for self tests, which compile several programs in ONE process.
#[cfg(test)]
pub(crate) fn reset() {
    EXTERNS.with(|e| e.borrow_mut().clear());
    EXPORTED.with(|e| e.borrow_mut().clear());
}

/// Registers one `extern fn`. `internal_name` is what `Op::Call` carries
/// (after the module system has possibly rewritten it to `module__name`),
/// `link_name` is the symbol that ends up in the assembly.
pub fn mark_extern(internal_name: &str, link_name: &str) {
    EXTERNS.with(|e| {
        e.borrow_mut().insert(internal_name.to_string(), link_name.to_string());
    });
}

/// Registers one `#[export_c]` function. `internal_name` is the (possibly
/// module-rewritten) name lowering/codegen use internally, `export_name`
/// the bare symbol C is meant to see.
pub fn mark_exported(internal_name: &str, export_name: &str) {
    EXPORTED.with(|e| {
        e.borrow_mut().insert(internal_name.to_string(), export_name.to_string());
    });
}

/// Is `name` (the internal FIR/call name) an `extern fn`? If so, its
/// (unmangled) link symbol.
pub fn extern_link_name(name: &str) -> Option<String> {
    EXTERNS.with(|e| e.borrow().get(name).cloned())
}

/// Is `name` marked `#[export_c]`? If so, its (unmangled) export symbol.
pub fn export_link_name(name: &str) -> Option<String> {
    EXPORTED.with(|e| e.borrow().get(name).cloned())
}

/// Registers every `extern fn` and `#[export_c]` function of `prog` under
/// its (already module-resolved) internal name.
///
/// Called from `sema::Checker::run`, right after `collect_fns` — so this
/// sees the SAME `FnDecl.name` values that end up in FIR (module rewriting
/// has already happened in `modules.rs::build_program`, which runs before
/// `sema::check` at all).
pub fn register(prog: &crate::ast::Program) {
    for f in &prog.funcs {
        if let Some(info) = &f.extern_info {
            let link = info.link_name.clone().unwrap_or_else(|| source_name(&f.name));
            mark_extern(&f.name, &link);
        } else if f.attrs.iter().any(|a| a.name == "export_c") {
            mark_exported(&f.name, &source_name(&f.name));
        }
    }
}

/// Strips a module rewrite (`modules.rs`: `module__name`) back to the bare
/// source identifier, so that the C symbol never carries a module prefix
/// nobody outside Firn knows about. Firn identifiers cannot contain `__`
/// themselves as a leading separator in this position without having gone
/// through the module rewriter (`ast_canon`/lexer forbid a bare `_` run
/// that would collide — checked by `modules::tests`), so the split is
/// unambiguous in practice: only the LAST `__`-separated segment is kept,
/// matching how `modules.rs` builds the name in the first place
/// (`format!("{}__{}", alias, name)`, never nested).
fn source_name(internal: &str) -> String {
    match internal.rsplit_once("__") {
        Some((_, rest)) if !rest.is_empty() => rest.to_string(),
        _ => internal.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extern_without_link_name_uses_bare_name() {
        reset();
        mark_extern("strlen", "strlen");
        assert_eq!(extern_link_name("strlen"), Some("strlen".to_string()));
        assert_eq!(extern_link_name("other"), None);
    }

    #[test]
    fn explicit_link_name_wins() {
        reset();
        mark_extern("c_exit", "exit");
        assert_eq!(extern_link_name("c_exit"), Some("exit".to_string()));
    }

    #[test]
    fn export_c_is_a_separate_table() {
        reset();
        mark_exported("firn_add", "firn_add");
        assert_eq!(export_link_name("firn_add"), Some("firn_add".to_string()));
        assert_eq!(extern_link_name("firn_add"), None);
    }

    #[test]
    fn source_name_strips_one_module_prefix() {
        assert_eq!(source_name("helper__square"), "square");
        assert_eq!(source_name("square"), "square");
    }
}
