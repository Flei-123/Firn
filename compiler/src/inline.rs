//! Inlining (Einbetten von Funktionsrumpfen) mit Groessenheuristik.
//!
//! Arbeitsweise: Ein `Op::Call` auf eine im selben `fir::Module` vorhandene
//! Funktion wird durch eine Kopie ihres Rumpfes ersetzt.
//!
//!  * Der Aufrufblock wird an der Aufrufstelle **geteilt**; der Rumpf der
//!    aufgerufenen Funktion kommt als eigene Blockgruppe dazwischen.
//!  * FIR kennt keine Phi-Knoten. Der Rueckgabewert wird deshalb ueber eine
//!    `alloca` im Eintrittsblock des Aufrufers gefuehrt: jedes `ret v` des
//!    Rumpfes wird zu `store slot, v` + `br <Fortsetzung>`, und der urspruengliche
//!    Ergebniswert wird am Anfang der Fortsetzung mit `load slot` definiert.
//!    Bei genau einem `ret` loest `mem2reg` (einmal geschriebene alloca)
//!    diesen Umweg direkt wieder auf.
//!  * `alloca`s des Rumpfes wandern in den Eintrittsblock des Aufrufers
//!    (FIR-Invariante: alle `alloca` stehen in `bb0`).
//!
//! **Modulgrenzen:** Das Modulsystem uebersetzt alle `.fi`-Dateien in EIN
//! `fir::Module` (getrennte Uebersetzung, gemeinsames Modul). Damit ist jeder
//! Aufruf einer importierten Funktion fuer diesen Durchgang genauso sichtbar
//! wie ein lokaler — Inlining wirkt ueber Modulgrenzen hinweg.
//!
//! Heuristik (bewusst konservativ, damit Uebersetzungszeit und Codegroesse
//! nicht explodieren):
//!  * Rumpf hoechstens `MAX_CALLEE_INSTS` Instruktionen,
//!    Bloecke hoechstens `MAX_CALLEE_BLOCKS`.
//!  * Aufrufer hoechstens `MAX_CALLER_INSTS` Instruktionen (danach Stopp).
//!  * keine Rekursion: kann die aufgerufene Funktion den Aufrufer im
//!    Aufrufgraphen wieder erreichen, wird nicht eingebettet.
//!  * Funktionen mit `secret`-Werten oder `#[constant_time]` bleiben aussen vor
//!    (SPEC §9: die Pruefung im Codegenerator ist funktionsweise).
//!  * hoechstens `MAX_INLINES` Einbettungen je Modul.

use crate::fir::{FTy, Func, Inst, Module, Op, Term, Val};
use std::collections::{HashMap, HashSet};

const MAX_CALLEE_INSTS: usize = 40;
const MAX_CALLEE_BLOCKS: usize = 8;
/// Obergrenze fuer den AUFRUFER. Sie schuetzt Uebersetzungszeit und
/// Codegroesse — aber sie darf nicht die heisseste Funktion des Programms
/// aussperren.
///
/// GEMESSEN (14.08.2026): der HTML5-Tokenizer `tokenizer__tokenize` hat 4.139
/// FIR-Instruktionen. Mit der alten Grenze von 4.000 bekam ausgerechnet die
/// Funktion, die jedes Zeichen jeder Seite anfasst, KEINE einzige Einbettung —
/// obwohl `sink_emit_char` mit 18 Instruktionen und einem Block weit unter
/// jeder Callee-Grenze liegt. Eine grosse Funktion ist nicht automatisch kalt;
/// bei einer Zustandsmaschine ist das Gegenteil der Fall.
const MAX_CALLER_INSTS: usize = 24000;
const MAX_INLINES: usize = 2000;

/// Kann `from` ueber Aufrufe `to` erreichen?
fn reaches(m: &Module, from: &str, to: &str) -> bool {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut stack = vec![from];
    while let Some(cur) = stack.pop() {
        if cur == to {
            return true;
        }
        if !seen.insert(cur) {
            continue;
        }
        if let Some(f) = m.funcs.iter().find(|f| f.name == cur) {
            for b in &f.blocks {
                for i in &b.insts {
                    if let Op::Call { name, .. } = &i.op {
                        stack.push(name.as_str());
                    }
                }
            }
        }
    }
    false
}

/// Kann `name` sich selbst ueber mindestens einen Aufruf wieder erreichen
/// (direkte oder indirekte Rekursion)?
///
/// Solche Rümpfe werden NICHT eingebettet. Inlining entrollt eine
/// Rekursionsstufe und verlagert ihre Rahmen in den Aufrufer — Programmcode,
/// dessen Wirkung auf der Stapeltiefe beruht (das Stapel-Scrubbing des
/// konservativen GC, `lib/gc`: `__gc_scrub_tief`), verliert dadurch seine
/// Wirkung. GEMESSEN in Runde 37: mit erhoehten Grenzen (60/10) wurde
/// `__gc_scrub_tief` (29 Insts, 9 Bloecke, rekursiv) in `main` eingebettet —
/// `tests/520_gc_weak.fi` fiel mit Exit 6 aus, weil Phantom-Zeiger im
/// ungescrubbten Stapel den Sammler naehrten.
fn reaches_itself_self(m: &Module, name: &str) -> bool {
    if let Some(f) = m.funcs.iter().find(|f| f.name == name) {
        for b in &f.blocks {
            for i in &b.insts {
                if let Op::Call { name: target, .. } = &i.op {
                    if reaches(m, target, name) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn inlinable(callee: &Func) -> bool {
    // Schleifenfreie Ruempfe OHNE Rueckgabewert (Wirkung ueber
    // Zeiger-Argumente, z. B. die Sink-Mutatoren des Tokenizers) duerfen mehr
    // Bloecke haben: ihr Kontrollfluss ist ein DAG, und weil `dst` leer ist,
    // entsteht im Aufrufer nicht einmal die Ergebnis-Alloca — der Rahmen des
    // Aufrufers bleibt bis auf echte Rumpf-Allocas unveraendert. Das ist
    // der Unterschied zu Wertruempfen: deren Ergebnis-Zelle wandert in den
    // Eintrittsblock des Aufrufers und veraendert dessen Rahmenlayout —
    // fatal fuer den stapel-scannenden konservativen GC
    // (`tests/520_gc_weak.fi`, Runde 37: `__gc_strong_raw` in `anlegen`
    // eingebettet -> Phantom-Zeiger, Exit 6).
    !callee.constant_time
        && callee.secret.is_empty()
        && callee.inst_count() <= MAX_CALLEE_INSTS
        && callee.blocks.len() <= MAX_CALLEE_BLOCKS
        && !callee.blocks.iter().any(|b| matches!(b.term, Term::Unset))
        && callee.blocks.iter().enumerate().all(|(i, b)| b.id as usize == i)
}

/// Sucht eine lohnende Aufrufstelle im Aufrufer `ci`.
/// `selbst_rek`: je Funktion vorberechnet (aendert sich durch Einbettungen
/// nicht — mutiert wird nur der Aufrufer).
fn find_site(m: &Module, ci: usize, self_rec: &[bool]) -> Option<(usize, usize, usize)> {
    let caller = &m.funcs[ci];
    if caller.constant_time || !caller.secret.is_empty() {
        return None;
    }
    if caller.inst_count() > MAX_CALLER_INSTS {
        return None;
    }
    if caller.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return None;
    }
    for (bi, b) in caller.blocks.iter().enumerate() {
        for (ii, inst) in b.insts.iter().enumerate() {
            if let Op::Call { name, args } = &inst.op {
                let gi = match m.funcs.iter().position(|f| &f.name == name) {
                    Some(g) => g,
                    None => continue,
                };
                let callee = &m.funcs[gi];
                if gi == ci || !inlinable(callee) {
                    continue;
                }
                if callee.params.len() != args.len() {
                    continue;
                }
                if inst.dst.is_some() && callee.ret == FTy::Void {
                    continue;
                }
                // Rekursion (auch indirekt) wird nicht eingebettet.
                if reaches(m, &callee.name, &caller.name) {
                    continue;
                }
                // Selbst-erreichbare Rümpfe ebenfalls nicht (siehe oben).
                if self_rec[gi] {
                    continue;
                }
                return Some((bi, ii, gi));
            }
        }
    }
    None
}

/// Bettet genau eine Aufrufstelle ein.
fn inline_one(m: &mut Module, ci: usize, bi: usize, mut ii: usize, gi: usize) {
    let callee = m.funcs[gi].clone();
    let (args, dst, ret_ty) = match &m.funcs[ci].blocks[bi].insts[ii] {
        Inst { dst, ty, op: Op::Call { args, .. } } => (args.clone(), *dst, *ty),
        _ => return,
    };

    // 1. Ergebnis-Slot und Rumpf-Allocas im Eintrittsblock anlegen.
    //    `Func::alloca` fuegt vorne ein — das verschiebt die Aufrufstelle,
    //    wenn sie selbst in bb0 liegt.
    let mut shift = 0usize;
    let result_slot = if dst.is_some() {
        shift += 1;
        Some(m.funcs[ci].alloca(ret_ty.bytes().max(1), ret_ty.bytes().max(1)))
    } else {
        None
    };
    let mut valmap: HashMap<Val, Val> = HashMap::new();
    for (k, a) in args.iter().enumerate() {
        valmap.insert(k as Val, *a);
    }
    for b in &callee.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Alloca { size, align }) = (i.dst, &i.op) {
                let nv = m.funcs[ci].alloca(*size, *align);
                valmap.insert(d, nv);
                shift += 1;
            }
        }
    }
    if bi == 0 {
        ii += shift;
    }

    // 2. Restliche Werte des Rumpfes auf neue Ids abbilden.
    let f = &mut m.funcs[ci];
    for b in &callee.blocks {
        for i in &b.insts {
            if let Some(d) = i.dst {
                if valmap.contains_key(&d) {
                    continue;
                }
                let nv = f.val_types.len() as Val;
                f.val_types.push(callee.val_types.get(d as usize).copied().unwrap_or(FTy::Void));
                valmap.insert(d, nv);
            }
        }
    }
    let mv = |v: Val| -> Val { valmap.get(&v).copied().unwrap_or(v) };

    // 3. Bloecke des Rumpfes + Fortsetzungsblock anlegen.
    let mut blockmap: HashMap<u32, u32> = HashMap::new();
    for b in &callee.blocks {
        let nb = f.add_block();
        blockmap.insert(b.id, nb);
    }
    let cont = f.add_block();

    // 4. Aufrufblock teilen.
    let tail: Vec<Inst> = f.blocks[bi].insts.split_off(ii + 1);
    f.blocks[bi].insts.pop(); // der `call` selbst faellt weg
    let old_term = std::mem::replace(&mut f.blocks[bi].term, Term::Br(blockmap[&callee.entry()]));
    f.blocks[cont as usize].insts = tail;
    f.blocks[cont as usize].term = old_term;

    // 5. Ergebniswert am Anfang der Fortsetzung definieren.
    if let (Some(d), Some(slot)) = (dst, result_slot) {
        f.blocks[cont as usize]
            .insts
            .insert(0, Inst { dst: Some(d), ty: ret_ty, op: Op::Load { addr: slot } });
    }

    // 6. Rumpf kopieren.
    for b in &callee.blocks {
        let nb = blockmap[&b.id] as usize;
        for i in &b.insts {
            if matches!(i.op, Op::Alloca { .. }) {
                continue; // steht bereits im Eintrittsblock
            }
            let op = remap_op(&i.op, &mv);
            f.blocks[nb].insts.push(Inst { dst: i.dst.map(&mv), ty: i.ty, op });
        }
        f.blocks[nb].term = match &b.term {
            Term::Br(t) => Term::Br(blockmap[t]),
            Term::BrCond { cond, then_bb, else_bb } => Term::BrCond {
                cond: mv(*cond),
                then_bb: blockmap[then_bb],
                else_bb: blockmap[else_bb],
            },
            Term::Switch { val, ty, cases, default } => Term::Switch {
                val: mv(*val),
                ty: *ty,
                cases: cases.iter().map(|(k, t)| (*k, blockmap[t])).collect(),
                default: blockmap[default],
            },
            Term::Ret(v) => {
                if let (Some(v), Some(slot)) = (v, result_slot) {
                    f.blocks[nb].insts.push(Inst {
                        dst: None,
                        ty: ret_ty,
                        op: Op::Store { addr: slot, val: mv(*v) },
                    });
                }
                Term::Br(cont)
            }
            Term::Unset => Term::Br(cont),
        };
    }
}

fn remap_op(op: &Op, mv: &dyn Fn(Val) -> Val) -> Op {
    match op {
        Op::Const(c) => Op::Const(*c),
        Op::Alloca { size, align } => Op::Alloca { size: *size, align: *align },
        Op::Bin(o, a, b) => Op::Bin(*o, mv(*a), mv(*b)),
        Op::Cmp { op, ty, a, b } => Op::Cmp { op: *op, ty: *ty, a: mv(*a), b: mv(*b) },
        Op::Un(o, a) => Op::Un(*o, mv(*a)),
        Op::Cast { src, from } => Op::Cast { src: mv(*src), from: *from },
        Op::Load { addr } => Op::Load { addr: mv(*addr) },
        Op::Store { addr, val } => Op::Store { addr: mv(*addr), val: mv(*val) },
        Op::PtrAdd { base, off } => Op::PtrAdd { base: mv(*base), off: mv(*off) },
        Op::Call { name, args } => {
            Op::Call { name: name.clone(), args: args.iter().map(|a| mv(*a)).collect() }
        }
        Op::CallIndirect { target, args } => Op::CallIndirect {
            target: mv(*target),
            args: args.iter().map(|a| mv(*a)).collect(),
        },
        Op::VtabAddr { table } => Op::VtabAddr { table: table.clone() },
        Op::Syscall { args } => Op::Syscall { args: args.iter().map(|a| mv(*a)).collect() },
        Op::CopyMem { dst, src, size } => {
            Op::CopyMem { dst: mv(*dst), src: mv(*src), size: *size }
        }
        Op::Select { cond, a, b } => Op::Select { cond: mv(*cond), a: mv(*a), b: mv(*b) },
        Op::Barrier { val } => Op::Barrier { val: mv(*val) },
        Op::SecureZero { addr, size } => Op::SecureZero { addr: mv(*addr), size: mv(*size) },
        Op::AtomicAdd { addr, val } => Op::AtomicAdd { addr: mv(*addr), val: mv(*val) },
        Op::AtomicCas { addr, erw, new } => {
            Op::AtomicCas { addr: mv(*addr), erw: mv(*erw), new: mv(*new) }
        }
        Op::ThreadSpawn { arg, stack, ctid } => {
            Op::ThreadSpawn { arg: mv(*arg), stack: mv(*stack), ctid: mv(*ctid) }
        }
        Op::ThreadSelf => Op::ThreadSelf,
        Op::GcAddr { regs } => Op::GcAddr { regs: *regs },
        Op::Asm { template, out, in_regs, ins, clobber } => Op::Asm {
            template: template.clone(),
            out: out.clone(),
            in_regs: in_regs.clone(),
            ins: ins.iter().map(|a| mv(*a)).collect(),
            clobber: clobber.clone(),
        },
        Op::MmioLoad { addr } => Op::MmioLoad { addr: mv(*addr) },
        Op::MmioStore { addr, val } => Op::MmioStore { addr: mv(*addr), val: mv(*val) },
    }
}

/// Bettet ein, solange die Heuristik es erlaubt. Liefert die Anzahl der
/// eingebetteten Aufrufe.
pub fn inline_module(m: &mut Module) -> usize {
    let mut n = 0usize;
    let dbg = std::env::var("FIRNC_INLINE_DEBUG").is_ok();
    // Einmalig bestimmen: haengt nur am Rumpf der Aufgerufenen, der sich
    // durch Einbettungen nie aendert (mutiert wird nur der Aufrufer).
    let self_rec: Vec<bool> = m
        .funcs
        .iter()
        .map(|f| reaches_itself_self(m, &f.name))
        .collect();
    'outer: loop {
        for ci in 0..m.funcs.len() {
            if let Some((bi, ii, gi)) = find_site(m, ci, &self_rec) {
                if dbg {
                    eprintln!("inline: {} <- {} ({} insts, {} bloecke)",
                        m.funcs[ci].name, m.funcs[gi].name,
                        m.funcs[gi].inst_count(), m.funcs[gi].blocks.len());
                }
                inline_one(m, ci, bi, ii, gi);
                n += 1;
                if n >= MAX_INLINES {
                    break 'outer;
                }
                continue 'outer;
            }
        }
        break;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fir::{BinOp, CmpOp, Term};

    fn add_fn() -> Func {
        let mut g = Func::new("add", vec![FTy::I32, FTy::I32], FTy::I32);
        let s = g.push(0, FTy::I32, Op::Bin(BinOp::Add, 0, 1));
        g.set_term(0, Term::Ret(Some(s)));
        g
    }

    #[test]
    fn less_call_becomes_embedded_and_folded() {
        let mut m = Module::new();
        m.funcs.push(add_fn());
        let mut f = Func::new("main", vec![], FTy::I32);
        let a = f.push(0, FTy::I32, Op::Const(2));
        let b = f.push(0, FTy::I32, Op::Const(40));
        let r = f.push(0, FTy::I32, Op::Call { name: "add".into(), args: vec![a, b] });
        f.set_term(0, Term::Ret(Some(r)));
        m.funcs.push(f);
        assert_eq!(inline_module(&mut m), 1);
        let main = m.funcs.iter().find(|f| f.name == "main").expect("main");
        assert!(!main
            .blocks
            .iter()
            .any(|b| b.insts.iter().any(|i| matches!(i.op, Op::Call { .. }))));
        crate::opt::optimize(&mut m);
        let main = m.funcs.iter().find(|f| f.name == "main").expect("main");
        // 2 + 40 wird nach dem Einbetten zu einer einzigen Konstante
        assert_eq!(main.inst_count(), 1);
        assert!(main.blocks[0].insts.iter().any(|i| matches!(i.op, Op::Const(42))));
    }

    #[test]
    fn recursion_becomes_not_embedded() {
        let mut m = Module::new();
        let mut f = Func::new("fact", vec![FTy::I32], FTy::I32);
        let one = f.push(0, FTy::I32, Op::Const(1));
        let c = f.push(0, FTy::Bool, Op::Cmp { op: CmpOp::Le, ty: FTy::I32, a: 0, b: one });
        let bt = f.add_block();
        let be = f.add_block();
        f.set_term(0, Term::BrCond { cond: c, then_bb: bt, else_bb: be });
        f.set_term(bt, Term::Ret(Some(one)));
        let sub = f.push(be, FTy::I32, Op::Bin(BinOp::Sub, 0, one));
        let rc = f.push(be, FTy::I32, Op::Call { name: "fact".into(), args: vec![sub] });
        let mu = f.push(be, FTy::I32, Op::Bin(BinOp::Mul, 0, rc));
        f.set_term(be, Term::Ret(Some(mu)));
        m.funcs.push(f);
        assert_eq!(inline_module(&mut m), 0);
    }

    #[test]
    fn several_rets_stay_correct() {
        // fn max(a,b) { if a<b { return b } return a }
        let mut g = Func::new("max", vec![FTy::I32, FTy::I32], FTy::I32);
        let c = g.push(0, FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::I32, a: 0, b: 1 });
        let bt = g.add_block();
        let be = g.add_block();
        g.set_term(0, Term::BrCond { cond: c, then_bb: bt, else_bb: be });
        g.set_term(bt, Term::Ret(Some(1)));
        g.set_term(be, Term::Ret(Some(0)));
        let mut m = Module::new();
        m.funcs.push(g);
        let mut f = Func::new("main", vec![], FTy::I32);
        let a = f.push(0, FTy::I32, Op::Const(3));
        let b = f.push(0, FTy::I32, Op::Const(9));
        let r = f.push(0, FTy::I32, Op::Call { name: "max".into(), args: vec![a, b] });
        f.set_term(0, Term::Ret(Some(r)));
        m.funcs.push(f);
        assert_eq!(inline_module(&mut m), 1);
        crate::opt::optimize(&mut m);
        let main = m.funcs.iter().find(|f| f.name == "main").expect("main");
        assert!(main.blocks[0].insts.iter().any(|i| matches!(i.op, Op::Const(9))));
    }

    #[test]
    fn constant_time_funcs_stay_separate() {
        let mut m = Module::new();
        let mut g = add_fn();
        g.constant_time = true;
        m.funcs.push(g);
        let mut f = Func::new("main", vec![], FTy::I32);
        let a = f.push(0, FTy::I32, Op::Const(2));
        let r = f.push(0, FTy::I32, Op::Call { name: "add".into(), args: vec![a, a] });
        f.set_term(0, Term::Ret(Some(r)));
        m.funcs.push(f);
        assert_eq!(inline_module(&mut m), 0);
    }
}
