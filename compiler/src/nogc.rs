//! `#[no_gc]` — die Garantie fuer heisse Pfade, SPEC §3.5.4.
//!
//! In einer `#[no_gc]`-Funktion sind verboten:
//!  1. GC-Allokation (und jeder Aufruf, der einen Sammellauf ausloesen kann),
//!  2. Aufruf einer Funktion **ohne** `#[no_gc]`,
//!  3. Schreiben eines `Gc[T]`/`GcWeak[T]`-Zeigers in ein Feld oder ein
//!     Feldelement (Einfuegebarriere).
//!
//! Die Pruefung ist **transitiv**: weil jede aufgerufene Funktion selbst
//! `#[no_gc]` tragen muss, gilt die Zusage fuer den ganzen Aufrufbaum. Bricht
//! die Kette — auch ueber mehrere Ebenen oder ueber eine Modulgrenze hinweg —
//! gibt es einen Fehler mit Zeile und Spalte.
//!
//! Angebunden ueber die Zeile `// HOOK nogc` in `sema::Checker::run`. Die
//! GC-spezifischen Abfragen (1 und 3) kommen aus `gc.rs` — diese Datei kennt
//! den GC nur ueber diese zwei Funktionen und ist damit unabhaengig davon
//! baubar. Fuer die Selbsttests am Ende der Datei sind die beiden Abfragen
//! in `Regeln` gebuendelt, damit die Regeln 1 und 3 auch ohne gebauten
//! Sammler nachweisbar sind; im Compiler laeuft **immer** `Regeln::echt()`.
//!
//! ## Umfang, ehrlich benannt
//!
//! * Geprueft werden alle Aufrufe im Rumpf, **einschliesslich** der Rumpf-
//!   bloecke von `match`-Faellen (die liegen nicht im AST, sondern in der
//!   Registrierung von `sema_match.rs`).
//! * Compilerinterne Aufrufnamen (`__match#N`, `__try#`, `__catch#`,
//!   `Enum::Variante`) sind keine Funktionsaufrufe und loesen keinen Fehler
//!   aus; ihre Argumente werden trotzdem durchsucht.
//! * Aufrufe eines Namens, den es gar nicht gibt, meldet die Typpruefung
//!   selbst — hier gibt es dafuer keinen zweiten, verwirrenden Fehler.
//! * Aufrufe ueber Funktionszeiger gibt es in Stufe 0 nicht (`ExprKind::Call`
//!   traegt immer einen Namen), es gibt hier also kein Schlupfloch.
//!
//! Diese Datei gehoert dem Modul `nogc` (PLAN.md, Runde „Haertetest 2").

use std::collections::HashMap;

use crate::ast::{Block, Expr, ExprKind, FnDecl, Program, Stmt};
use crate::diag::Span;
use crate::sema::Checker;
use crate::types::Type;

/// Die beiden GC-Abfragen, die diese Datei braucht. Im Compiler immer
/// `Regeln::echt()` (Vertrag von `gc.rs`); die Selbsttests setzen eigene
/// Vorhersagen ein, damit die Regeln 1 und 3 pruefbar sind, bevor der
/// Sammler steht.
#[derive(Clone, Copy)]
struct Rules {
    is_alloc: fn(&str) -> bool,
    is_gc_ref: fn(&Type) -> bool,
}

impl Rules {
    fn real() -> Rules {
        Rules {
            is_alloc: crate::gc::is_gc_alloc_call,
            is_gc_ref: crate::gc::is_gc_ref,
        }
    }
}

/// Traegt die Funktion `#[no_gc]`?
pub(crate) fn has_no_gc(f: &FnDecl) -> bool {
    f.attrs.iter().any(|a| a.name == "no_gc")
}

/// Compilerintern erzeugter Aufrufname (kein Funktionsaufruf im Quelltext).
///
/// `__match#N` (sema_match.rs), `__try#`/`__catch#` (errors.rs) und
/// `Enum::Variante` (lower_match.rs) koennen nie aus einem Bezeichner des
/// Quelltextes entstehen — sie enthalten `#` bzw. `::`.
fn is_interner_name(name: &str) -> bool {
    name.contains('#') || name.contains("::")
}

/// `helper__square` (Modulsystem, `modules.rs`) wieder als `helper.square`
/// schreiben — die Meldung soll den Namen zeigen, der im Quelltext steht.
fn readable(name: &str) -> String {
    if name.starts_with('_') || is_interner_name(name) {
        return name.to_string();
    }
    let parts: Vec<&str> = name.split("__").collect();
    if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
        return format!("{}.{}", parts[0], parts[1]);
    }
    name.to_string()
}

/// `// HOOK nogc` in `sema::Checker::run`: prueft alle `#[no_gc]`-Funktionen.
pub(crate) fn hook_check(ck: &mut Checker, prog: &Program) {
    let findings = collect_findings(prog, &ck.expr_types, Rules::real());
    for (span, msg, note) in findings {
        ck.dg.error_note(span, msg, note);
    }
}

/// Der eigentliche Durchlauf, ohne `Checker` — dadurch einzeln testbar.
fn collect_findings(
    prog: &Program,
    expr_types: &[Type],
    rules: Rules,
) -> Vec<(Span, String, String)> {
    let mut marked: HashMap<&str, bool> = HashMap::new();
    for f in &prog.funcs {
        // Bei doppelt deklarierten Namen (eigener Fehler der Typpruefung)
        // zaehlt die strengere Angabe: markiert bleibt markiert.
        let e = marked.entry(f.name.as_str()).or_insert(false);
        *e |= has_no_gc(f);
    }
    if !marked.values().any(|v| *v) {
        return Vec::new();
    }
    let mut p = NoGcChecker {
        expr_types,
        marked: &marked,
        rules,
        who: String::new(),
        depth: 0,
        out: Vec::new(),
    };
    for f in &prog.funcs {
        if !has_no_gc(f) {
            continue;
        }
        p.who = readable(&f.name);
        p.check_block(&f.body);
    }
    let mut out = p.out;
    out.sort_by_key(|(s, _, _)| (s.file, s.line, s.col));
    out.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    out
}

struct NoGcChecker<'a> {
    expr_types: &'a [Type],
    marked: &'a HashMap<&'a str, bool>,
    rules: Rules,
    /// Name der gerade geprueften `#[no_gc]`-Funktion (fuer die Meldung).
    who: String,
    /// Schachtelungstiefe der `match`-Rumpfbloecke (Reissleine, s. u.).
    depth: u32,
    out: Vec<(Span, String, String)>,
}

/// Hoechste Schachtelung von `match`-Faellen, in die hineingesehen wird. Der
/// Parser begrenzt die Verschachtelung ohnehin auf 200; diese Schranke ist
/// die zweite Sicherung gegen eine Rekursionsexplosion.
const MAX_DEPTH: u32 = 256;

impl<'a> NoGcChecker<'a> {
    fn ty_of(&self, e: &Expr) -> Type {
        self.expr_types
            .get(e.id as usize)
            .cloned()
            .unwrap_or(Type::Error)
    }

    fn report(&mut self, span: Span, msg: String, note: String) {
        self.out.push((span, msg, note));
    }

    fn check_block(&mut self, b: &Block) {
        for s in &b.stmts {
            self.check_stmt(s);
        }
    }

    fn check_stmt(&mut self, s: &Stmt) {
        match s {
            // Der aufgeschobene Rumpf laeuft im selben Rahmen und unterliegt
            // denselben Regeln.
            Stmt::Defer(inner, _, _) => self.check_stmt(inner),
            Stmt::Let { init, .. } => self.check_expr(init),
            Stmt::Assign { target, value, span } => {
                self.check_write_target(target, *span);
                self.check_expr(target);
                self.check_expr(value);
            }
            Stmt::If { cond, then, els, .. } => {
                self.check_expr(cond);
                self.check_block(then);
                if let Some(e) = els {
                    self.check_stmt(e);
                }
            }
            Stmt::While { cond, body, .. } => {
                self.check_expr(cond);
                self.check_block(body);
            }
            Stmt::For { start, end, body, .. } => {
                self.check_expr(start);
                self.check_expr(end);
                self.check_block(body);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.check_expr(v);
                }
            }
            Stmt::Expr(e) => self.check_expr(e),
            Stmt::Block(b) => self.check_block(b),
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Error(_) => {}
        }
    }

    /// Regel 3: Schreiben eines GC-Zeigers in ein Feld (oder in ein
    /// Feldelement). Eine reine Zuweisung an eine oertliche Veraenderliche
    /// (`ExprKind::Ident`) liegt auf dem Stapel, braucht keine Einfuegebarriere
    /// und ist erlaubt.
    fn check_write_target(&mut self, target: &Expr, fallback: Span) {
        let ty = self.ty_of(target);
        if !(self.rules.is_gc_ref)(&ty) {
            return;
        }
        let (what, sp) = match &target.kind {
            ExprKind::Field(_, name, sp) => (format!("the GC field '{}'", name), *sp),
            ExprKind::Index(b, _) => match &b.kind {
                ExprKind::Ident(_) => return, // oertliches Feld auf dem Stapel
                _ => ("a GC element in memory".to_string(), target.span),
            },
            ExprKind::Unary(_, _) => ("a GC field behind a pointer".to_string(), target.span),
            _ => return,
        };
        let sp = if sp == Span::none() { fallback } else { sp };
        let who = self.who.clone();
        self.report(
            sp,
            format!("'{who}' is #[no_gc], but writes into {what}"),
            "SPEC 3.5.4: writing a Gc pointer into the heap needs the insertion \
             barrier and is forbidden in a #[no_gc] call tree"
                .to_string(),
        );
    }

    fn check_expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Call(name, args, sp) => {
                self.check_call(name, *sp, e.span);
                for a in args {
                    self.check_expr(a);
                }
                // `match` steht als Aufruf `__match#N` im AST; die Rumpf-
                // bloecke der Faelle liegen in der Registrierung von
                // sema_match.rs. Ohne diesen Abstieg waere jede Zustands-
                // maschine ein blinder Fleck.
                self.check_match_cases(name);
            }
            ExprKind::Unary(_, a) => self.check_expr(a),
            ExprKind::Binary(_, a, b) => {
                self.check_expr(a);
                self.check_expr(b);
            }
            ExprKind::Field(b, _, _) => self.check_expr(b),
            ExprKind::Index(b, i) => {
                self.check_expr(b);
                self.check_expr(i);
            }
            ExprKind::Syscall(args) | ExprKind::ArrayLit(args) => {
                for a in args {
                    self.check_expr(a);
                }
            }
            ExprKind::Cast(a, _) => self.check_expr(a),
            ExprKind::StructLit(_, fields, _) => {
                for (_, v, _) in fields {
                    self.check_expr(v);
                }
            }
            ExprKind::ArrayRepeat(v, n) => {
                self.check_expr(v);
                self.check_expr(n);
            }
            ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::Ident(_) => {}
        }
    }

    fn check_call(&mut self, name: &str, sp: Span, fallback: Span) {
        let sp = if sp == Span::none() { fallback } else { sp };
        let who = self.who.clone();
        if (self.rules.is_alloc)(name) {
            // Regel 1: GC-Allokation — kann einen Sammellauf ausloesen.
            self.report(
                sp,
                format!(
                    "'{who}' is #[no_gc], but allocates on the GC heap via '{}'",
                    readable(name)
                ),
                "SPEC 3.5.4: no collection run may be triggered in a #[no_gc] \
                 call tree"
                    .to_string(),
            );
            return;
        }
        // HOOK impl: `x.m(..)` steht bis zur Typpruefung als `"methode m"`
        // im Baum — welche Funktion gemeint ist, weiss erst der Typpruefer.
        // Diese Pruefung laeuft ohne Typtabelle der Empfaenger, also wird der
        // Fall AUSDRUECKLICH abgelehnt statt stillschweigend uebergangen: ein
        // Loch in einer Zusage waere schlimmer als eine fehlende Bequemlichkeit
        // (Runde 45, impls.rs).
        if let Some(m) = crate::impls::method_name(name) {
            self.report(
                sp,
                format!("'{who}' is #[no_gc], but calls the method '{m}'"),
                "SPEC 3.5.4: the promise holds transitively — which function stands behind a \
                 method call is decided by the receiver type; call the function \
                 directly here (Type__method) or give up #[no_gc]"
                    .to_string(),
            );
            return;
        }
        if is_interner_name(name) {
            return;
        }
        match self.marked.get(name) {
            Some(false) => {
                // Regel 2: Aufruf ohne #[no_gc] — bricht die Kette.
                self.report(
                    sp,
                    format!(
                        "'{who}' is #[no_gc], but calls '{}' without #[no_gc]",
                        readable(name)
                    ),
                    format!(
                        "SPEC 3.5.4: the promise holds transitively for the whole call tree — \
                         write #[no_gc] before '{}' or do not call it here",
                        readable(name)
                    ),
                );
            }
            // markiert: in Ordnung. Unbekannt: die Typpruefung meldet den
            // unbekannten Namen selbst, hier kein zweiter Fehler.
            Some(true) | None => {}
        }
    }

    fn check_match_cases(&mut self, name: &str) {
        let idx = match name
            .strip_prefix(crate::sema_match::MATCH_PREFIX)
            .and_then(|s| s.parse::<usize>().ok())
        {
            Some(i) => i,
            None => return,
        };
        let mi = match crate::sema_match::match_info(idx) {
            Some(m) => m,
            None => return,
        };
        if self.depth >= MAX_DEPTH {
            return;
        }
        self.depth += 1;
        self.check_expr(&mi.subject);
        for arm in &mi.arms {
            self.check_block(&arm.body);
        }
        self.depth -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Attr, ExprId};

    // ------------------------------------------------------- AST von Hand

    fn span(line: u32, col: u32) -> Span {
        Span { file: 0, line, col, len: 1 }
    }

    struct Build {
        next: ExprId,
    }

    impl Build {
        fn new() -> Build {
            Build { next: 0 }
        }
        fn expr(&mut self, sp: Span, kind: ExprKind) -> Expr {
            let id = self.next;
            self.next += 1;
            Expr { id, span: sp, kind }
        }
        fn call(&mut self, name: &str, sp: Span) -> Expr {
            self.expr(sp, ExprKind::Call(name.to_string(), Vec::new(), sp))
        }
        fn ident(&mut self, name: &str, sp: Span) -> Expr {
            self.expr(sp, ExprKind::Ident(name.to_string()))
        }
        fn field(&mut self, base: Expr, name: &str, sp: Span) -> Expr {
            self.expr(sp, ExprKind::Field(Box::new(base), name.to_string(), sp))
        }
    }

    fn fndecl(name: &str, no_gc: bool, stmts: Vec<Stmt>) -> FnDecl {
        FnDecl {
            name: name.to_string(),
            params: Vec::new(),
            ret: None,
            body: Block { stmts, span: span(1, 1) },
            span: span(1, 1),
            attrs: if no_gc {
                vec![Attr { name: "no_gc".to_string(), args: Vec::new(), span: span(1, 1) }]
            } else {
                Vec::new()
            },
        }
    }

    fn program(funcs: Vec<FnDecl>, expr_count: u32) -> Program {
        Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs,
            structs: Vec::new(),
            consts: Vec::new(),
            expr_count,
            comptime_blocks: Vec::new(),
        }
    }

    /// Vorhersagen fuer die Selbsttests: `gc_neu` alloziert, `Gc[T]` wird
    /// durch `Type::Ptr` vertreten.
    fn test_rules() -> Rules {
        fn alloc(n: &str) -> bool {
            n == "gc_new"
        }
        fn ptr(t: &Type) -> bool {
            matches!(t, Type::Ptr { .. })
        }
        Rules { is_alloc: alloc, is_gc_ref: ptr }
    }

    // ------------------------------------------------------------- Regeln

    #[test]
    fn regel1_gc_allocation_is_forbidden() {
        let mut b = Build::new();
        let call = b.call("gc_new", span(7, 12));
        let n = b.next;
        let prog = program(vec![fndecl("hot", true, vec![Stmt::Expr(call)])], n);
        let findings = collect_findings(&prog, &vec![Type::I32; n as usize], test_rules());
        assert_eq!(findings.len(), 1, "{:?}", findings);
        assert_eq!((findings[0].0.line, findings[0].0.col), (7, 12));
        assert!(findings[0].1.contains("allocates"), "{}", findings[0].1);
    }

    #[test]
    fn regel2_call_without_no_gc_is_forbidden() {
        let mut b = Build::new();
        let call = b.call("cold", span(9, 5));
        let n = b.next;
        let prog = program(
            vec![
                fndecl("hot", true, vec![Stmt::Expr(call)]),
                fndecl("cold", false, Vec::new()),
            ],
            n,
        );
        let findings = collect_findings(&prog, &vec![Type::I32; n as usize], test_rules());
        assert_eq!(findings.len(), 1, "{:?}", findings);
        assert_eq!((findings[0].0.line, findings[0].0.col), (9, 5));
        assert!(findings[0].1.contains("without #[no_gc]"), "{}", findings[0].1);
    }

    #[test]
    fn regel2_marked_call_is_allowed() {
        let mut b = Build::new();
        let call = b.call("also_hot", span(9, 5));
        let n = b.next;
        let prog = program(
            vec![
                fndecl("hot", true, vec![Stmt::Expr(call)]),
                fndecl("also_hot", true, Vec::new()),
            ],
            n,
        );
        assert!(collect_findings(&prog, &vec![Type::I32; n as usize], test_rules()).is_empty());
    }

    #[test]
    fn regel3_write_in_gc_field_is_forbidden() {
        let mut b = Build::new();
        let base = b.ident("node", span(4, 5));
        let target = b.field(base, "parent", span(4, 12));
        let value = b.ident("other", span(4, 26));
        let n = b.next;
        let target_id = target.id as usize;
        let stmt = Stmt::Assign { target: target, value: value, span: span(4, 5) };
        let prog = program(vec![fndecl("hot", true, vec![stmt])], n);
        let mut types = vec![Type::I32; n as usize];
        types[target_id] = Type::Ptr { mutable: true, inner: Box::new(Type::I32) };
        let findings = collect_findings(&prog, &types, test_rules());
        assert_eq!(findings.len(), 1, "{:?}", findings);
        assert_eq!((findings[0].0.line, findings[0].0.col), (4, 12));
        assert!(findings[0].1.contains("GC field 'parent'"), "{}", findings[0].1);
    }

    #[test]
    fn regel3_assign_an_local_mutable_is_allowed() {
        let mut b = Build::new();
        let target = b.ident("x", span(4, 5));
        let value = b.ident("y", span(4, 9));
        let n = b.next;
        let target_id = target.id as usize;
        let stmt = Stmt::Assign { target: target, value: value, span: span(4, 5) };
        let prog = program(vec![fndecl("hot", true, vec![stmt])], n);
        let mut types = vec![Type::I32; n as usize];
        types[target_id] = Type::Ptr { mutable: true, inner: Box::new(Type::I32) };
        assert!(collect_findings(&prog, &types, test_rules()).is_empty());
    }

    #[test]
    fn unmarked_func_becomes_not_checked() {
        let mut b = Build::new();
        let call = b.call("gc_new", span(3, 3));
        let n = b.next;
        let prog = program(vec![fndecl("cold", false, vec![Stmt::Expr(call)])], n);
        assert!(collect_findings(&prog, &vec![Type::I32; n as usize], test_rules()).is_empty());
    }

    #[test]
    fn internal_names_solve_no_error_out() {
        let mut b = Build::new();
        let m = b.call("__match#0", span(3, 3));
        let t = b.call("__try#", span(4, 3));
        let c = b.call("Color::Red", span(5, 3));
        let u = b.call("does_not_exist", span(6, 3));
        let n = b.next;
        let prog = program(
            vec![fndecl(
                "hot",
                true,
                vec![Stmt::Expr(m), Stmt::Expr(t), Stmt::Expr(c), Stmt::Expr(u)],
            )],
            n,
        );
        assert!(collect_findings(&prog, &vec![Type::I32; n as usize], test_rules()).is_empty());
    }

    #[test]
    fn module_name_becomes_readable_reported() {
        let mut b = Build::new();
        let call = b.call("helper__square", span(11, 12));
        let n = b.next;
        let prog = program(
            vec![
                fndecl("hot", true, vec![Stmt::Expr(call)]),
                fndecl("helper__square", false, Vec::new()),
            ],
            n,
        );
        let findings = collect_findings(&prog, &vec![Type::I32; n as usize], test_rules());
        assert_eq!(findings.len(), 1, "{:?}", findings);
        assert!(findings[0].1.contains("'helper.square'"), "{}", findings[0].1);
    }

    #[test]
    fn depth_nesting_becomes_reaches() {
        // Der Verstoss steckt in einer if-in-while-in-if-Kette.
        let mut b = Build::new();
        let call = b.call("cold", span(20, 9));
        let cond1 = b.expr(span(10, 1), ExprKind::Bool(true));
        let cond2 = b.expr(span(11, 1), ExprKind::Bool(true));
        let inner = Stmt::If {
            cond: cond2,
            then: Block { stmts: vec![Stmt::Expr(call)], span: span(11, 1) },
            els: None,
            span: span(11, 1),
        };
        let mid = Stmt::While {
            cond: cond1,
            body: Block { stmts: vec![inner], span: span(10, 1) },
            span: span(10, 1),
        };
        let n = b.next;
        let prog = program(
            vec![
                fndecl("hot", true, vec![Stmt::Block(Block { stmts: vec![mid], span: span(9, 1) })]),
                fndecl("cold", false, Vec::new()),
            ],
            n,
        );
        let findings = collect_findings(&prog, &vec![Type::I32; n as usize], test_rules());
        assert_eq!(findings.len(), 1, "{:?}", findings);
        assert_eq!((findings[0].0.line, findings[0].0.col), (20, 9));
    }

    #[test]
    fn has_no_gc_recognizes_the_attr() {
        assert!(has_no_gc(&fndecl("a", true, Vec::new())));
        assert!(!has_no_gc(&fndecl("a", false, Vec::new())));
    }

    #[test]
    fn real_rules_are_the_out_gc_rs() {
        // Der Vertrag: der Compiler fragt ausschliesslich gc.rs.
        let r = Rules::real();
        assert_eq!((r.is_alloc)("main"), crate::gc::is_gc_alloc_call("main"));
        assert_eq!((r.is_gc_ref)(&Type::I32), crate::gc::is_gc_ref(&Type::I32));
    }
}
