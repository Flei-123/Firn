//! Speicher -> Wert: Aufloesen von `alloca`/`store`/`load`, Kopierfortpflanzung
//! und Blockverschmelzung.
//!
//! Diese Datei enthaelt die Durchgaenge, die in Runde 1 fehlten und zu Recht
//! bemaengelt wurden:
//!
//!  * **mem2reg (einmal geschriebene alloca):** Eine `alloca`, deren Zeiger
//!    nirgends entkommt (nur als Adresse von `load`/`store` benutzt) und in die
//!    GENAU EINMAL geschrieben wird, wobei der `store` alle `load`s
//!    **dominiert**, wird aufgeloest: jeder `load` wird durch den gespeicherten
//!    Wert ersetzt. FIR kennt keine Phi-Knoten — deshalb ist die Dominanz-
//!    bedingung hier zwingend und nicht nur eine Optimierung (mehrfach
//!    geschriebene Zellen bleiben im Speicher; der Registerzuteiler
//!    (`regalloc.rs`) haelt sie dafuer dauerhaft in einem Register).
//!  * **lokale Speicherweiterleitung:** `store p, v` gefolgt von `load p` im
//!    selben Block ohne dazwischenliegenden Speichereffekt -> der `load` wird
//!    zu `v`. Ebenso `load p` ... `load p` (gemeinsamer Teilausdruck).
//!  * **Kopierfortpflanzung / algebraische Vereinfachung:** Identitaets-`cast`,
//!    `x+0`, `x-0`, `x*1`, `x*0`, `x|0`, `x^0`, `x&-1`, `x<<0`, `x>>0`,
//!    `x/1`, `ptradd p, 0`.
//!  * **Blockverschmelzung:** `A: ... br B` mit B als einzigem Nachfolger und
//!    A als einzigem Vorgaenger -> B wird an A angehaengt. Leere Bloecke mit
//!    reinem `br C` werden ueberbrueckt (Sprungfaedelung).
//!
//! HARTE REGEL (SPEC §9.2): `Op::Select`, `Op::Barrier`, `Op::SecureZero` und
//! jeder Wert aus `f.secret` werden hier NIE veraendert, ersetzt oder entfernt.
//! Ihre Operanden werden nicht umgeschrieben; ein `select` wird nie zu einer
//! Verzweigung.

use crate::fir::{BinOp, Func, Inst, Op, Term, Val};
use std::collections::HashMap;

/// Instruktionen, die der Optimierer als unantastbar behandelt (SPEC §9).
pub(crate) fn is_untouchable(op: &Op) -> bool {
    matches!(op, Op::Select { .. } | Op::Barrier { .. } | Op::SecureZero { .. })
}

// ------------------------------------------------------------- Hilfsmittel ---

/// Vorgaengerlisten. Setzt die FIR-Invariante `blocks[i].id == i` voraus.
pub(crate) fn preds(f: &Func) -> Vec<Vec<usize>> {
    let n = f.blocks.len();
    let mut p = vec![Vec::new(); n];
    for (i, b) in f.blocks.iter().enumerate() {
        for s in b.term.successors() {
            let s = s as usize;
            if s < n && !p[s].contains(&i) {
                p[s].push(i);
            }
        }
    }
    p
}

/// `dom[b][d] == true`  <=>  Block `d` dominiert Block `b`.
pub(crate) fn dominators(f: &Func) -> Vec<Vec<bool>> {
    let n = f.blocks.len();
    let pr = preds(f);
    let mut dom = vec![vec![true; n]; n];
    if n == 0 {
        return dom;
    }
    for (d, v) in dom[0].iter_mut().enumerate() {
        *v = d == 0;
    }
    let mut rounds = 0;
    loop {
        rounds += 1;
        let mut changed = false;
        for b in 1..n {
            let mut new = vec![false; n];
            if !pr[b].is_empty() {
                new = vec![true; n];
                for &p in &pr[b] {
                    for d in 0..n {
                        new[d] &= dom[p][d];
                    }
                }
            }
            new[b] = true;
            if new != dom[b] {
                dom[b] = new;
                changed = true;
            }
        }
        if !changed || rounds > n + 2 {
            break;
        }
    }
    dom
}

/// Ist der Wert `v` in `f` unantastbar (geheim)?
fn locked(f: &Func, v: Val) -> bool {
    f.is_secret(v)
}

/// Ersetzt Verwendungen gemaess `map` (nur einfache Ersetzung, keine Kette).
/// Liefert die Anzahl umgeschriebener Operanden.
pub(crate) fn replace_uses(f: &mut Func, map: &HashMap<Val, Val>) -> usize {
    if map.is_empty() {
        return 0;
    }
    let secret: Vec<Val> = f.secret.iter().copied().collect();
    let is_locked = |v: Val| secret.contains(&v);
    let mut n = 0usize;
    let rep = |v: &mut Val, n: &mut usize| {
        if let Some(&nv) = map.get(v) {
            if !is_locked(*v) && !is_locked(nv) {
                *v = nv;
                *n += 1;
            }
        }
    };
    for b in f.blocks.iter_mut() {
        for i in b.insts.iter_mut() {
            if is_untouchable(&i.op) {
                continue; // SPEC §9.2: Operanden bleiben, wie sie sind
            }
            match &mut i.op {
                Op::Const(_) | Op::Alloca { .. } | Op::GcAddr { .. } => {}
                Op::Bin(_, a, b2) => {
                    rep(a, &mut n);
                    rep(b2, &mut n);
                }
                Op::Cmp { a, b: b2, .. } => {
                    rep(a, &mut n);
                    rep(b2, &mut n);
                }
                Op::Un(_, a) => rep(a, &mut n),
                Op::Cast { src, .. } => rep(src, &mut n),
                Op::Load { addr } => rep(addr, &mut n),
                Op::Store { addr, val } => {
                    rep(addr, &mut n);
                    rep(val, &mut n);
                }
                Op::PtrAdd { base, off } => {
                    rep(base, &mut n);
                    rep(off, &mut n);
                }
                Op::Call { args, .. } | Op::Syscall { args } => {
                    for a in args.iter_mut() {
                        rep(a, &mut n);
                    }
                }
                Op::CallIndirect { target, args } => {
                    rep(target, &mut n);
                    for a in args.iter_mut() {
                        rep(a, &mut n);
                    }
                }
                Op::VtabAddr { .. } => {}
                Op::CopyMem { dst, src, .. } => {
                    rep(dst, &mut n);
                    rep(src, &mut n);
                }
                Op::Select { .. } | Op::Barrier { .. } | Op::SecureZero { .. } => {}
            }
        }
        match &mut b.term {
            Term::BrCond { cond, .. } => rep(cond, &mut n),
            Term::Switch { val, .. } => rep(val, &mut n),
            Term::Ret(Some(v)) => rep(v, &mut n),
            _ => {}
        }
    }
    n
}

// ------------------------------------------------------------------ mem2reg ---

/// Beschreibt, wie eine `alloca` benutzt wird.
struct CellUse {
    /// nur als Adresse von load/store (kein ptradd, kein Aufrufargument, ...)
    simple: bool,
    stores: Vec<(usize, usize)>, // (Block, Index)
    loads: Vec<(usize, usize)>,
}

fn scan_cells(f: &Func) -> HashMap<Val, CellUse> {
    let mut cells: HashMap<Val, CellUse> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Alloca { .. }) = (i.dst, &i.op) {
                cells.insert(d, CellUse { simple: true, stores: Vec::new(), loads: Vec::new() });
            }
        }
    }
    let mut buf = Vec::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        for (ii, i) in b.insts.iter().enumerate() {
            match &i.op {
                Op::Load { addr } => {
                    if let Some(c) = cells.get_mut(addr) {
                        c.loads.push((bi, ii));
                    }
                }
                Op::Store { addr, val } => {
                    if let Some(c) = cells.get_mut(addr) {
                        c.stores.push((bi, ii));
                    }
                    if let Some(c) = cells.get_mut(val) {
                        c.simple = false; // Zeiger entkommt als Wert
                    }
                }
                other => {
                    buf.clear();
                    other.uses(&mut buf);
                    for v in buf.iter() {
                        if let Some(c) = cells.get_mut(v) {
                            c.simple = false;
                        }
                    }
                }
            }
        }
        match &b.term {
            Term::Ret(Some(v)) => {
                if let Some(c) = cells.get_mut(v) {
                    c.simple = false;
                }
            }
            Term::BrCond { cond, .. } => {
                if let Some(c) = cells.get_mut(cond) {
                    c.simple = false;
                }
            }
            Term::Switch { val, .. } => {
                if let Some(c) = cells.get_mut(val) {
                    c.simple = false;
                }
            }
            _ => {}
        }
    }
    cells
}

/// Loest `alloca`s auf, in die genau einmal geschrieben wird und deren `store`
/// alle `load`s dominiert. Liefert die Anzahl ersetzter `load`s.
pub(crate) fn promote_single_store(f: &mut Func) -> usize {
    if f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return 0;
    }
    let cells = scan_cells(f);
    let dom = dominators(f);
    let mut map: HashMap<Val, Val> = HashMap::new();
    for (cell, u) in cells.iter() {
        if !u.simple || u.stores.len() != 1 || u.loads.is_empty() || locked(f, *cell) {
            continue;
        }
        let (sb, si) = u.stores[0];
        let (sty, sval) = match &f.blocks[sb].insts[si].op {
            Op::Store { val, .. } => (f.blocks[sb].insts[si].ty, *val),
            _ => continue,
        };
        if locked(f, sval) {
            continue;
        }
        // Der gespeicherte Wert darf nicht die Zelle selbst sein.
        if sval == *cell {
            continue;
        }
        let mut ok = true;
        for &(lb, li) in &u.loads {
            let lty = f.blocks[lb].insts[li].ty;
            if lty != sty {
                ok = false; // andere Breite: Speichersemantik, nicht anfassen
                break;
            }
            let dominates = if lb == sb { li > si } else { dom[lb][sb] };
            if !dominates {
                ok = false;
                break;
            }
        }
        if !ok {
            continue;
        }
        for &(lb, li) in &u.loads {
            if let Some(d) = f.blocks[lb].insts[li].dst {
                if !locked(f, d) {
                    map.insert(d, sval);
                }
            }
        }
    }
    if map.is_empty() {
        return 0;
    }
    let n = map.len();
    replace_uses(f, &map);
    n
}

/// Entfernt `alloca`s, deren Zeiger nicht entkommt und die NIE gelesen werden:
/// samt aller `store`s dorthin (tote Speicherung). Genau das bleibt uebrig,
/// nachdem `promote_single_store` die `load`s aufgeloest hat — in Runde 1 blieb
/// dieser Rest stehen. Liefert die Anzahl entfernter Instruktionen.
pub(crate) fn remove_dead_stores(f: &mut Func) -> usize {
    let cells = scan_cells(f);
    let dead: Vec<Val> = cells
        .iter()
        .filter(|(v, u)| u.simple && u.loads.is_empty() && !locked(f, **v))
        .map(|(v, _)| *v)
        .collect();
    if dead.is_empty() {
        return 0;
    }
    let mut n = 0usize;
    for b in f.blocks.iter_mut() {
        let before = b.insts.len();
        b.insts.retain(|i| match &i.op {
            Op::Store { addr, .. } => !dead.contains(addr),
            Op::Alloca { .. } => match i.dst {
                Some(d) => !dead.contains(&d),
                None => true,
            },
            _ => true,
        });
        n += before - b.insts.len();
    }
    n
}

// ----------------------------------------------- lokale Speicherweiterleitung ---

/// Wird durch diese Instruktion Speicher veraendert (aliasfrei nicht beweisbar)?
fn clobbers_memory(op: &Op) -> bool {
    matches!(
        op,
        Op::Store { .. }
            | Op::Call { .. }
            | Op::CallIndirect { .. }
            | Op::Syscall { .. }
            | Op::CopyMem { .. }
            | Op::SecureZero { .. }
    )
}

/// `store p, v; ... ; load p` -> `v` und `load p; ...; load p` -> erster Wert,
/// jeweils nur innerhalb eines Blocks und nur ohne dazwischenliegenden
/// Speichereffekt. Liefert die Anzahl weitergeleiteter `load`s.
pub(crate) fn forward_local_loads(f: &mut Func) -> usize {
    let mut map: HashMap<Val, Val> = HashMap::new();
    for b in &f.blocks {
        // bekannte Zelleninhalte: Adresswert -> (Typ, Wert)
        let mut known: HashMap<Val, (crate::fir::FTy, Val)> = HashMap::new();
        for i in &b.insts {
            match &i.op {
                Op::Load { addr } => {
                    if let Some(d) = i.dst {
                        match known.get(addr) {
                            Some(&(t, v)) if t == i.ty && !locked(f, v) && !locked(f, d) => {
                                map.insert(d, v);
                            }
                            _ => {
                                known.insert(*addr, (i.ty, d));
                            }
                        }
                    }
                }
                Op::Store { addr, val } => {
                    // jeder andere Eintrag koennte dieselbe Zelle meinen
                    known.clear();
                    known.insert(*addr, (i.ty, *val));
                }
                other => {
                    if clobbers_memory(other) {
                        known.clear();
                    }
                }
            }
        }
    }
    if map.is_empty() {
        return 0;
    }
    let n = map.len();
    replace_uses(f, &map);
    n
}

// ------------------------------------- Kopierfortpflanzung / Vereinfachung ---

/// Identitaeten und triviale algebraische Vereinfachungen. Liefert die Anzahl
/// der Ersetzungen.
pub(crate) fn copy_propagate(f: &mut Func) -> usize {
    let mut consts: HashMap<Val, i128> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Const(c)) = (i.dst, &i.op) {
                consts.insert(d, *c);
            }
        }
    }
    let mut map: HashMap<Val, Val> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            let d = match i.dst {
                Some(d) => d,
                None => continue,
            };
            if locked(f, d) || is_untouchable(&i.op) {
                continue;
            }
            let same = match &i.op {
                Op::Cast { src, from } => {
                    // Gleiche Breite UND gleiches Vorzeichen: reine Umdeutung
                    // desselben Bitmusters (z. B. `usize` <-> `*mut T`).
                    //
                    // `f64` DARF HIER NICHT MITSPIELEN. Es ist 64 Bit breit und
                    // gilt als vorzeichenlos — nach der Regel oben sah
                    // `u64 -> f64` also wie eine reine Umdeutung aus, und die
                    // Umwandlung verschwand ersatzlos. Aus `100 as f64` wurde
                    // damit das Bitmuster 100 statt des Wertes 100.0. Es ist
                    // aber genau umgekehrt: von allen Umwandlungen ist die
                    // zwischen Ganzzahl und Gleitkomma die einzige, die die
                    // Bits WIRKLICH aendert (`cvtsi2sd`).
                    //
                    // Gefunden beim Vergleich des in Firn geschriebenen Lexers
                    // gegen `firnc0` (Runde 20): `10.0` ergab zwei verschiedene
                    // Tokenstroeme, je nachdem ob der Optimierer lief.
                    let gleitwechsel = (*from == crate::fir::FTy::F64)
                        != (i.ty == crate::fir::FTy::F64);
                    if !gleitwechsel
                        && (*from == i.ty
                            || (from.bits() == i.ty.bits()
                                && from.signed() == i.ty.signed()
                                && *from != crate::fir::FTy::Bool
                                && i.ty != crate::fir::FTy::Bool))
                    {
                        Some(*src)
                    } else {
                        None
                    }
                }
                Op::PtrAdd { base, off } => {
                    if consts.get(off) == Some(&0) {
                        Some(*base)
                    } else {
                        None
                    }
                }
                Op::Bin(op, a, b2) => {
                    let ca = consts.get(a).copied();
                    let cb = consts.get(b2).copied();
                    let all_ones = |t: crate::fir::FTy| t.truncate(-1);
                    match op {
                        BinOp::Add => {
                            if cb == Some(0) {
                                Some(*a)
                            } else if ca == Some(0) {
                                Some(*b2)
                            } else {
                                None
                            }
                        }
                        BinOp::Sub | BinOp::Shl | BinOp::Shr => {
                            if cb == Some(0) {
                                Some(*a)
                            } else {
                                None
                            }
                        }
                        BinOp::Or | BinOp::Xor => {
                            if cb == Some(0) {
                                Some(*a)
                            } else if ca == Some(0) {
                                Some(*b2)
                            } else {
                                None
                            }
                        }
                        BinOp::Mul | BinOp::Div => {
                            if cb == Some(1) {
                                Some(*a)
                            } else if *op == BinOp::Mul && ca == Some(1) {
                                Some(*b2)
                            } else {
                                None
                            }
                        }
                        BinOp::And => {
                            if cb == Some(all_ones(i.ty)) {
                                Some(*a)
                            } else if ca == Some(all_ones(i.ty)) {
                                Some(*b2)
                            } else {
                                None
                            }
                        }
                        BinOp::Rem => None,
                    }
                }
                _ => None,
            };
            if let Some(s) = same {
                if s != d
                    && !locked(f, s)
                    && f.val_ty(s).bits() == f.val_ty(d).bits()
                    && f.val_ty(s).signed() == f.val_ty(d).signed()
                {
                    map.insert(d, s);
                }
            }
        }
    }
    if map.is_empty() {
        return 0;
    }
    // Ketten aufloesen (a->b->c), aber ohne Zyklusgefahr.
    let keys: Vec<Val> = map.keys().copied().collect();
    for k in keys {
        let mut cur = map[&k];
        let mut steps = 0;
        while let Some(&next) = map.get(&cur) {
            if next == cur || steps > 64 {
                break;
            }
            cur = next;
            steps += 1;
        }
        map.insert(k, cur);
    }
    let n = map.len();
    replace_uses(f, &map);
    n
}

// -------------------------------------------------------- Blockverschmelzung ---

/// Verschmilzt `A -> B`, wenn A genau einen Nachfolger (B) und B genau einen
/// Vorgaenger (A) hat, und ueberbrueckt leere `br`-Bloecke.
/// Liefert die Anzahl entfernter Bloecke.
pub(crate) fn merge_blocks(f: &mut Func) -> usize {
    if f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return 0;
    }
    let mut removed = 0usize;
    // (1) Sprungfaedelung: leerer Block mit `br C` wird uebersprungen.
    let mut rounds = 0;
    loop {
        rounds += 1;
        let n = f.blocks.len();
        let mut redirect: Vec<Option<u32>> = vec![None; n];
        for (i, b) in f.blocks.iter().enumerate() {
            if i == 0 || !b.insts.is_empty() {
                continue;
            }
            if let Term::Br(t) = b.term {
                if t as usize != i {
                    redirect[i] = Some(t);
                }
            }
        }
        if redirect.iter().all(|r| r.is_none()) {
            break;
        }
        // Ketten aufloesen (mit Deckel gegen Zyklen)
        let resolve = |mut t: u32| -> u32 {
            let mut steps = 0;
            while let Some(nt) = redirect[t as usize] {
                if nt == t || steps > 64 {
                    break;
                }
                t = nt;
                steps += 1;
            }
            t
        };
        let mut changed = false;
        let mut new_terms: Vec<Term> = Vec::with_capacity(n);
        for b in f.blocks.iter() {
            let t = match &b.term {
                Term::Br(t) => {
                    let r = resolve(*t);
                    if r != *t {
                        changed = true;
                    }
                    Term::Br(r)
                }
                Term::BrCond { cond, then_bb, else_bb } => {
                    let (a, b2) = (resolve(*then_bb), resolve(*else_bb));
                    if a != *then_bb || b2 != *else_bb {
                        changed = true;
                    }
                    Term::BrCond { cond: *cond, then_bb: a, else_bb: b2 }
                }
                Term::Switch { val, ty, cases, default } => {
                    let cs: Vec<(i128, u32)> = cases.iter().map(|(k, t)| (*k, resolve(*t))).collect();
                    let d = resolve(*default);
                    if cs != *cases || d != *default {
                        changed = true;
                    }
                    Term::Switch { val: *val, ty: *ty, cases: cs, default: d }
                }
                other => other.clone(),
            };
            new_terms.push(t);
        }
        if !changed {
            break;
        }
        for (b, t) in f.blocks.iter_mut().zip(new_terms) {
            b.term = t;
        }
        removed += 1;
        if rounds > 16 {
            break;
        }
    }

    // (2) Verschmelzen: A endet mit `br B`, B hat nur A als Vorgaenger.
    let mut rounds = 0;
    loop {
        rounds += 1;
        // Nur erreichbare Vorgaenger zaehlen — unerreichbare Bloecke raeumt
        // `opt.rs` gleich danach weg.
        let mut reach = vec![false; f.blocks.len()];
        let mut stack = vec![0usize];
        if !reach.is_empty() {
            reach[0] = true;
        }
        while let Some(bi) = stack.pop() {
            for sblk in f.blocks[bi].term.successors() {
                let sblk = sblk as usize;
                if sblk < reach.len() && !reach[sblk] {
                    reach[sblk] = true;
                    stack.push(sblk);
                }
            }
        }
        let mut pr = preds(f);
        for (i, p) in pr.iter_mut().enumerate() {
            let _ = i;
            p.retain(|&x| reach[x]);
        }
        let mut target: Option<(usize, usize)> = None;
        for (i, b) in f.blocks.iter().enumerate() {
            if let Term::Br(t) = b.term {
                let t = t as usize;
                if t != i && t != 0 && t < f.blocks.len() && pr[t].len() == 1 && pr[t][0] == i {
                    // Allocas duerfen nur im Eintrittsblock stehen: beim
                    // Verschmelzen in bb0 ist das erfuellt, sonst nur, wenn B
                    // keine Alloca enthaelt.
                    let has_alloca =
                        f.blocks[t].insts.iter().any(|x| matches!(x.op, Op::Alloca { .. }));
                    if has_alloca && i != 0 {
                        continue;
                    }
                    target = Some((i, t));
                    break;
                }
            }
        }
        let (a, b) = match target {
            Some(x) => x,
            None => break,
        };
        let moved = std::mem::take(&mut f.blocks[b].insts);
        let term = f.blocks[b].term.clone();
        if a == 0 {
            // Allocas muessen vorne bleiben.
            let (allocas, rest): (Vec<Inst>, Vec<Inst>) =
                moved.into_iter().partition(|x| matches!(x.op, Op::Alloca { .. }));
            let pos = f.blocks[0]
                .insts
                .iter()
                .take_while(|x| matches!(x.op, Op::Alloca { .. }))
                .count();
            for (k, ins) in allocas.into_iter().enumerate() {
                f.blocks[0].insts.insert(pos + k, ins);
            }
            f.blocks[0].insts.extend(rest);
        } else {
            f.blocks[a].insts.extend(moved);
        }
        f.blocks[a].term = term;
        f.blocks[b].term = Term::Unset; // wird unerreichbar -> DCE raeumt auf
        f.blocks[b].insts.clear();
        // Block b ist jetzt ohne Vorgaenger; die Neunummerierung erledigt opt.rs.
        removed += 1;
        if rounds > 4096 {
            break;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fir::{CmpOp, FTy, Module, Term};

    #[test]
    fn einmal_geschriebene_alloca_wird_aufgeloest() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let slot = f.alloca(4, 4);
        let c = f.push(0, FTy::I32, Op::Const(7));
        f.push_void(0, FTy::I32, Op::Store { addr: slot, val: c });
        let b1 = f.add_block();
        f.set_term(0, Term::Br(b1));
        let l = f.push(b1, FTy::I32, Op::Load { addr: slot });
        f.set_term(b1, Term::Ret(Some(l)));
        assert_eq!(promote_single_store(&mut f), 1);
        assert!(matches!(f.blocks[1].term, Term::Ret(Some(v)) if v == c));
        // nach der kompletten Optimierung bleibt nur noch die Konstante
        let mut m = Module::new();
        m.funcs.push(f);
        crate::opt::optimize(&mut m);
        assert_eq!(m.funcs[0].inst_count(), 1);
    }

    #[test]
    fn mehrfach_geschriebene_alloca_bleibt_stehen() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let slot = f.alloca(4, 4);
        let c = f.push(0, FTy::I32, Op::Const(1));
        f.push_void(0, FTy::I32, Op::Store { addr: slot, val: c });
        let b1 = f.add_block();
        let b2 = f.add_block();
        let cond = f.push(0, FTy::Bool, Op::Load { addr: slot });
        f.set_term(0, Term::BrCond { cond, then_bb: b1, else_bb: b2 });
        let c2 = f.push(b1, FTy::I32, Op::Const(2));
        f.push_void(b1, FTy::I32, Op::Store { addr: slot, val: c2 });
        f.set_term(b1, Term::Br(b2));
        let l = f.push(b2, FTy::I32, Op::Load { addr: slot });
        f.set_term(b2, Term::Ret(Some(l)));
        assert_eq!(promote_single_store(&mut f), 0);
    }

    #[test]
    fn load_nach_store_wird_im_block_weitergeleitet() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let slot = f.alloca(4, 4);
        let c = f.push(0, FTy::I32, Op::Const(5));
        f.push_void(0, FTy::I32, Op::Store { addr: slot, val: c });
        let l = f.push(0, FTy::I32, Op::Load { addr: slot });
        let s = f.push(0, FTy::I32, Op::Bin(BinOp::Add, l, l));
        f.set_term(0, Term::Ret(Some(s)));
        assert_eq!(forward_local_loads(&mut f), 1);
        assert!(matches!(f.blocks[0].insts.last().unwrap().op, Op::Bin(BinOp::Add, x, y) if x == c && y == c));
    }

    #[test]
    fn aufruf_zwischen_store_und_load_verhindert_weiterleitung() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let slot = f.alloca(4, 4);
        let c = f.push(0, FTy::I32, Op::Const(5));
        f.push_void(0, FTy::I32, Op::Store { addr: slot, val: c });
        f.push_void(0, FTy::Void, Op::Call { name: "g".into(), args: vec![] });
        let l = f.push(0, FTy::I32, Op::Load { addr: slot });
        f.set_term(0, Term::Ret(Some(l)));
        assert_eq!(forward_local_loads(&mut f), 0);
    }

    #[test]
    fn algebraische_identitaeten() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let p = f.push(0, FTy::I32, Op::Call { name: "g".into(), args: vec![] });
        let z = f.push(0, FTy::I32, Op::Const(0));
        let a = f.push(0, FTy::I32, Op::Bin(BinOp::Add, p, z));
        let one = f.push(0, FTy::I32, Op::Const(1));
        let b = f.push(0, FTy::I32, Op::Bin(BinOp::Mul, a, one));
        f.set_term(0, Term::Ret(Some(b)));
        assert!(copy_propagate(&mut f) >= 1);
        let mut m = Module::new();
        m.funcs.push(f);
        crate::opt::optimize(&mut m);
        // uebrig bleibt nur der Aufruf (unrein) und `ret %aufruf`
        assert!(matches!(m.funcs[0].blocks[0].term, Term::Ret(Some(v)) if v == p));
    }

    #[test]
    fn leere_bloecke_werden_verschmolzen() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let b1 = f.add_block();
        let b2 = f.add_block();
        f.set_term(0, Term::Br(b1));
        f.set_term(b1, Term::Br(b2));
        let c = f.push(b2, FTy::I32, Op::Const(3));
        f.set_term(b2, Term::Ret(Some(c)));
        assert!(merge_blocks(&mut f) > 0);
        let mut m = Module::new();
        m.funcs.push(f);
        crate::opt::optimize(&mut m);
        assert_eq!(m.funcs[0].blocks.len(), 1);
        assert!(matches!(m.funcs[0].blocks[0].term, Term::Ret(Some(_))));
    }

    #[test]
    fn secret_werte_bleiben_unangetastet() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let slot = f.alloca(4, 4);
        let c = f.push(0, FTy::I32, Op::Const(9));
        f.secret.insert(c);
        f.push_void(0, FTy::I32, Op::Store { addr: slot, val: c });
        let l = f.push(0, FTy::I32, Op::Load { addr: slot });
        f.set_term(0, Term::Ret(Some(l)));
        assert_eq!(forward_local_loads(&mut f), 0);
        assert_eq!(promote_single_store(&mut f), 0);
    }

    #[test]
    fn select_bleibt_select() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let c = f.push(0, FTy::Bool, Op::Const(1));
        let a = f.push(0, FTy::I32, Op::Const(1));
        let z = f.push(0, FTy::I32, Op::Const(0));
        let a2 = f.push(0, FTy::I32, Op::Bin(BinOp::Add, a, z));
        let s = f.push(0, FTy::I32, Op::Select { cond: c, a: a2, b: z });
        f.set_term(0, Term::Ret(Some(s)));
        copy_propagate(&mut f);
        // der select-Operand wurde NICHT umgeschrieben
        assert!(matches!(f.blocks[0].insts.last().unwrap().op, Op::Select { a, .. } if a == a2));
        let mut m = Module::new();
        m.funcs.push(f);
        crate::opt::optimize(&mut m);
        assert!(m.funcs[0]
            .blocks
            .iter()
            .any(|b| b.insts.iter().any(|i| matches!(i.op, Op::Select { .. }))));
        let _ = CmpOp::Eq;
    }
}
