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
struct Regeln {
    ist_alloc: fn(&str) -> bool,
    ist_gc_zeiger: fn(&Type) -> bool,
}

impl Regeln {
    fn echt() -> Regeln {
        Regeln {
            ist_alloc: crate::gc::ist_gc_alloc_aufruf,
            ist_gc_zeiger: crate::gc::ist_gc_zeiger,
        }
    }
}

/// Traegt die Funktion `#[no_gc]`?
pub(crate) fn hat_no_gc(f: &FnDecl) -> bool {
    f.attrs.iter().any(|a| a.name == "no_gc")
}

/// Compilerintern erzeugter Aufrufname (kein Funktionsaufruf im Quelltext).
///
/// `__match#N` (sema_match.rs), `__try#`/`__catch#` (errors.rs) und
/// `Enum::Variante` (lower_match.rs) koennen nie aus einem Bezeichner des
/// Quelltextes entstehen — sie enthalten `#` bzw. `::`.
fn ist_interner_name(name: &str) -> bool {
    name.contains('#') || name.contains("::")
}

/// `helfer__quadrat` (Modulsystem, `modules.rs`) wieder als `helfer.quadrat`
/// schreiben — die Meldung soll den Namen zeigen, der im Quelltext steht.
fn lesbar(name: &str) -> String {
    if name.starts_with('_') || ist_interner_name(name) {
        return name.to_string();
    }
    let teile: Vec<&str> = name.split("__").collect();
    if teile.len() == 2 && !teile[0].is_empty() && !teile[1].is_empty() {
        return format!("{}.{}", teile[0], teile[1]);
    }
    name.to_string()
}

/// `// HOOK nogc` in `sema::Checker::run`: prueft alle `#[no_gc]`-Funktionen.
pub(crate) fn hook_check(ck: &mut Checker, prog: &Program) {
    let befunde = sammle_befunde(prog, &ck.expr_types, Regeln::echt());
    for (span, msg, note) in befunde {
        ck.dg.error_note(span, msg, note);
    }
}

/// Der eigentliche Durchlauf, ohne `Checker` — dadurch einzeln testbar.
fn sammle_befunde(
    prog: &Program,
    expr_types: &[Type],
    regeln: Regeln,
) -> Vec<(Span, String, String)> {
    let mut markiert: HashMap<&str, bool> = HashMap::new();
    for f in &prog.funcs {
        // Bei doppelt deklarierten Namen (eigener Fehler der Typpruefung)
        // zaehlt die strengere Angabe: markiert bleibt markiert.
        let e = markiert.entry(f.name.as_str()).or_insert(false);
        *e |= hat_no_gc(f);
    }
    if !markiert.values().any(|v| *v) {
        return Vec::new();
    }
    let mut p = Pruefer {
        expr_types,
        markiert: &markiert,
        regeln,
        wer: String::new(),
        tiefe: 0,
        out: Vec::new(),
    };
    for f in &prog.funcs {
        if !hat_no_gc(f) {
            continue;
        }
        p.wer = lesbar(&f.name);
        p.pruefe_block(&f.body);
    }
    let mut out = p.out;
    out.sort_by_key(|(s, _, _)| (s.file, s.line, s.col));
    out.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    out
}

struct Pruefer<'a> {
    expr_types: &'a [Type],
    markiert: &'a HashMap<&'a str, bool>,
    regeln: Regeln,
    /// Name der gerade geprueften `#[no_gc]`-Funktion (fuer die Meldung).
    wer: String,
    /// Schachtelungstiefe der `match`-Rumpfbloecke (Reissleine, s. u.).
    tiefe: u32,
    out: Vec<(Span, String, String)>,
}

/// Hoechste Schachtelung von `match`-Faellen, in die hineingesehen wird. Der
/// Parser begrenzt die Verschachtelung ohnehin auf 200; diese Schranke ist
/// die zweite Sicherung gegen eine Rekursionsexplosion.
const MAX_TIEFE: u32 = 256;

impl<'a> Pruefer<'a> {
    fn typ_von(&self, e: &Expr) -> Type {
        self.expr_types
            .get(e.id as usize)
            .cloned()
            .unwrap_or(Type::Error)
    }

    fn melde(&mut self, span: Span, msg: String, note: String) {
        self.out.push((span, msg, note));
    }

    fn pruefe_block(&mut self, b: &Block) {
        for s in &b.stmts {
            self.pruefe_stmt(s);
        }
    }

    fn pruefe_stmt(&mut self, s: &Stmt) {
        match s {
            // Der aufgeschobene Rumpf laeuft im selben Rahmen und unterliegt
            // denselben Regeln.
            Stmt::Defer(inner, _, _) => self.pruefe_stmt(inner),
            Stmt::Let { init, .. } => self.pruefe_expr(init),
            Stmt::Assign { target, value, span } => {
                self.pruefe_schreibziel(target, *span);
                self.pruefe_expr(target);
                self.pruefe_expr(value);
            }
            Stmt::If { cond, then, els, .. } => {
                self.pruefe_expr(cond);
                self.pruefe_block(then);
                if let Some(e) = els {
                    self.pruefe_stmt(e);
                }
            }
            Stmt::While { cond, body, .. } => {
                self.pruefe_expr(cond);
                self.pruefe_block(body);
            }
            Stmt::For { start, end, body, .. } => {
                self.pruefe_expr(start);
                self.pruefe_expr(end);
                self.pruefe_block(body);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.pruefe_expr(v);
                }
            }
            Stmt::Expr(e) => self.pruefe_expr(e),
            Stmt::Block(b) => self.pruefe_block(b),
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Error(_) => {}
        }
    }

    /// Regel 3: Schreiben eines GC-Zeigers in ein Feld (oder in ein
    /// Feldelement). Eine reine Zuweisung an eine oertliche Veraenderliche
    /// (`ExprKind::Ident`) liegt auf dem Stapel, braucht keine Einfuegebarriere
    /// und ist erlaubt.
    fn pruefe_schreibziel(&mut self, target: &Expr, fallback: Span) {
        let ty = self.typ_von(target);
        if !(self.regeln.ist_gc_zeiger)(&ty) {
            return;
        }
        let (was, sp) = match &target.kind {
            ExprKind::Field(_, name, sp) => (format!("das GC-Feld '{}'", name), *sp),
            ExprKind::Index(b, _) => match &b.kind {
                ExprKind::Ident(_) => return, // oertliches Feld auf dem Stapel
                _ => ("ein GC-Element im Speicher".to_string(), target.span),
            },
            ExprKind::Unary(_, _) => ("ein GC-Feld hinter einem Zeiger".to_string(), target.span),
            _ => return,
        };
        let sp = if sp == Span::none() { fallback } else { sp };
        let wer = self.wer.clone();
        self.melde(
            sp,
            format!("'{wer}' ist #[no_gc], schreibt aber in {was}"),
            "SPEC 3.5.4: das Schreiben eines Gc-Zeigers in den Heap braucht die \
             Einfuegebarriere und ist in einem #[no_gc]-Aufrufbaum verboten"
                .to_string(),
        );
    }

    fn pruefe_expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Call(name, args, sp) => {
                self.pruefe_aufruf(name, *sp, e.span);
                for a in args {
                    self.pruefe_expr(a);
                }
                // `match` steht als Aufruf `__match#N` im AST; die Rumpf-
                // bloecke der Faelle liegen in der Registrierung von
                // sema_match.rs. Ohne diesen Abstieg waere jede Zustands-
                // maschine ein blinder Fleck.
                self.pruefe_match_faelle(name);
            }
            ExprKind::Unary(_, a) => self.pruefe_expr(a),
            ExprKind::Binary(_, a, b) => {
                self.pruefe_expr(a);
                self.pruefe_expr(b);
            }
            ExprKind::Field(b, _, _) => self.pruefe_expr(b),
            ExprKind::Index(b, i) => {
                self.pruefe_expr(b);
                self.pruefe_expr(i);
            }
            ExprKind::Syscall(args) | ExprKind::ArrayLit(args) => {
                for a in args {
                    self.pruefe_expr(a);
                }
            }
            ExprKind::Cast(a, _) => self.pruefe_expr(a),
            ExprKind::StructLit(_, felder, _) => {
                for (_, v, _) in felder {
                    self.pruefe_expr(v);
                }
            }
            ExprKind::ArrayRepeat(v, n) => {
                self.pruefe_expr(v);
                self.pruefe_expr(n);
            }
            ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::Ident(_) => {}
        }
    }

    fn pruefe_aufruf(&mut self, name: &str, sp: Span, fallback: Span) {
        let sp = if sp == Span::none() { fallback } else { sp };
        let wer = self.wer.clone();
        if (self.regeln.ist_alloc)(name) {
            // Regel 1: GC-Allokation — kann einen Sammellauf ausloesen.
            self.melde(
                sp,
                format!(
                    "'{wer}' ist #[no_gc], alloziert aber ueber '{}' auf dem GC-Heap",
                    lesbar(name)
                ),
                "SPEC 3.5.4: in einem #[no_gc]-Aufrufbaum darf kein Sammellauf \
                 ausgeloest werden"
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
        if let Some(m) = crate::impls::methodenname(name) {
            self.melde(
                sp,
                format!("'{wer}' ist #[no_gc], ruft aber die methode '{m}'"),
                "SPEC 3.5.4: die zusage gilt transitiv — welche funktion hinter einem \
                 methodenaufruf steht, entscheidet der empfaengertyp; rufe die funktion \
                 hier direkt auf (Typ__methode) oder verzichte auf #[no_gc]"
                    .to_string(),
            );
            return;
        }
        if ist_interner_name(name) {
            return;
        }
        match self.markiert.get(name) {
            Some(false) => {
                // Regel 2: Aufruf ohne #[no_gc] — bricht die Kette.
                self.melde(
                    sp,
                    format!(
                        "'{wer}' ist #[no_gc], ruft aber '{}' ohne #[no_gc]",
                        lesbar(name)
                    ),
                    format!(
                        "SPEC 3.5.4: die Zusage gilt transitiv fuer den ganzen Aufrufbaum — \
                         schreibe #[no_gc] vor '{}' oder rufe es hier nicht auf",
                        lesbar(name)
                    ),
                );
            }
            // markiert: in Ordnung. Unbekannt: die Typpruefung meldet den
            // unbekannten Namen selbst, hier kein zweiter Fehler.
            Some(true) | None => {}
        }
    }

    fn pruefe_match_faelle(&mut self, name: &str) {
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
        if self.tiefe >= MAX_TIEFE {
            return;
        }
        self.tiefe += 1;
        self.pruefe_expr(&mi.subject);
        for arm in &mi.arms {
            self.pruefe_block(&arm.body);
        }
        self.tiefe -= 1;
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

    struct Bau {
        naechste: ExprId,
    }

    impl Bau {
        fn neu() -> Bau {
            Bau { naechste: 0 }
        }
        fn expr(&mut self, sp: Span, kind: ExprKind) -> Expr {
            let id = self.naechste;
            self.naechste += 1;
            Expr { id, span: sp, kind }
        }
        fn aufruf(&mut self, name: &str, sp: Span) -> Expr {
            self.expr(sp, ExprKind::Call(name.to_string(), Vec::new(), sp))
        }
        fn ident(&mut self, name: &str, sp: Span) -> Expr {
            self.expr(sp, ExprKind::Ident(name.to_string()))
        }
        fn feld(&mut self, basis: Expr, name: &str, sp: Span) -> Expr {
            self.expr(sp, ExprKind::Field(Box::new(basis), name.to_string(), sp))
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

    fn programm(funcs: Vec<FnDecl>, expr_count: u32) -> Program {
        Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs,
            structs: Vec::new(),
            consts: Vec::new(),
            expr_count,
            comptime_bloecke: Vec::new(),
        }
    }

    /// Vorhersagen fuer die Selbsttests: `gc_neu` alloziert, `Gc[T]` wird
    /// durch `Type::Ptr` vertreten.
    fn test_regeln() -> Regeln {
        fn alloc(n: &str) -> bool {
            n == "gc_neu"
        }
        fn zeiger(t: &Type) -> bool {
            matches!(t, Type::Ptr { .. })
        }
        Regeln { ist_alloc: alloc, ist_gc_zeiger: zeiger }
    }

    // ------------------------------------------------------------- Regeln

    #[test]
    fn regel1_gc_allokation_ist_verboten() {
        let mut b = Bau::neu();
        let ruf = b.aufruf("gc_neu", span(7, 12));
        let n = b.naechste;
        let prog = programm(vec![fndecl("heiss", true, vec![Stmt::Expr(ruf)])], n);
        let befunde = sammle_befunde(&prog, &vec![Type::I32; n as usize], test_regeln());
        assert_eq!(befunde.len(), 1, "{:?}", befunde);
        assert_eq!((befunde[0].0.line, befunde[0].0.col), (7, 12));
        assert!(befunde[0].1.contains("alloziert"), "{}", befunde[0].1);
    }

    #[test]
    fn regel2_aufruf_ohne_no_gc_ist_verboten() {
        let mut b = Bau::neu();
        let ruf = b.aufruf("kalt", span(9, 5));
        let n = b.naechste;
        let prog = programm(
            vec![
                fndecl("heiss", true, vec![Stmt::Expr(ruf)]),
                fndecl("kalt", false, Vec::new()),
            ],
            n,
        );
        let befunde = sammle_befunde(&prog, &vec![Type::I32; n as usize], test_regeln());
        assert_eq!(befunde.len(), 1, "{:?}", befunde);
        assert_eq!((befunde[0].0.line, befunde[0].0.col), (9, 5));
        assert!(befunde[0].1.contains("ohne #[no_gc]"), "{}", befunde[0].1);
    }

    #[test]
    fn regel2_markierter_aufruf_ist_erlaubt() {
        let mut b = Bau::neu();
        let ruf = b.aufruf("auch_heiss", span(9, 5));
        let n = b.naechste;
        let prog = programm(
            vec![
                fndecl("heiss", true, vec![Stmt::Expr(ruf)]),
                fndecl("auch_heiss", true, Vec::new()),
            ],
            n,
        );
        assert!(sammle_befunde(&prog, &vec![Type::I32; n as usize], test_regeln()).is_empty());
    }

    #[test]
    fn regel3_schreiben_in_gc_feld_ist_verboten() {
        let mut b = Bau::neu();
        let basis = b.ident("knoten", span(4, 5));
        let ziel = b.feld(basis, "elternteil", span(4, 12));
        let wert = b.ident("anderer", span(4, 26));
        let n = b.naechste;
        let ziel_id = ziel.id as usize;
        let stmt = Stmt::Assign { target: ziel, value: wert, span: span(4, 5) };
        let prog = programm(vec![fndecl("heiss", true, vec![stmt])], n);
        let mut typen = vec![Type::I32; n as usize];
        typen[ziel_id] = Type::Ptr { mutable: true, inner: Box::new(Type::I32) };
        let befunde = sammle_befunde(&prog, &typen, test_regeln());
        assert_eq!(befunde.len(), 1, "{:?}", befunde);
        assert_eq!((befunde[0].0.line, befunde[0].0.col), (4, 12));
        assert!(befunde[0].1.contains("GC-Feld 'elternteil'"), "{}", befunde[0].1);
    }

    #[test]
    fn regel3_zuweisung_an_oertliche_veraenderliche_ist_erlaubt() {
        let mut b = Bau::neu();
        let ziel = b.ident("x", span(4, 5));
        let wert = b.ident("y", span(4, 9));
        let n = b.naechste;
        let ziel_id = ziel.id as usize;
        let stmt = Stmt::Assign { target: ziel, value: wert, span: span(4, 5) };
        let prog = programm(vec![fndecl("heiss", true, vec![stmt])], n);
        let mut typen = vec![Type::I32; n as usize];
        typen[ziel_id] = Type::Ptr { mutable: true, inner: Box::new(Type::I32) };
        assert!(sammle_befunde(&prog, &typen, test_regeln()).is_empty());
    }

    #[test]
    fn unmarkierte_funktion_wird_nicht_geprueft() {
        let mut b = Bau::neu();
        let ruf = b.aufruf("gc_neu", span(3, 3));
        let n = b.naechste;
        let prog = programm(vec![fndecl("kalt", false, vec![Stmt::Expr(ruf)])], n);
        assert!(sammle_befunde(&prog, &vec![Type::I32; n as usize], test_regeln()).is_empty());
    }

    #[test]
    fn interne_namen_loesen_keinen_fehler_aus() {
        let mut b = Bau::neu();
        let m = b.aufruf("__match#0", span(3, 3));
        let t = b.aufruf("__try#", span(4, 3));
        let c = b.aufruf("Farbe::Rot", span(5, 3));
        let u = b.aufruf("gibtesnicht", span(6, 3));
        let n = b.naechste;
        let prog = programm(
            vec![fndecl(
                "heiss",
                true,
                vec![Stmt::Expr(m), Stmt::Expr(t), Stmt::Expr(c), Stmt::Expr(u)],
            )],
            n,
        );
        assert!(sammle_befunde(&prog, &vec![Type::I32; n as usize], test_regeln()).is_empty());
    }

    #[test]
    fn modulname_wird_lesbar_gemeldet() {
        let mut b = Bau::neu();
        let ruf = b.aufruf("helfer__quadrat", span(11, 12));
        let n = b.naechste;
        let prog = programm(
            vec![
                fndecl("heiss", true, vec![Stmt::Expr(ruf)]),
                fndecl("helfer__quadrat", false, Vec::new()),
            ],
            n,
        );
        let befunde = sammle_befunde(&prog, &vec![Type::I32; n as usize], test_regeln());
        assert_eq!(befunde.len(), 1, "{:?}", befunde);
        assert!(befunde[0].1.contains("'helfer.quadrat'"), "{}", befunde[0].1);
    }

    #[test]
    fn tiefe_verschachtelung_wird_erreicht() {
        // Der Verstoss steckt in einer if-in-while-in-if-Kette.
        let mut b = Bau::neu();
        let ruf = b.aufruf("kalt", span(20, 9));
        let cond1 = b.expr(span(10, 1), ExprKind::Bool(true));
        let cond2 = b.expr(span(11, 1), ExprKind::Bool(true));
        let innen = Stmt::If {
            cond: cond2,
            then: Block { stmts: vec![Stmt::Expr(ruf)], span: span(11, 1) },
            els: None,
            span: span(11, 1),
        };
        let mitte = Stmt::While {
            cond: cond1,
            body: Block { stmts: vec![innen], span: span(10, 1) },
            span: span(10, 1),
        };
        let n = b.naechste;
        let prog = programm(
            vec![
                fndecl("heiss", true, vec![Stmt::Block(Block { stmts: vec![mitte], span: span(9, 1) })]),
                fndecl("kalt", false, Vec::new()),
            ],
            n,
        );
        let befunde = sammle_befunde(&prog, &vec![Type::I32; n as usize], test_regeln());
        assert_eq!(befunde.len(), 1, "{:?}", befunde);
        assert_eq!((befunde[0].0.line, befunde[0].0.col), (20, 9));
    }

    #[test]
    fn hat_no_gc_erkennt_das_attribut() {
        assert!(hat_no_gc(&fndecl("a", true, Vec::new())));
        assert!(!hat_no_gc(&fndecl("a", false, Vec::new())));
    }

    #[test]
    fn echte_regeln_sind_die_aus_gc_rs() {
        // Der Vertrag: der Compiler fragt ausschliesslich gc.rs.
        let r = Regeln::echt();
        assert_eq!((r.ist_alloc)("main"), crate::gc::ist_gc_alloc_aufruf("main"));
        assert_eq!((r.ist_gc_zeiger)(&Type::I32), crate::gc::ist_gc_zeiger(&Type::I32));
    }
}
