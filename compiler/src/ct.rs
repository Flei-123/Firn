//! Constant-Time-Primitive (SPEC §9.2/§9.3) — `select`, `barrier`,
//! `secure_zero`.
//!
//! Diese drei Primitive sind der Teil von SPEC §9, der OHNE den Typqualifizierer
//! `secret[T]` schon heute etwas Handfestes tut und den der Codegenerator
//! bereits kennt (`fir::Op::Select`, `Op::Barrier`, `Op::SecureZero`):
//!
//! * `select(bedingung, a, b)` — datenunabhaengige Auswahl, wird im Backend zu
//!   `cmov`. Kein Durchgang darf daraus eine Verzweigung machen (SPEC §9.2);
//!   `mem2reg` und `opt` behandeln `Op::Select` deshalb als unantastbar.
//! * `barrier(x)` — undurchsichtige Sperre: liefert `x` unveraendert zurueck,
//!   gilt aber fuer jeden Durchgang als undurchschaubar (kein CSE, keine
//!   Konstantenfaltung ueber die Sperre hinweg).
//! * `secure_zero(zeiger, anzahl)` — nullt `anzahl` Bytes ab `zeiger` und gilt
//!   NIE als toter Code (SPEC §9.3, `C3`).
//!
//! **Bewusst noch nicht hier:** `secret[T]` als Typqualifizierer, die Ausbreitung
//! der Markierung durch Ausdruecke, `declassify` und die Wirkung von
//! `#[constant_time]`. Solange es keine `secret`-Werte gibt, ist die Pruefung im
//! Codegenerator (`f.constant_time && f.is_secret(cond)`) zwar vorhanden, aber
//! ohne Futter — deshalb bleibt `#[constant_time]` in `attrs.rs` weiter als
//! *nicht umgesetzt* gefuehrt und meldet einen sauberen Fehler. Festgehalten in
//! SPEC §14.1.
//!
//! `barrier(inout x)`/`secure_zero(inout buf)` aus SPEC §9 brauchen `inout`,
//! das es in Stufe 0 nicht gibt; die Stufe-0-Form nimmt den Wert bzw. einen
//! Zeiger samt Laenge. Auch das steht in SPEC §14.1.

use crate::ast::Expr;
use crate::diag::Span;
use crate::fir::{FTy, Op, Val};
use crate::lower::Lower;
use crate::sema::Checker;
use crate::types::Type;

/// Namen der eingebauten Primitive.
pub(crate) const SELECT: &str = "select";
pub(crate) const BARRIER: &str = "barrier";
pub(crate) const SECURE_ZERO: &str = "secure_zero";

/// Ist `name` der Name eines eingebauten Constant-Time-Primitivs?
pub(crate) fn is_ct_call(name: &str) -> bool {
    matches!(name, SELECT | BARRIER | SECURE_ZERO)
}

/// Darf an einem Wert dieses Typs `select`/`barrier` arbeiten?
/// Nur skalare Typen: Ganzzahl, `bool`, Zeiger.
fn ist_skalar(t: &Type) -> bool {
    t.is_concrete_int() || *t == Type::Bool || t.is_ptr()
}

// ------------------------------------------------------------------- Typphase

/// Hook aus `sema::call`. Liefert `None`, wenn der Name kein Primitiv ist oder
/// im Programm eine gleichnamige Funktion steht — die gewinnt dann.
pub(crate) fn hook_call(
    ck: &mut Checker,
    name: &str,
    args: &[Expr],
    nspan: Span,
    espan: Span,
) -> Option<Type> {
    if !is_ct_call(name) || ck.fns.contains_key(name) {
        return None;
    }
    Some(match name {
        SELECT => check_select(ck, args, nspan, espan),
        BARRIER => check_barrier(ck, args, nspan, espan),
        _ => check_secure_zero(ck, args, nspan, espan),
    })
}

/// Erwartete Argumentzahl pruefen; bei Abweichung werden die vorhandenen
/// Argumente trotzdem durchgetypt, damit keine ExprId ohne Typ bleibt.
fn stellenzahl(ck: &mut Checker, name: &str, args: &[Expr], soll: usize, nspan: Span) -> bool {
    if args.len() == soll {
        return true;
    }
    for a in args {
        ck.type_out_expr(a);
    }
    ck.dg.error_note(
        nspan,
        format!(
            "'{}' erwartet {} argument(e), gefunden {}",
            name,
            soll,
            args.len()
        ),
        match name {
            SELECT => "aufruf: select(bedingung, a, b)",
            BARRIER => "aufruf: barrier(wert)",
            _ => "aufruf: secure_zero(zeiger, anzahl_bytes)",
        },
    );
    false
}

fn check_select(ck: &mut Checker, args: &[Expr], nspan: Span, espan: Span) -> Type {
    if !stellenzahl(ck, SELECT, args, 3, nspan) {
        return Type::Error;
    }
    let ct = ck.expr(&args[0], Some(&Type::Bool));
    if !ct.is_error() && ct != Type::Bool {
        ck.dg.error(
            args[0].span,
            format!(
                "die bedingung von 'select' muss bool sein, gefunden {}",
                ck.tcx.name_of(&ct)
            ),
        );
    }
    let ta = ck.expr(&args[1], None);
    let tb = ck.expr(&args[2], Some(&ta));
    if ta.is_error() || tb.is_error() {
        return Type::Error;
    }
    if !ist_skalar(&ta) {
        ck.dg.error_note(
            args[1].span,
            format!(
                "'select' arbeitet nur auf skalaren werten, gefunden {}",
                ck.tcx.name_of(&ta)
            ),
            "erlaubt sind ganzzahl-, bool- und zeigertypen",
        );
        return Type::Error;
    }
    if ta != tb {
        ck.dg.error_note(
            espan,
            format!(
                "beide zweige von 'select' muessen denselben typ haben, gefunden {} und {}",
                ck.tcx.name_of(&ta),
                ck.tcx.name_of(&tb)
            ),
            "es gibt keine implizite umwandlung; schreibe z. B. 'x as i32'",
        );
        return Type::Error;
    }
    ta
}

fn check_barrier(ck: &mut Checker, args: &[Expr], nspan: Span, _espan: Span) -> Type {
    if !stellenzahl(ck, BARRIER, args, 1, nspan) {
        return Type::Error;
    }
    let t = ck.expr(&args[0], None);
    if t.is_error() {
        return Type::Error;
    }
    if !ist_skalar(&t) {
        ck.dg.error_note(
            args[0].span,
            format!(
                "'barrier' arbeitet nur auf skalaren werten, gefunden {}",
                ck.tcx.name_of(&t)
            ),
            "erlaubt sind ganzzahl-, bool- und zeigertypen",
        );
        return Type::Error;
    }
    t
}

fn check_secure_zero(ck: &mut Checker, args: &[Expr], nspan: Span, _espan: Span) -> Type {
    if !stellenzahl(ck, SECURE_ZERO, args, 2, nspan) {
        return Type::Error;
    }
    let tp = ck.expr(&args[0], None);
    if !tp.is_error() && !tp.is_ptr() {
        ck.dg.error_note(
            args[0].span,
            format!(
                "das erste argument von 'secure_zero' muss ein zeiger sein, gefunden {}",
                ck.tcx.name_of(&tp)
            ),
            "aufruf: secure_zero(zeiger, anzahl_bytes)",
        );
    }
    let tn = ck.expr(&args[1], Some(&Type::Usize));
    if !tn.is_error() && !tn.is_concrete_int() {
        ck.dg.error(
            args[1].span,
            format!(
                "die byteanzahl von 'secure_zero' muss eine ganzzahl sein, gefunden {}",
                ck.tcx.name_of(&tn)
            ),
        );
    }
    Type::Void
}

// ---------------------------------------------------------------- Lowerphase

/// Hook aus `lower::lower_call`. Erzeugt die FIR-Instruktion.
pub(crate) fn lower_ct_call(
    lw: &mut Lower,
    name: &str,
    args: &[Expr],
    span: Span,
) -> Option<Option<Val>> {
    match name {
        SELECT => {
            if args.len() != 3 {
                return lw.ice(span, "'select' mit falscher argumentzahl im lowering");
            }
            let ty = lw.fty_of(&args[1])?;
            let cond = lw.lower_expr(&args[0])?;
            let a = lw.lower_expr(&args[1])?;
            let b = lw.lower_expr(&args[2])?;
            Some(Some(lw.push(ty, Op::Select { cond, a, b })))
        }
        BARRIER => {
            if args.len() != 1 {
                return lw.ice(span, "'barrier' mit falscher argumentzahl im lowering");
            }
            let ty = lw.fty_of(&args[0])?;
            let val = lw.lower_expr(&args[0])?;
            Some(Some(lw.push(ty, Op::Barrier { val })))
        }
        _ => {
            if args.len() != 2 {
                return lw.ice(span, "'secure_zero' mit falscher argumentzahl im lowering");
            }
            let addr = lw.lower_expr(&args[0])?;
            let n = lw.lower_expr(&args[1])?;
            let size = angleichen(lw, &args[1], n)?;
            lw.push_void(FTy::Void, Op::SecureZero { addr, size });
            Some(None)
        }
    }
}

/// Die Byteanzahl von `secure_zero` wird als `u64` gebraucht; schmalere
/// Ganzzahlen werden erweitert (vorzeichenrichtig nach Quelltyp).
fn angleichen(lw: &mut Lower, arg: &Expr, v: Val) -> Option<Val> {
    let from = lw.fty_of(arg)?;
    if from == FTy::U64 || from == FTy::I64 {
        return Some(v);
    }
    Some(lw.push(FTy::U64, Op::Cast { src: v, from }))
}

#[cfg(test)]
mod tests {
    /// Uebersetzt Quelltext bis zum Assembler — genau der Weg, den `firnc`
    /// nimmt (mit Optimierer).
    fn asm_von(src: &str) -> String {
        let mut dg = crate::diag::Diags::new("ct_test", src);
        let toks = crate::lexer::lex(src, &mut dg);
        let mut prog = crate::parser::parse(&toks, &mut dg);
        crate::mono::expand(&mut prog, &mut dg);
        let info = crate::sema::check(&prog, &mut dg).expect("typpruefung");
        let mut m = crate::lower::lower(&prog, &info, &mut dg).expect("lowering");
        assert!(!dg.has_errors(), "{}", dg.render());
        crate::opt::optimize(&mut m);
        crate::codegen_x86::emit(&m).expect("codegen")
    }

    /// NACHWEIS (SPEC §9.2): `select` wird ein `cmov` — und in der Funktion,
    /// die nur aus dem `select` besteht, entsteht KEIN bedingter Sprung.
    #[test]
    fn select_wird_cmov_und_nie_ein_sprung() {
        let asm = asm_von(
            "fn waehle(b: bool, a: i32, c: i32) -> i32 { return select(b, a, c) }\n\
             fn main() -> i32 { return waehle(true, 1 as i32, 2 as i32) }\n",
        );
        assert!(asm.contains("cmov"), "kein cmov:\n{}", asm);
        let koerper = asm.split("waehle:").nth(1).expect("funktion fehlt");
        let koerper = koerper.split("\nmain:").next().unwrap_or(koerper);
        for zeile in koerper.lines() {
            let l = zeile.trim();
            assert!(
                !(l.starts_with('j') && !l.starts_with("jmp")),
                "bedingter sprung in 'waehle': {}\n{}",
                l,
                asm
            );
        }
    }

    /// NACHWEIS (SPEC §9.3, `C3`): `secure_zero` bleibt stehen, obwohl der
    /// Puffer danach nie wieder gelesen wird — der Optimierer darf es nicht
    /// als toten Speicherzugriff entfernen.
    #[test]
    fn secure_zero_ueberlebt_den_optimierer() {
        let asm = asm_von(
            "fn main() -> i32 {\n\
                 var buf: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8]\n\
                 secure_zero(&buf[0], 8 as usize)\n\
                 return 0\n\
             }\n",
        );
        assert!(asm.contains("rep stosb"), "secure_zero entfernt:\n{}", asm);
    }

    /// NACHWEIS (SPEC §9.2): `barrier` ueberlebt die Konstantenfaltung — der
    /// Wert wird NICHT durch die Konstante ersetzt.
    #[test]
    fn barrier_bleibt_undurchsichtig() {
        let asm = asm_von("fn main() -> i32 { let a: i32 = barrier(7 as i32)\n return a }\n");
        let koerper = asm.split("main:").nth(1).expect("main fehlt");
        assert!(
            !koerper.contains("mov rax, 7") && !koerper.contains("mov eax, 7"),
            "barrier wegoptimiert:\n{}",
            asm
        );
    }

    /// Eine eigene Funktion mit dem Namen eines Primitivs gewinnt — sonst
    /// koennte ein Programm durch das neue Primitiv still die Bedeutung
    /// wechseln.
    #[test]
    fn eigene_funktion_verdeckt_das_primitiv() {
        let asm = asm_von(
            "fn barrier(x: i32) -> i32 { return x + 1 as i32 }\n\
             fn main() -> i32 { return barrier(1 as i32) }\n",
        );
        assert!(asm.contains("barrier:"), "eigene funktion fehlt:\n{}", asm);
    }
}
