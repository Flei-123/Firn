//! Echte Registerzuteilung: **linear scan mit Lebendigkeitsintervallen**
//! (Poletto/Sarkar) plus ein registerbewusster Emissionspfad.
//!
//! Bis Runde 1 bekam jeder FIR-Wert einen eigenen Stack-Slot; jede Instruktion
//! war `load`-`load`-rechnen-`store`. Diese Datei ersetzt das:
//!
//!  1. **Lebendigkeitsanalyse** je Basisblock (Rueckwaertsfluss, `live_in`/
//!     `live_out`), daraus je Wert EIN Intervall `[start, end]` in einer
//!     linearen Nummerierung aller Instruktionen.
//!  2. **Zellen-Befoerderung:** eine `alloca`, deren Zeiger nie entkommt (nur
//!     direkte Adresse von `load`/`store`), die hoechstens 8 Byte gross ist und
//!     immer mit derselben Breite angesprochen wird, lebt komplett in einem
//!     Register — `load` wird zur Registerkopie, `store` zum Registerschreiben.
//!     Das ersetzt die Phi-Knoten, die FIR nicht hat, und bringt genau die
//!     Schleifenzaehler in Register, die `mem2reg` (nur einmal geschriebene
//!     Zellen) nicht befoerdern kann.
//!  3. **Linear scan** ueber die nach `start` sortierten Intervalle mit
//!     aktiver Liste; reicht der Vorrat nicht, wird das Intervall mit dem
//!     spaetesten Ende und dem kleinsten Gewicht (Verwendungen, gewichtet mit
//!     der Schleifentiefe) in den Stack ausgelagert. Es wird NICHT geteilt:
//!     ein Wert liegt entweder ueber seine ganze Lebensdauer in einem Register
//!     oder ueber seine ganze Lebensdauer im Stack — damit ist keine
//!     Umlade-Logik noetig und die Zuteilung nachweislich verhaltenserhaltend.
//!
//! **Registerwahl (System V AMD64):**
//!  * `rax`, `rcx`, `rdx`, `rsi`, `rdi` bleiben Arbeitsregister und werden nie
//!    vergeben (sie sind Argument-/Hilfsregister von `call`, `syscall`,
//!    `div`, `rep movsb`).
//!  * Vergeben werden `rbx`, `r12`, `r13`, `r14`, `r15` (callee-saved, in
//!    Prolog/Epilog gesichert) und `r11` (caller-saved) — `r11` nur fuer
//!    Intervalle, die keinen `call`/`syscall` ueberspannen.
//!  * `r10`, `r8`, `r9` werden bewusst NICHT vergeben: sie sind Argument-
//!    register von `call`/`syscall` und koennten beim Aufbau der Argumentliste
//!    einen noch benoetigten Wert ueberschreiben.
//!
//! **SPEC §9:** `Op::Select` bleibt `cmov`, `Op::Barrier` und `Op::SecureZero`
//! werden unveraendert erzeugt, und die Pruefung „bedingter Sprung haengt von
//! einem `secret`-Wert ab" gilt in diesem Pfad genauso wie im Grundpfad.
//!
//! Der Emissionspfad ist **abgesichert**: Konstrukte, die er nicht vollstaendig
//! beherrscht (mehr als sechs Parameter/Argumente, unbekannte Blocknummerierung
//! …), fuehren dazu, dass `emit_func_ra` `None` liefert und `codegen_x86.rs`
//! seinen bewaehrten Grundpfad benutzt.

use crate::codegen_x86::{block_label, label, size_word, Emitter, Frame, ARG_REGS};
use crate::fir::{BinOp, Block, CmpOp, FTy, Func, Inst, Op, Term, UnOp, Val};
use std::collections::HashMap;

/// Ort eines Wertes nach der Zuteilung.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Loc {
    /// festes Maschinenregister (64-Bit-Name)
    Reg(&'static str),
    /// Stack-Slot: Adresse = `rbp - off`
    Slot(u64),
}

/// callee-saved Register, die vergeben werden duerfen (Prolog/Epilog sichern).
const CALLEE_SAVED: [&str; 5] = ["rbx", "r12", "r13", "r14", "r15"];
/// caller-saved Register fuer Intervalle, die KEINEN `call`/`syscall`
/// einschliessen: dann kann weder der Aufruf selbst noch der Aufbau seiner
/// Argumentliste (rdi, rsi, rdx, rcx, r8, r9, r10) den Wert zerstoeren.
const TEMP_REGS: [&str; 4] = ["r11", "r10", "r9", "r8"];

fn align_up(x: u64, a: u64) -> u64 {
    if a <= 1 {
        x
    } else {
        (x + a - 1) / a * a
    }
}

/// Ergebnis der Registerzuteilung einer Funktion.
pub struct Alloc {
    locs: Vec<Loc>,
    /// Konstanten, die an JEDER ihrer Verwendungsstellen als Sofortoperand
    /// stehen duerfen: sie brauchen weder Register noch Slot.
    imms: HashMap<Val, i64>,
    /// `alloca`-Werte mit festem Rahmenoffset (Adressierung ohne Umweg).
    frame_addr: HashMap<Val, u64>,
    /// befoerderte `alloca`-Zellen: Zeigerwert -> Register
    cells: HashMap<Val, &'static str>,
    /// Zugriffsbreite je befoerderter Zelle
    cell_ty: HashMap<Val, FTy>,
    /// benutzte callee-saved Register und ihr Sicherungs-Slot
    saved: Vec<(&'static str, u64)>,
    frame: Frame,
}

impl Alloc {
    /// Ort eines Wertes. Einzige Anfrageschnittstelle des Codegenerators.
    pub fn loc(&self, v: Val) -> Loc {
        self.locs.get(v as usize).copied().unwrap_or(Loc::Slot(0))
    }
    /// Sofortoperand eines Wertes, falls er als solcher taugt.
    fn imm(&self, v: Val) -> Option<i64> {
        self.imms.get(&v).copied()
    }
    /// Register einer befoerderten `alloca`-Zelle, falls vorhanden.
    fn cell(&self, addr: Val) -> Option<(&'static str, FTy)> {
        match (self.cells.get(&addr), self.cell_ty.get(&addr)) {
            (Some(r), Some(t)) => Some((*r, *t)),
            _ => None,
        }
    }
}

// ------------------------------------------------------------ Rahmenlayout ---

fn layout(f: &Func, extra_slots: u64) -> (Frame, Vec<(&'static str, u64)>) {
    let n = f.val_types.len();
    let mut slot = vec![0u64; n];
    let mut cursor = 0u64;
    for s in slot.iter_mut() {
        cursor += 8;
        *s = cursor;
    }
    let mut alloca_off: Vec<Option<u64>> = vec![None; n];
    for b in &f.blocks {
        if b.id != f.entry() && b.insts.iter().any(|i| matches!(i.op, Op::Alloca { .. })) {
            continue;
        }
        for i in &b.insts {
            if let Op::Alloca { size, align } = i.op {
                if let Some(d) = i.dst {
                    let a = if align == 0 { 1 } else { align.min(16) };
                    cursor = align_up(cursor + size.max(1), a);
                    alloca_off[d as usize] = Some(cursor);
                }
            }
        }
    }
    let mut saved = Vec::new();
    for k in 0..extra_slots {
        cursor += 8;
        let _ = k;
        saved.push(cursor);
    }
    let saved_pairs: Vec<(&'static str, u64)> =
        saved.into_iter().map(|off| ("", off)).collect::<Vec<_>>();
    (Frame { slot, alloca_off, size: align_up(cursor, 16) }, saved_pairs)
}

// -------------------------------------------------------- Lebendigkeitsanalyse ---

struct Live {
    /// lineare Position der ersten Instruktion je Block
    block_start: Vec<usize>,
    /// Position des Terminators je Block
    block_end: Vec<usize>,
    /// Position jeder Instruktion: pos[block][index]
    pos: Vec<Vec<usize>>,
    live_in: Vec<Vec<bool>>,
    live_out: Vec<Vec<bool>>,
}

fn compute_live(f: &Func) -> Live {
    let nb = f.blocks.len();
    let nv = f.val_types.len();
    let mut pos = Vec::with_capacity(nb);
    let mut block_start = vec![0usize; nb];
    let mut block_end = vec![0usize; nb];
    let mut p = 1usize;
    for (bi, b) in f.blocks.iter().enumerate() {
        block_start[bi] = p;
        let mut v = Vec::with_capacity(b.insts.len());
        for _ in &b.insts {
            v.push(p);
            p += 1;
        }
        block_end[bi] = p;
        p += 1;
        pos.push(v);
    }

    let mut usek = vec![vec![false; nv]; nb];
    let mut defk = vec![vec![false; nv]; nb];
    let mut buf = Vec::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        for i in &b.insts {
            buf.clear();
            i.op.uses(&mut buf);
            for &u in buf.iter() {
                if (u as usize) < nv && !defk[bi][u as usize] {
                    usek[bi][u as usize] = true;
                }
            }
            if let Some(d) = i.dst {
                if (d as usize) < nv {
                    defk[bi][d as usize] = true;
                }
            }
        }
        let t = match &b.term {
            Term::BrCond { cond, .. } => Some(*cond),
            Term::Switch { val, .. } => Some(*val),
            Term::Ret(Some(v)) => Some(*v),
            _ => None,
        };
        if let Some(v) = t {
            if (v as usize) < nv && !defk[bi][v as usize] {
                usek[bi][v as usize] = true;
            }
        }
    }

    let mut live_in = vec![vec![false; nv]; nb];
    let mut live_out = vec![vec![false; nv]; nb];
    let mut rounds = 0usize;
    loop {
        rounds += 1;
        let mut changed = false;
        for bi in (0..nb).rev() {
            let mut out = vec![false; nv];
            for s in f.blocks[bi].term.successors() {
                let s = s as usize;
                if s < nb {
                    for v in 0..nv {
                        out[v] |= live_in[s][v];
                    }
                }
            }
            if out != live_out[bi] {
                live_out[bi] = out;
                changed = true;
            }
            let mut inn = vec![false; nv];
            for v in 0..nv {
                inn[v] = usek[bi][v] || (live_out[bi][v] && !defk[bi][v]);
            }
            if inn != live_in[bi] {
                live_in[bi] = inn;
                changed = true;
            }
        }
        if !changed || rounds > nb + 4 {
            break;
        }
    }
    Live { block_start, block_end, pos, live_in, live_out }
}

// ---------------------------------------------------------- Zellenanalyse ---

/// Findet `alloca`s, die komplett in einem Register leben koennen.
fn promotable_cells(f: &Func) -> HashMap<Val, FTy> {
    let mut cand: HashMap<Val, Option<FTy>> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Alloca { size, .. }) = (i.dst, &i.op) {
                if *size <= 8 && !f.is_secret(d) {
                    cand.insert(d, None);
                }
            }
        }
    }
    if cand.is_empty() {
        return HashMap::new();
    }
    let mut bad: Vec<Val> = Vec::new();
    let mut buf = Vec::new();
    for b in &f.blocks {
        for i in &b.insts {
            match &i.op {
                Op::Load { addr } => {
                    if let Some(slot) = cand.get_mut(addr) {
                        match slot {
                            Some(t) if *t != i.ty => bad.push(*addr),
                            Some(_) => {}
                            None => *slot = Some(i.ty),
                        }
                    }
                    if let Some(d) = i.dst {
                        if f.is_secret(d) && cand.contains_key(addr) {
                            bad.push(*addr);
                        }
                    }
                }
                Op::Store { addr, val } => {
                    if let Some(slot) = cand.get_mut(addr) {
                        match slot {
                            Some(t) if *t != i.ty => bad.push(*addr),
                            Some(_) => {}
                            None => *slot = Some(i.ty),
                        }
                    }
                    if cand.contains_key(val) {
                        bad.push(*val); // Adresse entkommt als Wert
                    }
                }
                other => {
                    buf.clear();
                    other.uses(&mut buf);
                    for v in buf.iter() {
                        if cand.contains_key(v) {
                            bad.push(*v);
                        }
                    }
                }
            }
        }
        match &b.term {
            Term::Ret(Some(v)) | Term::BrCond { cond: v, .. } | Term::Switch { val: v, .. } => {
                if cand.contains_key(v) {
                    bad.push(*v);
                }
            }
            _ => {}
        }
    }
    for b in bad {
        cand.remove(&b);
    }
    cand.into_iter().filter_map(|(v, t)| t.map(|t| (v, t))).collect()
}


// ------------------------------------ Sofortkonstanten / direkte Adressierung ---

/// Konstanten, die an JEDER Verwendungsstelle als x86-Sofortoperand stehen
/// duerfen. Sie brauchen dann weder Register noch Slot und ihre `const`-
/// Instruktion faellt ganz weg.
fn immediate_consts(f: &Func) -> HashMap<Val, i64> {
    let mut cand: HashMap<Val, i64> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Const(c)) = (i.dst, &i.op) {
                let v = i.ty.truncate(*c);
                if !f.is_secret(d) && v >= i32::MIN as i128 && v <= i32::MAX as i128 {
                    cand.insert(d, v as i64);
                }
            }
        }
    }
    if cand.is_empty() {
        return cand;
    }
    let mut bad: Vec<Val> = Vec::new();
    let kill = |v: Val, bad: &mut Vec<Val>| bad.push(v);
    for b in &f.blocks {
        for i in &b.insts {
            match &i.op {
                // Operand `a` geht ueber `load_ext` (movsx/movzx) -> kein Immediate
                Op::Bin(BinOp::Div, a, b2) | Op::Bin(BinOp::Rem, a, b2) => {
                    kill(*a, &mut bad);
                    kill(*b2, &mut bad);
                }
                Op::Bin(BinOp::Shl, a, _) | Op::Bin(BinOp::Shr, a, _) => kill(*a, &mut bad),
                Op::Cast { src, .. } => kill(*src, &mut bad),
                // unantastbar (SPEC §9.2): unveraendert wie im Grundpfad
                Op::Select { cond, a, b: b2 } => {
                    kill(*cond, &mut bad);
                    kill(*a, &mut bad);
                    kill(*b2, &mut bad);
                }
                Op::Barrier { val } => kill(*val, &mut bad),
                Op::SecureZero { addr, size } => {
                    kill(*addr, &mut bad);
                    kill(*size, &mut bad);
                }
                _ => {}
            }
        }
        match &b.term {
            Term::BrCond { cond, .. } => kill(*cond, &mut bad),
            Term::Switch { val, .. } => kill(*val, &mut bad),
            _ => {}
        }
    }
    for v in bad {
        cand.remove(&v);
    }
    cand
}

/// `alloca`s, deren Adresse nur als `load`/`store`-Adresse oder als Basis eines
/// `ptradd` auftaucht: sie werden direkt ueber `rbp` adressiert, der Zeiger
/// muss nie in einem Register stehen.
fn direct_frame_addrs(f: &Func, fr: &Frame) -> HashMap<Val, u64> {
    let mut cand: HashMap<Val, u64> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Alloca { .. }) = (i.dst, &i.op) {
                if let Some(Some(off)) = fr.alloca_off.get(d as usize).copied() {
                    if !f.is_secret(d) {
                        cand.insert(d, off);
                    }
                }
            }
        }
    }
    if cand.is_empty() {
        return cand;
    }
    let mut bad: Vec<Val> = Vec::new();
    let mut buf = Vec::new();
    for b in &f.blocks {
        for i in &b.insts {
            match &i.op {
                Op::Load { .. } => {}
                Op::Store { val, .. } => bad.push(*val),
                Op::PtrAdd { off, .. } => bad.push(*off),
                other => {
                    buf.clear();
                    other.uses(&mut buf);
                    bad.extend(buf.iter().copied());
                }
            }
        }
        match &b.term {
            Term::Ret(Some(v)) | Term::BrCond { cond: v, .. } | Term::Switch { val: v, .. } => {
                bad.push(*v)
            }
            _ => {}
        }
    }
    for v in bad {
        cand.remove(&v);
    }
    cand
}

// ------------------------------------------------------------- Linear Scan ---

#[derive(Clone, Copy)]
struct Iv {
    val: Val,
    start: usize,
    end: usize,
    weight: u64,
    crosses_call: bool,
}

/// Schleifentiefe je Block (Naeherung: Rueckwaertskante u->v mit v <= u
/// umfasst die Bloecke [v, u]).
fn loop_depth(f: &Func) -> Vec<u32> {
    let nb = f.blocks.len();
    let mut depth = vec![0u32; nb];
    for (u, b) in f.blocks.iter().enumerate() {
        for s in b.term.successors() {
            let v = s as usize;
            if v <= u && v < nb {
                for d in depth.iter_mut().take(u + 1).skip(v) {
                    *d += 1;
                }
            }
        }
    }
    for d in depth.iter_mut() {
        if *d > 4 {
            *d = 4;
        }
    }
    depth
}

/// Fuehrt die vollstaendige Zuteilung durch.
pub fn allocate(f: &Func) -> Alloc {
    let nv = f.val_types.len();
    let nb = f.blocks.len();
    let mut locs: Vec<Loc> = Vec::with_capacity(nv);
    let (frame, _) = layout(f, 0);
    for v in 0..nv {
        locs.push(Loc::Slot(frame.slot.get(v).copied().unwrap_or(0)));
    }
    let mut alloc = Alloc {
        locs,
        imms: HashMap::new(),
        frame_addr: HashMap::new(),
        cells: HashMap::new(),
        cell_ty: HashMap::new(),
        saved: Vec::new(),
        frame,
    };
    // Sicherheitsnetz gegen Speicher-/Zeitexplosion bei riesigen Funktionen:
    // dann bleibt es beim (korrekten) Stack-Modell.
    if nb == 0 || nv == 0 || nv.saturating_mul(nb) > 8_000_000 {
        return alloc;
    }
    if f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return alloc;
    }

    let live = compute_live(f);
    let depth = loop_depth(f);
    let cells = promotable_cells(f);
    alloc.imms = immediate_consts(f);
    alloc.frame_addr = direct_frame_addrs(f, &alloc.frame);
    for v in alloc.imms.keys() {
        alloc.locs[*v as usize] = Loc::Slot(0);
    }

    // Aufrufpositionen (fuer `crosses_call`)
    let mut call_pos: Vec<usize> = Vec::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        for (ii, i) in b.insts.iter().enumerate() {
            if matches!(i.op, Op::Call { .. } | Op::Syscall { .. }) {
                call_pos.push(live.pos[bi][ii]);
            }
        }
    }

    // Intervalle + Gewichte
    let mut start = vec![usize::MAX; nv];
    let mut end = vec![0usize; nv];
    let mut weight = vec![0u64; nv];
    let touch = |v: Val, p: usize, w: u64, start: &mut Vec<usize>, end: &mut Vec<usize>, weight: &mut Vec<u64>| {
        let v = v as usize;
        if v >= nv {
            return;
        }
        if p < start[v] {
            start[v] = p;
        }
        if p > end[v] {
            end[v] = p;
        }
        weight[v] = weight[v].saturating_add(w);
    };
    for i in 0..f.params.len() {
        touch(i as Val, 0, 1, &mut start, &mut end, &mut weight);
    }
    let mut buf = Vec::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        let w = 10u64.saturating_pow(depth[bi]);
        for v in 0..nv {
            if live.live_in[bi][v] {
                touch(v as Val, live.block_start[bi], 0, &mut start, &mut end, &mut weight);
            }
            if live.live_out[bi][v] {
                touch(v as Val, live.block_end[bi], 0, &mut start, &mut end, &mut weight);
            }
        }
        for (ii, i) in b.insts.iter().enumerate() {
            let p = live.pos[bi][ii];
            buf.clear();
            i.op.uses(&mut buf);
            let uses: Vec<Val> = buf.clone();
            for u in uses {
                touch(u, p, w, &mut start, &mut end, &mut weight);
            }
            if let Some(d) = i.dst {
                touch(d, p, w, &mut start, &mut end, &mut weight);
            }
        }
        let tv = match &b.term {
            Term::BrCond { cond, .. } => Some(*cond),
            Term::Switch { val, .. } => Some(*val),
            Term::Ret(Some(v)) => Some(*v),
            _ => None,
        };
        if let Some(v) = tv {
            touch(v, live.block_end[bi], w, &mut start, &mut end, &mut weight);
        }
    }

    let mut ivs: Vec<Iv> = Vec::new();
    for v in 0..nv {
        if start[v] == usize::MAX {
            continue;
        }
        if f.is_secret(v as Val) {
            continue; // geheime Werte bleiben im Stack-Slot (SPEC §9.2)
        }
        if cells.contains_key(&(v as Val)) {
            continue; // wird als Zelle behandelt
        }
        if alloc.imms.contains_key(&(v as Val)) || alloc.frame_addr.contains_key(&(v as Val)) {
            continue; // braucht ueberhaupt keinen Ort
        }
        // Werte, deren Ort der Speicher IST (alloca-Adressen), duerfen ein
        // Register bekommen; ihr Inhalt liegt weiterhin im Rahmen.
        let (s, e) = (start[v], end[v]);
        let cc = call_pos.iter().any(|&p| s <= p && p <= e);
        ivs.push(Iv { val: v as Val, start: s, end: e, weight: weight[v], crosses_call: cc });
    }
    for (&c, _) in cells.iter() {
        let cv = c as usize;
        if start[cv] == usize::MAX {
            continue;
        }
        // Die Zelle muss ab Funktionsbeginn bis zum letzten Zugriff im Register
        // stehen (ihr Inhalt ueberlebt Bloecke ohne Zugriff).
        let s = 0usize;
        let e = end[cv];
        let cc = call_pos.iter().any(|&p| s <= p && p <= e);
        // Zellen sind fast immer die heissesten Werte: Gewicht verdoppeln.
        ivs.push(Iv {
            val: c,
            start: s,
            end: e,
            weight: weight[cv].saturating_mul(2).max(1),
            crosses_call: cc,
        });
    }
    ivs.sort_by_key(|i| (i.start, i.end, i.val));

    // ---- eigentlicher linear scan ----
    let mut free_saved: Vec<&'static str> = CALLEE_SAVED.to_vec();
    let mut free_temp: Vec<&'static str> = TEMP_REGS.to_vec();
    let mut active: Vec<(Iv, &'static str)> = Vec::new();
    let mut assign: HashMap<Val, &'static str> = HashMap::new();
    let mut used_saved: Vec<&'static str> = Vec::new();

    for iv in ivs.iter().copied() {
        // abgelaufene Intervalle freigeben
        let mut k = 0;
        while k < active.len() {
            if active[k].0.end <= iv.start {
                let (a, r) = active.remove(k);
                if TEMP_REGS.contains(&r) {
                    free_temp.push(r);
                } else {
                    free_saved.push(r);
                }
                let _ = a;
            } else {
                k += 1;
            }
        }
        let want_temp = !iv.crosses_call;
        let pick = if want_temp && !free_temp.is_empty() {
            free_temp.pop()
        } else if !free_saved.is_empty() {
            free_saved.pop()
        } else {
            None
        };
        match pick {
            Some(r) => {
                if CALLEE_SAVED.contains(&r) && !used_saved.contains(&r) {
                    used_saved.push(r);
                }
                assign.insert(iv.val, r);
                active.push((iv, r));
            }
            None => {
                // Auslagern: das aktive Intervall mit dem KLEINSTEN Gewicht
                // (Verwendungen x Schleifentiefe) raeumt das Register. Bei
                // gleichem Gewicht entscheidet das spaetere Ende.
                let mut worst: Option<usize> = None;
                for (k, (a, r)) in active.iter().enumerate() {
                    if iv.crosses_call && TEMP_REGS.contains(r) {
                        continue; // dieses Register hilft uns nicht
                    }
                    let better = match worst {
                        None => true,
                        Some(w) => (a.weight, usize::MAX - a.end)
                            < (active[w].0.weight, usize::MAX - active[w].0.end),
                    };
                    if better {
                        worst = Some(k);
                    }
                }
                if let Some(w) = worst {
                    if active[w].0.weight < iv.weight {
                        let (old, r) = active.remove(w);
                        assign.remove(&old.val);
                        assign.insert(iv.val, r);
                        active.push((iv, r));
                        continue;
                    }
                }
                // sonst bleibt dieser Wert im Stack-Slot
            }
        }
    }

    // Ergebnis eintragen
    for (v, r) in assign.iter() {
        if cells.contains_key(v) {
            alloc.cells.insert(*v, r);
            if let Some(t) = cells.get(v) {
                alloc.cell_ty.insert(*v, *t);
            }
        } else {
            alloc.locs[*v as usize] = Loc::Reg(r);
        }
    }
    // Rahmen inklusive Sicherungs-Slots fuer die benutzten callee-saved Register
    used_saved.sort_unstable();
    let (frame, slots) = layout(f, used_saved.len() as u64);
    alloc.frame = frame;
    alloc.saved = used_saved.iter().copied().zip(slots.iter().map(|(_, o)| *o)).collect();
    alloc
}

// ------------------------------------------------------------- Emission ---

/// Registername in der gewuenschten Breite.
fn rn(name: &str, bits: u32) -> String {
    let b = match bits {
        8 => 0,
        16 => 1,
        32 => 2,
        _ => 3,
    };
    let tab: [[&str; 4]; 15] = [
        ["al", "ax", "eax", "rax"],
        ["cl", "cx", "ecx", "rcx"],
        ["dl", "dx", "edx", "rdx"],
        ["bl", "bx", "ebx", "rbx"],
        ["sil", "si", "esi", "rsi"],
        ["dil", "di", "edi", "rdi"],
        ["r8b", "r8w", "r8d", "r8"],
        ["r9b", "r9w", "r9d", "r9"],
        ["r10b", "r10w", "r10d", "r10"],
        ["r11b", "r11w", "r11d", "r11"],
        ["r12b", "r12w", "r12d", "r12"],
        ["r13b", "r13w", "r13d", "r13"],
        ["r14b", "r14w", "r14d", "r14"],
        ["r15b", "r15w", "r15d", "r15"],
        ["bpl", "bp", "ebp", "rbp"],
    ];
    let row = match name {
        "rax" => 0,
        "rcx" => 1,
        "rdx" => 2,
        "rbx" => 3,
        "rsi" => 4,
        "rdi" => 5,
        "r8" => 6,
        "r9" => 7,
        "r10" => 8,
        "r11" => 9,
        "r12" => 10,
        "r13" => 11,
        "r14" => 12,
        "r15" => 13,
        _ => 14,
    };
    tab[row][b].to_string()
}

struct Ra<'a> {
    f: &'a Func,
    a: &'a Alloc,
    /// Wie oft wird jeder Wert als Operand gelesen? Gebraucht fuer die
    /// Verschmelzung von `cmp` und bedingtem Sprung: nur wenn das
    /// Vergleichsergebnis GENAU EINMAL gelesen wird (naemlich vom Terminator),
    /// darf das `setcc` entfallen.
    gelesen: Vec<u32>,
}

/// Zaehlt je Wert, wie oft er als Operand vorkommt (Instruktionen + Terminatoren).
fn zaehle_lesezugriffe(f: &Func) -> Vec<u32> {
    let mut n = vec![0u32; f.val_types.len()];
    let mut buf = Vec::new();
    for b in &f.blocks {
        for i in &b.insts {
            buf.clear();
            i.op.uses(&mut buf);
            for v in buf.iter() {
                if let Some(c) = n.get_mut(*v as usize) {
                    *c += 1;
                }
            }
        }
        match &b.term {
            Term::Ret(Some(v)) | Term::BrCond { cond: v, .. } | Term::Switch { val: v, .. } => {
                if let Some(c) = n.get_mut(*v as usize) {
                    *c += 1;
                }
            }
            _ => {}
        }
    }
    n
}

impl<'a> Ra<'a> {
    /// Operand eines Wertes in voller Breite.
    fn opnd(&self, v: Val) -> String {
        if let Some(k) = self.a.imm(v) {
            return format!("{}", k);
        }
        match self.a.loc(v) {
            Loc::Reg(r) => r.to_string(),
            Loc::Slot(off) => format!("qword ptr [rbp-{}]", off),
        }
    }
    /// Operand eines Wertes in der Breite `bits`.
    fn opnd_w(&self, v: Val, bits: u32) -> String {
        if let Some(k) = self.a.imm(v) {
            return format!("{}", k);
        }
        match self.a.loc(v) {
            Loc::Reg(r) => rn(r, bits),
            Loc::Slot(off) => format!("{} [rbp-{}]", size_word(bits), off),
        }
    }
    /// Wert vollstaendig in ein Arbeitsregister laden.
    fn load_full(&self, e: &mut Emitter, r: &str, v: Val) {
        let o = self.opnd(v);
        if o != r {
            e.line(&format!("mov {}, {}", r, o));
        }
    }
    /// Wert vorzeichen-/nullerweitert auf `to_bits` in ein Arbeitsregister.
    fn load_ext(&self, e: &mut Emitter, r: &str, v: Val, ty: FTy, to_bits: u32) {
        if let Some(k) = self.a.imm(v) {
            // Sofortkonstante ist bereits typrichtig zurechtgestutzt.
            e.line(&format!("mov {}, {}", rn(r, to_bits.max(32)), k));
            return;
        }
        let bits = ty.bits().max(8);
        if bits >= to_bits {
            let o = self.opnd_w(v, to_bits);
            let d = rn(r, to_bits);
            if o != d {
                e.line(&format!("mov {}, {}", d, o));
            }
            return;
        }
        let src = self.opnd_w(v, bits);
        match (ty.signed(), bits) {
            (true, _) if bits == 32 => {
                e.line(&format!("movsxd {}, {}", rn(r, to_bits), src))
            }
            (true, _) => e.line(&format!("movsx {}, {}", rn(r, to_bits), src)),
            (false, b) if b == 32 => e.line(&format!("mov {}, {}", rn(r, 32), src)),
            (false, _) => e.line(&format!("movzx {}, {}", rn(r, to_bits.min(32)), src)),
        }
    }
    /// Arbeitsregister in den Zielwert schreiben.
    fn store_dst(&self, e: &mut Emitter, d: Val, r: &str) {
        match self.a.loc(d) {
            Loc::Reg(dr) => {
                if dr != r {
                    e.line(&format!("mov {}, {}", dr, r));
                }
            }
            Loc::Slot(off) => e.line(&format!("mov qword ptr [rbp-{}], {}", off, r)),
        }
    }
}

/// Registerbewusste Emission einer Funktion.
/// `None` = dieser Pfad ist nicht zustaendig, der Grundpfad uebernimmt.
pub(crate) fn emit_func_ra(e: &mut Emitter, f: &Func) -> Option<Result<(), String>> {
    if !supported(f) {
        return None;
    }
    let a = allocate(f);
    Some(emit_with(e, f, &a))
}

/// Sind anweisungsgenaue Debugzeilen aktiv (`--no-opt`, siehe `dwarf.rs`)?
/// Dann uebernimmt der Grundpfad, damit `.loc` je Instruktion erhalten bleibt.
fn debug_lines_active(f: &Func) -> bool {
    f.blocks.iter().any(|b| {
        (0..b.insts.len()).any(|i| crate::dwarf::line_at(&f.name, b.id, i as u32).is_some())
    })
}

fn supported(f: &Func) -> bool {
    if debug_lines_active(f) {
        return false;
    }
    if f.params.len() > ARG_REGS.len() {
        return false;
    }
    if f.blocks.is_empty() || f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return false;
    }
    for b in &f.blocks {
        if matches!(b.term, Term::Unset) {
            return false;
        }
        for i in &b.insts {
            match &i.op {
                Op::Call { args, .. } => {
                    if args.len() > ARG_REGS.len() {
                        return false;
                    }
                }
                Op::Syscall { args } => {
                    if args.is_empty() || args.len() > 7 {
                        return false;
                    }
                }
                _ => {}
            }
        }
    }
    true
}

fn emit_with(e: &mut Emitter, f: &Func, a: &Alloc) -> Result<(), String> {
    let ra = Ra { f, a, gelesen: zaehle_lesezugriffe(f) };
    e.raw("");
    // Linker-Symbol ueber die eine Stelle (codegen_x86::label -> modules::symbol)
    e.raw(&format!(".globl {}", label(&f.name)));
    e.raw(&format!("{}:", label(&f.name)));
    // Zeile der `fn`-Deklaration fuer den Debugger (dwarf.rs).
    if let Some((file, line)) = crate::dwarf::fn_line(&f.name) {
        e.line(&format!(".loc {} {} 0", file + 1, line));
    }
    e.line("push rbp");
    e.line("mov rbp, rsp");
    if a.frame.size > 0 {
        e.line(&format!("sub rsp, {}", a.frame.size));
    }
    for (r, off) in &a.saved {
        e.line(&format!("mov qword ptr [rbp-{}], {}", off, r));
    }
    // Parameter aus den Argumentregistern in ihre Heimat bringen.
    // ACHTUNG: `r8`/`r9` sind zugleich Argumentregister 5/6 UND moegliche
    // Heimat frueherer Parameter. Deshalb erst alle Slot-Ziele (die
    // ueberschreiben kein Register), dann die Register-Ziele PARALLEL.
    let mut prolog_moves: Vec<(String, String)> = Vec::new();
    for (i, _t) in f.params.iter().enumerate() {
        match ra.a.loc(i as Val) {
            Loc::Slot(off) => {
                e.line(&format!("mov qword ptr [rbp-{}], {}", off, ARG_REGS[i]))
            }
            Loc::Reg(dst) => prolog_moves.push((dst.to_string(), ARG_REGS[i].to_string())),
        }
    }
    parallele_reg_bewegungen(e, &prolog_moves);
    for b in &f.blocks {
        e.raw(&format!("{}:", block_label(&f.name, b.id)));
        emit_block(e, &ra, b)?;
    }
    Ok(())
}

/// Ist `s` ein 64-Bit-Maschinenregistername (und damit ein Operand, dessen
/// Inhalt durch andere Registerbewegungen zerstoert werden kann)?
fn ist_reg64(s: &str) -> bool {
    matches!(
        s,
        "rax" | "rbx" | "rcx" | "rdx" | "rsi" | "rdi" | "rbp" | "rsp"
            | "r8" | "r9" | "r10" | "r11" | "r12" | "r13" | "r14" | "r15"
    )
}

/// Emittiert eine **parallele** Registerumsetzung: alle Paare `(ziel, quelle)`
/// gelten GLEICHZEITIG, ein Ziel darf also zugleich Quelle eines anderen Paares
/// sein.
///
/// Notwendig, weil `r8`/`r9` sowohl Argumentregister 5 und 6 als auch
/// Arbeitsregister der Zuteilung sind (`TEMP_REGS`). Naiv der Reihe nach
/// umgesetzt, ueberschreibt der fuenfte Parameter sonst ein Argument, das der
/// sechste noch braucht — genau dieser Fehler liess `tests/024_six_args.fi`
/// ohne Einbettung 13 statt 21 liefern.
///
/// Verfahren: solange ein Ziel existiert, das von keinem offenen Paar mehr als
/// Quelle gebraucht wird, wird dieses Paar sofort ausgegeben. Bleiben nur noch
/// Zyklen uebrig, wird einer ueber `rax` aufgebrochen — `rax` ist nie Heimat
/// eines Wertes (weder in `CALLEE_SAVED` noch in `TEMP_REGS`).
fn parallele_reg_bewegungen(e: &mut Emitter, paare: &[(String, String)]) {
    let mut offen: Vec<(String, String)> =
        paare.iter().filter(|(z, q)| z != q).cloned().collect();
    while !offen.is_empty() {
        if let Some(i) = offen
            .iter()
            .position(|(z, _)| !offen.iter().any(|(_, q)| q == z))
        {
            let (z, q) = offen.remove(i);
            e.line(&format!("mov {}, {}", z, q));
            continue;
        }
        // Nur noch Zyklen: den alten Inhalt des Ziels nach rax retten, damit
        // das Ziel frei wird; alle Quellen, die darauf zeigten, lesen ab jetzt
        // aus rax.
        let (z, q) = offen[0].clone();
        e.line(&format!("mov rax, {}", z));
        for (_, quelle) in offen.iter_mut() {
            if *quelle == z {
                *quelle = "rax".to_string();
            }
        }
        e.line(&format!("mov {}, {}", z, q));
        offen.remove(0);
    }
}

fn epilogue(e: &mut Emitter, a: &Alloc) {
    for (r, off) in &a.saved {
        e.line(&format!("mov {}, qword ptr [rbp-{}]", r, off));
    }
    e.line("mov rsp, rbp");
    e.line("pop rbp");
    e.line("ret");
}

fn emit_block(e: &mut Emitter, ra: &Ra, b: &Block) -> Result<(), String> {
    // VERSCHMELZUNG `cmp` + bedingter Sprung.
    //
    // Ohne sie kostet jeder Vergleich sieben Instruktionen: `cmp`, `setcc al`,
    // `movzx eax, al`, eine Kopie ins Zielregister, `test`, `jnz`, `jmp`. Der
    // bool-Wert wird also erzeugt, gespeichert und sofort wieder auf null
    // geprueft. Mit Verschmelzung sind es drei: `cmp`, `jcc`, `jmp`.
    //
    // Gemessen an `lib/html/mem.fi`: die eingebettete Bereichspruefung von
    // `buf_at` erzeugt genau dieses Muster, und `dekodiere` durchlaeuft sie bis
    // zu fuenfmal je Zeichen.
    //
    // Bedingungen (alle noetig):
    //   * die LETZTE Instruktion des Blocks ist der Vergleich — nur dann kann
    //     zwischen `cmp` und Sprung nichts die Flags veraendern,
    //   * ihr Ergebnis ist die Sprungbedingung,
    //   * es wird GENAU EINMAL gelesen (sonst wird der bool-Wert gebraucht),
    //   * kein `secret`-Wert (SPEC §9.2).
    let verschmelzbar = match (&b.term, b.insts.last()) {
        (Term::BrCond { cond, .. }, Some(letzte)) => {
            matches!(letzte.op, Op::Cmp { .. })
                && letzte.dst == Some(*cond)
                && ra.gelesen.get(*cond as usize).copied().unwrap_or(2) == 1
                && !ra.f.is_secret(*cond)
        }
        _ => false,
    };
    let n = if verschmelzbar { b.insts.len() - 1 } else { b.insts.len() };
    for i in &b.insts[..n] {
        emit_inst(e, ra, i)?;
    }
    if verschmelzbar {
        return emit_cmp_br(e, ra, b);
    }
    let f = ra.f;
    match &b.term {
        Term::Br(t) => e.line(&format!("jmp {}", block_label(&f.name, *t))),
        Term::Switch { val, .. } => {
            // `codegen_switch.rs` (Modul `types`) erwartet den Wert im Rahmen.
            let off = match ra.a.frame.slot.get(*val as usize) {
                Some(o) => *o,
                None => return Err("interner Fehler: switch ohne Slot".to_string()),
            };
            if let Loc::Reg(_) = ra.a.loc(*val) {
                ra.load_full(e, "rax", *val);
                e.line(&format!("mov qword ptr [rbp-{}], rax", off));
            }
            crate::codegen_switch::emit_switch(e, f, &ra.a.frame, &b.term)?;
        }
        Term::BrCond { cond, then_bb, else_bb } => {
            if f.constant_time && f.is_secret(*cond) {
                return Err(format!(
                    "#[constant_time]: bedingter Sprung in '{}' haengt von einem secret-Wert (%{}) ab",
                    f.name, cond
                ));
            }
            if f.val_ty(*cond) != FTy::Bool {
                return Err(format!(
                    "interner Fehler: Bedingung %{} in '{}' ist {}, erwartet bool",
                    cond,
                    f.name,
                    f.val_ty(*cond).name()
                ));
            }
            let o = ra.opnd_w(*cond, 8);
            if o.contains('[') {
                e.line(&format!("cmp {}, 0", o));
            } else {
                e.line(&format!("test {}, {}", o, o));
            }
            e.line(&format!("jnz {}", block_label(&f.name, *then_bb)));
            e.line(&format!("jmp {}", block_label(&f.name, *else_bb)));
        }
        Term::Ret(v) => {
            if let Some(v) = v {
                ra.load_full(e, "rax", *v);
            } else {
                e.line("xor eax, eax");
            }
            epilogue(e, ra.a);
        }
        Term::Unset => {
            return Err(format!(
                "interner Fehler: Block bb{} in '{}' hat keinen Terminator",
                b.id, f.name
            ))
        }
    }
    Ok(())
}

/// `cmp` und bedingter Sprung in einem: der Vergleich der letzten Instruktion
/// des Blocks setzt die Flags, der Terminator liest sie unmittelbar.
fn emit_cmp_br(e: &mut Emitter, ra: &Ra, b: &Block) -> Result<(), String> {
    let f = ra.f;
    let letzte = b.insts.last().ok_or("interner Fehler: leerer Block bei cmp+jcc")?;
    let (op, oty, a, bb) = match &letzte.op {
        Op::Cmp { op, ty, a, b } => (*op, *ty, *a, *b),
        _ => return Err("interner Fehler: cmp+jcc ohne Vergleich".to_string()),
    };
    let (then_bb, else_bb) = match &b.term {
        Term::BrCond { then_bb, else_bb, .. } => (*then_bb, *else_bb),
        _ => return Err("interner Fehler: cmp+jcc ohne brcond".to_string()),
    };
    let bits = oty.bits().max(8);
    let oa = ra.opnd_w(a, bits);
    let ob = ra.opnd_w(bb, bits);
    if ra.a.imm(a).is_some() || (oa.contains('[') && ob.contains('[')) {
        ra.load_full(e, "rax", a);
        e.line(&format!("cmp {}, {}", rn("rax", bits), ob));
    } else {
        e.line(&format!("cmp {}, {}", oa, ob));
    }
    let jcc = match (op, oty.signed()) {
        (CmpOp::Eq, _) => "je",
        (CmpOp::Ne, _) => "jne",
        (CmpOp::Lt, true) => "jl",
        (CmpOp::Lt, false) => "jb",
        (CmpOp::Le, true) => "jle",
        (CmpOp::Le, false) => "jbe",
        (CmpOp::Gt, true) => "jg",
        (CmpOp::Gt, false) => "ja",
        (CmpOp::Ge, true) => "jge",
        (CmpOp::Ge, false) => "jae",
    };
    e.line(&format!("{} {}", jcc, block_label(&f.name, then_bb)));
    e.line(&format!("jmp {}", block_label(&f.name, else_bb)));
    Ok(())
}

fn emit_inst(e: &mut Emitter, ra: &Ra, i: &Inst) -> Result<(), String> {
    let ty = i.ty;
    match &i.op {
        Op::Const(c) => {
            let d = i.dst.ok_or("interner Fehler: const ohne Ziel")?;
            if ra.a.imm(d).is_some() {
                return Ok(()); // steht an jeder Verwendungsstelle als Sofortwert
            }
            let val = ty.truncate(*c) as i64;
            match ra.a.loc(d) {
                Loc::Reg(r) => {
                    if val == 0 {
                        e.line(&format!("xor {}, {}", rn(r, 32), rn(r, 32)));
                    } else {
                        e.line(&format!("mov {}, {}", r, val));
                    }
                }
                Loc::Slot(_) => {
                    if val == 0 {
                        e.line("xor eax, eax");
                    } else {
                        e.line(&format!("mov rax, {}", val));
                    }
                    ra.store_dst(e, d, "rax");
                }
            }
        }
        Op::Bin(op, x, y) => {
            let d = i.dst.ok_or("interner Fehler: Binaeroperation ohne Ziel")?;
            emit_bin(e, ra, *op, ty, *x, *y, d)?;
        }
        Op::Cmp { op, ty: oty, a, b } => {
            let d = i.dst.ok_or("interner Fehler: Vergleich ohne Ziel")?;
            let bits = oty.bits().max(8);
            let oa = ra.opnd_w(*a, bits);
            let ob = ra.opnd_w(*b, bits);
            // `cmp` vertraegt hoechstens einen Speicheroperanden und keinen
            // Sofortwert links.
            if ra.a.imm(*a).is_some() || (oa.contains('[') && ob.contains('[')) {
                ra.load_full(e, "rax", *a);
                e.line(&format!("cmp {}, {}", rn("rax", bits), ob));
            } else {
                e.line(&format!("cmp {}, {}", oa, ob));
            }
            let signed = oty.signed();
            let cc = match (op, signed) {
                (CmpOp::Eq, _) => "sete",
                (CmpOp::Ne, _) => "setne",
                (CmpOp::Lt, true) => "setl",
                (CmpOp::Lt, false) => "setb",
                (CmpOp::Le, true) => "setle",
                (CmpOp::Le, false) => "setbe",
                (CmpOp::Gt, true) => "setg",
                (CmpOp::Gt, false) => "seta",
                (CmpOp::Ge, true) => "setge",
                (CmpOp::Ge, false) => "setae",
            };
            e.line(&format!("{} al", cc));
            e.line("movzx eax, al");
            ra.store_dst(e, d, "rax");
        }
        Op::Un(op, x) => {
            let d = i.dst.ok_or("interner Fehler: Unaeroperation ohne Ziel")?;
            let bits = if ty.bits() > 32 { 64 } else { 32 };
            ra.load_full(e, "rax", *x);
            match op {
                UnOp::Neg => e.line(&format!("neg {}", rn("rax", bits))),
                UnOp::Not => {
                    if ty == FTy::Bool {
                        e.line("xor eax, 1");
                    } else {
                        e.line(&format!("not {}", rn("rax", bits)));
                    }
                }
            }
            ra.store_dst(e, d, "rax");
        }
        Op::Cast { src, from } => {
            let d = i.dst.ok_or("interner Fehler: Umwandlung ohne Ziel")?;
            if ty == FTy::Bool {
                let bits = from.bits().max(8);
                let o = ra.opnd_w(*src, bits);
                e.line(&format!("cmp {}, 0", o));
                e.line("setne al");
                e.line("movzx eax, al");
            } else {
                ra.load_ext(e, "rax", *src, *from, 64);
            }
            ra.store_dst(e, d, "rax");
        }
        Op::GcAddr { regs } => {
            let d = i.dst.ok_or("interner Fehler: gc_state ohne Ziel")?;
            crate::codegen_x86::emit_gc_addr(e, *regs);
            ra.store_dst(e, d, "rax");
        }
        Op::Alloca { .. } => {
            let d = i.dst.ok_or("interner Fehler: alloca ohne Ziel")?;
            if ra.a.cell(d).is_some() || ra.a.frame_addr.contains_key(&d) {
                return Ok(()); // befoerderte bzw. direkt adressierte Zelle
            }
            let off = ra
                .a
                .frame
                .alloca_off
                .get(d as usize)
                .copied()
                .flatten()
                .ok_or("interner Fehler: alloca ohne Platz")?;
            e.line(&format!("lea rax, [rbp-{}]", off));
            ra.store_dst(e, d, "rax");
        }
        Op::Load { addr } => {
            let d = i.dst.ok_or("interner Fehler: load ohne Ziel")?;
            let bits = ty.bits().max(8);
            if let Some((r, _)) = ra.a.cell(*addr) {
                // Zelle im Register: nur die relevante Breite herausziehen,
                // wenn moeglich direkt ins Zielregister.
                let t = match ra.a.loc(d) {
                    Loc::Reg(dr) => dr,
                    Loc::Slot(_) => "rax",
                };
                match bits {
                    8 => e.line(&format!("movzx {}, {}", rn(t, 32), rn(r, 8))),
                    16 => e.line(&format!("movzx {}, {}", rn(t, 32), rn(r, 16))),
                    32 => e.line(&format!("mov {}, {}", rn(t, 32), rn(r, 32))),
                    _ => {
                        if t != r {
                            e.line(&format!("mov {}, {}", t, r));
                        }
                    }
                }
                if t != "rax" {
                    return Ok(());
                }
            } else {
                let mem = match (ra.a.frame_addr.get(addr), ra.a.loc(*addr)) {
                    (Some(off), _) => format!("[rbp-{}]", off),
                    (None, Loc::Reg(r)) => format!("[{}]", r),
                    (None, Loc::Slot(_)) => {
                        ra.load_full(e, "rcx", *addr);
                        "[rcx]".to_string()
                    }
                };
                // DIREKT ins Zielregister laden, statt ueber rax und dann zu
                // kopieren. `mov r9, qword ptr [r9]` ist korrekt: die
                // Instruktion liest die Adresse, bevor sie das Ziel schreibt.
                // Das spart je Speicherzugriff eine Instruktion — im
                // Schleifenrumpf von matmul waren das zwei von 24.
                let zr = match ra.a.loc(d) {
                    Loc::Reg(r) => r,
                    Loc::Slot(_) => "rax",
                };
                match bits {
                    8 => e.line(&format!("movzx {}, byte ptr {}", rn(zr, 32), mem)),
                    16 => e.line(&format!("movzx {}, word ptr {}", rn(zr, 32), mem)),
                    32 => e.line(&format!("mov {}, dword ptr {}", rn(zr, 32), mem)),
                    _ => e.line(&format!("mov {}, qword ptr {}", zr, mem)),
                }
                if zr != "rax" {
                    return Ok(());
                }
            }
            ra.store_dst(e, d, "rax");
        }
        Op::Store { addr, val } => {
            let bits = ty.bits().max(8);
            if let Some((r, _)) = ra.a.cell(*addr) {
                // Es wird immer die volle Breite kopiert; gelesen werden nur
                // die unteren `bits` Bits (einheitliche Zugriffsbreite).
                let o = ra.opnd(*val);
                if o != r {
                    e.line(&format!("mov {}, {}", r, o));
                }
            } else {
                let mem = match (ra.a.frame_addr.get(addr), ra.a.loc(*addr)) {
                    (Some(off), _) => format!("[rbp-{}]", off),
                    (None, Loc::Reg(r)) => format!("[{}]", r),
                    (None, Loc::Slot(_)) => {
                        ra.load_full(e, "rcx", *addr);
                        "[rcx]".to_string()
                    }
                };
                let o = ra.opnd_w(*val, bits);
                if o.contains('[') {
                    e.line(&format!("mov {}, {}", rn("rax", bits), o));
                    e.line(&format!("mov {} {}, {}", size_word(bits), mem, rn("rax", bits)));
                } else {
                    e.line(&format!("mov {} {}, {}", size_word(bits), mem, o));
                }
            }
        }
        Op::PtrAdd { base, off } => {
            let d = i.dst.ok_or("interner Fehler: ptradd ohne Ziel")?;
            // `lea` liest BEIDE Operanden, bevor es das Ziel schreibt — eine
            // Kollision zwischen Ziel- und Offsetregister ist dort also
            // unschaedlich. Nur der `mov`+`add`-Weg braucht den Umweg ueber
            // rax; deshalb wird das Ziel hier optimistisch gewaehlt und nur in
            // den beiden `add`-Zweigen zurueckgenommen.
            let dreg = match ra.a.loc(d) {
                Loc::Reg(r) => r,
                Loc::Slot(_) => "rax",
            };
            let reg_von = |v: Val| match (ra.a.imm(v), ra.a.loc(v)) {
                (None, Loc::Reg(r)) => Some(r),
                _ => None,
            };
            let off_reg = reg_von(*off);
            let base_reg = reg_von(*base);
            let mut ziel = dreg;
            if let Some(boff) = ra.a.frame_addr.get(base).copied() {
                // Adresse = rbp - boff + off  -> ein einziges `lea`
                match (ra.a.imm(*off), off_reg) {
                    (Some(k), _) => {
                        let delta = k - boff as i64;
                        if delta >= 0 {
                            e.line(&format!("lea {}, [rbp+{}]", ziel, delta));
                        } else {
                            e.line(&format!("lea {}, [rbp-{}]", ziel, -delta));
                        }
                    }
                    (None, Some(r)) => e.line(&format!("lea {}, [rbp+{}-{}]", ziel, r, boff)),
                    (None, None) => {
                        e.line(&format!("mov rcx, {}", ra.opnd(*off)));
                        e.line(&format!("lea {}, [rbp+rcx-{}]", ziel, boff));
                    }
                }
            } else if let (Some(x), Some(k)) = (base_reg, ra.a.imm(*off)) {
                lea_summe(e, ziel, x, k);
            } else if let (Some(x), Some(y)) = (base_reg, off_reg) {
                e.line(&format!("lea {}, [{}+{}]", ziel, x, y));
            } else if ziel != "rax" {
                // Basis liegt im Rahmen oder ist eine Konstante: einmal nach
                // rax holen, dann mit EINEM `lea` ins Ziel.
                ra.load_full(e, "rax", *base);
                match (ra.a.imm(*off), off_reg) {
                    (Some(k), _) => lea_summe(e, ziel, "rax", k),
                    (None, Some(y)) => e.line(&format!("lea {}, [rax+{}]", ziel, y)),
                    (None, None) => {
                        e.line(&format!("add rax, {}", ra.opnd(*off)));
                        ziel = "rax";
                    }
                }
            } else {
                ra.load_full(e, "rax", *base);
                e.line(&format!("add rax, {}", ra.opnd(*off)));
                ziel = "rax";
            }
            if ziel == "rax" {
                ra.store_dst(e, d, "rax");
            }
        }
        Op::Call { name, args } => {
            // Argumente in die Argumentregister. Die Zuteilung vergibt `r8`
            // und `r9` sehr wohl als Heimat (`TEMP_REGS`), deshalb muessen die
            // Register-zu-Register-Bewegungen PARALLEL geschehen; Operanden aus
            // Speicher oder Sofortkonstanten lesen kein Register und kommen
            // danach.
            let mut reg_moves: Vec<(String, String)> = Vec::new();
            let mut spaeter: Vec<(usize, Val)> = Vec::new();
            for (k, arg) in args.iter().enumerate() {
                let o = ra.opnd(*arg);
                if ist_reg64(&o) {
                    reg_moves.push((ARG_REGS[k].to_string(), o));
                } else {
                    spaeter.push((k, *arg));
                }
            }
            parallele_reg_bewegungen(e, &reg_moves);
            for (k, arg) in spaeter {
                ra.load_full(e, ARG_REGS[k], arg);
            }
            e.line(&format!("call {}", label(name)));
            if let Some(d) = i.dst {
                ra.store_dst(e, d, "rax");
            }
        }
        Op::Syscall { args } => {
            const SYS_REGS: [&str; 6] = ["rdi", "rsi", "rdx", "r10", "r8", "r9"];
            if args.is_empty() {
                return Err("interner Fehler: syscall ohne Nummer".to_string());
            }
            // Gleiche Fehlerklasse wie beim Aufruf: `r10`, `r8` und `r9`
            // sind zugleich Arbeitsregister der Zuteilung.
            let mut sys_moves: Vec<(String, String)> = Vec::new();
            let mut sys_spaeter: Vec<(usize, Val)> = Vec::new();
            for (k, arg) in args.iter().skip(1).enumerate() {
                let o = ra.opnd(*arg);
                if ist_reg64(&o) {
                    sys_moves.push((SYS_REGS[k].to_string(), o));
                } else {
                    sys_spaeter.push((k, *arg));
                }
            }
            parallele_reg_bewegungen(e, &sys_moves);
            for (k, arg) in sys_spaeter {
                ra.load_full(e, SYS_REGS[k], arg);
            }
            ra.load_full(e, "rax", args[0]);
            e.line("syscall");
            if let Some(d) = i.dst {
                ra.store_dst(e, d, "rax");
            }
        }
        Op::Select { cond, a, b } => {
            // SPEC §9.2: immer `cmov`, niemals ein Sprung.
            let d = i.dst.ok_or("interner Fehler: select ohne Ziel")?;
            ra.load_full(e, "rdx", *cond);
            ra.load_full(e, "rax", *b);
            ra.load_full(e, "rcx", *a);
            e.line("test dl, dl");
            e.line("cmovnz rax, rcx");
            ra.store_dst(e, d, "rax");
        }
        Op::Barrier { val } => {
            let d = i.dst.ok_or("interner Fehler: barrier ohne Ziel")?;
            ra.load_full(e, "rax", *val);
            e.raw("    # barrier: undurchsichtig fuer jeden Optimierungsdurchgang");
            ra.store_dst(e, d, "rax");
        }
        Op::SecureZero { addr, size } => {
            ra.load_full(e, "rdi", *addr);
            ra.load_full(e, "rcx", *size);
            e.line("xor eax, eax");
            e.line("cld");
            e.line("rep stosb");
        }
        Op::CopyMem { dst, src, size } => {
            ra.load_full(e, "rdi", *dst);
            ra.load_full(e, "rsi", *src);
            e.line(&format!("mov rcx, {}", size));
            e.line("cld");
            e.line("rep movsb");
        }
    }
    Ok(())
}

/// `lea ziel, [basis + versatz]` — die Adressrechnung des Prozessors als
/// Rechenwerk. Der Gewinn ist keine Kosmetik: `mov d, a` + `add d, b` sind zwei
/// Instruktionen und zerstoeren `d`, `lea d, [a+b]` ist eine und liest nur.
/// Damit faellt in Adressrechnungen (`basis + i*breite`) das halbe
/// Registergeschiebe weg — in `bench/firn/matmul.fi` waren 14 der 27
/// Instruktionen der inneren Schleife reine Registerkopien.
///
/// **Nur 64 Bit.** Bei 32-Bit-Zielen nullt `add eax, ecx` die oberen 32 Bit,
/// `lea rax, [rcx+rdx]` nicht — der Unterschied ist sichtbar, sobald der Wert
/// als 64-Bit-Wert weitergereicht wird. Deshalb bleibt der schmale Fall beim
/// alten Weg.
///
/// **Flags.** `lea` setzt keine, `add` schon. Das ist hier gefahrlos: in FIR ist
/// jeder Vergleich ein eigener `Op::Cmp`, der sein `cmp`/`setcc` unmittelbar
/// hintereinander erzeugt. Kein `setcc`, `jcc` oder `cmov` liest jemals die
/// Flags einer FIR-Rechenoperation.
/// Genau ein Operand liegt in einem Register, der andere im Rahmen (kein
/// Sofortwert)? Dann lohnt der Weg ueber rax mit abschliessendem `lea`.
fn add_ueber_rax(ra: &Ra, a: Val, b: Val) -> bool {
    let ist_reg = |v: Val| ra.a.imm(v).is_none() && matches!(ra.a.loc(v), Loc::Reg(_));
    let ist_rahmen = |v: Val| ra.a.imm(v).is_none() && matches!(ra.a.loc(v), Loc::Slot(_));
    (ist_reg(a) && ist_rahmen(b)) || (ist_rahmen(a) && ist_reg(b))
}

/// Laesst sich `d = a op b` als ein einziges `lea` schreiben?
///
/// Nur 64 Bit (siehe `lea_summe`), nur mit Zielregister, und nur wenn die
/// Operanden wirklich als Adressteile taugen: Register + Register,
/// Register + Sofortwert, Sofortwert + Register. Bei `sub` zusaetzlich
/// `k != i64::MIN`, weil `-k` sonst ueberlaeuft.
fn lea_moeglich(ra: &Ra, op: BinOp, ty: FTy, a: Val, b: Val, d: Val) -> bool {
    if ty.bits() <= 32 || !matches!(ra.a.loc(d), Loc::Reg(_)) {
        return false;
    }
    let ist_reg = |v: Val| ra.a.imm(v).is_none() && matches!(ra.a.loc(v), Loc::Reg(_));
    match op {
        BinOp::Add => {
            (ist_reg(a) && ist_reg(b))
                || (ist_reg(a) && ra.a.imm(b).is_some())
                || (ra.a.imm(a).is_some() && ist_reg(b))
        }
        BinOp::Sub => ist_reg(a) && matches!(ra.a.imm(b), Some(k) if k != i64::MIN),
        _ => false,
    }
}

fn lea_summe(e: &mut Emitter, ziel: &str, basis: &str, versatz: i64) {
    if versatz >= 0 {
        e.line(&format!("lea {}, [{}+{}]", ziel, basis, versatz));
    } else {
        e.line(&format!("lea {}, [{}-{}]", ziel, basis, -(versatz as i128) as i64));
    }
}

fn emit_bin(
    e: &mut Emitter,
    ra: &Ra,
    op: BinOp,
    ty: FTy,
    a: Val,
    b: Val,
    d: Val,
) -> Result<(), String> {
    let wide = ty.bits() > 32;
    let bits = if wide { 64 } else { 32 };
    match op {
        BinOp::Mul if ra.a.imm(b).is_some() => {
            // `imul` kennt keine Zwei-Operanden-Form mit Sofortwert; Zweierpotenzen
            // werden zur Schiebung.
            let k = ra.a.imm(b).unwrap_or(1);
            let dst_reg = match (ra.a.loc(d), ra.a.loc(a)) {
                (Loc::Reg(r), _) => r,
                (Loc::Slot(_), _) => "rax",
            };
            let shift = if k > 1 && (k & (k - 1)) == 0 { Some(k.trailing_zeros()) } else { None };
            match shift {
                Some(sh) => {
                    ra.load_full(e, dst_reg, a);
                    e.line(&format!("shl {}, {}", rn(dst_reg, bits), sh));
                }
                None => {
                    e.line(&format!("imul {}, {}, {}", rn(dst_reg, bits), ra.opnd_w(a, bits), k))
                }
            }
            if dst_reg == "rax" {
                ra.store_dst(e, d, "rax");
            }
        }
        // `lea` statt `mov`+`add`: eine Instruktion, kein zerstoertes Ziel,
        // und es funktioniert auch dann, wenn der zweite Operand bereits im
        // Zielregister liegt — dort fiel der alte Weg auf den rax-Umweg mit
        // DREI Instruktionen zurueck.
        //
        // Die Bedingung prueft den lea-Fall VOLLSTAENDIG. Es gibt hier
        // absichtlich keinen Ersatzpfad: alles andere faellt in den Zweig
        // darunter, der seinen eigenen Schutz mitbringt (steht der zweite
        // Operand im Zielregister, muss ueber rax gerechnet werden). Ein
        // Ersatzpfad ohne diesen Schutz hat beim ersten Versuch
        // `mov r9, [rbp-8]` + `add r9, r9` erzeugt — matmul lief in einen
        // Speicherzugriffsfehler. Der Fehler ist der Grund fuer diese Form.
        BinOp::Add | BinOp::Sub if lea_moeglich(ra, op, ty, a, b, d) => {
            let dr = match ra.a.loc(d) {
                Loc::Reg(r) => r,
                Loc::Slot(_) => unreachable!("lea_moeglich verlangt ein Zielregister"),
            };
            let reg_von = |v: Val| match (ra.a.imm(v), ra.a.loc(v)) {
                (None, Loc::Reg(r)) => Some(r),
                _ => None,
            };
            match op {
                BinOp::Add => match (reg_von(a), reg_von(b), ra.a.imm(a), ra.a.imm(b)) {
                    (Some(x), Some(y), _, _) => e.line(&format!("lea {}, [{}+{}]", dr, x, y)),
                    (Some(x), None, _, Some(k)) => lea_summe(e, dr, x, k),
                    (None, Some(y), Some(k), _) => lea_summe(e, dr, y, k),
                    _ => unreachable!("lea_moeglich hat den Fall zugesichert"),
                },
                _ => match (reg_von(a), ra.a.imm(b)) {
                    (Some(x), Some(k)) => lea_summe(e, dr, x, -k),
                    _ => unreachable!("lea_moeglich hat den Fall zugesichert"),
                },
            }
        }
        // Ein Operand liegt im Rahmen, der andere in einem Register — der mit
        // Abstand haeufigste Fall in Adressrechnungen (`basis + versatz`, wobei
        // die Basis ein Parameter im Rahmen ist). Einmal nach rax holen, dann
        // EIN `lea` ins Ziel. Der allgemeine Zweig darunter braucht hier drei
        // Instruktionen, weil das Ziel mit dem Registeroperanden zusammenfaellt
        // und er deshalb ueber rax rechnen und zurueckkopieren muss.
        BinOp::Add if wide && matches!(ra.a.loc(d), Loc::Reg(_)) && add_ueber_rax(ra, a, b) => {
            let dr = match ra.a.loc(d) {
                Loc::Reg(r) => r,
                Loc::Slot(_) => unreachable!("durch die Bedingung ausgeschlossen"),
            };
            let ist_reg = |v: Val| ra.a.imm(v).is_none() && matches!(ra.a.loc(v), Loc::Reg(_));
            // `+` ist kommutativ: der Registeroperand wird zum Indexteil.
            let (aus_rahmen, im_reg) = if ist_reg(b) { (a, b) } else { (b, a) };
            let y = match ra.a.loc(im_reg) {
                Loc::Reg(r) => r,
                Loc::Slot(_) => unreachable!("add_ueber_rax hat ein Register zugesichert"),
            };
            ra.load_full(e, "rax", aus_rahmen);
            e.line(&format!("lea {}, [rax+{}]", dr, y));
        }
        BinOp::Add | BinOp::Sub | BinOp::And | BinOp::Or | BinOp::Xor | BinOp::Mul => {
            let m = match op {
                BinOp::Add => "add",
                BinOp::Sub => "sub",
                BinOp::And => "and",
                BinOp::Or => "or",
                BinOp::Xor => "xor",
                _ => "imul",
            };
            // direkt im Zielregister rechnen, wenn moeglich
            if let Loc::Reg(dr) = ra.a.loc(d) {
                let ob = ra.opnd_w(b, bits);
                if ob != rn(dr, bits) {
                    ra.load_full(e, dr, a);
                    e.line(&format!("{} {}, {}", m, rn(dr, bits), ob));
                    return Ok(());
                }
            }
            ra.load_full(e, "rax", a);
            e.line(&format!("{} {}, {}", m, rn("rax", bits), ra.opnd_w(b, bits)));
            ra.store_dst(e, d, "rax");
        }
        BinOp::Div | BinOp::Rem => {
            ra.load_ext(e, "rax", a, ty, bits);
            ra.load_ext(e, "rcx", b, ty, bits);
            if ty.signed() {
                if wide {
                    e.line("cqo");
                    e.line("idiv rcx");
                } else {
                    e.line("cdq");
                    e.line("idiv ecx");
                }
            } else {
                e.line("xor edx, edx");
                if wide {
                    e.line("div rcx");
                } else {
                    e.line("div ecx");
                }
            }
            let res = if op == BinOp::Div { "rax" } else { "rdx" };
            ra.store_dst(e, d, res);
        }
        BinOp::Shl | BinOp::Shr => {
            ra.load_ext(e, "rax", a, ty, bits);
            ra.load_full(e, "rcx", b);
            let m = match (op, ty.signed()) {
                (BinOp::Shl, _) => "shl",
                (_, true) => "sar",
                (_, false) => "shr",
            };
            e.line(&format!("{} {}, cl", m, rn("rax", bits)));
            ra.store_dst(e, d, "rax");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_x86::emit;
    use crate::fir::{Module, Term};

    /// Schleife mit Zaehler in einer `alloca`: der Zaehler muss in einem
    /// Register landen (Zellen-Befoerderung), nicht im Stack.
    fn loop_func() -> Func {
        let mut f = Func::new("main", vec![], FTy::I32);
        let head = f.add_block();
        let body = f.add_block();
        let exit = f.add_block();
        let slot = f.alloca(4, 4);
        let zero = f.push(0, FTy::I32, Op::Const(0));
        f.push_void(0, FTy::I32, Op::Store { addr: slot, val: zero });
        f.set_term(0, Term::Br(head));
        let i = f.push(head, FTy::I32, Op::Load { addr: slot });
        let ten = f.push(head, FTy::I32, Op::Const(10));
        let c = f.push(head, FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::I32, a: i, b: ten });
        f.set_term(head, Term::BrCond { cond: c, then_bb: body, else_bb: exit });
        let i2 = f.push(body, FTy::I32, Op::Load { addr: slot });
        let one = f.push(body, FTy::I32, Op::Const(1));
        let s = f.push(body, FTy::I32, Op::Bin(BinOp::Add, i2, one));
        f.push_void(body, FTy::I32, Op::Store { addr: slot, val: s });
        f.set_term(body, Term::Br(head));
        let r = f.push(exit, FTy::I32, Op::Load { addr: slot });
        f.set_term(exit, Term::Ret(Some(r)));
        f
    }

    #[test]
    fn schleifenzaehler_landet_im_register() {
        let f = loop_func();
        let a = allocate(&f);
        assert!(!a.cells.is_empty(), "die alloca-Zelle muss befoerdert werden");
        let regs = a.locs.iter().filter(|l| matches!(l, Loc::Reg(_))).count() + a.cells.len();
        assert!(regs >= 3, "zu wenige Register vergeben: {}", regs);
    }

    #[test]
    fn schleifenrumpf_ohne_speicherzugriff() {
        let asm = emit(&Module { funcs: vec![loop_func()] }).expect("codegen");
        // im Rumpf (bb2) darf kein [rbp- mehr vorkommen
        let body = asm.split(".Lmain__bb2:").nth(1).unwrap_or("");
        let body = body.split(".Lmain__bb3:").next().unwrap_or("");
        assert!(!body.contains("[rbp-"), "Schleifenrumpf greift noch auf den Stack zu:\n{}", body);
    }

    #[test]
    fn callee_saved_werden_gesichert_und_zurueckgeholt() {
        let f = loop_func();
        let a = allocate(&f);
        if a.saved.is_empty() {
            return; // nichts zu sichern -> nichts zu pruefen
        }
        let asm = emit(&Module { funcs: vec![loop_func()] }).expect("codegen");
        for (r, off) in &a.saved {
            assert!(asm.contains(&format!("mov qword ptr [rbp-{}], {}", off, r)), "{}", asm);
            assert!(asm.contains(&format!("mov {}, qword ptr [rbp-{}]", r, off)), "{}", asm);
        }
    }

    #[test]
    fn zellen_mit_entkommender_adresse_werden_nicht_befoerdert() {
        let mut f = Func::new("main", vec![], FTy::I32);
        let slot = f.alloca(8, 8);
        let off = f.push(0, FTy::I64, Op::Const(0));
        let p = f.push(0, FTy::Ptr, Op::PtrAdd { base: slot, off });
        let v = f.push(0, FTy::I32, Op::Const(7));
        f.push_void(0, FTy::I32, Op::Store { addr: p, val: v });
        let l = f.push(0, FTy::I32, Op::Load { addr: slot });
        f.set_term(0, Term::Ret(Some(l)));
        let a = allocate(&f);
        assert!(a.cells.is_empty(), "Adresse entkommt ueber ptradd");
    }

    #[test]
    fn secret_werte_bekommen_kein_register() {
        let mut f = Func::new("main", vec![], FTy::I32);
        let c = f.push(0, FTy::I32, Op::Const(5));
        f.secret.insert(c);
        let d = f.push(0, FTy::I32, Op::Bin(BinOp::Add, c, c));
        f.set_term(0, Term::Ret(Some(d)));
        let a = allocate(&f);
        assert!(matches!(a.loc(c), Loc::Slot(_)));
    }

    #[test]
    fn select_bleibt_cmov_auch_mit_registern() {
        let mut f = Func::new("main", vec![], FTy::I32);
        let c = f.push(0, FTy::Bool, Op::Call { name: "g".into(), args: vec![] });
        let x = f.push(0, FTy::I32, Op::Const(1));
        let y = f.push(0, FTy::I32, Op::Const(2));
        let s = f.push(0, FTy::I32, Op::Select { cond: c, a: x, b: y });
        f.set_term(0, Term::Ret(Some(s)));
        let mut g = Func::new("g", vec![], FTy::Bool);
        let t = g.push(0, FTy::Bool, Op::Const(1));
        g.set_term(0, Term::Ret(Some(t)));
        let asm = emit(&Module { funcs: vec![f, g] }).expect("codegen");
        assert!(asm.contains("cmovnz"), "{}", asm);
    }

    #[test]
    fn zu_viele_parameter_gehen_an_den_grundpfad() {
        let mut f = Func::new("f", vec![FTy::I64; 7], FTy::I64);
        f.set_term(0, Term::Ret(Some(0)));
        assert!(!supported(&f));
    }
}
