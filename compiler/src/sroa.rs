// SPDX-License-Identifier: MPL-2.0
//! **Runde TEMPO 12 -- ein Struct auf dem Stapel wird zu einzelnen Zellen.**
//!
//! ## Warum es diesen Pass gibt
//!
//! Der Huffman-Leser des MP3-Dekoders haelt seinen Zustand in einem kleinen
//! Struct und reicht einen Zeiger darauf an vier Helfer weiter:
//!
//! ```firn
//! var lage: HuffLage = HuffLage { cache: .., sh: .., next: .. }
//! let l: *mut HuffLage = &lage
//! ... flush_bits(l, w) ... peek_bits(l, 5) ... check_bits(l)
//! ```
//!
//! Nach dem Einbetten der Helfer ist davon nur noch ein `alloca` uebrig,
//! auf das mit festen Versaetzen geladen und geschrieben wird. `mem2reg`
//! befoerdert aber nur Zellen, die als GANZES gelesen und geschrieben werden
//! -- ein `ptradd` auf die Zelle macht sie fuer ihn unantastbar. Also lagen
//! `cache`, `sh` und `next` die ganze Funktion ueber im Rahmen, und jede
//! Bitoperation des heissesten Dekoderteils ging durch den Speicher:
//! `mov -0x1cc0(%rbp),%r8d ... mov %r13d,-0x1cc0(%rbp)`. In C ist genau
//! dieser Zustand (`bs_cache`, `bs_sh`, `bs_next` in minimp3) eine Handvoll
//! lokaler Variablen und steht in Registern.
//!
//! ## Was der Pass tut
//!
//! Ein `alloca`, dessen Adresse NUR so benutzt wird:
//!
//!   * `load`/`store` direkt auf die Zelle (Versatz 0), oder
//!   * `p = ptradd zelle, K` mit konstantem `K`, und `p` wiederum NUR als
//!     Adresse eines `load`/`store`,
//!
//! wird in eine Zelle je Versatz zerlegt. Jede neue Zelle ist wieder ein
//! gewoehnliches `alloca`, das `mem2reg` in der naechsten Runde befoerdert.
//!
//! ## Warum das nichts brechen kann
//!
//! Die Adresse verlaesst die Funktion nie (sie wird weder gespeichert noch
//! uebergeben noch verglichen -- jede solche Benutzung laesst den Pass die
//! Finger davon lassen). Also kann niemand ausser den aufgezaehlten Zugriffen
//! den Speicher sehen. Verlangt wird ausserdem, dass alle Zugriffe auf einen
//! Versatz denselben Typ haben und sich keine zwei Felder ueberlappen -- dann
//! ist jeder Zugriff genau ein Feld, und ein Feld in einer eigenen Zelle
//! verhaelt sich wie dasselbe Feld im Struct. Ein `copymem` auf oder von der
//! Zelle (Struct-Zuweisung als Ganzes) ist eine andere Benutzung und
//! verhindert den Pass; das ist der naechste Schritt, nicht dieser.
//!
//! Abschaltbar mit `--no-pass=sroa` oder `FIRN_NO_SROA=1`.

use crate::fir::{Func, Op, Term, Val};
use std::collections::{BTreeMap, HashMap, HashSet};

pub(crate) fn split(f: &mut Func) -> usize {
    if std::env::var_os("FIRN_NO_SROA").is_some() {
        return 0;
    }
    // Konstanten (fuer die Versaetze).
    let mut konst: HashMap<Val, i128> = HashMap::new();
    let mut zellen: HashMap<Val, (u64, u64)> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            match (&i.op, i.dst) {
                (Op::Const(c), Some(d)) => {
                    konst.insert(d, *c);
                }
                (Op::Alloca { size, align }, Some(d)) => {
                    zellen.insert(d, (*size, *align));
                }
                _ => {}
            }
        }
    }
    if zellen.is_empty() {
        return 0;
    }
    // ptradd-Ergebnis -> (Zelle, Versatz)
    let mut feldzeiger: HashMap<Val, (Val, i128)> = HashMap::new();
    let mut schlecht: HashSet<Val> = HashSet::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Op::PtrAdd { base, off }, Some(d)) = (&i.op, i.dst) {
                if zellen.contains_key(base) {
                    match konst.get(off) {
                        Some(&k) if k >= 0 => {
                            feldzeiger.insert(d, (*base, k));
                        }
                        _ => {
                            schlecht.insert(*base);
                        }
                    }
                }
            }
        }
    }
    // Zelle -> Versatz -> Typ; jede andere Benutzung macht die Zelle schlecht.
    let mut felder: HashMap<Val, BTreeMap<i128, crate::fir::FTy>> = HashMap::new();
    let wurzel = |v: Val| -> Option<(Val, i128)> {
        if zellen.contains_key(&v) {
            Some((v, 0))
        } else {
            feldzeiger.get(&v).copied()
        }
    };
    let mut buf: Vec<Val> = Vec::new();
    for b in &f.blocks {
        for i in &b.insts {
            match &i.op {
                Op::Load { addr } | Op::Store { addr, .. } => {
                    if let Some((z, k)) = wurzel(*addr) {
                        let e = felder.entry(z).or_default();
                        match e.get(&k) {
                            None => {
                                e.insert(k, i.ty);
                            }
                            Some(t) if *t == i.ty => {}
                            _ => {
                                schlecht.insert(z);
                            }
                        }
                    }
                    if let Op::Store { val, .. } = &i.op {
                        if let Some((z, _)) = wurzel(*val) {
                            schlecht.insert(z);
                        }
                    }
                }
                Op::PtrAdd { base, .. } if zellen.contains_key(base) => {
                    // schon oben eingeordnet; der Versatz selbst ist keine
                    // Benutzung einer Zelle
                }
                other => {
                    buf.clear();
                    other.uses(&mut buf);
                    for v in &buf {
                        if let Some((z, _)) = wurzel(*v) {
                            schlecht.insert(z);
                        }
                    }
                }
            }
        }
        let t = match &b.term {
            Term::Ret(Some(v)) => Some(*v),
            Term::BrCond { cond, .. } => Some(*cond),
            Term::Switch { val, .. } => Some(*val),
            _ => None,
        };
        if let Some(v) = t {
            if let Some((z, _)) = wurzel(v) {
                schlecht.insert(z);
            }
        }
        // phi-Eintraege zaehlen als Benutzung
        for i in &b.insts {
            if let Op::Phi { incoming } = &i.op {
                for (_, v) in incoming.iter() {
                    if let Some((z, _)) = wurzel(*v) {
                        schlecht.insert(z);
                    }
                }
            }
        }
    }
    // Auswahl: mindestens ein ptradd (sonst kann mem2reg es schon), keine
    // Ueberlappung, alles innerhalb der Zelle, kein geheimer Wert.
    let mut kandidaten: Vec<Val> = felder
        .iter()
        .filter(|(z, fs)| {
            if schlecht.contains(z) || f.is_secret(**z) {
                return false;
            }
            if !feldzeiger.values().any(|(w, _)| w == *z) {
                return false;
            }
            let (size, _) = zellen[*z];
            let mut ende: i128 = 0;
            for (&k, t) in fs.iter() {
                let n = t.bytes() as i128;
                if n == 0 || k < ende || k + n > size as i128 {
                    return false;
                }
                ende = k + n;
            }
            true
        })
        .map(|(z, _)| *z)
        .collect();
    if kandidaten.is_empty() {
        return 0;
    }
    // Reihenfolge fest (Fixpunkt: zwei Laeufe muessen denselben Text geben).
    kandidaten.sort_unstable();
    let mut neu: HashMap<(Val, i128), Val> = HashMap::new();
    for z in &kandidaten {
        let (_, align) = zellen[z];
        for (&k, t) in felder[z].iter() {
            let n = t.bytes();
            let a = n.min(align).max(1);
            let nv = f.alloca(n, a);
            neu.insert((*z, k), nv);
        }
    }
    let ziel = |v: Val| -> Option<Val> {
        let (z, k) = if zellen.contains_key(&v) {
            (v, 0)
        } else {
            *feldzeiger.get(&v)?
        };
        neu.get(&(z, k)).copied()
    };
    for b in f.blocks.iter_mut() {
        for i in b.insts.iter_mut() {
            match &mut i.op {
                Op::Load { addr } | Op::Store { addr, .. } => {
                    if let Some(nv) = ziel(*addr) {
                        *addr = nv;
                    }
                }
                _ => {}
            }
        }
    }
    // Die alten ptradd und die alte Zelle sind jetzt tot; `dce` raeumt sie.
    kandidaten.len()
}
