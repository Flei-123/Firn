// SPDX-License-Identifier: MPL-2.0
//! **Runde TEMPO 10 — Lebensdauern zerschneiden.**
//!
//! ## Das gemessene Problem
//!
//! `FIRN_RA_STATS=1` sagt fuer den MP3-Dekoder: `synth` hat 44 gleichzeitig
//! lebende Werte, `l3_huffman` 63 — bei **vierzehn** Registern. Was darueber
//! liegt, bleibt im Rahmen, und jede Verwendung holt es von dort:
//!
//! ```text
//!   mov  -0x1760(%rbp),%rax      ; den Zeiger holen
//!   lea  (%rax,%r10,1),%r11      ; benutzen
//! ```
//!
//! Zwei Befehle statt einem, und das **je Durchlauf**. Ueber das ganze
//! Programm gezaehlt: 10,0 von 137,7 Millionen Befehlen sind "aus dem Rahmen
//! holen" — der groesste Einzelposten, der noch steht.
//!
//! ## Warum der Zuteiler das nicht von selbst loest
//!
//! Der lineare Scan kennt je Wert EIN Intervall, von der ersten bis zur
//! letzten Beruehrung, und EINEN Platz. Ein Zeiger, der am Anfang der
//! Funktion gesetzt und am Ende noch einmal gebraucht wird, belegt sein
//! Register also ueber die ganze Funktion — oder gar keines. Dazwischen
//! liegt die heisse Schleife, in der genau dieses Register fehlt.
//!
//! Die Lehrbuchantwort heisst *live range splitting*: das Intervall in
//! Stuecke schneiden und jedem Stueck einen eigenen Platz geben. Im Zuteiler
//! selbst waere das ein Umbau jeder Ausgabestelle — `loc(v)` muesste von der
//! POSITION abhaengen.
//!
//! ## Der Weg ohne Umbau: schneiden mit einer Kopie
//!
//! Dasselbe Ergebnis bekommt man, indem man das Stueck zu einem EIGENEN WERT
//! macht. Vor der Schleife steht
//!
//! ```text
//! P:  %v2 = copy %v
//! ```
//!
//! und jede Verwendung von `%v` **innerhalb** der Schleife liest ab jetzt
//! `%v2`. Damit hat `%v2` ein kurzes Intervall mit hohem Gewicht (Leser mal
//! Schleifentiefe) und bekommt fast sicher ein Register, waehrend `%v`
//! ruhig im Rahmen liegen bleiben darf. Aus einem Holen je Durchlauf wird
//! eines je Schleifeneintritt.
//!
//! Der Zuteiler braucht dafuer keine Zeile. Und wenn der Schnitt nichts
//! bringt — weil `%v` nach der Schleife gar nicht mehr gebraucht wird —,
//! macht ihn das Verschmelzen aus TEMPO 8/10 von selbst wieder rueckgaengig:
//! `%v` und `%v2` stoeren sich dann nicht und bekommen denselben Platz, die
//! Kopie verschwindet.
//!
//! ## Wann geschnitten wird
//!
//! * Der Wert wird in der Schleife **mindestens zweimal gelesen**. Bei einem
//!   einzigen Leser waere die Kopie genau so teuer wie das Holen.
//! * Der Wert wird in der Schleife **nicht geschrieben** — sonst waeren `%v`
//!   und `%v2` nach dem ersten Durchlauf verschiedene Dinge.
//! * Er ist **keine Konstante** (die steht als unmittelbarer Operand im
//!   Befehl und braucht nie einen Platz) und **kein `alloca`** (dessen
//!   Adresse rechnet `direct_frame_addrs` ohnehin ohne Register aus).
//! * Er ist nicht `secret` (SPEC §9.2).
//! * Die Schleife hat einen **Vorkopf** mit genau einem Ausgang, wie bei
//!   `licm`.
//!
//! **`phi`-Anweisungen werden nicht umgeschrieben.** Ein `phi` im
//! Schleifenkopf liest fuer die Kante aus dem Vorkopf einen Wert, der VOR
//! der Kopie gilt; die Kante aus dem Rumpf liest einen anderen. Das
//! auseinanderzuhalten waere moeglich, aber der Gewinn liegt in den
//! gewoehnlichen Verwendungen, nicht in den phis.

use crate::fir::{Func, Inst, Op, Term, Val};
use std::collections::HashSet;

/// Schneidet Lebensdauern an Schleifengrenzen. Liefert die Anzahl der
/// eingesetzten Kopien.
pub(crate) fn split_at_loops(f: &mut Func) -> usize {
    let n = f.blocks.len();
    if n < 3 || f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return 0;
    }
    // Dieselbe billige Vorpruefung wie in `licm`: ohne Rueckwaertskante gibt
    // es keine Schleife.
    let backward = f
        .blocks
        .iter()
        .enumerate()
        .any(|(b, blk)| blk.term.successors().into_iter().any(|s| (s as usize) <= b));
    if !backward {
        return 0;
    }
    let preds = crate::mem2reg::preds(f);
    let dom = crate::mem2reg::dominators(f);

    let mut edges: Vec<(usize, usize)> = Vec::new();
    for (b, blk) in f.blocks.iter().enumerate() {
        for s in blk.term.successors() {
            let h = s as usize;
            if h < n && dom[b][h] {
                edges.push((h, b));
            }
        }
    }
    if edges.is_empty() {
        return 0;
    }
    // Innerste Schleifen zuerst: der kleinere Rumpf liegt weiter innen.
    let mut loops: Vec<(usize, HashSet<usize>)> = edges
        .into_iter()
        .map(|(h, b)| (h, crate::licm::natural_loop(h, b, &preds)))
        .collect();
    loops.sort_by_key(|(_, body)| body.len());

    // Wo wird was geschrieben? (einmal fuer die ganze Funktion)
    let nv = f.val_types.len();
    let mut def_in: Vec<Option<usize>> = vec![None; nv];
    for (bi, b) in f.blocks.iter().enumerate() {
        for i in &b.insts {
            if let Some(d) = i.dst {
                if (d as usize) < nv {
                    def_in[d as usize] = Some(bi);
                }
            }
        }
    }
    // Welche Werte sind Konstanten oder `alloca`-Adressen?
    let mut raw: Vec<bool> = vec![false; nv];
    for b in &f.blocks {
        for i in &b.insts {
            if let Some(d) = i.dst {
                if (d as usize) < nv && matches!(i.op, Op::Const(_) | Op::Alloca { .. }) {
                    raw[d as usize] = true;
                }
            }
        }
    }

    let at_least: u32 = match std::env::var("FIRN_SPLIT_MIN") {
        Ok(v) => v.parse().unwrap_or(3),
        Err(_) => 3,
    };
    let mut inserted = 0usize;
    let mut already: HashSet<usize> = HashSet::new(); // Kopf schon bearbeitet

    for (head, body) in loops {
        if !already.insert(head) {
            continue;
        }
        let preheader = match crate::licm::preheader_of(f, head, &body, &preds) {
            Some(p) => p,
            None => continue,
        };
        // --- zaehlen: welcher Wert wird im Rumpf wie oft GELESEN? --------
        let mut readers: Vec<u32> = vec![0; f.val_types.len()];
        let mut written: HashSet<Val> = HashSet::new();
        let mut buf = Vec::new();
        for &bi in body.iter() {
            for i in &f.blocks[bi].insts {
                if let Some(d) = i.dst {
                    written.insert(d);
                }
                // Ein `phi` wird nicht umgeschrieben, also zaehlt er auch
                // nicht als Leser.
                if matches!(i.op, Op::Phi { .. }) {
                    continue;
                }
                buf.clear();
                i.op.uses(&mut buf);
                for u in buf.iter() {
                    if let Some(c) = readers.get_mut(*u as usize) {
                        *c += 1;
                    }
                }
            }
            match &f.blocks[bi].term {
                Term::BrCond { cond: v, .. }
                | Term::Switch { val: v, .. }
                | Term::Ret(Some(v)) => {
                    if let Some(c) = readers.get_mut(*v as usize) {
                        *c += 1;
                    }
                }
                _ => {}
            }
        }
        // --- Bewerber sammeln --------------------------------------------
        let mut candidates: Vec<(u32, Val)> = Vec::new();
        for v in 0..f.val_types.len() {
            let vv = v as Val;
            if readers[v] < at_least || written.contains(&vv) {
                continue;
            }
            if f.is_secret(vv) || raw.get(v).copied().unwrap_or(false) {
                continue;
            }
            // Ausserhalb der Schleife definiert, und die Definition muss den
            // Vorkopf beherrschen (sonst gibt es den Wert dort nicht).
            if v >= f.params.len() {
                match def_in.get(v).copied().flatten() {
                    Some(db) => {
                        if body.contains(&db) {
                            continue;
                        }
                        if db != preheader && !dom[preheader][db] {
                            continue;
                        }
                    }
                    None => continue,
                }
            }
            candidates.push((readers[v], vv));
        }
        if candidates.is_empty() {
            continue;
        }
        candidates.sort_by_key(|(c, v)| (std::cmp::Reverse(*c), *v));

        // --- schneiden ----------------------------------------------------
        let mut map: Vec<(Val, Val)> = Vec::new();
        for (_, v) in candidates.iter().copied() {
            let ty = f.val_ty(v);
            let neu = f.new_val_pub(ty);
            let loc = f.blocks[preheader].insts.last().map(|i| i.loc).unwrap_or_default();
            f.blocks[preheader].insts.push(Inst::like(Some(neu), ty, Op::Copy { src: v }, loc));
            map.push((v, neu));
            inserted += 1;
        }
        // Verwendungen im Rumpf umschreiben (phis ausgenommen).
        for &bi in body.iter() {
            for i in f.blocks[bi].insts.iter_mut() {
                if matches!(i.op, Op::Phi { .. }) {
                    continue;
                }
                i.op.for_each_use_mut(|v| {
                    for (alt, neu) in map.iter().copied() {
                        if *v == alt {
                            *v = neu;
                            break;
                        }
                    }
                });
            }
            match &mut f.blocks[bi].term {
                Term::BrCond { cond: v, .. }
                | Term::Switch { val: v, .. }
                | Term::Ret(Some(v)) => {
                    for (alt, neu) in map.iter().copied() {
                        if *v == alt {
                            *v = neu;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    inserted
}


// ---------------------------------------------------------------------------
// NACH DER ZUTEILUNG — der Schnitt, der weiss, wofuer er gut ist
// ---------------------------------------------------------------------------
//
// DIE MESSUNG, DIE DIESEN ZWEITEN ANLAUF ERZWUNGEN HAT. Die Fassung oben
// schneidet im Optimierer, also BEVOR jemand weiss, welcher Wert ueberhaupt
// ein Register bekommt. Gemessen am MP3-Dekoder: 137,7 -> 140,4 Mio Befehle,
// also zwei Prozent SCHLECHTER. Der Grund ist einfach und im Nachhinein
// offensichtlich: wo der neue Wert auch nur einen Platz bekommt, zahlt man
// die Kopie im Vorkopf und gewinnt nichts, denn der Rumpf liest dann eben
// den anderen Platz.
//
// Also andersherum. `emit_func_ra` teilt EINMAL zu, fragt hier nach, welche
// Werte wirklich im Rahmen gelandet sind UND in einer Schleife mehrfach
// gelesen werden, schneidet nur diese, und teilt noch einmal zu. Bekommt
// dabei kein einziger der neuen Werte ein Register, wird der Schnitt
// verworfen — dann kostet er nur Uebersetzungszeit und kein einziges Bit im
// Programm.

/// Werte, die im Rahmen gelandet sind und in einer Schleife mehrfach gelesen
/// werden: fuer jeden eine Kopie in den Vorkopf, und im Rumpf liest alles
/// die Kopie. `None` = es gibt nichts zu schneiden.
///
/// Liefert die geaenderte Funktion und die Liste der neuen Werte.
pub(crate) fn after_allocation(
    f: &Func,
    in_frame: &dyn Fn(Val) -> bool,
) -> Option<(Func, Vec<Val>)> {
    let n = f.blocks.len();
    if n < 3 || f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return None;
    }
    let backward = f
        .blocks
        .iter()
        .enumerate()
        .any(|(b, blk)| blk.term.successors().into_iter().any(|s| (s as usize) <= b));
    if !backward {
        return None;
    }
    let preds = crate::mem2reg::preds(f);
    let dom = crate::mem2reg::dominators(f);
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for (b, blk) in f.blocks.iter().enumerate() {
        for s in blk.term.successors() {
            let h = s as usize;
            if h < n && dom[b][h] {
                edges.push((h, b));
            }
        }
    }
    if edges.is_empty() {
        return None;
    }
    let mut loops: Vec<(usize, HashSet<usize>)> = edges
        .into_iter()
        .map(|(h, b)| (h, crate::licm::natural_loop(h, b, &preds)))
        .collect();
    loops.sort_by_key(|(_, body)| body.len());

    let nv = f.val_types.len();
    let mut def_in: Vec<Option<usize>> = vec![None; nv];
    let mut raw: Vec<bool> = vec![false; nv];
    for (bi, b) in f.blocks.iter().enumerate() {
        for i in &b.insts {
            if let Some(d) = i.dst {
                if (d as usize) < nv {
                    def_in[d as usize] = Some(bi);
                    if matches!(i.op, Op::Const(_) | Op::Alloca { .. }) {
                        raw[d as usize] = true;
                    }
                }
            }
        }
    }
    let at_least: u32 = match std::env::var("FIRN_SPLIT_MIN") {
        Ok(v) => v.parse().unwrap_or(3),
        Err(_) => 3,
    };

    let mut g = f.clone();
    let mut fresh: Vec<Val> = Vec::new();
    let mut done: HashSet<usize> = HashSet::new();

    for (head, body) in loops {
        if !done.insert(head) {
            continue;
        }
        let preheader = match crate::licm::preheader_of(f, head, &body, &preds) {
            Some(p) => p,
            None => continue,
        };
        let mut readers: Vec<u32> = vec![0; nv];
        let mut written: HashSet<Val> = HashSet::new();
        let mut buf = Vec::new();
        for &bi in body.iter() {
            for i in &f.blocks[bi].insts {
                if let Some(d) = i.dst {
                    written.insert(d);
                }
                buf.clear();
                i.op.uses(&mut buf);
                for u in buf.iter() {
                    if let Some(c) = readers.get_mut(*u as usize) {
                        *c += 1;
                    }
                }
            }
            match &f.blocks[bi].term {
                Term::BrCond { cond: v, .. }
                | Term::Switch { val: v, .. }
                | Term::Ret(Some(v)) => {
                    if let Some(c) = readers.get_mut(*v as usize) {
                        *c += 1;
                    }
                }
                _ => {}
            }
        }
        let mut candidates: Vec<(u32, Val)> = Vec::new();
        for v in 0..nv {
            let vv = v as Val;
            if readers[v] < at_least || written.contains(&vv) {
                continue;
            }
            if f.is_secret(vv) || raw[v] {
                continue;
            }
            // DAS IST DER UNTERSCHIED ZUR FASSUNG OBEN: nur was der Zuteiler
            // wirklich in den Rahmen gelegt hat.
            if !in_frame(vv) {
                continue;
            }
            if v >= f.params.len() {
                match def_in[v] {
                    Some(db) => {
                        if body.contains(&db) {
                            continue;
                        }
                        if db != preheader && !dom[preheader][db] {
                            continue;
                        }
                    }
                    None => continue,
                }
            }
            candidates.push((readers[v], vv));
        }
        if candidates.is_empty() {
            continue;
        }
        candidates.sort_by_key(|(c, v)| (std::cmp::Reverse(*c), *v));
        let mut map: Vec<(Val, Val)> = Vec::new();
        for (_, v) in candidates.iter().copied() {
            let ty = g.val_ty(v);
            let neu = g.new_val_pub(ty);
            let loc = g.blocks[preheader].insts.last().map(|i| i.loc).unwrap_or_default();
            g.blocks[preheader].insts.push(Inst::like(Some(neu), ty, Op::Copy { src: v }, loc));
            map.push((v, neu));
            fresh.push(neu);
        }
        for &bi in body.iter() {
            for i in g.blocks[bi].insts.iter_mut() {
                i.op.for_each_use_mut(|x| {
                    for (alt, neu) in map.iter().copied() {
                        if *x == alt {
                            *x = neu;
                            break;
                        }
                    }
                });
            }
            match &mut g.blocks[bi].term {
                Term::BrCond { cond: v, .. }
                | Term::Switch { val: v, .. }
                | Term::Ret(Some(v)) => {
                    for (alt, neu) in map.iter().copied() {
                        if *v == alt {
                            *v = neu;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    if fresh.is_empty() {
        None
    } else {
        // Der Schnitt darf nicht sofort wieder verschmolzen werden.
        for v in fresh.iter() {
            g.no_coalesce.insert(*v);
        }
        Some((g, fresh))
    }
}
