// SPDX-License-Identifier: MPL-2.0
//! **Runde TEMPO 9 — die Schleife, die nur Nullen schreibt.**
//!
//! ## Warum es diesen Pass gibt
//!
//! `rt.mem_set` ist in Firn geschrieben, und zwar so:
//!
//! ```firn
//! fn mem_set(target: u64, value: u8, n: usize) {
//!     var i: usize = 0
//!     while i < n {
//!         st8(target, i, value)
//!         i = i + 1
//!     }
//! }
//! ```
//!
//! Das ist richtig und ueberall verwendbar — und es kostet **fuenf Befehle
//! je Oktett**. Gemessen am MP3-Dekoder (`callgrind`, 8 s Ton): der Aufruf
//! `mem_set(grbuf, 0, 4608)` einmal je Granulat steht fuer **14,2 von 160,6
//! Millionen Befehlen, also neun Prozent des ganzen Programms** — und zwar
//! nur, um ein Feld auf null zu setzen.
//!
//! ```text
//!   cmp   $0x1200,%rdx
//!   jae   fertig
//!   mov   -0x8b0(%rbp),%r11     ; den Zeiger JEDES MAL neu holen
//!   movb  $0x0,(%r11,%rdx,1)
//!   lea   0x1(%rdx),%rdx
//!   jmp   kopf
//! ```
//!
//! x86 hat dafuer einen Befehl (`rep stosb`), aarch64 eine kurze Schleife in
//! Achtbytes. Beide stehen im Erzeuger laengst — als `Op::SecureZero`, das
//! `secure_zero(inout buf)` bedient. Es tut **genau** das, was hier gebraucht
//! wird: `size` Oktette ab `addr` auf null. Der Pass muss die Schleife also
//! nur wiedererkennen und durch diese eine Anweisung ersetzen.
//!
//! ## Das Muster
//!
//! Nach `mem2reg` und `licm` sieht die Schleife in FIR immer gleich aus:
//!
//! ```text
//! P:    ...                          <- Vorkopf
//!       br H
//! H:    %i = phi [P %null, B %i2]
//!       %c = cmp.lt.uXX %i, %n
//!       brcond %c, B, X
//! B:    %a = add %base, %i
//!       store.u8 %wert, %a
//!       %i2 = add %i, %eins
//!       br H
//! ```
//!
//! Daraus wird `secure_zero(%base, %n)` im Vorkopf und `br X` in `H`; den
//! Rest raeumt `dce` weg.
//!
//! ## Die Bedingungen, und warum jede einzelne noetig ist
//!
//! * **Der Rumpf enthaelt GENAU diese drei Anweisungen.** Alles andere —
//!   ein zweiter Speicherzugriff, ein Aufruf, ein Lesen — waere eine Wirkung,
//!   die `rep stosb` nicht hat.
//! * **Der geschriebene Wert ist die Konstante 0 und ein Oktett breit.**
//!   `SecureZero` kann nur Nullen; ein `mem_set(p, 7, n)` bleibt die
//!   Schleife.
//! * **Der Vergleich ist VORZEICHENLOS.** Bei `i64` koennte `n` negativ
//!   sein: die Schleife laeuft dann null Mal, `rep stosb` mit `rcx = -1`
//!   schriebe dagegen den halben Adressraum voll. Das ist kein theoretischer
//!   Fall, sondern der Unterschied zwischen "nichts tun" und "Rechner weg".
//! * **`%base` und `%n` sind ausserhalb der Schleife definiert** (Parameter,
//!   Konstante oder ein Block, der den Vorkopf beherrscht). Sonst gibt es sie
//!   im Vorkopf noch gar nicht.
//! * **Die Schrittweite ist genau 1 und der Anfang genau 0.** Nur dann
//!   trifft die Schleife jedes Oktett von `base` bis `base+n` und keines
//!   sonst.
//! * **`%i`, `%i2` und `%a` werden ausserhalb der Schleife nicht gelesen.**
//!   Der Endwert von `%i` waere `n`, aber das nachzureichen ist Arbeit fuer
//!   einen Fall, den es im Bestand nicht gibt.
//! * **Kleine konstante Laengen bleiben Schleife.** `rep stosb` hat auf
//!   heutigen Prozessoren eine Anlaufzeit von einigen Dutzend Takten; unter
//!   sechzehn Oktetten ist die Schleife schneller.
//!
//! ## Was der Pass NICHT tut
//!
//! Er erkennt kein `mem_copy` (dafuer gibt es `Op::CopyMem`, aber die
//! Schleife dort liest UND schreibt, und die Frage nach Ueberlappung ist eine
//! andere) und keinen Wert ausser null. Beides waere eine eigene Runde mit
//! eigener Messung.

use crate::fir::{BinOp, CmpOp, FTy, Func, Inst, Op, Term, Val};

/// Laufen lassen; liefert die Anzahl der ersetzten Schleifen.
pub fn recognise(f: &mut Func) -> usize {
    let nb = f.blocks.len();
    if nb < 3 || f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return 0;
    }
    // Vorgaenger je Block.
    let mut preds: Vec<Vec<usize>> = vec![Vec::new(); nb];
    for (i, b) in f.blocks.iter().enumerate() {
        for s in b.term.successors() {
            let s = s as usize;
            if s < nb && !preds[s].contains(&i) {
                preds[s].push(i);
            }
        }
    }
    let dom = crate::mem2reg::dominators(f);
    // Wo wird welcher Wert definiert? (Block, Anweisung)
    let nv = f.val_types.len();
    let mut defblock: Vec<Option<usize>> = vec![None; nv];
    for (bi, b) in f.blocks.iter().enumerate() {
        for i in &b.insts {
            if let Some(d) = i.dst {
                if (d as usize) < nv {
                    defblock[d as usize] = Some(bi);
                }
            }
        }
    }
    let npar = f.params.len();

    let mut plan: Vec<(usize, usize, usize, Val, Val)> = Vec::new(); // (P, H, B, base, n)
    let mut used: Vec<bool> = vec![false; nb];

    'head: for h in 0..nb {
        if used[h] {
            continue;
        }
        let hit = match pattern(f, &preds, &defblock, npar, &dom, h) {
            Some(t) => t,
            None => continue 'head,
        };
        let (p, b, base, n) = hit;
        used[h] = true;
        used[b] = true;
        plan.push((p, h, b, base, n));
    }
    if plan.is_empty() {
        return 0;
    }
    let count = plan.len();
    for (p, h, _b, base, n) in plan {
        // Das Ziel der Schleife ist der Ausgang von `H`.
        let x = match f.blocks[h].term {
            Term::BrCond { then_bb, else_bb, .. } => {
                // `then` ist der Rumpf, also ist `else` der Ausgang.
                let body = then_bb;
                let _ = body;
                else_bb
            }
            _ => continue,
        };
        // `secure_zero` ans Ende des Vorkopfs, vor dessen Sprung.
        let loc = f.blocks[p].insts.last().map(|i| i.loc).unwrap_or_default();
        f.blocks[p].insts.push(Inst {
            dst: None,
            ty: FTy::Void,
            op: Op::SecureZero { addr: base, size: n },
            loc,
        });
        f.blocks[h].term = Term::Br(x);
    }
    count
}

/// Passt an `h` das Muster? Liefert `(Vorkopf, Rumpf, base, n)`.
fn pattern(
    f: &Func,
    preds: &[Vec<usize>],
    defblock: &[Option<usize>],
    npar: usize,
    dom: &[Vec<bool>],
    h: usize,
) -> Option<(usize, usize, Val, Val)> {
    // --- der Kopf: ein phi, ein Vergleich, ein bedingter Sprung ----------
    let kb = &f.blocks[h];
    if kb.insts.len() != 2 {
        return None;
    }
    let (cond, body, ausgang) = match kb.term {
        Term::BrCond { cond, then_bb, else_bb } => (cond, then_bb as usize, else_bb as usize),
        _ => return None,
    };
    if body >= f.blocks.len() || ausgang >= f.blocks.len() || body == h {
        return None;
    }
    let iv = match (&kb.insts[0].op, kb.insts[0].dst) {
        (Op::Phi { incoming }, Some(d)) => {
            if incoming.len() != 2 {
                return None;
            }
            (d, incoming.clone())
        }
        _ => return None,
    };
    let (i_val, inc) = iv;
    // Der Vergleich: `i < n`, VORZEICHENLOS.
    let (n_val, cmp_ty) = match (&kb.insts[1].op, kb.insts[1].dst) {
        (Op::Cmp { op: CmpOp::Lt, ty, a, b }, Some(d)) if d == cond && *a == i_val => (*b, *ty),
        _ => return None,
    };
    if cmp_ty.signed() || cmp_ty.is_float() {
        return None;
    }

    // --- der Rumpf: genau drei Anweisungen, Sprung zurueck ---------------
    let rb = &f.blocks[body];
    if rb.insts.len() != 3 || !matches!(rb.term, Term::Br(t) if t as usize == h) {
        return None;
    }
    if preds[body].len() != 1 || preds[body][0] != h {
        return None;
    }
    // Adresse, Speichern, Fortschalten -- in beliebiger Reihenfolge.
    let mut addr: Option<(Val, Val)> = None; // (Ergebnis, base)
    let mut value: Option<(Val, Val)> = None; // (gespeicherter Wert, Adresse)
    let mut step: Option<(Val, Val)> = None; // (Ergebnis, Schrittkonstante)
    let mut store_ty = FTy::Void;
    for inst in rb.insts.iter() {
        match (&inst.op, inst.dst) {
            (Op::Bin(BinOp::Add, a, b), Some(d)) if *a == i_val || *b == i_val => {
                let other = if *a == i_val { *b } else { *a };
                // Die Fortschaltung erkennt man daran, dass ihr Ergebnis auf
                // der Rueckwaertskante des phi steht.
                if inc.iter().any(|(q, v)| *q as usize == body && *v == d) {
                    if step.is_some() {
                        return None;
                    }
                    step = Some((d, other));
                } else {
                    if addr.is_some() {
                        return None;
                    }
                    addr = Some((d, other));
                }
            }
            (Op::Store { val, addr: a }, None) => {
                if value.is_some() {
                    return None;
                }
                store_ty = inst.ty;
                value = Some((*val, *a));
            }
            _ => return None,
        }
    }
    let (a_val, base) = addr?;
    let (v_val, a_used) = value?;
    let (i2_val, step) = step?;
    if a_used != a_val {
        return None;
    }
    // Ein Oktett je Durchlauf.
    if store_ty.bits() != 8 {
        return None;
    }

    // --- die Konstanten: Anfang 0, Schrittweite 1, Wert 0 ----------------
    let vor = inc.iter().find(|(q, _)| *q as usize != body)?;
    let p = vor.0 as usize;
    if p >= f.blocks.len() {
        return None;
    }
    if const_of(f, vor.1) != Some(0) {
        return None;
    }
    if const_of(f, step) != Some(1) {
        return None;
    }
    if const_of(f, v_val) != Some(0) {
        return None;
    }
    // Der Vorkopf muss GENAU EIN Nachfolger haben (sein Sprung geht nach
    // `h`), sonst schreibt das eingesetzte `secure_zero` auch auf dem Weg,
    // der die Schleife nie betritt.
    if !matches!(f.blocks[p].term, Term::Br(t) if t as usize == h) {
        return None;
    }
    // `h` hat genau zwei Vorgaenger: den Vorkopf und den Rumpf.
    if preds[h].len() != 2 || !preds[h].contains(&p) || !preds[h].contains(&body) {
        return None;
    }

    // --- `base` und `n` gibt es im Vorkopf schon ------------------------
    for v in [base, n_val] {
        if (v as usize) < npar {
            continue; // Parameter
        }
        match defblock.get(v as usize).copied().flatten() {
            Some(db) => {
                if db != p && !dom[p][db] {
                    return None;
                }
            }
            None => return None,
        }
    }
    // Weder `base` noch `n` duerfen die Schleifenvariable sein.
    if base == i_val || n_val == i_val || base == n_val {
        return None;
    }

    // --- nichts aus der Schleife wird draussen gelesen -------------------
    let mut buf = Vec::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        if bi == body {
            continue;
        }
        for inst in &b.insts {
            // Der phi im Kopf liest die Fortschaltung -- das ist die
            // Rueckwaertskante und zaehlt nicht.
            if bi == h && matches!(inst.op, Op::Phi { .. }) {
                continue;
            }
            // Der Vergleich im Kopf liest die Schleifenvariable.
            if bi == h && inst.dst == Some(cond) {
                continue;
            }
            buf.clear();
            inst.op.uses(&mut buf);
            if buf.iter().any(|u| *u == i_val || *u == i2_val || *u == a_val) {
                return None;
            }
        }
        match &b.term {
            Term::BrCond { cond: c, .. } => {
                if *c == i_val || *c == i2_val || *c == a_val {
                    return None;
                }
            }
            Term::Switch { val, .. } | Term::Ret(Some(val)) => {
                if *val == i_val || *val == i2_val || *val == a_val {
                    return None;
                }
            }
            _ => {}
        }
    }

    // --- winzige feste Laengen bleiben Schleife --------------------------
    if let Some(k) = const_of(f, n_val) {
        if k < 16 {
            return None;
        }
    }
    Some((p, body, base, n_val))
}

/// Wert einer Konstante, wenn der Wert eine ist.
fn const_of(f: &Func, v: Val) -> Option<i128> {
    for b in &f.blocks {
        for i in &b.insts {
            if i.dst == Some(v) {
                return match &i.op {
                    Op::Const(c) => Some(i.ty.truncate(*c)),
                    _ => None,
                };
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fir::{Block, Func};

    /// `fn f(p: u64, n: u64) { var i = 0; while i < n { st8(p, i, 0); i += 1 } }`
    fn loop_fn(signed: bool) -> Func {
        let mut f = Func::new("t", vec![FTy::U64, FTy::U64], FTy::Void);
        // %0 = p, %1 = n
        let null = f.new_val_pub(FTy::U64);
        let one = f.new_val_pub(FTy::U64);
        let value = f.new_val_pub(FTy::U8);
        let i = f.new_val_pub(FTy::U64);
        let c = f.new_val_pub(FTy::Bool);
        let a = f.new_val_pub(FTy::U64);
        let i2 = f.new_val_pub(FTy::U64);
        let ct = if signed { FTy::I64 } else { FTy::U64 };
        f.blocks = vec![
            Block {
                id: 0,
                insts: vec![
                    Inst::new(Some(null), FTy::U64, Op::Const(0)),
                    Inst::new(Some(one), FTy::U64, Op::Const(1)),
                    Inst::new(Some(value), FTy::U8, Op::Const(0)),
                ],
                term: Term::Br(1),
            },
            Block {
                id: 1,
                insts: vec![
                    Inst::new(
                        Some(i),
                        FTy::U64,
                        Op::Phi { incoming: vec![(0, null), (2, i2)] },
                    ),
                    Inst::new(
                        Some(c),
                        FTy::Bool,
                        Op::Cmp { op: CmpOp::Lt, ty: ct, a: i, b: 1 },
                    ),
                ],
                term: Term::BrCond { cond: c, then_bb: 2, else_bb: 3 },
            },
            Block {
                id: 2,
                insts: vec![
                    Inst::new(Some(a), FTy::U64, Op::Bin(BinOp::Add, 0, i)),
                    Inst::new(None, FTy::U8, Op::Store { val: value, addr: a }),
                    Inst::new(Some(i2), FTy::U64, Op::Bin(BinOp::Add, i, one)),
                ],
                term: Term::Br(1),
            },
            Block { id: 3, insts: vec![], term: Term::Ret(None) },
        ];
        f
    }

    #[test]
    fn the_unsigned_zero_loop_becomes_one_instruction() {
        let mut f = loop_fn(false);
        assert_eq!(recognise(&mut f), 1);
        assert!(f.blocks[0].insts.iter().any(|i| matches!(i.op, Op::SecureZero { .. })));
        assert!(matches!(f.blocks[1].term, Term::Br(3)));
    }

    /// DER GEFAEHRLICHE FALL: mit Vorzeichen laeuft die Schleife bei
    /// negativem `n` null Mal -- `rep stosb` mit `rcx = -1` nicht.
    #[test]
    fn the_signed_comparison_stays_a_loop() {
        let mut f = loop_fn(true);
        assert_eq!(recognise(&mut f), 0);
    }

    /// Ein zweiter Speicherzugriff im Rumpf ist keine Nullschleife mehr.
    #[test]
    fn a_second_store_in_the_body_is_refused() {
        let mut f = loop_fn(false);
        let extra = f.new_val_pub(FTy::U8);
        f.blocks[2].insts.insert(
            1,
            Inst::new(Some(extra), FTy::U8, Op::Load { addr: 0 }),
        );
        assert_eq!(recognise(&mut f), 0);
    }
}
