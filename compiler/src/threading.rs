//! **Sprungfaedelung durch Bool-Zellen** (Runde 51).
//!
//! SCHNITTSTELLE (fest):
//!   `pub(crate) fn thread_bool_cells(f: &mut Func) -> usize`
//!
//! ## Warum dieser Durchgang
//!
//! FIR hat **keine Phi-Knoten** (fir.rs, Invariante). Die Kurzschluss-
//! operatoren `&&` und `||` muessen ihr Ergebnis deshalb ueber eine
//! `alloca`-Zelle zusammenfuehren. Aus `if c < 0x80 && c != 13` wird:
//!
//! ```text
//! bbA: %1 = cmp.lt %c, 128 ; store.bool %1, %zelle ; brcond %1, bbB, bbJ
//! bbB: %2 = cmp.ne %c, 13  ; store.bool %2, %zelle ; br bbJ
//! bbJ: %3 = load.bool %zelle ; brcond %3, bbT, bbE
//! ```
//!
//! `mem2reg` kann diese Zelle nicht aufloesen — sie wird zweimal geschrieben,
//! und ohne Phi gibt es keinen Wert, der beide Pfade vertritt. Im Maschinen-
//! code kostet das je Durchlauf sieben Instruktionen statt zwei:
//!
//! ```text
//! setb  %al                       ; Bool herstellen
//! movzbl %al,%r11d
//! mov   %r11b,-0xae1(%rbp)        ; in die Zelle
//! test  %r11b,%r11b               ; sofort wieder pruefen
//! jne   bbB
//! jmp   bbJ
//! bbJ:  movzbl -0xae1(%rbp),%r11d ; aus der Zelle
//!       test %r11b,%r11b
//!       je   bbE
//! ```
//!
//! Gemessen im Tokenizer-Benchmark (realweb, callgrind, instruktionsgenau):
//! die Muster „setcc+movzx+store+reload+test+jcc" und „setcc+movzx+store"
//! zusammen **137,0 Mio von 958,0 Mio Instruktionen = 14,3 %**.
//!
//! ## Was der Durchgang tut
//!
//! Er faedelt die Kante am Zusammenfluss vorbei. Ein **Weichenblock** ist ein
//! Block, der aus GENAU EINER Instruktion `%v = load.bool %zelle` besteht und
//! mit `brcond %v, T, E` endet. Ein Vorgaenger, der unmittelbar vor seinem
//! Terminator `store.bool %x, %zelle` ausfuehrt, weiss den Inhalt der Zelle
//! auf dieser Kante bereits — also darf er direkt springen:
//!
//! * Terminator `br J`            ->  `brcond %x, T, E`
//! * Terminator `brcond %x, A, J` ->  `brcond %x, A, E`   (auf der J-Kante
//!   ist `%x` falsch, der Weichenblock wuerde also nach E gehen)
//! * Terminator `brcond %x, J, B` ->  `brcond %x, T, B`   (spiegelbildlich)
//!
//! Danach steht `cmp` wieder unmittelbar vor dem Terminator, und die
//! bestehende Verschmelzung `cmp`+`jcc` in `regalloc.rs` greift; der Rest
//! (toter `store`, unerreichbarer Weichenblock) faellt in `mem2reg::
//! remove_dead_stores` und der Blockbereinigung von `opt.rs`.
//!
//! ## Warum das richtig ist
//!
//! * Der `store` ist die letzte Instruktion vor dem Terminator — zwischen ihm
//!   und dem Sprung kann **nichts** die Zelle mehr aendern. Zugelassen sind
//!   dazwischen nur Instruktionen ohne Speicherwirkung (kein `store`, `call`,
//!   `syscall`, `copymem`, `atomicadd`, `securezero`).
//! * Die Zelle ist eine `alloca`, deren Zeiger **nicht entkommt** (`simple`
//!   aus `scan_cells`): sie ist nur Adresse von `load`/`store`. Ein fremder
//!   Schreibzugriff ist damit ausgeschlossen.
//! * `%x` ist im Vorgaenger verfuegbar — es ist Operand seines eigenen
//!   `store`. Die Lebensspanne wird nicht verlaengert, sie endet nur eine
//!   Instruktion spaeter am Terminator DESSELBEN Blocks. Damit faellt dieser
//!   Durchgang NICHT in die Klasse aus Runde 40/41 (dort wurde eine
//!   Lebensspanne ueber `call`-Grenzen hinweg gedehnt, ohne dass der
//!   Registerverteiler davon wusste). Hier gibt es keine neue Spanne ueber
//!   einen Block hinaus, und der Verteiler sieht den Terminator-Operanden
//!   ohnehin (`Term::BrCond` ist Teil seiner Lebensdaueranalyse).
//! * `store` und `alloca` bleiben stehen; erst `remove_dead_stores` raeumt
//!   sie weg, und nur dann, wenn die Zelle wirklich nirgends mehr gelesen
//!   wird. Der Durchgang ist damit debugerhaltend.
//! * SPEC §9.2: geheime Werte (`secret`) und `#[constant_time]`-Funktionen
//!   werden nicht angefasst — aus einem Datenfluss darf nie ein Sprung
//!   werden.
//!
//! Abschaltbar mit `--no-pass=thread-bool`.

use crate::fir::{BlockId, FTy, Func, Op, Term, Val};
use std::collections::HashMap;

/// Aendert diese Instruktion Speicher, den wir nicht ueberblicken?
fn disturbs_memory(op: &Op) -> bool {
    matches!(
        op,
        Op::Store { .. }
            | Op::Call { .. }
            | Op::CallIndirect { .. }
            | Op::Syscall { .. }
            | Op::CopyMem { .. }
            | Op::AtomicAdd { .. }
            | Op::SecureZero { .. }
    )
}

/// Ein Weichenblock: nur `load.bool` aus einer Zelle, dann `brcond`.
struct Fork {
    cell: Val,
    then: BlockId,
    els: BlockId,
}

pub(crate) fn thread_bool_cells(f: &mut Func) -> usize {
    // SPEC §9.2: in constant-time-Funktionen entsteht hier nie ein Sprung.
    if f.constant_time {
        return 0;
    }
    // Invariante blocks[i].id == i — sonst rechnen die Indizes falsch.
    if f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return 0;
    }
    let simple = simple_cells(f);
    if simple.is_empty() {
        return 0;
    }

    // 1. Weichenbloecke einsammeln.
    let mut forks: HashMap<BlockId, Fork> = HashMap::new();
    for b in &f.blocks {
        if b.id == 0 || b.insts.len() != 1 {
            continue; // bb0 traegt die allocas
        }
        let i = &b.insts[0];
        let (d, addr) = match (i.dst, &i.op) {
            (Some(d), Op::Load { addr }) => (d, *addr),
            _ => continue,
        };
        if i.ty != FTy::Bool || !simple.contains(&addr) || f.is_secret(d) {
            continue;
        }
        let (cond, then, els) = match &b.term {
            Term::BrCond { cond, then_bb, else_bb } => (*cond, *then_bb, *else_bb),
            _ => continue,
        };
        if cond != d {
            continue;
        }
        forks.insert(b.id, Fork { cell: addr, then, els });
    }
    if forks.is_empty() {
        return 0;
    }

    // 2. Vorgaenger umschreiben.
    let mut n = 0usize;
    for pi in 0..f.blocks.len() {
        let p = &f.blocks[pi];
        // Welcher Weichenblock ist ueberhaupt Nachfolger?
        let targets = p.term.successors();
        if !targets.iter().any(|z| forks.contains_key(z)) {
            continue;
        }
        // Der zuletzt geschriebene Zelleninhalt am Blockende.
        let (cell, x) = match last_bool_store(f, pi) {
            Some(v) => v,
            None => continue,
        };
        if !simple.contains(&cell) || f.is_secret(x) || f.val_ty(x) != FTy::Bool {
            continue;
        }
        let new = match &f.blocks[pi].term {
            Term::Br(t) => match forks.get(t) {
                Some(w) if w.cell == cell && *t != pi as BlockId => {
                    Some(Term::BrCond { cond: x, then_bb: w.then, else_bb: w.els })
                }
                _ => None,
            },
            Term::BrCond { cond, then_bb, else_bb } if *cond == x => {
                let nt = match forks.get(then_bb) {
                    Some(w) if w.cell == cell && *then_bb != pi as BlockId => w.then,
                    _ => *then_bb,
                };
                let ne = match forks.get(else_bb) {
                    Some(w) if w.cell == cell && *else_bb != pi as BlockId => w.els,
                    _ => *else_bb,
                };
                if nt == *then_bb && ne == *else_bb {
                    None
                } else {
                    Some(Term::BrCond { cond: x, then_bb: nt, else_bb: ne })
                }
            }
            _ => None,
        };
        if let Some(t) = new {
            f.blocks[pi].term = t;
            n += 1;
        }
    }
    n
}

/// `alloca`s, deren Zeiger NICHT entkommt (nur Adresse von `load`/`store`).
fn simple_cells(f: &Func) -> std::collections::HashSet<Val> {
    use std::collections::HashSet;
    let mut cells: HashSet<Val> = HashSet::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Alloca { .. }) = (i.dst, &i.op) {
                cells.insert(d);
            }
        }
    }
    if cells.is_empty() {
        return cells;
    }
    let mut out: HashSet<Val> = HashSet::new();
    let mut buf = Vec::new();
    for b in &f.blocks {
        for i in &b.insts {
            match &i.op {
                // Adresse eines Zugriffs ist erlaubt; der GESPEICHERTE Wert
                // waere ein entkommender Zeiger.
                Op::Load { .. } => {}
                Op::Store { val, .. } => {
                    if cells.contains(val) {
                        out.insert(*val);
                    }
                }
                other => {
                    buf.clear();
                    other.uses(&mut buf);
                    for v in &buf {
                        if cells.contains(v) {
                            out.insert(*v);
                        }
                    }
                }
            }
        }
        match &b.term {
            Term::Ret(Some(v)) | Term::BrCond { cond: v, .. } | Term::Switch { val: v, .. } => {
                if cells.contains(v) {
                    out.insert(*v);
                }
            }
            _ => {}
        }
    }
    for v in out {
        cells.remove(&v);
    }
    cells.retain(|v| !f.is_secret(*v));
    cells
}

/// Der Bool-Wert, der am Ende von Block `pi` garantiert in einer Zelle steht:
/// der letzte `store.bool`, dem bis zum Terminator keine Speicherwirkung mehr
/// folgt. Liefert `(Zelle, Wert)`.
fn last_bool_store(f: &Func, pi: usize) -> Option<(Val, Val)> {
    let insts = &f.blocks[pi].insts;
    for i in insts.iter().rev() {
        match &i.op {
            Op::Store { addr, val } => {
                if i.ty != FTy::Bool {
                    return None; // fremder Schreibzugriff dazwischen
                }
                return Some((*addr, *val));
            }
            op if disturbs_memory(op) => return None,
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fir::{CmpOp, Module};

    /// `a < b && a != c` — genau die Form, die `dekodiere` erzeugt.
    /// bb0 = Eintritt, bb1 = rechte Seite, bb2 = Weiche, bb3/bb4 = Ziele.
    fn and_func() -> Func {
        let mut f = Func::new("t", vec![FTy::U32, FTy::U32, FTy::U32], FTy::U32);
        let bb_b = f.add_block();
        let bb_j = f.add_block();
        let bb_t = f.add_block();
        let bb_e = f.add_block();
        let cell = f.alloca(1, 1);
        let c1 = f.push(0, FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::U32, a: 0, b: 1 });
        f.push_void(0, FTy::Bool, Op::Store { addr: cell, val: c1 });
        f.set_term(0, Term::BrCond { cond: c1, then_bb: bb_b, else_bb: bb_j });
        let c2 = f.push(bb_b, FTy::Bool, Op::Cmp { op: CmpOp::Ne, ty: FTy::U32, a: 0, b: 2 });
        f.push_void(bb_b, FTy::Bool, Op::Store { addr: cell, val: c2 });
        f.set_term(bb_b, Term::Br(bb_j));
        let l = f.push(bb_j, FTy::Bool, Op::Load { addr: cell });
        f.set_term(bb_j, Term::BrCond { cond: l, then_bb: bb_t, else_bb: bb_e });
        let one = f.push(bb_t, FTy::U32, Op::Const(1));
        f.set_term(bb_t, Term::Ret(Some(one)));
        let null = f.push(bb_e, FTy::U32, Op::Const(0));
        f.set_term(bb_e, Term::Ret(Some(null)));
        f
    }

    #[test]
    fn and_short_circuit_becomes_threaded() {
        let mut f = and_func();
        let n = thread_bool_cells(&mut f);
        assert_eq!(n, 2, "beide Vorgaenger der Weiche muessen gefaedelt werden");
        match &f.blocks[0].term {
            Term::BrCond { then_bb, else_bb, .. } => {
                assert_eq!(*then_bb, 1);
                assert_eq!(*else_bb, 4, "falsche Kante geht direkt nach bb_e");
            }
            t => panic!("bb0: {:?}", t),
        }
        match &f.blocks[1].term {
            Term::BrCond { then_bb, else_bb, .. } => {
                assert_eq!(*then_bb, 3);
                assert_eq!(*else_bb, 4);
            }
            t => panic!("bb1: {:?}", t),
        }
        // Der Weichenblock selbst bleibt unveraendert stehen (die
        // Blockbereinigung in opt.rs raeumt ihn spaeter weg).
        assert_eq!(f.blocks[2].insts.len(), 1);
    }

    #[test]
    fn second_run_changes_nothing_more() {
        let mut f = and_func();
        assert_eq!(thread_bool_cells(&mut f), 2);
        assert_eq!(thread_bool_cells(&mut f), 0, "Fixpunkt nach einem Lauf");
    }

    #[test]
    fn cell_the_escapes_becomes_not_threaded() {
        let mut f = and_func();
        let cell = 3; // %0..%2 sind Parameter, %3 die alloca
        f.push_void(3, FTy::Void, Op::Call { name: "foreign".into(), args: vec![cell] });
        assert!(!simple_cells(&f).contains(&cell));
        assert_eq!(thread_bool_cells(&mut f), 0);
    }

    #[test]
    fn call_between_store_and_jump_blocked() {
        let mut f = and_func();
        f.push_void(1, FTy::Void, Op::Call { name: "foreign".into(), args: vec![] });
        // bb1 hat jetzt einen Aufruf HINTER dem store — dort darf nicht
        // gefaedelt werden, bb0 aber schon.
        assert_eq!(thread_bool_cells(&mut f), 1);
        assert!(matches!(f.blocks[1].term, Term::Br(2)));
    }

    #[test]
    fn foreign_store_between_store_and_jump_blocked() {
        let mut f = and_func();
        let p = f.push(1, FTy::Ptr, Op::Const(0));
        let w = f.push(1, FTy::U64, Op::Const(7));
        f.push_void(1, FTy::U64, Op::Store { addr: p, val: w });
        assert_eq!(thread_bool_cells(&mut f), 1);
        assert!(matches!(f.blocks[1].term, Term::Br(2)));
    }

    #[test]
    fn constant_time_stays_untouched() {
        let mut f = and_func();
        f.constant_time = true;
        assert_eq!(thread_bool_cells(&mut f), 0);
    }

    #[test]
    fn secret_value_stays_untouched() {
        let mut f = and_func();
        let c1 = 4; // %0..%2 Parameter, %3 = alloca, %4 = cmp.lt
        f.secret.insert(c1);
        // Nur der Vorgaenger mit dem geheimen Wert bleibt stehen.
        assert_eq!(thread_bool_cells(&mut f), 1);
        match &f.blocks[0].term {
            Term::BrCond { then_bb, else_bb, .. } => {
                assert_eq!((*then_bb, *else_bb), (1, 2), "bb0 unveraendert");
            }
            t => panic!("bb0: {:?}", t),
        }
    }

    #[test]
    fn module_stays_compilable() {
        let mut m = Module::default();
        m.funcs.push(and_func());
        for f in m.funcs.iter_mut() {
            thread_bool_cells(f);
        }
        assert_eq!(m.funcs.len(), 1);
    }
}
