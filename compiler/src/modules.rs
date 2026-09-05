// SPDX-License-Identifier: GPL-2.0-only
//! Minimal module system: several `.fi` files get merged into ONE program
//! and compiled into ONE binary.
//!
//! Syntax (SPEC §12):
//!   * `import path.module` — loads `path/module.<suffix>`. The search runs
//!     in this order: (1) relative to the IMPORTING file,
//!     (2) relative to the directory of the root file, (3) in `$FIRNLIB`,
//!     (4) in `<directory of the compiler binary>/../lib`
//!     (installation fallback; `firnc1` reads `/proc/self/exe` for it).
//!     The module is addressed under the last path part.
//!   * `export { a, b }` — visibility list per module. Without it everything
//!     is visible.
//!   * `module.item` — access to an element of a loaded module.
//!
//! Method: every file is parsed ON ITS OWN (own `ExprId` range, own file
//! number in the source map). After that the names of the non-root modules
//! are rewritten to `module__name` and the qualified accesses are resolved.
//! The type checker sees a single, flat program.
//!
//! LIMIT (honestly): this is whole program compilation with separate
//! namespaces, no separate object file format — there is no `.o` file per
//! module and there are no interface files.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::ast::{Block, Expr, ExprKind, Program, Stmt, TypeExpr};
use crate::config;
use crate::diag::{Diag, Diags, Span};
use crate::package;
use crate::package_world::{self, World};
use crate::lexer::{self, TokKind};
use crate::parser;

/// One source file of the compilation that got read.
pub struct SourceFile {
    pub id: u32,
    pub path: PathBuf,
    pub src: String,
}

/// Builds `<base>/<part1>/<part2>....<suffix>` — the path that an
/// `import part1.part2` means inside a search directory.
fn module_path(base: &Path, parts: &[String]) -> PathBuf {
    let mut p = base.to_path_buf();
    for part in parts {
        p.push(part);
    }
    p.set_extension(config::FILE_EXT);
    p
}

/// `$FIRNLIB` as search directory: set and not empty, otherwise nothing.
/// A pure function, so that the rule stays testable.
fn firnlib_path(value: Option<&str>) -> Option<PathBuf> {
    match value {
        Some(v) if !v.is_empty() => Some(PathBuf::from(v)),
        _ => None,
    }
}

/// Additional search directories for `import`, in this order: first
/// `$FIRNLIB`, then `<directory of the compiler binary>/../lib`
/// (installation layout `bin/firnc` + `lib/`). Both come AFTER the two
/// places that existed so far (importing file, root file), so that
/// existing resolutions stay unchanged. `firnc1` keeps the same order in
/// `bin/firnc1.fi` (`imports_collect`).
fn extra_search_paths() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    if let Some(p) = firnlib_path(std::env::var("FIRNLIB").ok().as_deref()) {
        out.push(p);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            out.push(dir.join("..").join("lib"));
        }
    }
    out
}

/// Error of the module resolution. `Diag` is the familiar message with a
/// source excerpt; `Paket` is a fully formatted text that `firnc0` and
/// `firnc1` print CHARACTER FOR CHARACTER alike (round 48).
pub enum Error {
    Diag(Diag),
    Package(String),
}

/// One entry of the queue: the file and the place of the `import` that
/// requested it. Which package it belongs to follows from its path
/// (`World::package_of`) — not from who requested it.
struct Waiting {
    path: PathBuf,
    span: Span,
}

/// Finds the root file and all modules reachable through `import`.
/// The root file always has the number 0.
///
/// SEARCH ORDER (round 48, deterministic, first hit wins):
///   1. next to the importing file
///   2. next to the root file
///   3. in the `source` directories of the package the importing file
///      belongs to
///   4. in a `needs` dependency of that package, if the first path part is
///      its name
///   5. in `$FIRNLIB`
///   6. in `<directory of the compiler binary>/../lib`
///
/// Without a manifest `welt` is empty, steps 3 and 4 fall away, and the
/// resolution is character for character the one from before round 48.
pub fn resolve(root: &Path, world: &World) -> Result<Vec<SourceFile>, Error> {
    let work_dir = package_world::cwd();
    let base = root.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let mut out: Vec<SourceFile> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut queue: Vec<Waiting> = vec![Waiting {
        path: root.to_path_buf(),
        span: Span::none(),
    }];
    // module name -> path seen first, for the conflict check.
    let mut names: HashMap<String, String> = HashMap::new();
    while !queue.is_empty() {
        let entry = queue.remove(0);
        let path = entry.path;
        let span = entry.span;
        let key = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if !seen.insert(key) {
            continue;
        }
        let src = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                return Err(Error::Diag(Diag {
                    msg: format!("cannot read '{}': {}", path.display(), e),
                    span,
                    label: "here".to_string(),
                    note: Some(format!(
                        "the search runs relative to the importing file, relative to '{}', in the sources of the project, in its dependencies, then in $FIRNLIB and in <directory of the compiler binary>/../lib",
                        if base.as_os_str().is_empty() {
                            ".".to_string()
                        } else {
                            base.display().to_string()
                        }
                    )),
                    help: None,
                }));
            }
        };
        let id = out.len() as u32;
        let abs_file = package_world::absolute(&path.display().to_string(), &work_dir);
        // ROUND 93: from here on the file is known under the spelling that
        // does NOT name this machine -- relative to the working directory if
        // it lies inside it. The path travels into the diagnostics, into
        // `.file`/`.debug_line` AND into the message table of the checked
        // arithmetic, which the program prints at runtime. The package
        // search hands out absolute paths for every dependency, so without
        // this line the artifact of a package build depends on where the
        // checkout sits (`ACCEPTANCE.md` item 5).
        let path = PathBuf::from(package_world::build_path(
            &path.display().to_string(),
            &work_dir,
        ));
        let my_package = if world.is_empty() {
            None
        } else {
            world.package_of(&abs_file)
        };
        // NAME CONFLICT: two different files with the same module name
        // would fall onto the same internal renaming `module__name` and
        // silently cover each other. Checked with a manifest only — without
        // a manifest everything stays as before.
        if !world.is_empty() {
            let mname = package::module_name(&abs_file);
            match names.get(&mname) {
                Some(before) if *before != abs_file => {
                    return Err(Error::Package(package_world::text_name_clash(
                        &mname, before, &abs_file,
                    )));
                }
                _ => {
                    names.insert(mname, abs_file.clone());
                }
            }
        }
        // IMPORT PATH: first relative to the FILE that writes the import —
        // only after that relative to the root file.
        //
        // Formerly only the second rule held. With it no library could load
        // another one: `lib/rt/vec.fi` with `import rt` looked for `rt.fi`
        // next to the MAIN PROGRAM instead of next to itself
        // (docs/SELF_HOSTING.md §7, blocker B3).
        //
        // The fallback to the root stays, so that existing programs keep
        // running unchanged: `tests/*.fi` load `modules.mathe`, and there
        // both ways are the same one.
        let own = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let extras = extra_search_paths();
        for (parts, ispan) in scan_imports(&src, id) {
            let mut p = module_path(&own, &parts);
            if !p.exists() {
                let q = module_path(&base, &parts);
                if q.exists() {
                    p = q;
                }
            }
            // (3) and (4): the view of the package to which this file belongs.
            if !p.exists() {
                if let Some(pi) = my_package {
                    if let Some(q) = package_candidate(world, pi, &parts) {
                        p = q;
                    }
                }
            }
            if !p.exists() {
                for z in &extras {
                    let q = module_path(z, &parts);
                    if q.exists() {
                        p = q;
                        break;
                    }
                }
            }
            // VISIBILITY: if the hit leads into ANOTHER package, it must be
            // a registered dependency and the module must appear in that
            // package's `public` list.
            if !world.is_empty() {
                let target = package_world::absolute(&p.display().to_string(), &work_dir);
                if let (Some(zp), Some(mp)) = (world.package_of(&target), my_package) {
                    if zp != mp {
                        if world.edge(mp, world.name(zp)) != Some(zp) {
                            return Err(Error::Package(package_world::text_no_dependency(
                                world.name(zp),
                                world.name(mp),
                                &world.packages[mp].manifestpfad,
                            )));
                        }
                        let module = package::module_name(&target);
                        if !world.packages[zp].manifest.is_public(&module) {
                            return Err(Error::Package(package_world::text_not_public(
                                &module,
                                world.name(zp),
                                &world.packages[zp].manifestpfad,
                            )));
                        }
                    }
                }
            }
            queue.push(Waiting { path: p, span: ispan });
        }
        out.push(SourceFile { id, path, src });
    }
    // HOOK gc: the collector runtime is pulled in automatically as soon as
    // a `gc class` appears anywhere (gc.rs, SPEC 3.5) — no `import`, no extra
    // command line option.
    // Round 49: HERE the runtime really becomes part of the program — and only
    // then must the state block appear in the assembler (codegen_x86::emit).
    if let Some(f) = gc_runtime(&out) {
        crate::gc::runtime_remember();
        out.push(f);
    }
    Ok(out)
}

/// Steps 3 and 4 of the search: project sources, then dependencies.
/// Yields the first path that really exists.
fn package_candidate(world: &World, pi: usize, parts: &[String]) -> Option<PathBuf> {
    let p = &world.packages[pi];
    // (3) own source directories
    for q in &p.manifest.sources {
        let base = package::join(&p.root, q);
        let cand = module_path(Path::new(&base), parts);
        if cand.exists() {
            return Some(cand);
        }
    }
    // (4) dependency: the FIRST path part names the package.
    let header = parts.first()?;
    let di = world.edge(pi, header)?;
    let dp = &world.packages[di];
    let rest: Vec<String> = if parts.len() > 1 {
        parts[1..].to_vec()
    } else {
        vec![header.clone()]
    };
    for q in &dp.manifest.sources {
        let base = package::join(&dp.root, q);
        let cand = module_path(Path::new(&base), &rest);
        if cand.exists() {
            return Some(cand);
        }
    }
    None
}

/// Path of the GC runtime pulled in (the module name stays empty: its names
/// are program wide, exactly like the error set names).
pub(crate) fn gc_runtime(files: &[SourceFile]) -> Option<SourceFile> {
    let mut needs = false;
    let mut has_allocerror = false;
    // Round 47: the finalizer dispatcher counts from the root file ONLY.
    // Inside a module it would be called `module__gc_finalize` and the
    // runtime would no longer find it (see `module_name` further down).
    let mut has_finalizer = false;
    // Round 53: `GcVec`/`GcMap` join only when they show up.
    let mut needs_collections = false;
    // Round 49: the same for the thread dispatcher.
    let mut has_thread_work = false;
    for f in files {
        let mut dg = Diags::new("<gc-search>", &f.src);
        let toks = lexer::lex_file(&f.src, f.id, &mut dg);
        needs |= crate::gc::source_needs_gc(&toks);
        has_allocerror |= crate::gc::source_has_allocerror(&toks);
        needs_collections |= crate::gc::source_needs_collections(&toks);
        if f.id == 0 {
            has_finalizer = crate::gc::source_has_finalizer(&toks);
            has_thread_work = crate::gc::source_has_thread_work(&toks);
        }
    }
    if !needs {
        return None;
    }
    Some(SourceFile {
        id: files.len() as u32,
        path: PathBuf::from(crate::gc::RUNTIME_PATH),
        src: crate::gc::runtime_source(
            !has_allocerror,
            !has_finalizer,
            !has_thread_work,
            needs_collections,
        ),
    })
}

/// Looks for `import a.b` declarations without parsing the file fully.
fn scan_imports(src: &str, file: u32) -> Vec<(Vec<String>, Span)> {
    let mut dg = Diags::new("<import-search>", src);
    let toks = lexer::lex_file(src, file, &mut dg);
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < toks.len() {
        if toks[i].kind != TokKind::KwImport {
            i += 1;
            continue;
        }
        let span = toks[i].span;
        i += 1;
        let mut parts: Vec<String> = Vec::new();
        loop {
            match toks.get(i).map(|t| &t.kind) {
                Some(TokKind::Ident(n)) => {
                    parts.push(n.clone());
                    i += 1;
                }
                _ => break,
            }
            if toks.get(i).map(|t| &t.kind) == Some(&TokKind::Dot) {
                i += 1;
            } else {
                break;
            }
        }
        if !parts.is_empty() {
            out.push((parts, span));
        }
    }
    out
}

/// Module name of a file = file name without suffix. The root file has the
/// empty module name (its names stay unchanged, `main` is called `main`).
fn module_name(f: &SourceFile) -> String {
    if f.id == 0 {
        return String::new();
    }
    // The GC runtime lives in the root namespace: `gc_init()` is called
    // `gc_init()` in every module, without `import` and without a prefix.
    if f.path == Path::new(crate::gc::RUNTIME_PATH) {
        return String::new();
    }
    f.path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| format!("m{}", f.id))
}

/// **Version of the symbol naming scheme** (DESIGN_GOALS.md §4, foundation).
///
/// It appears in **every** linker symbol produced. Once the scheme changes,
/// all symbols change — then the linker reports a missing name instead of
/// silently binding two incompatible compilation states together.
pub const SYMBOL_SCHEMA: u32 = 0;

/// Reserved prefix of produced symbols. Firn identifiers cannot produce it
/// (they may contain no dot), which is why user code can never hit a
/// produced symbol by accident.
pub const SYMBOL_PREFIX: &str = "_F";

/// The entry point keeps its bare name: `_start` calls it, and that is an
/// agreement with the linker, no Firn matter.
pub const ENTRY_SYMBOL: &str = "main";

/// Linker name of an item, derived from its **internal** name.
///
/// The internal name comes about in `mangle` (root file: unchanged, module:
/// `module__name`) and is what the type checker and the IR work with. Only
/// the code generator turns it into a symbol:
///
/// ```text
/// _F0.add             item of the root file
/// _F0.helper__square item of a module
/// _F0.add.v3          with ABI version (later, #[abi_stable(3)])
/// main                the entry point, unchanged
/// ```
///
/// **Why already now?** `DESIGN_GOALS.md` §4: if Firn printed `main` and
/// `add` as bare symbols today and needed versioned ones later, that would
/// be a break for everything built already. The room for the version costs
/// nothing today and makes a stable ABI (`#[abi_stable]`) later a mere
/// extension rather than a cut. The separation *internal name* <->
/// *linker symbol* is the real gain along the way: error messages keep
/// showing the source name.
pub fn symbol(interner_name: &str, abi_version: Option<u32>) -> String {
    if interner_name == ENTRY_SYMBOL {
        return interner_name.to_string();
    }
    // ROUND 58: the generated functions of the closures carry a `#` in their
    // name, so that no source text can ever write them (`fnval.rs`). The
    // assembler does not accept the character — it becomes a dot, which the
    // naming scheme uses anyway. No other name of the compiler contains a
    // `#` in a place where a symbol arises from it.
    let interner_name: &str = &interner_name.replace('#', ".");
    match abi_version {
        Some(v) => format!("{}{}.{}.v{}", SYMBOL_PREFIX, SYMBOL_SCHEMA, interner_name, v),
        None => format!("{}{}.{}", SYMBOL_PREFIX, SYMBOL_SCHEMA, interner_name),
    }
}

fn mangle(module: &str, name: &str) -> String {
    if module.is_empty() {
        name.to_string()
    } else {
        format!("{}__{}", module, name)
    }
}

/// What a module offers to the outside.
struct ModuleInfo {
    name: String,
    /// all names declared in the module (functions, structs, constants)
    items: HashSet<String>,
    /// `export` list; empty = everything visible
    exports: HashSet<String>,
}

/// Reads, parses and merges all modules into one program.
/// Reports errors through `dg`; `None` means: there were errors.
pub fn build_program(files: &[SourceFile], dg: &mut Diags) -> Option<Program> {
    let mut progs: Vec<Program> = Vec::new();
    let mut base_id = 0u32;

    // LEX ALL FILES FIRST, THEN PARSE.
    //
    // Reason: the advance search for generic templates
    // (`sema_generic::hook_prescan`) used to run per file IMMEDIATELY before
    // parsing it. The root file is parsed first — so it did not yet know
    // the templates of the modules, and `var v: Vec[i32]` with `Vec`
    // from a module failed in the parser
    // (docs/SELF_HOSTING.md §7, blocker B1).
    //
    // That is why the reset of the hooks belongs here, ONCE for the whole
    // compilation, and after that all files get scanned up front.
    parser::reset_hooks();
    let mut all: Vec<Vec<lexer::Token>> = Vec::new();
    for f in files {
        let toks = lexer::lex_file(&f.src, f.id, dg);
        crate::sema_generic::hook_prescan(&toks);
        all.push(toks);
    }
    for (i, f) in files.iter().enumerate() {
        let p = parser::parse_module(&all[i], dg, f.id, base_id);
        base_id = p.expr_count;
        progs.push(p);
    }
    if dg.has_errors() {
        return None;
    }
    // HOOK profil (prof.rs, round 52): the profile stands in the FIRST file
    // (the root file); `--profile=` wins. It has to be settled here already,
    // because the `import` rule right below asks for it — the type checker
    // runs much later.
    if let Some(root) = progs.first() {
        crate::prof::define(root, None);
    }

    // HOOK profil (prof.rs, round 73): which of the parsed modules declares
    // `profile kernel` ITSELF? That declaration is what admits a `std`
    // module under the kernel profile — and because the module lands in the
    // same compilation unit, the claim gets checked by
    // `sema::check_profile` like every other line of the program.
    let declares_kernel: Vec<bool> = progs
        .iter()
        .map(|p| p.profile.as_ref().map(|(n, _)| n == "kernel").unwrap_or(false))
        .collect();

    // Which module offers what?
    let mut infos: Vec<ModuleInfo> = Vec::new();
    for (f, p) in files.iter().zip(progs.iter()) {
        let mut items: HashSet<String> = HashSet::new();
        for x in &p.funcs {
            items.insert(x.name.clone());
        }
        for x in &p.structs {
            items.insert(x.name.clone());
        }
        for x in &p.consts {
            items.insert(x.name.clone());
        }
        // ROUND 89: a `static` is an item of the module like any other, so
        // `export { COUNT }` and `mod.COUNT` work without a rule of their own.
        for x in &p.statics {
            items.insert(x.name.clone());
        }
        for imp in &p.imports {
            // HOOK profil (prof.rs, round 52/73): under the kernel profile the
            // standard library is barred, unless the module being imported
            // declares `profile kernel` itself. The check sits here because
            // only here are the inclusions of EVERY file known with their
            // position.
            let target = imp.path.last().cloned().unwrap_or_default();
            let at = files.iter().position(|g| module_name(g) == target);
            let module_is_kernel = at.map(|i| declares_kernel[i]).unwrap_or(false);
            crate::prof::hook_import(dg, &imp.path, imp.span, module_is_kernel);
            let known = at.is_some();
            if !known {
                dg.error(
                    imp.span,
                    format!("module '{}' was not found", imp.path.join(".")),
                );
            }
        }
        infos.push(ModuleInfo {
            name: module_name(f),
            items,
            exports: p.exports.iter().map(|(n, _)| n.clone()).collect(),
        });
    }

    let mut merged = Program::default();
    merged.profile = progs.first().and_then(|p| p.profile.clone());
    merged.expr_count = base_id;

    for (idx, mut p) in progs.into_iter().enumerate() {
        let mut r = Renamer {
            me: idx,
            infos: &infos,
            alias: p
                .imports
                .iter()
                .map(|i| (i.alias.clone(), i.path.last().cloned().unwrap_or_default()))
                .collect(),
            dg,
            locals: Vec::new(),
        };
        // First rename the own declarations ...
        let m = infos[idx].name.clone();
        for f in p.funcs.iter_mut() {
            // METHODS OF A BASE TYPE STAY UNTOUCHED (round 50).
            // `impl Ord for i32` creates `i32__less`; the type `i32`
            // belongs to no module, so its methods do not either. If
            // that became `vec__i32__less`, `x.less(..)` would keep
            // looking for `i32__less` and find nothing — the same rule
            // as for interfaces, gc classes and generic templates.
            if crate::iface::is_base_ty_method(&f.name) {
                continue;
            }
            f.name = mangle(&m, &f.name);
        }
        for s in p.structs.iter_mut() {
            s.name = mangle(&m, &s.name);
        }
        for c in p.consts.iter_mut() {
            c.name = mangle(&m, &c.name);
        }
        for g in p.statics.iter_mut() {
            g.name = mangle(&m, &g.name);
        }
        // ... then all references in the bodies.
        for f in p.funcs.iter_mut() {
            r.locals.clear();
            r.push_scope();
            for prm in f.params.iter_mut() {
                r.ty(&mut prm.ty);
                r.declare(&prm.name);
            }
            if let Some(t) = f.ret.as_mut() {
                r.ty(t);
            }
            r.block(&mut f.body);
            r.pop_scope();
        }
        for s in p.structs.iter_mut() {
            for (_, t, _) in s.fields.iter_mut() {
                r.ty(t);
            }
        }
        // REWRITE THE GENERIC TEMPLATES OF THIS FILE too.
        //
        // They do not sit in `Program::funcs` but in
        // `sema_generic::REG` — the rewriting above therefore never reached
        // them, and a template from a module saw the names of the ROOT
        // FILE alone. Even a helper function in the same file reported
        // "unknown function" (docs/SELF_HOSTING.md §7, blocker B2).
        //
        // The NAME of the template stays untouched: the instantiation looks
        // for it later under the original name (`mono::expand_fn` through
        // `Instantiation::base`), and generic names hold program wide.
        let file_id = files[idx].id;
        for name in crate::sema_generic::fn_templates_the_file(file_id) {
            crate::sema_generic::with_fn_template(&name, |decl| {
                r.locals.clear();
                r.push_scope();
                for prm in decl.params.iter_mut() {
                    r.ty(&mut prm.ty);
                    r.declare(&prm.name);
                }
                if let Some(t) = decl.ret.as_mut() {
                    r.ty(t);
                }
                r.block(&mut decl.body);
                r.pop_scope();
            });
        }
        for name in crate::sema_generic::struct_templates_the_file(file_id) {
            crate::sema_generic::with_struct_template(&name, |decl| {
                for (_, t, _) in decl.fields.iter_mut() {
                    r.ty(t);
                }
            });
        }
        for c in p.consts.iter_mut() {
            r.ty(&mut c.ty);
            r.expr(&mut c.value);
        }
        for g in p.statics.iter_mut() {
            r.ty(&mut g.ty);
            r.expr(&mut g.value);
        }
        merged.funcs.append(&mut p.funcs);
        merged.structs.append(&mut p.structs);
        merged.consts.append(&mut p.consts);
        merged.statics.append(&mut p.statics);
        // `comptime { … }` blocks belong to the merged program — otherwise
        // they never run (SPEC §6.4).
        merged.comptime_blocks.append(&mut p.comptime_blocks);
    }
    if dg.has_errors() {
        return None;
    }
    Some(merged)
}

/// Rewrites names in the AST of a module to their final form.
struct Renamer<'a, 'b> {
    me: usize,
    infos: &'a [ModuleInfo],
    /// alias name -> last path part (= module name of the target file)
    alias: HashMap<String, String>,
    dg: &'b mut Diags,
    locals: Vec<HashSet<String>>,
}

impl<'a, 'b> Renamer<'a, 'b> {
    fn push_scope(&mut self) {
        self.locals.push(HashSet::new());
    }
    fn pop_scope(&mut self) {
        self.locals.pop();
    }
    fn declare(&mut self, name: &str) {
        if let Some(s) = self.locals.last_mut() {
            s.insert(name.to_string());
        }
    }
    fn is_local(&self, name: &str) -> bool {
        self.locals.iter().any(|s| s.contains(name))
    }

    /// Resolves a name. `is_value` tells values (which local names can cover)
    /// apart from function/type names.
    fn resolve(&mut self, name: &str, span: Span, is_value: bool) -> Option<String> {
        if let Some((m, rest)) = name.split_once('.') {
            // qualified access module.item
            let target = match self.alias.get(m) {
                Some(t) => t.clone(),
                None => return None,
            };
            let info = match self.infos.iter().find(|i| i.name == target) {
                Some(i) => i,
                None => {
                    self.dg
                        .error(span, format!("module '{}' is not imported", m));
                    return None;
                }
            };
            if !info.items.contains(rest) {
                self.dg.error(
                    span,
                    format!("module '{}' has no element '{}'", m, rest),
                );
                return None;
            }
            if !info.exports.is_empty() && !info.exports.contains(rest) {
                self.dg.error_note(
                    span,
                    format!("'{}' is not exported by module '{}'", rest, m),
                    "add the name to the 'export' list of the module",
                );
                return None;
            }
            return Some(mangle(&info.name, rest));
        }
        if is_value && self.is_local(name) {
            return None;
        }
        let me = &self.infos[self.me];
        if me.items.contains(name) {
            return Some(mangle(&me.name, name));
        }
        None
    }

    /// Labels that a pattern binds are local to the body of the case.
    fn declare_pattern(&mut self, p: &crate::sema_match::Pattern) {
        match p {
            crate::sema_match::Pattern::Bind(n, _) => self.declare(n),
            crate::sema_match::Pattern::Variant { subs, .. } => {
                for s in subs {
                    self.declare_pattern(s);
                }
            }
            _ => {}
        }
    }

    fn ty(&mut self, t: &mut TypeExpr) {
        match t {
            TypeExpr::Named(name, span) => {
                // HOOK fehlerunionen (round 76): `E!T` leaves only the
                // placeholder `__eu#<n>` in the tree, the success type `T`
                // lies aside in `errors::REG`. Without this branch a module
                // never qualified it, and a function could not return an
                // error union over a struct of its own module
                // (`errors::pending_inner`).
                if let Some((idx, mut inner)) = crate::errors::pending_inner(name) {
                    self.ty(&mut inner);
                    crate::errors::set_pending_inner(idx, inner);
                    return;
                }
                if let Some(n) = self.resolve(name, *span, false) {
                    *name = n;
                }
            }
            TypeExpr::Ptr { inner, .. } => self.ty(inner),
            TypeExpr::Array { elem, .. } => self.ty(elem),
            // Round 58: the names in the signature of a function value get
            // qualified like every other type name.
            TypeExpr::Fn { params, ret, .. } => {
                for p in params.iter_mut() {
                    self.ty(p);
                }
                if let Some(r) = ret {
                    self.ty(r);
                }
            }
        }
    }

    fn block(&mut self, b: &mut Block) {
        self.push_scope();
        for s in b.stmts.iter_mut() {
            self.stmt(s);
        }
        self.pop_scope();
    }

    fn stmt(&mut self, s: &mut Stmt) {
        match s {
            // `defer` is only a wrapper: its content is rewritten like every
            // other statement.
            Stmt::Defer(inner, _, _) => self.stmt(inner),
            Stmt::Let { name, ty, init, .. } => {
                if let Some(t) = ty.as_mut() {
                    self.ty(t);
                }
                self.expr(init);
                self.declare(name);
            }
            Stmt::Assign { target, value, .. }
            | Stmt::AssignOp { target, value, .. } => {
                self.expr(target);
                self.expr(value);
            }
            // ROUND 70: the step has no value expression.
            Stmt::Step { target, .. } => self.expr(target),
            Stmt::If { cond, then, els, .. } => {
                self.expr(cond);
                self.block(then);
                if let Some(e) = els.as_mut() {
                    self.stmt(e);
                }
            }
            Stmt::While { cond, body, .. } => {
                self.expr(cond);
                self.block(body);
            }
            Stmt::For { name, start, end, body, .. } => {
                self.expr(start);
                self.expr(end);
                self.push_scope();
                self.declare(name);
                self.block(body);
                self.pop_scope();
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value.as_mut() {
                    self.expr(v);
                }
            }
            Stmt::Expr(e) => self.expr(e),
            Stmt::Block(b) => self.block(b),
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Error(_) => {}
        }
    }

    fn expr(&mut self, e: &mut Expr) {
        let span = e.span;
        match &mut e.kind {
            ExprKind::Int(_) | ExprKind::Float(..) | ExprKind::FloatF32(_) | ExprKind::Bool(_) => {}
            // ROUND 70: the text literal carries its array literal inside.
            ExprKind::Text(_, inner) => self.expr(inner),
            // Round 58: the closure body is resolved INSIDE the enclosing
            // function — only that way does a captured name stay a local one
            // instead of being qualified into a module name.
            ExprKind::Lambda(d) => {
                self.push_scope();
                for p in d.params.iter_mut() {
                    self.ty(&mut p.ty);
                    self.declare(&p.name);
                }
                if let Some(t) = d.ret.as_mut() {
                    self.ty(t);
                }
                self.block(&mut d.body);
                self.pop_scope();
            }
            ExprKind::Ident(name) => {
                if let Some(n) = self.resolve(name, span, true) {
                    *name = n;
                }
            }
            ExprKind::Unary(_, a) => self.expr(a),
            ExprKind::Binary(_, a, b) => {
                self.expr(a);
                self.expr(b);
            }
            ExprKind::Field(b, _, _) => self.expr(b),
            ExprKind::Index(b, i) => {
                self.expr(b);
                self.expr(i);
            }
            ExprKind::Call(name, args, nspan) => {
                // HOOK types: the body blocks of a `match` sit in the registry
                // of `sema_match`, not in the AST. Without this branch the
                // names in them would stay unrewritten — `match` inside an
                // imported module would be unusable.
                if let Some(idx) = name
                    .strip_prefix(crate::sema_match::MATCH_PREFIX)
                    .and_then(|s| s.parse::<usize>().ok())
                {
                    if let Some(mut info) = crate::sema_match::take_match(idx) {
                        self.expr(&mut info.subject);
                        for arm in info.arms.iter_mut() {
                            self.push_scope();
                            self.declare_pattern(&arm.pat);
                            self.block(&mut arm.body);
                            self.pop_scope();
                        }
                        crate::sema_match::put_match(idx, info);
                    }
                    return;
                }
                if let Some(n) = self.resolve(name, *nspan, false) {
                    *name = n;
                }
                for a in args.iter_mut() {
                    self.expr(a);
                }
            }
            ExprKind::Syscall(args) => {
                for a in args.iter_mut() {
                    self.expr(a);
                }
            }
            ExprKind::Cast(a, t) => {
                self.expr(a);
                self.ty(t);
            }
            ExprKind::StructLit(name, fields, nspan) => {
                if let Some(n) = self.resolve(name, *nspan, false) {
                    *name = n;
                }
                for (_, v, _) in fields.iter_mut() {
                    self.expr(v);
                }
            }
            ExprKind::ArrayLit(els) => {
                for x in els.iter_mut() {
                    self.expr(x);
                }
            }
            ExprKind::ArrayRepeat(v, n) => {
                self.expr(v);
                self.expr(n);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_become_found() {
        let src = "import std.io\nimport helper\nfn main() -> i32 { return 0 }\n";
        let found = scan_imports(src, 0);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].0, vec!["std".to_string(), "io".to_string()]);
        assert_eq!(found[1].0, vec!["helper".to_string()]);
    }

    #[test]
    fn search_paths_and_module_path() {
        // module_path: parts become <base>/a/b.fi.
        assert_eq!(
            module_path(Path::new("x"), &["std".to_string(), "math".to_string()]),
            PathBuf::from("x/std/math.fi")
        );
        // FIRNLIB: empty or unset means "no additional path".
        assert_eq!(firnlib_path(None), None);
        assert_eq!(firnlib_path(Some("")), None);
        assert_eq!(
            firnlib_path(Some("/opt/firn/lib")),
            Some(PathBuf::from("/opt/firn/lib"))
        );
    }

    #[test]
    fn names_become_per_module_different() {
        assert_eq!(mangle("", "main"), "main");
        assert_eq!(mangle("helper", "square"), "helper__square");
        assert_eq!(mangle("", "square"), "square");
        // Linker symbols: reserved prefix + scheme version (DESIGN_GOALS 4)
        assert_eq!(symbol("square", None), "_F0.square");
        assert_eq!(symbol("helper__square", None), "_F0.helper__square");
        // The room for the ABI version is there.
        assert_eq!(symbol("helper__square", Some(3)), "_F0.helper__square.v3");
        // The entry point keeps its bare name.
        assert_eq!(symbol("main", None), "main");
        // User code cannot produce the prefix: identifiers have no dots.
        assert!(symbol("a", None).starts_with(SYMBOL_PREFIX));
    }
}
