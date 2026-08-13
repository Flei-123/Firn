//! Codeerzeugung fuer `Term::Switch` (SPEC §6.3, `P4`).
//!
//! SCHNITTSTELLE (fest, wird von `codegen_x86.rs` aufgerufen):
//!   `pub(crate) fn emit_switch(e, f, fr, term) -> Result<(), String>`
//!
//! Zwei Verfahren:
//!  * **Sprungtabelle** — sobald mindestens `MIN_TABLE_CASES` Marken vorliegen
//!    und die Dichte `cases.len() * 100 / (max - min + 1)` mindestens
//!    `MIN_DENSITY` Prozent betraegt. Die Tabelle steht in `.rodata`, der
//!    Sprung ist ein indirekter `jmp qword ptr [...]`; ausserhalb von
//!    `[min, max]` geht es nach `default`.
//!  * **Vergleichskette** — sonst (wenige oder weit gestreute Marken).
//!
//! Beide Verfahren sind verhaltensgleich; der Optimierer aendert daran nichts.

use crate::codegen_x86::{block_label, load_ext, load_full, reg, Emitter, Frame};
use crate::fir::{FTy, Func, Term};

/// ab so vielen Faellen lohnt eine Tabelle
pub(crate) const MIN_TABLE_CASES: usize = 8;
/// Mindestdichte in Prozent
pub(crate) const MIN_DENSITY: usize = 40;
/// Obergrenze fuer die Groesse einer Tabelle (Eintraege), damit sparsame
/// Programme nicht ungewollt viel `.rodata` bekommen.
const MAX_TABLE_ENTRIES: i128 = 65536;

pub(crate) fn emit_switch(
    e: &mut Emitter,
    f: &Func,
    fr: &Frame,
    term: &Term,
) -> Result<(), String> {
    let (val, ty, cases, default) = match term {
        Term::Switch { val, ty, cases, default } => (*val, *ty, cases, *default),
        _ => return Err("interner Fehler: emit_switch ohne switch".to_string()),
    };
    // SPEC §9.1: ueber einen geheimen Wert darf nicht verzweigt werden.
    if f.constant_time && f.is_secret(val) {
        return Err(format!(
            "#[constant_time]: switch in '{}' haengt von einem secret-Wert (%{}) ab",
            f.name, val
        ));
    }
    if ty == FTy::Void {
        return Err("interner Fehler: switch ueber void".to_string());
    }
    if cases.is_empty() {
        e.line(&format!("jmp {}", block_label(&f.name, default)));
        return Ok(());
    }
    let bits = if ty.bits() > 32 { 64 } else { 32 };
    load_ext(e, fr, "rax", val, ty, bits);
    let _ = load_full;

    if let Some((min, max)) = table_range(cases) {
        emit_table(e, f, cases, default, min, max, bits);
        return Ok(());
    }
    for (k, target) in cases.iter() {
        e.line(&format!("cmp {}, {}", reg("rax", bits), *k as i64));
        e.line(&format!("je {}", block_label(&f.name, *target)));
    }
    e.line(&format!("jmp {}", block_label(&f.name, default)));
    Ok(())
}

/// Lohnt sich eine Sprungtabelle? Liefert `[min, max]` der Marken.
fn table_range(cases: &[(i128, crate::fir::BlockId)]) -> Option<(i128, i128)> {
    if cases.len() < MIN_TABLE_CASES {
        return None;
    }
    let min = cases.iter().map(|(k, _)| *k).min()?;
    let max = cases.iter().map(|(k, _)| *k).max()?;
    let weite = max - min + 1;
    if weite <= 0 || weite > MAX_TABLE_ENTRIES {
        return None;
    }
    let dichte = (cases.len() as i128) * 100 / weite;
    if dichte < MIN_DENSITY as i128 {
        return None;
    }
    Some((min, max))
}

fn emit_table(
    e: &mut Emitter,
    f: &Func,
    cases: &[(i128, crate::fir::BlockId)],
    default: crate::fir::BlockId,
    min: i128,
    max: i128,
    bits: u32,
) {
    let weite = max - min + 1;
    let label = table_label(e, &f.name);
    let dflt = block_label(&f.name, default);

    // Index = Wert - min; ausserhalb von [0, weite) geht es nach default.
    if bits == 32 {
        if min != 0 {
            e.line(&format!("sub eax, {}", min as i64));
        }
        e.line(&format!("cmp eax, {}", (weite - 1) as i64));
        e.line(&format!("ja {}", dflt));
        // 32-Bit-Operationen nullen die oberen 32 Bit von rax bereits.
        if min == 0 {
            e.line("mov eax, eax");
        }
    } else {
        if min != 0 {
            e.line(&format!("mov rcx, {}", min as i64));
            e.line("sub rax, rcx");
        }
        e.line(&format!("mov rcx, {}", (weite - 1) as i64));
        e.line("cmp rax, rcx");
        e.line(&format!("ja {}", dflt));
    }
    e.line(&format!("lea rdx, [rip + {}]", label));
    e.line("jmp qword ptr [rdx + rax*8]");

    // Tabelle in .rodata; fehlende Marken zeigen auf den Vorgabezweig.
    e.raw(".section .rodata");
    e.raw(".align 8");
    e.raw(&format!("{}:", label));
    let mut i = 0usize;
    let mut k = min;
    while k <= max {
        while i < cases.len() && cases[i].0 < k {
            i += 1;
        }
        if i < cases.len() && cases[i].0 == k {
            e.raw(&format!(".quad {}", block_label(&f.name, cases[i].1)));
        } else {
            e.raw(&format!(".quad {}", dflt));
        }
        k += 1;
    }
    e.raw(".text");
}

/// Eindeutige Marke fuer eine Tabelle innerhalb der Ausgabe.
fn table_label(e: &Emitter, fname: &str) -> String {
    let base = format!(".Ltbl_{}", fname);
    let n = e.out.matches(&format!("{}_", base)).count();
    format!("{}_{}", base, n)
}

#[cfg(test)]
mod tests {
    use crate::codegen_x86::emit;
    use crate::fir::{FTy, Func, Module, Op, Term};

    /// Wenige Marken: Vergleichskette.
    #[test]
    fn switch_erzeugt_vergleichskette() {
        let mut f = Func::new("main", vec![], FTy::I32);
        let v = f.push(0, FTy::I32, Op::Const(2));
        let b1 = f.add_block();
        let b2 = f.add_block();
        let bd = f.add_block();
        f.set_term(0, Term::Switch { val: v, ty: FTy::I32, cases: vec![(1, b1), (2, b2)], default: bd });
        let c1 = f.push(b1, FTy::I32, Op::Const(10));
        f.set_term(b1, Term::Ret(Some(c1)));
        let c2 = f.push(b2, FTy::I32, Op::Const(20));
        f.set_term(b2, Term::Ret(Some(c2)));
        let cd = f.push(bd, FTy::I32, Op::Const(30));
        f.set_term(bd, Term::Ret(Some(cd)));
        let asm = emit(&Module { funcs: vec![f] }).expect("codegen");
        assert!(asm.contains("je .Lmain__bb1"), "{}", asm);
        assert!(asm.contains("je .Lmain__bb2"), "{}", asm);
        assert!(asm.contains("jmp .Lmain__bb3"), "{}", asm);
        assert!(!asm.contains("jmp qword ptr"), "unerwartete Tabelle:\n{}", asm);
    }

    /// Viele dichte Marken: Sprungtabelle in `.rodata` mit indirektem Sprung.
    #[test]
    fn dichter_switch_erzeugt_sprungtabelle() {
        let mut f = Func::new("main", vec![], FTy::I32);
        let v = f.push(0, FTy::I32, Op::Const(3));
        let mut cases = Vec::new();
        for i in 0..12i128 {
            let b = f.add_block();
            let c = f.push(b, FTy::I32, Op::Const(i));
            f.set_term(b, Term::Ret(Some(c)));
            cases.push((i, b));
        }
        let bd = f.add_block();
        let cd = f.push(bd, FTy::I32, Op::Const(99));
        f.set_term(bd, Term::Ret(Some(cd)));
        f.set_term(0, Term::Switch { val: v, ty: FTy::I32, cases, default: bd });
        let asm = emit(&Module { funcs: vec![f] }).expect("codegen");
        assert!(asm.contains("jmp qword ptr [rdx + rax*8]"), "{}", asm);
        assert!(asm.contains(".section .rodata"), "{}", asm);
        assert!(asm.contains(".quad .Lmain__bb5"), "{}", asm);
        // keine Vergleichskette mehr
        assert!(!asm.contains("je .Lmain__bb5"), "{}", asm);
    }

    /// Weit gestreute Marken: keine Tabelle (Dichte zu gering).
    #[test]
    fn sparsamer_switch_bleibt_kette() {
        let mut f = Func::new("main", vec![], FTy::I32);
        let v = f.push(0, FTy::I32, Op::Const(3));
        let mut cases = Vec::new();
        for i in 0..10i128 {
            let b = f.add_block();
            let c = f.push(b, FTy::I32, Op::Const(i));
            f.set_term(b, Term::Ret(Some(c)));
            cases.push((i * 1000, b));
        }
        let bd = f.add_block();
        let cd = f.push(bd, FTy::I32, Op::Const(99));
        f.set_term(bd, Term::Ret(Some(cd)));
        f.set_term(0, Term::Switch { val: v, ty: FTy::I32, cases, default: bd });
        let asm = emit(&Module { funcs: vec![f] }).expect("codegen");
        assert!(!asm.contains("jmp qword ptr"), "{}", asm);
        assert!(asm.contains("je .Lmain__bb1"), "{}", asm);
    }

    /// NACHWEIS (SPEC §6.3, `P4`): die Zustandsmaschine mit 32 Zustaenden in
    /// `tests/230_zustandsmaschine.fi` bekommt eine echte Sprungtabelle —
    /// ein indirekter Sprung ueber `.rodata`, keine Kette aus 32 `cmp`.
    #[test]
    fn sprungtabelle_bei_30_zustaenden() {
        let pfad = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/230_zustandsmaschine.fi");
        let src = std::fs::read_to_string(pfad).expect("testprogramm fehlt");
        let mut dg = crate::diag::Diags::new(pfad, &src);
        let toks = crate::lexer::lex(&src, &mut dg);
        let mut prog = crate::parser::parse(&toks, &mut dg);
        crate::mono::expand(&mut prog, &mut dg);
        let info = crate::sema::check(&prog, &mut dg).expect("typpruefung");
        let mut m = crate::lower::lower(&prog, &info, &mut dg).expect("lowering");
        assert!(!dg.has_errors(), "{}", dg.render());
        crate::opt::optimize(&mut m);
        let asm = emit(&m).expect("codegen");
        assert!(asm.contains("jmp qword ptr ["), "keine sprungtabelle:\n{}", asm);
        assert!(asm.contains(".section .rodata"), "tabelle nicht in .rodata:\n{}", asm);
        let eintraege = asm.matches(".quad .Lmain__bb").count();
        assert!(eintraege >= 32, "nur {} tabelleneintraege", eintraege);
        let vergleiche = asm.lines().filter(|l| l.trim().starts_with("cmp ")).count();
        assert!(vergleiche <= 4, "{} vergleiche statt tabelle:\n{}", vergleiche, asm);
    }

    /// `select` muss ein `cmov` werden — niemals ein Sprung (SPEC §9.2).
    #[test]
    fn select_wird_cmov_und_nie_ein_sprung() {
        let mut f = Func::new("main", vec![], FTy::I32);
        let c = f.push(0, FTy::Bool, Op::Const(1));
        let a = f.push(0, FTy::I32, Op::Const(7));
        let b = f.push(0, FTy::I32, Op::Const(9));
        let s = f.push(0, FTy::I32, Op::Select { cond: c, a, b });
        f.set_term(0, Term::Ret(Some(s)));
        let asm = emit(&Module { funcs: vec![f] }).expect("codegen");
        assert!(asm.contains("cmovnz"), "{}", asm);
        let body = asm.split("main:").nth(1).unwrap();
        for line in body.lines() {
            let l = line.trim();
            assert!(!(l.starts_with('j') && !l.starts_with("jmp")), "bedingter Sprung: {}", l);
        }
    }
}
