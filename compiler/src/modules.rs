//! Minimales Modulsystem: mehrere `.fi`-Dateien werden zu EINEM Programm
//! zusammengefuehrt und zu EINEM Binary uebersetzt.
//!
//! Syntax (SPEC §12):
//!   * `import pfad.modul` — bindet `pfad/modul.<endung>` ein. Gesucht wird
//!     in dieser Reihenfolge: (1) relativ zur IMPORTIERENDEN Datei,
//!     (2) relativ zum Verzeichnis der Wurzeldatei, (3) in `$FIRNLIB`,
//!     (4) in `<Verzeichnis des Compiler-Binarys>/../lib`
//!     (Installationsfallback; `firnc1` liest dafuer `/proc/self/exe`).
//!     Angesprochen wird das Modul unter dem letzten Pfadteil.
//!   * `export { a, b }` — Sichtbarkeitsliste je Modul. Fehlt sie, ist alles
//!     sichtbar.
//!   * `modul.name` — Zugriff auf ein Element eines eingebundenen Moduls.
//!
//! Verfahren: jede Datei wird EINZELN geparst (eigener `ExprId`-Bereich, eigene
//! Dateinummer in der Quelltextkarte). Danach werden die Namen der Nicht-Wurzel-
//! module auf `modul__name` umgeschrieben und die qualifizierten Zugriffe
//! aufgeloest. Der Typpruefer sieht ein einziges, flaches Programm.
//!
//! GRENZE (ehrlich): das ist Gesamtprogramm-Uebersetzung mit getrennten
//! Namensraeumen, kein getrenntes Objektdateiformat — es gibt keine `.o`-Datei
//! je Modul und keine Schnittstellendateien.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::ast::{Block, Expr, ExprKind, Program, Stmt, TypeExpr};
use crate::config;
use crate::diag::{Diag, Diags, Span};
use crate::package;
use crate::package_world::{self, World};
use crate::lexer::{self, TokKind};
use crate::parser;

/// Eine eingelesene Quelldatei der Uebersetzung.
pub struct SourceFile {
    pub id: u32,
    pub path: PathBuf,
    pub src: String,
}

/// Baut `<basis>/<teil1>/<teil2>....<endung>` — den Pfad, den ein
/// `import teil1.teil2` in einem Suchverzeichnis meint.
fn module_path(base: &Path, parts: &[String]) -> PathBuf {
    let mut p = base.to_path_buf();
    for part in parts {
        p.push(part);
    }
    p.set_extension(config::FILE_EXT);
    p
}

/// `$FIRNLIB` als Suchverzeichnis: gesetzt und nicht leer, sonst nichts.
/// Reine Funktion, damit die Regel testbar bleibt.
fn firnlib_path(value: Option<&str>) -> Option<PathBuf> {
    match value {
        Some(v) if !v.is_empty() => Some(PathBuf::from(v)),
        _ => None,
    }
}

/// Zusaetzliche Suchverzeichnisse fuer `import`, in dieser Reihenfolge:
/// erst `$FIRNLIB`, dann `<Verzeichnis des Compiler-Binarys>/../lib`
/// (Installationslayout `bin/firnc` + `lib/`). Beide kommen NACH den
/// beiden bisherigen Orten (importierende Datei, Wurzeldatei), damit
/// bestehende Aufloesungen unveraendert bleiben. `firnc1` haelt in
/// `bin/firnc1.fi` (`imports_sammeln`) dieselbe Reihenfolge ein.
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

/// Fehler der Modulaufloesung. `Diag` ist die gewohnte Meldung mit
/// Quelltextausschnitt; `Paket` ist ein fertig formatierter Text, den
/// `firnc0` und `firnc1` ZEICHENGLEICH ausgeben (Runde 48).
pub enum Error {
    Diag(Diag),
    Package(String),
}

/// Ein Eintrag der Warteschlange: die Datei und der Ort des `import`, der
/// sie angefordert hat. Zu welchem Paket sie gehoert, ergibt sich aus ihrem
/// Pfad (`Welt::paket_von`) — nicht daraus, wer sie angefordert hat.
struct Waiting {
    path: PathBuf,
    span: Span,
}

/// Findet die Wurzeldatei und alle ueber `import` erreichbaren Module.
/// Die Wurzeldatei hat immer die Nummer 0.
///
/// SUCHREIHENFOLGE (Runde 48, deterministisch, erster Treffer gewinnt):
///   1. neben der importierenden Datei
///   2. neben der Wurzeldatei
///   3. in den `quelle`-Verzeichnissen des Pakets, zu dem die importierende
///      Datei gehoert
///   4. in einer `brauche`-Abhaengigkeit dieses Pakets, wenn der erste
///      Pfadteil ihr Name ist
///   5. in `$FIRNLIB`
///   6. in `<Verzeichnis des Compiler-Binarys>/../lib`
///
/// Ohne Manifest ist `welt` leer, die Schritte 3 und 4 entfallen, und die
/// Aufloesung ist Zeichen fuer Zeichen die von vor Runde 48.
pub fn resolve(root: &Path, world: &World) -> Result<Vec<SourceFile>, Error> {
    let work_dir = package_world::cwd();
    let base = root.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let mut out: Vec<SourceFile> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut queue: Vec<Waiting> = vec![Waiting {
        path: root.to_path_buf(),
        span: Span::none(),
    }];
    // Modulname -> zuerst gesehener Pfad, fuer die Konfliktpruefung.
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
                    msg: format!("kann '{}' nicht lesen: {}", path.display(), e),
                    span,
                    label: "hier".to_string(),
                    note: Some(format!(
                        "gesucht wird relativ zur importierenden datei, relativ zu '{}', in den quellen des projekts, in seinen abhaengigkeiten, dann in $FIRNLIB und in <verzeichnis des compiler-binarys>/../lib",
                        if base.as_os_str().is_empty() {
                            ".".to_string()
                        } else {
                            base.display().to_string()
                        }
                    )),
                }));
            }
        };
        let id = out.len() as u32;
        let abs_file = package_world::absolute(&path.display().to_string(), &work_dir);
        let my_package = if world.is_empty() {
            None
        } else {
            world.package_of(&abs_file)
        };
        // NAMENSKONFLIKT: zwei verschiedene Dateien mit demselben Modulnamen
        // wuerden auf dieselbe interne Umbenennung `modul__name` fallen und
        // sich still ueberdecken. Nur mit Manifest geprueft — ohne Manifest
        // bleibt alles wie bisher.
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
        // IMPORTPFAD: zuerst relativ zur DATEI, die den Import schreibt —
        // erst danach relativ zur Wurzeldatei.
        //
        // Vorher galt nur die zweite Regel. Damit konnte eine Bibliothek keine
        // andere einbinden: `lib/rt/vec.fi` mit `import rt` suchte `rt.fi`
        // neben dem HAUPTPROGRAMM statt neben sich selbst
        // (docs/SELBSTHOSTING.md §7, Blocker B3).
        //
        // Der Rueckfall auf die Wurzel bleibt, damit bestehende Programme
        // unveraendert weiterlaufen: `tests/*.fi` binden `modules.mathe` ein,
        // und dort sind beide Wege derselbe.
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
            // (3) und (4): die Sicht des Pakets, zu dem diese Datei gehoert.
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
            // SICHTBARKEIT: fuehrt der Treffer in ein ANDERES Paket, muss es
            // eine eingetragene Abhaengigkeit sein und das Modul muss in
            // dessen `oeffentlich`-Liste stehen.
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
    // HOOK gc: die Sammler-Laufzeit wird automatisch eingezogen, sobald
    // irgendwo ein `gc class` steht (gc.rs, SPEC 3.5) — kein `import`, keine
    // zusaetzliche Kommandozeilenoption.
    // Runde 49: HIER wird die Laufzeit wirklich Teil des Programms — und nur
    // dann muss der Zustandsblock im Assembler stehen (codegen_x86::emit).
    if let Some(f) = gc_runtime(&out) {
        crate::gc::runtime_remember();
        out.push(f);
    }
    Ok(out)
}

/// Schritt 3 und 4 der Suche: Projektquellen, dann Abhaengigkeiten.
/// Liefert den ersten Pfad, der wirklich existiert.
fn package_candidate(world: &World, pi: usize, parts: &[String]) -> Option<PathBuf> {
    let p = &world.packages[pi];
    // (3) eigene Quellverzeichnisse
    for q in &p.manifest.sources {
        let base = package::join(&p.root, q);
        let cand = module_path(Path::new(&base), parts);
        if cand.exists() {
            return Some(cand);
        }
    }
    // (4) Abhaengigkeit: der ERSTE Pfadteil nennt das Paket.
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

/// Pfad der eingezogenen GC-Laufzeit (Modulname bleibt leer: ihre Namen sind
/// programmweit, genau wie die Fehlermengennamen).
pub(crate) fn gc_runtime(files: &[SourceFile]) -> Option<SourceFile> {
    let mut needs = false;
    let mut has_allocerror = false;
    // Runde 47: der Finalisierer-Verteiler zaehlt NUR aus der Wurzeldatei.
    // In einem Modul hiesse er `modul__gc_finalisiere` und die Laufzeit
    // faende ihn nicht mehr (siehe `module_name` weiter unten).
    let mut has_finalizer = false;
    // Runde 53: `GcVec`/`GcMap` kommen nur dazu, wenn sie vorkommen.
    let mut needs_collections = false;
    // Runde 49: dasselbe fuer den Fadenverteiler.
    let mut has_thread_work = false;
    for f in files {
        let mut dg = Diags::new("<gc-suche>", &f.src);
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
        path: PathBuf::from(crate::gc::LAUFZEIT_PFAD),
        src: crate::gc::runtime_source(
            !has_allocerror,
            !has_finalizer,
            !has_thread_work,
            needs_collections,
        ),
    })
}

/// Sucht `import a.b`-Deklarationen, ohne die Datei vollstaendig zu parsen.
fn scan_imports(src: &str, file: u32) -> Vec<(Vec<String>, Span)> {
    let mut dg = Diags::new("<import-suche>", src);
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

/// Modulname einer Datei = Dateiname ohne Endung. Die Wurzeldatei hat den
/// leeren Modulnamen (ihre Namen bleiben unveraendert, `main` heisst `main`).
fn module_name(f: &SourceFile) -> String {
    if f.id == 0 {
        return String::new();
    }
    // Die GC-Laufzeit liegt im Wurzelnamensraum: `gc_init()` heisst in jedem
    // Modul `gc_init()`, ohne `import` und ohne Modulpraefix.
    if f.path == Path::new(crate::gc::LAUFZEIT_PFAD) {
        return String::new();
    }
    f.path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| format!("m{}", f.id))
}

/// **Version des Symbol-Namensschemas** (DESIGNZIELE.md §4, Fundamentpunkt).
///
/// Sie steht in **jedem** erzeugten Linker-Symbol. Aendert sich das Schema,
/// aendern sich alle Symbole — dann meldet der Linker einen fehlenden Namen,
/// statt zwei unvertraegliche Uebersetzungsstaende still zusammenzubinden.
pub const SYMBOL_SCHEMA: u32 = 0;

/// Reservierter Praefix erzeugter Symbole. Firn-Bezeichner koennen ihn nicht
/// erzeugen (sie duerfen keinen Punkt enthalten), deshalb kann Nutzercode nie
/// versehentlich ein erzeugtes Symbol treffen.
pub const SYMBOL_PREFIX: &str = "_F";

/// Der Einstiegspunkt behaelt seinen nackten Namen: `_start` ruft ihn, und das
/// ist eine Verabredung mit dem Linker, keine Firn-Angelegenheit.
pub const ENTRY_SYMBOL: &str = "main";

/// Linker-Name eines Elements aus seinem **internen** Namen.
///
/// Der interne Name entsteht in `mangle` (Wurzeldatei: unveraendert, Modul:
/// `modul__name`) und ist das, womit Typpruefer und IR arbeiten. Erst der
/// Codegenerator macht daraus ein Symbol:
///
/// ```text
/// _F0.add             Element der Wurzeldatei
/// _F0.helper__square Element eines Moduls
/// _F0.add.v3          mit ABI-Version (spaeter, #[abi_stable(3)])
/// main                der Einstiegspunkt, unveraendert
/// ```
///
/// **Warum jetzt schon?** `DESIGNZIELE.md` §4: Gibt Firn heute `main` und `add`
/// als nackte Symbole aus und braucht spaeter versionierte, ist das ein Bruch
/// fuer alles, was bereits gebaut wurde. Der Platz fuer die Version kostet
/// heute nichts und macht ein stabiles ABI (`#[abi_stable]`) spaeter zu einer
/// Erweiterung statt zu einem Schnitt. Die Trennung *interner Name* <->
/// *Linker-Symbol* ist dabei der eigentliche Gewinn: Fehlermeldungen zeigen
/// weiter den Quelltextnamen.
pub fn symbol(interner_name: &str, abi_version: Option<u32>) -> String {
    if interner_name == ENTRY_SYMBOL {
        return interner_name.to_string();
    }
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

/// Was ein Modul nach aussen anbietet.
struct ModuleInfo {
    name: String,
    /// alle im Modul deklarierten Namen (Funktionen, Structs, Konstanten)
    items: HashSet<String>,
    /// `export`-Liste; leer = alles sichtbar
    exports: HashSet<String>,
}

/// Liest, parst und verschmilzt alle Module zu einem Programm.
/// Meldet Fehler ueber `dg`; `None` heisst: es gab Fehler.
pub fn build_program(files: &[SourceFile], dg: &mut Diags) -> Option<Program> {
    let mut progs: Vec<Program> = Vec::new();
    let mut base_id = 0u32;

    // ZUERST ALLE DATEIEN LEXEN, DANN PARSEN.
    //
    // Grund: die Vorabsuche nach generischen Vorlagen
    // (`sema_generic::hook_prescan`) lief bisher je Datei UNMITTELBAR vor
    // deren Parsen. Die Wurzeldatei wird zuerst geparst — sie kannte die
    // Vorlagen der Module also noch nicht, und `var v: Vec[i32]` mit `Vec`
    // aus einem Modul scheiterte am Parser
    // (docs/SELBSTHOSTING.md §7, Blocker B1).
    //
    // Der Reset der Hooks gehoert deshalb hierher, EINMAL fuer die ganze
    // Uebersetzung, und danach werden alle Dateien vorab gescannt.
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
    // HOOK profil (prof.rs, Runde 52): das Profil steht in der ERSTEN Datei
    // (der Wurzeldatei); `--profile=` gewinnt. Es muss hier schon feststehen,
    // weil die `import`-Regel unmittelbar darunter danach fragt — der
    // Typpruefer laeuft erst viel spaeter.
    if let Some(root) = progs.first() {
        crate::prof::define(root, None);
    }

    // Was bietet welches Modul an?
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
        for im in &p.imports {
            // HOOK profil (prof.rs, Runde 52): im Kernel-Profil ist die
            // Standardbibliothek gesperrt. Die Pruefung sitzt hier, weil nur
            // hier die Einbindungen JEDER Datei mit Position bekannt sind.
            crate::prof::hook_import(dg, &im.path, im.span);
            let target = im.path.last().cloned().unwrap_or_default();
            let known = files.iter().any(|g| module_name(g) == target);
            if !known {
                dg.error(
                    im.span,
                    format!("modul '{}' wurde nicht gefunden", im.path.join(".")),
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
        // Zuerst die eigenen Deklarationen umbenennen ...
        let m = infos[idx].name.clone();
        for f in p.funcs.iter_mut() {
            // METHODEN EINES GRUNDTYPS BLEIBEN UNANGETASTET (Runde 50).
            // `impl Ord for i32` legt `i32__kleiner` an; der Typ `i32`
            // gehoert keinem Modul, seine Methoden also auch nicht. Wuerde
            // daraus `vec__i32__kleiner`, suchte `x.kleiner(..)` weiter
            // `i32__kleiner` und faende nichts — dieselbe Regel wie fuer
            // Schnittstellen, gc-Klassen und generische Vorlagen.
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
        // ... dann alle Verweise in den Rumpfen.
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
        // GENERISCHE VORLAGEN DIESER DATEI mit umschreiben.
        //
        // Sie liegen nicht in `Program::funcs`, sondern in
        // `sema_generic::REG` — das Umschreiben oben erreichte sie deshalb
        // nie, und eine Vorlage aus einem Modul sah nur die Namen der
        // WURZELDATEI. Selbst eine Hilfsfunktion in derselben Datei meldete
        // "unbekannte funktion" (docs/SELBSTHOSTING.md §7, Blocker B2).
        //
        // Der NAME der Vorlage bleibt unangetastet: die Auspraegung sucht ihn
        // spaeter unter dem urspruenglichen Namen (`mono::expand_fn` ueber
        // `Instantiation::base`), und generische Namen gelten programmweit.
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
        merged.funcs.append(&mut p.funcs);
        merged.structs.append(&mut p.structs);
        merged.consts.append(&mut p.consts);
        // `comptime { … }`-Bloecke gehoeren zum zusammengefuehrten Programm —
        // sonst laufen sie nie (SPEC §6.4).
        merged.comptime_blocks.append(&mut p.comptime_blocks);
    }
    if dg.has_errors() {
        return None;
    }
    Some(merged)
}

/// Schreibt Namen im AST eines Moduls auf ihre endgueltige Form um.
struct Renamer<'a, 'b> {
    me: usize,
    infos: &'a [ModuleInfo],
    /// Aliasname -> letzter Pfadteil (= Modulname der Zieldatei)
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

    /// Loest einen Namen auf. `is_value` unterscheidet Werte (koennen von
    /// lokalen Namen verdeckt werden) von Funktions-/Typnamen.
    fn resolve(&mut self, name: &str, span: Span, is_value: bool) -> Option<String> {
        if let Some((m, rest)) = name.split_once('.') {
            // qualifizierter Zugriff modul.name
            let target = match self.alias.get(m) {
                Some(t) => t.clone(),
                None => return None,
            };
            let info = match self.infos.iter().find(|i| i.name == target) {
                Some(i) => i,
                None => {
                    self.dg
                        .error(span, format!("modul '{}' ist nicht eingebunden", m));
                    return None;
                }
            };
            if !info.items.contains(rest) {
                self.dg.error(
                    span,
                    format!("modul '{}' hat kein element '{}'", m, rest),
                );
                return None;
            }
            if !info.exports.is_empty() && !info.exports.contains(rest) {
                self.dg.error_note(
                    span,
                    format!("'{}' wird von modul '{}' nicht exportiert", rest, m),
                    "ergaenze den namen in der 'export'-liste des moduls",
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

    /// Namen, die ein Muster bindet, sind im Rumpf des Falles lokal.
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
                if let Some(n) = self.resolve(name, *span, false) {
                    *name = n;
                }
            }
            TypeExpr::Ptr { inner, .. } => self.ty(inner),
            TypeExpr::Array { elem, .. } => self.ty(elem),
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
            // `defer` ist nur eine Huelle: der Inhalt wird wie jede andere
            // Anweisung umgeschrieben.
            Stmt::Defer(inner, _, _) => self.stmt(inner),
            Stmt::Let { name, ty, init, .. } => {
                if let Some(t) = ty.as_mut() {
                    self.ty(t);
                }
                self.expr(init);
                self.declare(name);
            }
            Stmt::Assign { target, value, .. } => {
                self.expr(target);
                self.expr(value);
            }
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
            ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) => {}
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
                // HOOK types: die Rumpfbloecke eines `match` liegen in der
                // Registrierung von `sema_match`, nicht im AST. Ohne diesen
                // Zweig blieben Namen darin unumgeschrieben — `match` in einem
                // importierten Modul waere unbenutzbar.
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
        let src = "import std.io\nimport helfer\nfn main() -> i32 { return 0 }\n";
        let found = scan_imports(src, 0);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].0, vec!["std".to_string(), "io".to_string()]);
        assert_eq!(found[1].0, vec!["helfer".to_string()]);
    }

    #[test]
    fn search_paths_and_module_path() {
        // modul_pfad: teile werden zu <basis>/a/b.fi.
        assert_eq!(
            module_path(Path::new("x"), &["std".to_string(), "math".to_string()]),
            PathBuf::from("x/std/math.fi")
        );
        // FIRNLIB: leer oder ungesetzt heisst "kein zusaetzlicher pfad".
        assert_eq!(firnlib_path(None), None);
        assert_eq!(firnlib_path(Some("")), None);
        assert_eq!(
            firnlib_path(Some("/opt/firn/lib")),
            Some(PathBuf::from("/opt/firn/lib"))
        );
    }

    #[test]
    fn names_become_je_module_different() {
        assert_eq!(mangle("", "main"), "main");
        assert_eq!(mangle("helper", "square"), "helper__square");
        assert_eq!(mangle("", "square"), "square");
        // Linker-Symbole: reservierter Praefix + Schemaversion (DESIGNZIELE 4)
        assert_eq!(symbol("square", None), "_F0.square");
        assert_eq!(symbol("helper__square", None), "_F0.helper__square");
        // Platz fuer die ABI-Version ist da.
        assert_eq!(symbol("helper__square", Some(3)), "_F0.helper__square.v3");
        // Der Einstiegspunkt behaelt seinen nackten Namen.
        assert_eq!(symbol("main", None), "main");
        // Nutzercode kann den Praefix nicht erzeugen: Bezeichner haben keine Punkte.
        assert!(symbol("a", None).starts_with(SYMBOL_PREFIX));
    }
}
