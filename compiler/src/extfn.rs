// SPDX-License-Identifier: MPL-2.0
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

use crate::ast::{StructDecl, TypeExpr};
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    /// internal call name (post module-rewrite) -> C link name
    static EXTERNS: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
    /// internal names of functions marked `#[export_c]`
    static EXPORTED: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
    /// libraries named by `#[link_lib(..)]`, in first-seen order
    static LIBS: RefCell<Vec<String>> = RefCell::new(Vec::new());
}

/// One `#[export_c]` function as the C header needs it.
#[derive(Clone)]
struct ExportSig {
    c_name: String,
    params: Vec<(String, TypeExpr, u8)>,
    ret: Option<TypeExpr>,
}

thread_local! {
    /// exported signatures, in source order (for `--header` / `--shared`)
    static SIGS: RefCell<Vec<ExportSig>> = RefCell::new(Vec::new());
    /// every struct declaration of the program, by source name
    static STRUCTS: RefCell<HashMap<String, StructDecl>> = RefCell::new(HashMap::new());
}

/// Only for self tests, which compile several programs in ONE process.
#[cfg(test)]
pub(crate) fn reset() {
    SIGS.with(|e| e.borrow_mut().clear());
    STRUCTS.with(|e| e.borrow_mut().clear());
    EXTERNS.with(|e| e.borrow_mut().clear());
    EXPORTED.with(|e| e.borrow_mut().clear());
    LIBS.with(|e| e.borrow_mut().clear());
}

/// The libraries `#[link_lib(..)]` asked for (empty = a static image).
pub fn link_libs() -> Vec<String> {
    LIBS.with(|e| e.borrow().clone())
}

/// The `ld` argument for one `#[link_lib(name)]`. The well known names map
/// to the versioned runtime file (`libc.so.6`), because the unversioned
/// `libc.so` is a linker script that only exists with the dev package.
pub fn link_lib_arg(name: &str) -> String {
    match name {
        "c" => "-l:libc.so.6".to_string(),
        "dl" => "-l:libdl.so.2".to_string(),
        "m" => "-l:libm.so.6".to_string(),
        "pthread" => "-l:libpthread.so.0".to_string(),
        other => format!("-l{}", other),
    }
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
    STRUCTS.with(|m| {
        let mut m = m.borrow_mut();
        for st in &prog.structs {
            m.insert(source_name(&st.name), st.clone());
        }
    });
    for f in &prog.funcs {
        for a in f.attrs.iter().filter(|a| a.name == "link_lib") {
            if let Some(l) = a.args.first() {
                LIBS.with(|e| {
                    let mut v = e.borrow_mut();
                    if !v.contains(l) {
                        v.push(l.clone());
                    }
                });
            }
        }
        if let Some(info) = &f.extern_info {
            let link = info.link_name.clone().unwrap_or_else(|| source_name(&f.name));
            mark_extern(&f.name, &link);
        } else if f.attrs.iter().any(|a| a.name == "export_c") {
            mark_exported(&f.name, &source_name(&f.name));
            SIGS.with(|v| {
                v.borrow_mut().push(ExportSig {
                    c_name: source_name(&f.name),
                    params: f.params.iter().map(|p| (p.name.clone(), p.ty.clone(), p.refk)).collect(),
                    ret: f.ret.clone(),
                })
            });
        }
        // ROUND CERTUS-WINDOWS: a function Windows is allowed to call back.
        // The arity is recorded here because the thunk cannot be written
        // without it.
        if f.attrs.iter().any(|a| a.name == "win_callback") {
            crate::win::mark_callback(&f.name, f.params.len() as u32);
        }
    }
}

/// The export symbols of this compilation, in source order (`--shared`
/// builds the linker version script from them).
pub fn export_names() -> Vec<String> {
    SIGS.with(|v| v.borrow().iter().map(|s| s.c_name.clone()).collect())
}

/// C spelling of a Firn scalar type name, `None` if it is not one.
fn c_scalar(n: &str) -> Option<&'static str> {
    Some(match n {
        "i8" => "int8_t",
        "i16" => "int16_t",
        "i32" => "int32_t",
        "i64" => "int64_t",
        "u8" => "uint8_t",
        "u16" => "uint16_t",
        "u32" => "uint32_t",
        "u64" => "uint64_t",
        "usize" => "size_t",
        "isize" => "ptrdiff_t",
        "bool" => "bool",
        "f32" => "float",
        "f64" => "double",
        "v128" => "firn_v128",
        _ => return None,
    })
}

/// C spelling of a type; struct names met on the way land in `need`.
fn c_type(t: &TypeExpr, need: &mut Vec<String>) -> String {
    match t {
        TypeExpr::Named(n, _) => {
            if let Some(c) = c_scalar(n) {
                return c.to_string();
            }
            let n = source_name(n);
            if !need.contains(&n) {
                need.push(n.clone());
            }
            n
        }
        TypeExpr::Ptr { mutable, inner, .. } => {
            let i = match &**inner {
                TypeExpr::Named(n, _) if c_scalar(n).is_none() && !STRUCTS.with(|m| m.borrow().contains_key(&source_name(n))) => {
                    "void".to_string()
                }
                other => c_type(other, need),
            };
            if *mutable { format!("{}*", i) } else { format!("const {}*", i) }
        }
        // an array parameter decays; function values are records, C sees an address
        TypeExpr::Array { elem, .. } => format!("{}*", c_type(elem, need)),
        TypeExpr::Fn { .. } | TypeExpr::Tuple(..) => "void*".to_string(),
    }
}

fn emit_struct(name: &str, done: &mut Vec<String>, out: &mut String) {
    if done.iter().any(|d| d == name) {
        return;
    }
    done.push(name.to_string());
    let Some(sd) = STRUCTS.with(|m| m.borrow().get(name).cloned()) else {
        return;
    };
    let mut need = Vec::new();
    let mut lines = String::new();
    for (fname, fty, _) in &sd.fields {
        match fty {
            TypeExpr::Array { elem, len, .. } => {
                lines.push_str(&format!("    {} {}[{}];\n", c_type(elem, &mut need), fname, len))
            }
            other => lines.push_str(&format!("    {} {};\n", c_type(other, &mut need), fname)),
        }
    }
    for n in need {
        // pointers to a struct only need the tag; by value needs the body. Emit the body first (a cycle is cut by `done`).
        emit_struct(&n, done, out);
    }
    out.push_str(&format!("typedef struct {} {{\n{}}} {};\n\n", name, lines, name));
}

/// The C header for every `#[export_c]` function: scalar types from
/// `<stdint.h>`, structs as plain C structs (Firn lays fields out like C),
/// function values and tuples as `void*`. `guard` is the include guard.
pub fn header_text(guard: &str) -> String {
    let sigs = SIGS.with(|v| v.borrow().clone());
    let mut need = Vec::new();
    let mut protos = String::new();
    for s in &sigs {
        let ret = match &s.ret {
            Some(t) => c_type(t, &mut need),
            None => "void".to_string(),
        };
        let mut ps: Vec<String> = Vec::new();
        for (n, t, refk) in &s.params {
            let ty = c_type(t, &mut need);
            let ty = if *refk == 1 { format!("{}*", ty) } else { ty };
            ps.push(format!("{} {}", ty, n));
        }
        let args = if ps.is_empty() { "void".to_string() } else { ps.join(", ") };
        protos.push_str(&format!("{} {}({});\n", ret, s.c_name, args));
    }
    let mut structs = String::new();
    let mut done = Vec::new();
    for n in need {
        emit_struct(&n, &mut done, &mut structs);
    }
    format!(
        "/* C header written by the Firn compiler with --header; regenerate it, never edit it. */\n#ifndef {g}\n#define {g}\n\n#include <stdbool.h>\n#include <stddef.h>\n#include <stdint.h>\n\n#ifdef __cplusplus\nextern \"C\" {{\n#endif\n\ntypedef float firn_v128 __attribute__((vector_size(16)));\n\n{s}{p}\n#ifdef __cplusplus\n}}\n#endif\n\n#endif /* {g} */\n",
        g = guard,
        s = structs,
        p = protos
    )
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
    fn header_maps_scalars_and_pointers() {
        reset();
        let mut need = Vec::new();
        let u = TypeExpr::Named("u32".into(), crate::diag::Span::none());
        assert_eq!(c_type(&u, &mut need), "uint32_t");
        let p = TypeExpr::Ptr { mutable: false, inner: Box::new(u), span: crate::diag::Span::none() };
        assert_eq!(c_type(&p, &mut need), "const uint32_t*");
        assert!(need.is_empty());
    }

    #[test]
    fn source_name_strips_one_module_prefix() {
        assert_eq!(source_name("helper__square"), "square");
        assert_eq!(source_name("square"), "square");
    }
}
