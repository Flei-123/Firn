//! Codeerzeugung fuer `Term::Switch` (SPEC §6.3, `P4`).
//!
//! SCHNITTSTELLE (fest, wird von `codegen_x86.rs` aufgerufen):
//!   `pub(crate) fn emit_switch(e, f, fr, term) -> Result<(), String>`
//!
//! Zustand dieser Datei: **Vergleichskette** — korrekt, aber linear. Der Ausbau
//! zur echten **Sprungtabelle** (indirekter Sprung ueber `.quad`-Tabelle, wenn
//! die Marken dicht liegen) gehoert dem Modul `types` und findet AUSSCHLIESSLICH
//! hier statt; `codegen_x86.rs` wird dafuer nicht angefasst.
//!
//! Bedingungen fuer die Sprungtabelle (Vorgabe, nicht verhandelbar):
//!  * mindestens `MIN_TABLE_CASES` Faelle
//!  * Dichte `cases.len() * 100 / (max - min + 1) >= MIN_DENSITY`
//!  * ausserhalb von `[min, max]` wird nach `default` gesprungen
//!  * die Tabelle steht in `.rodata`, der Sprung ist `jmp qword ptr [...]`

use crate::codegen_x86::{block_label, load_ext, load_full, Emitter, Frame};
use crate::fir::{FTy, Func, Term};

/// ab so vielen Faellen lohnt eine Tabelle
#[allow(dead_code)] // Modul `types` benutzt sie beim Tabellenbau, dann entfernen
pub(crate) const MIN_TABLE_CASES: usize = 8;
/// Mindestdichte in Prozent
#[allow(dead_code)] // Modul `types` benutzt sie beim Tabellenbau, dann entfernen
pub(crate) const MIN_DENSITY: usize = 40;

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
    let bits = if ty.bits() > 32 { 64 } else { 32 };
    if ty == FTy::Void {
        return Err("interner Fehler: switch ueber void".to_string());
    }
    load_ext(e, fr, "rax", val, ty, bits);
    let _ = load_full;
    for (k, target) in cases.iter() {
        e.line(&format!("cmp {}, {}", crate::codegen_x86::reg("rax", bits), *k as i64));
        e.line(&format!("je {}", block_label(&f.name, *target)));
    }
    e.line(&format!("jmp {}", block_label(&f.name, default)));
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::codegen_x86::emit;
    use crate::fir::{FTy, Func, Module, Op, Term};

    /// Belegt, dass die Vergleichskette als Grundlage wirklich Code erzeugt.
    /// Das Modul `types` ersetzt sie durch eine Sprungtabelle und erweitert
    /// diesen Test um die Pruefung auf `jmp qword ptr`.
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
