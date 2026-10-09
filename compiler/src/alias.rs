// SPDX-License-Identifier: MPL-2.0
//! Type aliases: `type Idx = u32` (docs/GAPS.md B14, SPEC "Type aliases").
//!
//! An alias is **another name for the SAME type**, not a new type: `Idx` and
//! `u32` pass into each other without a cast, an `impl` for the one is an
//! `impl` for the other, and a type error names the target. That is why an
//! alias never reaches the type checker: it is replaced by what it stands
//! for while the program is read.
//!
//! ```text
//! type Idx = u32                    // a primitive
//! type Bytes = *mut u8              // a pointer
//! type Row = [i32; 16]              // an array
//! type Cb = fn(i32) -> i32          // a function type
//! type Vi = Vec[i32]                // an instantiated generic struct
//! type Span2 = core.Span            // a type of another module
//! ```
//!
//! ## Two places
//!
//! * **Same file, at parse time** (`hook_use`). The declarations of a file
//!   are found up front (`hook_begin`), so the ORDER of the declarations does
//!   not matter (`type A = B` may stand above `type B = u32`, and a function
//!   above both may use them). A use in a type position is replaced by a
//!   copy of the target right there, which also means that
//!   `Vec[Idx]` and `Vec[u32]` are ONE instantiation — the generic
//!   machinery sees `u32` and nothing else.
//! * **Other module, in `modules.rs`** (`Renamer::expand_alias`). `m.Idx`
//!   stays a qualified name while parsing (the parser cannot know the module's
//!   names yet); the renamer resolves it to the alias and puts the target in,
//!   renamed from the point of view of the module that declared it — so
//!   `type P = Item` in `m` yields `m__Item`, not an `Item` of the importer.
//!
//! ## Limits (documented, not hidden)
//!
//! * A generic alias (`type V[T] = Vec[T]`) is refused with an error.
//! * An alias is a name for a TYPE position. In an expression it is
//!   accepted as a struct literal name (`P { x: 1 }` when `P` names a
//!   struct); `impl P { ... }` on an alias is refused (write `impl Target`).
//! * An alias cannot have the name of a struct of the same file.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::ast::TypeExpr;
use crate::diag::Span;
use crate::lexer::TokKind;
use crate::parser::Parser;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    New,
    Visiting,
    Done,
    Failed,
}

#[derive(Clone, Debug)]
struct Entry {
    name: String,
    /// span of the name in the declaration
    span: Span,
    /// token index of the first token of the target type
    pos: usize,
    /// token index behind the target type (valid once resolved)
    end: usize,
    state: State,
    /// the target, with the aliases of the SAME file already replaced
    ty: Option<TypeExpr>,
    /// declared twice or clashing with a struct: still read (to know where
    /// the item ends), never used
    bad: bool,
}

#[derive(Default)]
struct Reg {
    files: HashMap<u32, Vec<Entry>>,
}

thread_local! {
    static REG: RefCell<Reg> = RefCell::new(Reg::default());
}

/// Empties the registry (one per compilation, `parser::reset_hooks`).
pub(crate) fn hook_reset() {
    REG.with(|r| r.borrow_mut().files.clear());
}

/// The alias declarations of one file as the renamer needs them:
/// (name, target, span of the name). Failed ones are left out — the error
/// is reported already.
pub(crate) fn decls_of_file(file: u32) -> Vec<(String, TypeExpr, Span)> {
    REG.with(|r| {
        r.borrow()
            .files
            .get(&file)
            .map(|v| {
                v.iter()
                    .filter_map(|e| e.ty.clone().map(|t| (e.name.clone(), t, e.span)))
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// Is the token at `i` the start of `type NAME =`?
fn alias_head_at(toks: &[crate::lexer::Token], i: usize) -> Option<(String, Span)> {
    match toks.get(i).map(|t| &t.kind) {
        Some(TokKind::Ident(w)) if w == "type" => {}
        _ => return None,
    }
    let (name, span) = match toks.get(i + 1) {
        Some(t) => match &t.kind {
            TokKind::Ident(n) => (n.clone(), t.span),
            _ => return None,
        },
        None => return None,
    };
    match toks.get(i + 2).map(|t| &t.kind) {
        Some(TokKind::Assign) | Some(TokKind::LBracket) => Some((name, span)),
        _ => None,
    }
}

/// `// HOOK alias` at the start of `Parser::program`: finds the alias
/// declarations of the file and resolves all of them.
pub(crate) fn hook_begin(p: &mut Parser) {
    let mut found: Vec<Entry> = Vec::new();
    let mut depth: i64 = 0;
    for i in 0..p.toks.len() {
        match &p.toks[i].kind {
            TokKind::LBrace => depth += 1,
            TokKind::RBrace => depth -= 1,
            _ => {}
        }
        if depth != 0 {
            continue;
        }
        if let Some((name, span)) = alias_head_at(p.toks, i) {
            // `type V[T] = ...` is reported when the item is reached.
            if !matches!(p.toks.get(i + 2).map(|t| &t.kind), Some(TokKind::Assign)) {
                continue;
            }
            found.push(Entry {
                name,
                span,
                pos: i + 3,
                end: i + 3,
                state: State::New,
                ty: None,
                bad: false,
            });
        }
    }
    if found.is_empty() {
        return;
    }
    // Duplicates and the clash with a struct of the same file are errors.
    let mut seen: HashMap<String, usize> = HashMap::new();
    for e in found.iter_mut() {
        if seen.insert(e.name.clone(), 0).is_some() {
            p.dg.error(e.span, format!("type alias '{}' is already declared", e.name));
            e.bad = true;
            continue;
        }
        let clash = (0..p.toks.len().saturating_sub(1)).any(|k| {
            matches!(p.toks[k].kind, TokKind::KwStruct)
                && matches!(&p.toks[k + 1].kind, TokKind::Ident(n) if *n == e.name)
        });
        if clash {
            p.dg.error(
                e.span,
                format!("'{}' is the name of a struct and of a type alias", e.name),
            );
            e.bad = true;
        }
    }
    let file = p.file;
    let n = found.len();
    REG.with(|r| {
        r.borrow_mut().files.insert(file, found);
    });
    // The targets may name a module (`type S = core.Span`), and an `import`
    // below the alias has not been read yet: borrow the names for now.
    let mut lent: Vec<String> = Vec::new();
    let mut i = 0;
    while i < p.toks.len() {
        if matches!(p.toks[i].kind, TokKind::KwImport) {
            let mut last: Option<String> = None;
            let mut j = i + 1;
            while let Some(TokKind::Ident(w)) = p.toks.get(j).map(|t| &t.kind) {
                last = Some(w.clone());
                j += 1;
                if matches!(p.toks.get(j).map(|t| &t.kind), Some(TokKind::Dot)) {
                    j += 1;
                } else {
                    break;
                }
            }
            if matches!(p.toks.get(j).map(|t| &t.kind), Some(TokKind::KwAs)) {
                if let Some(TokKind::Ident(w)) = p.toks.get(j + 1).map(|t| &t.kind) {
                    last = Some(w.clone());
                }
            }
            if let Some(w) = last {
                if p.modules.insert(w.clone()) {
                    lent.push(w);
                }
            }
        }
        i += 1;
    }
    for i in 0..n {
        resolve(p, file, i);
    }
    for w in lent {
        p.modules.remove(&w);
    }
}

/// Parses the target of the alias `i` of `file` (once).
fn resolve(p: &mut Parser, file: u32, i: usize) {
    let (state, pos) = REG.with(|r| {
        let reg = r.borrow();
        let e = &reg.files[&file][i];
        (e.state, e.pos)
    });
    if state != State::New {
        return;
    }
    set_state(file, i, State::Visiting);
    let saved = (
        p.pos,
        p.recovering,
        p.depth,
        p.infer_len_ok,
        p.no_struct_lit,
        p.paren_depth,
    );
    p.pos = pos;
    p.recovering = false;
    p.infer_len_ok = false;
    p.no_struct_lit = false;
    let t = p.parse_type();
    let end = p.pos;
    p.pos = saved.0;
    p.recovering = saved.1;
    p.depth = saved.2;
    p.infer_len_ok = saved.3;
    p.no_struct_lit = saved.4;
    p.paren_depth = saved.5;
    REG.with(|r| {
        let mut reg = r.borrow_mut();
        if let Some(e) = reg.files.get_mut(&file).and_then(|v| v.get_mut(i)) {
            // A cycle marks the entry `Failed` while it is still `Visiting`.
            if e.state == State::Visiting {
                e.state = if t.is_some() && !e.bad { State::Done } else { State::Failed };
                e.ty = if e.bad { None } else { t };
            }
            e.end = end;
        }
    });
}

fn set_state(file: u32, i: usize, s: State) {
    REG.with(|r| {
        if let Some(e) = r.borrow_mut().files.get_mut(&file).and_then(|v| v.get_mut(i)) {
            e.state = s;
        }
    });
}

/// What a name in a type position turned out to be.
pub(crate) enum Use {
    NotAlias,
    Expanded(TypeExpr),
    /// an alias, but broken — reported already
    Failed,
}

/// `// HOOK alias` in `Parser::parse_type_inner`: `name` is an unqualified
/// type name that has just been read at `sp`.
pub(crate) fn hook_use(p: &mut Parser, name: &str, sp: Span) -> Use {
    let file = p.file;
    let found = REG.with(|r| {
        r.borrow()
            .files
            .get(&file)
            .and_then(|v| v.iter().position(|e| e.name == name))
    });
    let i = match found {
        Some(i) => i,
        None => return Use::NotAlias,
    };
    if p.at(&TokKind::LBracket) {
        p.dg.error_note(
            sp,
            format!("the type alias '{}' takes no type arguments", name),
            "a generic alias (`type V[T] = Vec[T]`) does not exist; write the alias for the instantiation (`type Vi = Vec[i32]`)",
        );
        p.recovering = true;
        return Use::Failed;
    }
    resolve(p, file, i);
    let (state, ty) = REG.with(|r| {
        let reg = r.borrow();
        let e = &reg.files[&file][i];
        (e.state, e.ty.clone())
    });
    match (state, ty) {
        (State::Done, Some(t)) => Use::Expanded(t),
        (State::Visiting, _) => {
            p.dg.error_note(
                sp,
                format!("the type alias '{}' refers to itself", name),
                "an alias is another name for a type; a cycle of aliases names no type at all",
            );
            set_state(file, i, State::Failed);
            p.recovering = true;
            Use::Failed
        }
        _ => {
            // Already reported at its declaration.
            p.recovering = true;
            Use::Failed
        }
    }
}

/// `// HOOK alias` in `Parser::struct_lit`: `P { x: 1 }` where `P` is an
/// alias for a struct builds that struct. The name that comes back is the
/// target's (a qualified `m.Item` is left to the renamer).
pub(crate) fn hook_struct_name(p: &mut Parser, name: String, sp: Span) -> String {
    let file = p.file;
    let found = REG.with(|r| {
        r.borrow()
            .files
            .get(&file)
            .and_then(|v| v.iter().position(|e| e.name == name))
    });
    let i = match found {
        Some(i) => i,
        None => return name,
    };
    resolve(p, file, i);
    let (state, ty) = REG.with(|r| {
        let reg = r.borrow();
        let e = &reg.files[&file][i];
        (e.state, e.ty.clone())
    });
    match (state, ty) {
        (State::Done, Some(TypeExpr::Named(n, _))) => n,
        (State::Done, Some(_)) => {
            p.dg.error_note(
                sp,
                format!("the type alias '{}' does not name a struct", name),
                "only an alias for a struct can build one with `Name { field: value }`",
            );
            name
        }
        _ => name,
    }
}

/// `// HOOK alias` in `sizeof.rs::hook_primary`: `size_of[Idx]()` where the
/// operand is one plain type name. An alias for a plain name is that name;
/// an alias for anything composite cannot be written there (neither can the
/// composite type itself — `size_of[*mut u8]` does not parse either).
pub(crate) fn hook_plain_name(p: &mut Parser, name: String, sp: Span) -> String {
    let file = p.file;
    let found = REG.with(|r| {
        r.borrow()
            .files
            .get(&file)
            .and_then(|v| v.iter().position(|e| e.name == name))
    });
    let i = match found {
        Some(i) => i,
        None => return name,
    };
    resolve(p, file, i);
    let (state, ty) = REG.with(|r| {
        let reg = r.borrow();
        let e = &reg.files[&file][i];
        (e.state, e.ty.clone())
    });
    match (state, ty) {
        (State::Done, Some(TypeExpr::Named(n, _))) => n,
        (State::Done, Some(_)) => {
            p.dg.error_note(
                sp,
                format!("'size_of' takes one plain type name, and '{}' names a composite type", name),
                "give the composite type a struct, or use a name for the element type",
            );
            name
        }
        _ => name,
    }
}

/// `// HOOK alias` in `impls.rs::impl_decl`: `impl Idx { ... }` and
/// `impl I for Idx` with `type Idx = Point` mean `Point`, because an alias is
/// the same type. Only an alias for a plain name can carry methods (a
/// pointer or an array has no name for `Name__method` to hang on).
/// `None` = refused, reported.
pub(crate) fn hook_impl_name(p: &mut Parser, name: String, sp: Span) -> Option<String> {
    let file = p.file;
    let found = REG.with(|r| {
        r.borrow()
            .files
            .get(&file)
            .and_then(|v| v.iter().position(|e| e.name == name))
    });
    let i = match found {
        Some(i) => i,
        None => return Some(name),
    };
    resolve(p, file, i);
    let (state, ty) = REG.with(|r| {
        let reg = r.borrow();
        let e = &reg.files[&file][i];
        (e.state, e.ty.clone())
    });
    match (state, ty) {
        (State::Done, Some(TypeExpr::Named(n, _))) if !n.contains('.') => Some(n),
        (State::Done, _) => {
            p.dg.error_note(
                sp,
                format!("methods cannot be attached to the type alias '{}'", name),
                "an alias for a pointer, an array or a type of another module has no name of its own; write `impl` for the type itself",
            );
            None
        }
        _ => None,
    }
}

/// `// HOOK alias` in `Parser::program`: the item `type NAME = TYPE` itself.
/// The type has been read already (`hook_begin`); the item is skipped.
pub(crate) fn hook_item(p: &mut Parser) -> bool {
    if alias_head_at(p.toks, p.pos).is_none() {
        return false;
    }
    if !matches!(p.toks.get(p.pos + 2).map(|t| &t.kind), Some(TokKind::Assign)) {
        let sp = p.toks[p.pos + 1].span;
        p.dg.error_note(
            sp,
            "a type alias cannot have type parameters".to_string(),
            "write the alias for one instantiation: `type Vi = Vec[i32]`".to_string(),
        );
        p.recovering = false;
        p.sync_item();
        return true;
    }
    let file = p.file;
    let target = p.pos + 3;
    let end = REG.with(|r| {
        r.borrow()
            .files
            .get(&file)
            .and_then(|v| v.iter().find(|e| e.pos == target).map(|e| e.end))
    });
    match end {
        Some(e) if e > p.pos => p.pos = e,
        _ => {
            p.recovering = false;
            p.sync_item();
        }
    }
    true
}
