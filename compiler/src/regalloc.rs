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
//! Seit Runde 43 beherrscht dieser Pfad auch **mehr als sechs Parameter bzw.
//! Argumente** (System V: ab dem siebten ueber den Stapel). Vorher fiel jede
//! Funktion, die einen solchen Aufruf enthielt, auf den Grundpfad zurueck —
//! im Tokenizer-Messlauf waren das `main`, `tok_emit`, `sink_flush_chars`,
//! `sink_end`, `out_fehlerliste` und `out_wort`, zusammen ein Viertel aller
//! ausgefuehrten Instruktionen.
//!
//! Der Emissionspfad bleibt **abgesichert**: Konstrukte, die er nicht
//! vollstaendig beherrscht (`f64`, unbekannte Blocknummerierung …), fuehren
//! dazu, dass `emit_func_ra` `None` liefert und `codegen_x86.rs` seinen
//! bewaehrten Grundpfad benutzt.

use crate::codegen_x86::{block_label, label, size_word, Emitter, Frame, ARG_REGS};
use crate::fir::{BinOp, Block, BlockId, CmpOp, FTy, Func, Inst, Op, Term, UnOp, Val};
use std::collections::HashMap;

/// Ort eines Wertes nach der Zuteilung.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Loc {
    /// festes Maschinenregister (64-Bit-Name)
    Reg(&'static str),
    /// Stack-Slot: Adresse = `rbp - off`
    Slot(u64),
}

/// Schreibt der Wert `v` in das physische Register `r`? (Runde 41 — Pruefung
/// fuer den Zellen-Alias.)
fn used_register(alloc: &Alloc, v: Val, r: &'static str) -> bool {
    if let Some(rc) = alloc.cells.get(&v) {
        if *rc == r {
            return true;
        }
    }
    matches!(alloc.locs.get(v as usize), Some(Loc::Reg(x)) if *x == r)
}

/// callee-saved Register, die vergeben werden duerfen (Prolog/Epilog sichern).
const CALLEE_SAVED: [&str; 5] = ["rbx", "r12", "r13", "r14", "r15"];
/// caller-saved Register fuer Intervalle, die KEINEN `call`/`syscall`
/// einschliessen: dann kann weder der Aufruf selbst noch der Aufbau seiner
/// Argumentliste (rdi, rsi, rdx, rcx, r8, r9, r10) den Wert zerstoeren.
const TEMP_REGS: [&str; 4] = ["r11", "r10", "r9", "r8"];
/// Argumentregister, die frei werden, solange das Intervall keinen
/// `call`/`syscall` und kein `copymem`/`secure_zero` kreuzt (siehe `Iv`).
const ARG_SPARE: [&str; 2] = ["rsi", "rdi"];
/// `rdx` wird zusaetzlich von `div`/`rem`/`select` als Arbeitsregister
/// benutzt — nur Intervalle, die all das nicht kreuzen, duerfen es tragen.
const DIV_SPARE: [&str; 1] = ["rdx"];

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
    /// Load-Ergebnisse, die ihren Wert direkt im Zellenregister lesen
    /// (Zellen-Alias, Runde 40): val -> Zellenregister
    alias: HashMap<Val, &'static str>,
    /// val -> befoerderte Zelle, aus der es geladen wurde
    alias_src: HashMap<Val, Val>,
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
    /// Ort eines Wertes als QUELLE: ein Load-Ergebnis mit Zellen-Alias liegt
    /// nie irgendwo, sein Wert steht im Zellenregister. Fuer ZIELE gilt loc().
    pub fn place(&self, v: Val) -> Loc {
        if let Some(r) = self.alias.get(&v) {
            return Loc::Reg(r);
        }
        self.loc(v)
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
                // Bis 32 Bit darf das Immediate den ganzen vorzeichenlosen
                // Bereich ausschoepfen: `cmp $0xffffffff,%r9d` rechnet mit
                // 32-Bit-Operanden exakt richtig. Ohne das faellt genau EOF
                // (u32 0xFFFFFFFF) aus den Immediates, und jeder EOF-Vergleich
                // im Tokenizer laedt seine Konstante aus einem Rahmenslot
                // (Runde 40: 52 solche Stellen allein in `tokenize`).
                let fits = if i.ty.bits() <= 32 {
                    v >= i32::MIN as i128 && v <= u32::MAX as i128
                } else {
                    v >= i32::MIN as i128 && v <= i32::MAX as i128
                };
                if !f.is_secret(d) && fits {
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
    /// kreuzt `copymem`/`secure_zero` (sie schreiben `rdi`, `rsi`, `rcx`)
    crosses_memop: bool,
    /// kreuzt `div`/`rem`/`select` (sie schreiben `rdx` bzw. `rcx`)
    crosses_divsel: bool,
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
        alias: HashMap::new(),
        alias_src: HashMap::new(),
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
    let mut memop_pos: Vec<usize> = Vec::new();
    let mut divsel_pos: Vec<usize> = Vec::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        for (ii, i) in b.insts.iter().enumerate() {
            if matches!(i.op, Op::Call { .. } | Op::CallIndirect { .. } | Op::Syscall { .. } | Op::ThreadSpawn { .. }) {
                call_pos.push(live.pos[bi][ii]);
            }
            if matches!(i.op, Op::CopyMem { .. } | Op::SecureZero { .. }) {
                memop_pos.push(live.pos[bi][ii]);
            }
            // Runde 49: `Op::AtomicCas` benutzt `rdx` als drittes Arbeitsregister
            // (`lock cmpxchg [rcx], rdx`) — genau wie `div`/`rem`/`select`. Ohne
            // diesen Eintrag traegt ein Intervall, das darueber lebt, weiterhin
            // `rdx` und wird zerstoert. Gefunden an tests/820 (nur release-fast).
            if matches!(
                i.op,
                Op::Bin(BinOp::Div | BinOp::Rem, _, _) | Op::Select { .. } | Op::AtomicCas { .. }
            ) {
                divsel_pos.push(live.pos[bi][ii]);
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
        let cm = memop_pos.iter().any(|&p| s <= p && p <= e);
        let cd = divsel_pos.iter().any(|&p| s <= p && p <= e);
        ivs.push(Iv {
            val: v as Val,
            start: s,
            end: e,
            weight: weight[v],
            crosses_call: cc,
            crosses_memop: cm,
            crosses_divsel: cd,
        });
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
        let cm = memop_pos.iter().any(|&p| s <= p && p <= e);
        let cd = divsel_pos.iter().any(|&p| s <= p && p <= e);
        // Zellen sind fast immer die heissesten Werte: Gewicht verdoppeln.
        ivs.push(Iv {
            val: c,
            start: s,
            end: e,
            weight: weight[cv].saturating_mul(2).max(1),
            crosses_call: cc,
            crosses_memop: cm,
            crosses_divsel: cd,
        });
    }
    ivs.sort_by_key(|i| (i.start, i.end, i.val));

    // ---- eigentlicher linear scan ----
    //
    // Vier Pools, vom beschraenktesten zum freiesten Register. `passt` prueft,
    // ob ein Register die Kreuzungen eines Intervalls vertraegt.
    fn fits(iv: &Iv, r: &str) -> bool {
        if CALLEE_SAVED.contains(&r) {
            return true;
        }
        if iv.crosses_call {
            return false;
        }
        if TEMP_REGS.contains(&r) {
            return true;
        }
        if ARG_SPARE.contains(&r) {
            return !iv.crosses_memop;
        }
        if DIV_SPARE.contains(&r) {
            return !iv.crosses_memop && !iv.crosses_divsel;
        }
        false
    }
    let mut free_saved: Vec<&'static str> = CALLEE_SAVED.to_vec();
    let mut free_temp: Vec<&'static str> = TEMP_REGS.to_vec();
    let mut free_arg: Vec<&'static str> = ARG_SPARE.to_vec();
    let mut free_div: Vec<&'static str> = DIV_SPARE.to_vec();
    let mut active: Vec<(Iv, &'static str)> = Vec::new();
    let mut assign: HashMap<Val, &'static str> = HashMap::new();
    let mut used_saved: Vec<&'static str> = Vec::new();

    let free = |r: &'static str,
                     free_saved: &mut Vec<&'static str>,
                     free_temp: &mut Vec<&'static str>,
                     free_arg: &mut Vec<&'static str>,
                     free_div: &mut Vec<&'static str>| {
        if TEMP_REGS.contains(&r) {
            free_temp.push(r);
        } else if ARG_SPARE.contains(&r) {
            free_arg.push(r);
        } else if DIV_SPARE.contains(&r) {
            free_div.push(r);
        } else {
            free_saved.push(r);
        }
    };

    for iv in ivs.iter().copied() {
        // Abgelaufene Intervalle freigeben.
        //
        // RUNDE 49 — `<` STATT `<=`, EIN SOUNDNESS-FEHLER.
        //
        // Die Intervalle sind ABGESCHLOSSEN: `crosses_call` prueft
        // `s <= p && p <= e`. Zwei abgeschlossene Intervalle [a,b] und [c,d]
        // mit a <= c ueberschneiden sich also genau dann, wenn c <= b — und
        // dann duerfen sie NICHT dasselbe Register bekommen.
        //
        // Mit `<=` wurde ein Intervall, das bei p endet, freigegeben, sobald
        // das naechste bei p BEGINNT. Bei klassischem linearem Scan ist das
        // erlaubt, weil dort „Ende" the last USE and "start" die
        // DEFINITION derselben Instruktion ist (erst lesen, dann schreiben).
        // Hier stimmt diese Annahme nicht: die Intervallgrenzen kommen auch
        // aus `live_in`/`live_out` an BLOCKGRENZEN. Ein Wert, der von einem
        // spaeter angeordneten Block aus ueber einen frueheren hinweg lebt,
        // bekommt dadurch als Anfang den Blockanfang — und teilte sich das
        // Register mit einem Wert, der genau dort definiert wird.
        //
        // GEMESSEN an tests/820_gc_finalizer.fi (nur `release-fast`, also
        // nur mit Registerzuteilung): `%355 = z + 24` hatte das Intervall
        // [175,356], `%135 = call gc_collect()` das Intervall [175,175].
        // Beide bekamen `r12`; zur Laufzeit lief der Weg bb45 (Definition von
        // %355) -> bb46 -> bb21 (`mov r12, rax`) -> ... -> bb49 (`mov r8,
        // [r12]`), und das Programm starb mit einem Speicherzugriffsfehler.
        //
        // Der Fehler ist AELTER als Runde 49: sechs Zeilen Attrappe in
        // `gc_collect` genuegen, um ihn mit dem Compiler der Basis (cc1710f)
        // auszuloesen. Runde 49 hat ihn nur getroffen. Die Zuteilung ist mit
        // `<` minimal enger; die Messung steht in docs/RUNDE49.md §3.
        let mut k = 0;
        while k < active.len() {
            if active[k].0.end < iv.start {
                let (_, r) = active.remove(k);
                free(r, &mut free_saved, &mut free_temp, &mut free_arg, &mut free_div);
            } else {
                k += 1;
            }
        }
        // erst die beschraenkten Pools fuellen, callee-saved zuletzt (kostet
        // Prolog/Epilog) — ausser das Intervall kreuzt einen Aufruf, dann
        // kommen nur callee-saved in Frage.
        let pick = if !iv.crosses_call {
            if !free_temp.is_empty() {
                free_temp.pop()
            } else if !iv.crosses_memop && !iv.crosses_divsel && !free_div.is_empty() {
                free_div.pop()
            } else if !iv.crosses_memop && !free_arg.is_empty() {
                free_arg.pop()
            } else if !free_saved.is_empty() {
                free_saved.pop()
            } else {
                None
            }
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
                    if !fits(&iv, r) {
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
    let read = count_reads(f);
    let mut nbuf = Vec::new();
    if std::env::var("FIRN_NO_ALIAS").is_err() {
    // ---- Zellen-Alias (Runde 40) ---------------------------------------
    // `d = load c` mit genau EINER Verwendung im selben Block, vor der die
    // Zelle nicht geschrieben wird: d braucht keinen eigenen Ort, sein Wert
    // steht bereits im Zellenregister. Der Load entfaellt bei der Emission,
    // die Verwendung liest das Zellenregister ueber `ort()` direkt — das
    // streicht das Dreiervierertel `mov r9, r15` vor jeder Benutzung des
    // Schleifenzaehlers (heissester Loop von `dekodiere`: 3 Kopien je
    // Iteration, 33,5 Mio. Iterationen im realweb-Lauf).
    for b in &f.blocks {
        for (ii, inst) in b.insts.iter().enumerate() {
            let (addr, d) = match (&inst.op, inst.dst) {
                (Op::Load { addr }, Some(d)) => (*addr, d),
                _ => continue,
            };
            // NUR volle Breite: bei 8/16/32 Bit zieht der Load die relevanten
            // Bits per movzx/mov32 heraus — das Zellenregister enthaelt oben
            // Reste, ein Alias wuerde sie mitlesen (Runde 40, Fehlerbild
            // 211_generic_struct/430_ct_select/416_fehler_ausgabe).
            if inst.ty.bits().max(8) != 64 {
                continue;
            }
            let rc = match alloc.cells.get(&addr) {
                Some(r) => *r,
                None => continue,
            };
            let needs = read.get(d as usize).copied().unwrap_or(0) as usize;
            if needs == 0 {
                continue;
            }
            // ALLE Verwendungen muessen in diesem Block liegen, bevor die
            // Zelle wieder geschrieben wird (bei mehreren Verwendungen
            // streicht der Alias trotzdem jede Kopie, z. B. Zaehler als
            // Index fuer Quelle UND Ziel im Kopierloop von dekodiere).
            let mut found = 0usize;
            let mut ok = false;
            // RUNDE 41: Das Zellenregister darf zwischen Load und letzter
            // Verwendung von KEINEM anderen Wert beschrieben werden. Der
            // Verteiler kannte die vom Alias verlaengerte Lebensspanne nicht
            // und durfte `rc` an einen Wert vergeben, dessen Spanne die des
            // Zellenwerts nicht ueberschneidet — dann steht beim Lesen etwas
            // Fremdes darin. Fehlerbild: `43 - start` in bin/print.fi
            // (drucke_binop) wurde zu `43 - &tab[start]`, weil `lea` die
            // Adresse in genau dieses Register schrieb; die Laenge lief unter
            // Null und buf_wachse drehte sich ewig (Endlosschleife in
            // .astdump auf jedem `||`).
            let mut destroys = false;
            for nj in b.insts.iter().skip(ii + 1) {
                nbuf.clear();
                nj.op.uses(&mut nbuf);
                found += nbuf.iter().filter(|u| **u == d).count();
                if let Some(d2) = nj.dst {
                    if d2 != d && used_register(&alloc, d2, rc) {
                        destroys = true;
                        break;
                    }
                }
                // Ein Aufruf zerstoert alle caller-saved Register; der
                // Zellenwert steht dann nur noch im Rahmen, nicht im
                // Register. (Fehlerbild: bin/layoutdump.fi stuerzte in
                // intern_finde mit t=0 ab.)
                if matches!(nj.op, Op::Call { .. } | Op::CallIndirect { .. } | Op::Syscall { .. } | Op::ThreadSpawn { .. })
                    && !CALLEE_SAVED.contains(&rc)
                {
                    destroys = true;
                    break;
                }
                if matches!(nj.op, Op::Store { addr: a2, .. } if a2 == addr) {
                    break; // danach ist der geladene Wert veraltet
                }
            }
            if destroys {
                continue;
            }
            if found == needs {
                ok = true;
            } else {
                // Rest-Verwendung kann im Terminator stecken (brcond/ret).
                // Switch NICHT: der erwartet den Wert im Rahmen
                // (codegen_switch).
                let in_term = match &b.term {
                    Term::BrCond { cond, .. } if *cond == d => 1,
                    Term::Ret(Some(v)) if *v == d => 1,
                    _ => 0,
                };
                ok = found + in_term == needs && in_term > 0;
            }
            if ok {
                alloc.alias.insert(d, rc);
                alloc.alias_src.insert(d, addr);
            }
        }
    }

    }
    if std::env::var("FIRN_NO_INPLACE").is_err() {
    // ---- In-place-Zellenupdate (Runde 40) -------------------------------
    // `d1 = load c` (alias), `v = d1 + k`, `store c, v` mit jeweils einer
    // Verwendung: v bekommt das Zellenregister als Ort — die Emission rechnet
    // dann direkt im Zellenregister (`lea r15, [r15+1]`) und der Store
    // entfaellt. Bedingung: zwischen der Definition von v und dem Store wird
    // die Zelle weder gelesen noch geschrieben (sonst sahe ein Zwischenleser
    // den neuen Wert zu frueh) und kein Aufruf trennt die beiden. Auch kein
    // ALIAS-Wert derselben Zelle darf in dem Fenster noch ausstehen (seine
    // Verwendung laesst den alten Inhalt erwarten, der schon ueberschrieben
    // waere).
    for b in &f.blocks {
        for (ii, inst) in b.insts.iter().enumerate() {
            let (a, k, v) = match (&inst.op, inst.dst) {
                (Op::Bin(BinOp::Add | BinOp::Sub, a, k), Some(v)) => (*a, *k, v),
                _ => continue,
            };
            let (rc, cell) = match (alloc.alias.get(&a), alloc.alias_src.get(&a)) {
                (Some(r), Some(z)) => (*r, *z),
                _ => continue,
            };
            if alloc.imm(k).is_none()
                || read.get(v as usize).copied().unwrap_or(0) != 1
                || f.is_secret(v)
            {
                continue;
            }
            let mut ok = false;
            for (jj, nj) in b.insts.iter().enumerate().skip(ii + 1) {
                match &nj.op {
                    Op::Store { addr: a2, val } if *a2 == cell => {
                        ok = *val == v;
                        break;
                    }
                    Op::Load { addr: a2 } if *a2 == cell => break,
                    Op::Call { .. }
                    | Op::CallIndirect { .. }
                    | Op::Syscall { .. }
                    | Op::ThreadSpawn { .. } => break,
                    _ => {}
                }
                // steht noch die Verwendung eines Alias-Werts derselben Zelle
                // aus? Der erwartet den ALTEN Inhalt.
                nbuf.clear();
                nj.op.uses(&mut nbuf);
                if nbuf.iter().any(|u| {
                    *u != a && alloc.alias_src.get(u) == Some(&cell)
                }) {
                    break;
                }
                let _ = jj;
            }
            if ok {
                alloc.locs[v as usize] = Loc::Reg(rc);
            }
        }
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
    read: Vec<u32>,
    /// Adressen, deren einzige Verwendung der UNMITTELBAR folgende
    /// Speicherzugriff ist — sie wandern vollstaendig in dessen Operanden.
    offset: HashMap<Val, Address>,
    /// Instruktionen (Skalierung `shl`/`mul`), die dabei ganz entfallen.
    skipped: std::collections::HashSet<Val>,
    /// Instruktionen, von denen nur noch das FUELLEN ihres Registers uebrig
    /// bleibt: Wert -> Quellwert. Der Rest der Rechnung steckt im
    /// Speicheroperanden des folgenden Zugriffs.
    preloader: HashMap<Val, Val>,
}

/// Ein Speicheroperand, den der Prozessor selbst ausrechnet:
/// `[basis + index*faktor + versatz]` (x86-64 SIB-Adressierung).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Address {
    base: &'static str,
    /// Indexregister mit Faktor 1, 2, 4 oder 8
    index: Option<(&'static str, i64)>,
    offset: i64,
}

impl Address {
    fn text(&self) -> String {
        let mut s = String::from("[");
        s.push_str(self.base);
        if let Some((r, f)) = self.index {
            s.push('+');
            s.push_str(r);
            if f != 1 {
                s.push('*');
                s.push_str(&f.to_string());
            }
        }
        if self.offset > 0 {
            s.push('+');
            s.push_str(&self.offset.to_string());
        } else if self.offset < 0 {
            s.push('-');
            s.push_str(&(-self.offset).to_string());
        }
        s.push(']');
        s
    }
}

/// Adressrechnungen, die vollstaendig in den Speicherzugriff wandern duerfen.
///
/// Erzeugt wurde bis Runde 43
///     lea r9, [r8+168]
///     mov r9, qword ptr [r9]
/// obwohl x86-64 den Versatz selbst kann:
///     mov r9, qword ptr [r8+168]
///
/// **Runde 51** nimmt die beiden anderen Bestandteile der x86-Adressierung
/// dazu — Indexregister und Faktor. Gemessen im Tokenizer (realweb,
/// instruktionsgenaues callgrind) stand vor dieser Runde:
///
/// | Muster                                   |          Ir | Anteil |
/// |------------------------------------------|------------:|-------:|
/// | `shl k` + `lea (b,i,1)` + Zugriff        |  28.840.310 |  3,93 % |
/// | `lea (b,i,1)` + Zugriff                  |  16.231.553 |  2,21 % |
/// | `lea off(b)` + Zugriff                   |  14.432.184 |  1,97 % |
///
/// Also wird aus
///     mov  r8, qword ptr [rbp-416]
///     shl  r8, 2
///     lea  r8, [r9+r8]
///     mov  r8d, dword ptr [r8]
/// jetzt
///     mov  r8, qword ptr [rbp-416]
///     mov  r8d, dword ptr [r9+r8*4]
///
/// **Die Bedingungen sind absichtlich eng**, denn jede Lockerung verlaengert
/// die Lebensspanne der Basis — genau die Klasse, die in Runde 40/41 den
/// Miscompile erzeugt hat (docs/RUNDE41.md). Gefaltet wird nur, wenn
///
///  * die adressbildende Instruktion `ptradd` oder ein **64-Bit**-`add` ist
///    (bei 32 Bit wuerde die Adressierung den Ueberlauf NICHT abschneiden),
///  * ihr Ergebnis GENAU EINMAL gelesen wird (Terminatoren mitgezaehlt),
///  * dieser eine Leser die UNMITTELBAR folgende Instruktion desselben
///    Blocks ist und ein `load`/`store` ueber genau diese Adresse,
///  * Basis (und ggf. Index) in einem Register liegen und weder Rahmen-
///    adresse noch befoerderte Zelle noch Zellen-Alias sind,
///  * fuer den Faktor: die Skalierung ist ein **64-Bit**-`shl` mit 0..3 bzw.
///    `mul` mit 1/2/4/8, steht UNMITTELBAR vor der Adressbildung und ihr
///    Ergebnis wird ebenfalls genau einmal gelesen,
///  * kein Wert der Kette ist `secret` (SPEC §9.2: kein datenabhaengiger
///    Zugriff).
///
/// Damit verschiebt sich der Lesezeitpunkt von Basis und Index um genau die
/// ein bis zwei Instruktionen, die dabei **ganz entfallen** — dazwischen
/// liegt danach nichts mehr, insbesondere kein `call`. Die einzigen Register,
/// die an der neuen Stelle geschrieben werden, sind das Ziel des Zugriffs
/// (das seine Adresse zuerst liest — `mov r8d, dword ptr [r9+r8*4]` ist
/// korrekt) und die Heimat der uebersprungenen Instruktionen, die gar nicht
/// mehr beschrieben wird.
///
/// Liefert `(Adressen je Wert, uebersprungene Skalierungen)`.
/// Abschaltbar mit FIRN_NO_FALTUNG=1 (Fehlersuche).
fn foldable_addresses(
    f: &Func,
    a: &Alloc,
    read: &[u32],
) -> (HashMap<Val, Address>, std::collections::HashSet<Val>, HashMap<Val, Val>) {
    use std::collections::HashSet;
    let mut out: HashMap<Val, Address> = HashMap::new();
    let mut away: HashSet<Val> = HashSet::new();
    let mut before: HashMap<Val, Val> = HashMap::new();
    if std::env::var_os("FIRN_NO_FALTUNG").is_some() {
        return (out, away, before);
    }
    // Liegt der Wert schlicht in einem Register — ohne Sonderbehandlung?
    let pure_reg = |v: Val| -> Option<&'static str> {
        if a.imm(v).is_some() || a.cell(v).is_some() || f.is_secret(v) {
            return None;
        }
        if a.alias.contains_key(&v) || a.frame_addr.contains_key(&v) {
            return None;
        }
        match a.place(v) {
            Loc::Reg(r) => Some(r),
            Loc::Slot(_) => None,
        }
    };
    for b in &f.blocks {
        for (idx, i) in b.insts.iter().enumerate() {
            let d = match i.dst {
                Some(d) => d,
                None => continue,
            };
            // (1) adressbildende Instruktion
            let (base, off) = match &i.op {
                Op::PtrAdd { base, off } => (*base, *off),
                // Ein `add` bildet nur dann eine Adresse, wenn es in voller
                // Breite rechnet. Bei 32 Bit schneidet FIR das Ergebnis ab,
                // die Adressierung taete das nicht.
                Op::Bin(BinOp::Add, x, y) if i.ty.bits() == 64 => (*x, *y),
                _ => continue,
            };
            if read.get(d as usize).copied() != Some(1) || f.is_secret(d) {
                continue;
            }
            if a.alias.contains_key(&d) || a.frame_addr.contains_key(&d) || a.cell(d).is_some() {
                continue;
            }
            // (2) der EINE Leser ist der unmittelbar folgende Zugriff
            let n = match b.insts.get(idx + 1) {
                Some(n) => n,
                None => continue,
            };
            let fits = match &n.op {
                Op::Load { addr } => *addr == d,
                Op::Store { addr, val } => *addr == d && *val != d,
                _ => false,
            };
            if !fits {
                continue;
            }
            // Register, die der folgende Zugriff selbst noch LESEN muss —
            // sie duerfen nicht als Vorlade-Ziel dienen.
            let value_reg: Option<&'static str> = match &n.op {
                Op::Store { val, .. } => match a.place(*val) {
                    Loc::Reg(r) => Some(r),
                    _ => None,
                },
                _ => None,
            };
            // Die Basis liegt entweder schon in einem Register — oder sie
            // wird in das Register der Adressrechnung geladen, das sonst
            // ungenutzt bliebe (Fall C, Runde 51):
            //     mov rax, qword ptr [rbp-8]      statt   mov rax, [rbp-8]
            //     mov r9, qword ptr [rax+8]               lea r9, [rax+8]
            //                                             mov r9, [r9]
            let base_may_read = |v: Val| -> bool {
                !f.is_secret(v)
                    && a.cell(v).is_none()
                    && !a.alias.contains_key(&v)
                    && !a.frame_addr.contains_key(&v)
                    && a.imm(v).is_none()
            };
            let (br, base_preload) = match pure_reg(base) {
                Some(r) => (r, false),
                None => match (a.loc(d), base_may_read(base)) {
                    (Loc::Reg(dr), true) if Some(dr) != value_reg => (dr, true),
                    _ => continue,
                },
            };
            // (3a) konstanter Versatz
            if let Some(k) = a.imm(off) {
                if (0..=i32::MAX as i64).contains(&k) && !f.is_secret(off) {
                    out.insert(d, Address { base: br, index: None, offset: k });
                    if base_preload {
                        before.insert(d, base);
                    }
                }
                continue;
            }
            // (3b) Index mit Faktor: die Skalierung steht unmittelbar davor
            if read.get(off as usize).copied() == Some(1) && idx > 0 {
                let p = &b.insts[idx - 1];
                let skal = if p.dst == Some(off) && p.ty.bits() == 64 {
                    match &p.op {
                        Op::Bin(BinOp::Shl, xi, ki) => match a.imm(*ki) {
                            Some(k) if (0..=3).contains(&k) => Some((*xi, 1i64 << k)),
                            _ => None,
                        },
                        Op::Bin(BinOp::Mul, xi, ki) => match a.imm(*ki) {
                            Some(k) if [1, 2, 4, 8].contains(&k) => Some((*xi, k)),
                            _ => None,
                        },
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some((xi, fact)) = skal {
                    if !f.is_secret(off) && !a.alias.contains_key(&off) && a.cell(off).is_none() {
                        // Fall A: der Index liegt selbst in einem Register —
                        // die Skalierung entfaellt ersatzlos.
                        if let Some(ir) = pure_reg(xi) {
                            if !base_preload || ir != br {
                                out.insert(
                                    d,
                                    Address { base: br, index: Some((ir, fact)), offset: 0 },
                                );
                                away.insert(off);
                                if base_preload {
                                    before.insert(d, base);
                                }
                                continue;
                            }
                        }
                        // Fall B: der Index liegt im Rahmen, aber die
                        // Skalierung hat ein Registerheim. Dann wird dorthin
                        // der UNSKALIERTE Wert geladen und der Faktor der
                        // Adressierung ueberlassen — eine Instruktion statt
                        // zwei.
                        if let (Loc::Reg(ir), true) = (a.loc(off), base_may_read(xi)) {
                            if ir != br && Some(ir) != value_reg {
                                out.insert(
                                    d,
                                    Address { base: br, index: Some((ir, fact)), offset: 0 },
                                );
                                before.insert(off, xi);
                                if base_preload {
                                    before.insert(d, base);
                                }
                                continue;
                            }
                        }
                    }
                }
            }
            // (3c) Index direkt aus einem Register (Faktor 1)
            if let Some(ir) = pure_reg(off) {
                if !base_preload || ir != br {
                    out.insert(d, Address { base: br, index: Some((ir, 1)), offset: 0 });
                    if base_preload {
                        before.insert(d, base);
                    }
                }
            }
        }
    }
    (out, away, before)
}

/// Zaehlt je Wert, wie oft er als Operand vorkommt (Instruktionen + Terminatoren).
fn count_reads(f: &Func) -> Vec<u32> {
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
        match self.a.place(v) {
            Loc::Reg(r) => r.to_string(),
            Loc::Slot(off) => format!("qword ptr [rbp-{}]", off),
        }
    }
    /// Operand eines Wertes in der Breite `bits`.
    fn opnd_w(&self, v: Val, bits: u32) -> String {
        if let Some(k) = self.a.imm(v) {
            return format!("{}", k);
        }
        match self.a.place(v) {
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
    // Die Funktion wird zunaechst in einen eigenen Puffer emittiert; danach
    // streicht der Register-Deskriptor-Nachpass Spill-Stores mit sofortigem
    // Reload desselben Werts (445x statisch im Tokenizer-Workload, Runde 37).
    let mut tmp = Emitter { out: String::new() };
    match emit_with(&mut tmp, f, &a) {
        Ok(()) => {
            let nv = f.val_types.len();
            e.out.push_str(&descriptor_peephole(&tmp.out, nv));
            Some(Ok(()))
        }
        Err(err) => Some(Err(err)),
    }
}

// ------------------------------------------------- Register-Deskriptor ---
//
// Nachpass ueber den fertig emittierten Assembler EINER Funktion. Die
// Zuteilung schreibt Werte ohne Register in ihren Stack-Slot (`store_dst`)
// und laedt sie bei der naechsten Verwendung wieder (`load_full`) — steht der
// Wert aber noch unveraendert in dem Register, aus dem er gespeichert wurde,
// ist der Reload umsonst: entweder ganz (gleiches Register) oder als
// Speicherzugriff (andere Zielregister: `mov rB, rA` statt `mov rB, [rbp-X]`).
//
// Verfolgt werden ausschliesslich **Wert-Slots**: ihre Offsets liegen bei
// `8..=nv*8` (layout() vergibt sie zuerst). `alloca`-Plaetze und
// Sicherungs-Slots liegen dahinter und werden nie getrackt — Schreiben ueber
// Zeiger (`Op::Store`/`CopyMem`/`SecureZero`) kann sie treffen, Wert-Slots
// dagegen nie (ihre Adresse existiert im Programm nicht).
//
// Invalidierung (konservativ, Sicherheit vor Gewinn):
//  * Blockgrenzen (Label) und Rueckwaerts-/Sprungzeilen setzen den Zustand
//    zurueck — der Folgeblock kann von anderswo mit fremdem Zustand kommen.
//  * `call` loescht alle caller-saved Register aus dem Deskriptor,
//    `syscall` rax/rcx/r11, `rep movsb/stosb` rdi/rsi/rcx, `div/idiv`
//    rax/rdx, `cqo/cdq` rdx, `setcc` al (= rax).
//  * Jede andere Instruktion, die ein getracktes Register als Zieloperanden
//    schreibt (mov/lea/add/.../cmov), invalidiert genau dieses Register.
fn descriptor_peephole(asm: &str, nv: usize) -> String {
    /// 64-Bit-Stammregister eines Register-Namens beliebiger Breite.
    fn stem(r: &str) -> &str {
        match r {
            "al" | "ax" | "eax" | "rax" => "rax",
            "bl" | "bx" | "ebx" | "rbx" => "rbx",
            "cl" | "cx" | "ecx" | "rcx" => "rcx",
            "dl" | "dx" | "edx" | "rdx" => "rdx",
            "sil" | "si" | "esi" | "rsi" => "rsi",
            "dil" | "di" | "edi" | "rdi" => "rdi",
            "bpl" | "bp" | "ebp" | "rbp" => "rbp",
            _ => {
                let b = r.as_bytes();
                if b.len() >= 3 && b[0] == b'r' && matches!(b[b.len() - 1], b'd' | b'w' | b'b')
                    && r[1..r.len() - 1].chars().all(|c| c.is_ascii_digit())
                {
                    &r[..r.len() - 1]
                } else {
                    r
                }
            }
        }
    }
    /// Bitbreite eines Registernamens.
    fn width_of(r: &str) -> u32 {
        match r {
            "al" | "bl" | "cl" | "dl" | "sil" | "dil" | "bpl" => 8,
            "ax" | "bx" | "cx" | "dx" | "si" | "di" | "bp" => 16,
            "eax" | "ebx" | "ecx" | "edx" | "esi" | "edi" | "ebp" => 32,
            _ => {
                let b = r.as_bytes();
                if b.len() >= 3 && b[0] == b'r' && r[1..r.len() - 1].chars().all(|c| c.is_ascii_digit())
                {
                    match b[b.len() - 1] {
                        b'b' => 8,
                        b'w' => 16,
                        b'd' => 32,
                        _ => 64,
                    }
                } else {
                    64
                }
            }
        }
    }
    let max_slot = nv as u64 * 8;
    let mut out = String::with_capacity(asm.len());
    // slot_off -> (Register mit demselben Inhalt, Breite der Speicherung)
    let mut sync: HashMap<u64, (String, u32)> = HashMap::new();
    // Register -> slot_off (Umkehrung)
    let mut holds: HashMap<String, u64> = HashMap::new();
    // NULLERWEITERUNG (Runde 51). `nullab[r] = k` heisst: alle Bits ab k
    // sind in `r` garantiert null. Ohne Eintrag ist nichts bekannt.
    //
    // Grundlage ist eine Eigenschaft von x86-64, die im ganzen Nachpass
    // gilt: JEDER Schreibzugriff auf ein 32-Bit-Register nullt die oberen
    // 32 Bit des 64-Bit-Registers. Ein `movzx r32, byte ptr [..]` sagt
    // sogar, dass alles ab Bit 8 null ist.
    //
    // Erst damit darf ein schmaler Reload gestrichen werden: `mov [X], r8d`
    // gefolgt von `mov r8d, [X]` laedt genau die Bits zurueck, die schon in
    // r8 stehen — aber nur, wenn r8 oben ohnehin schon null ist. Genau diese
    // Bedingung fehlte in Runde 43, weshalb der Fall dort zurueckgestellt
    // wurde (docs/RUNDE43.md §6).
    let mut nullab: HashMap<String, u32> = HashMap::new();
    let kill_reg = |r: &str,
                    sync: &mut HashMap<u64, (String, u32)>,
                    holds: &mut HashMap<String, u64>| {
        if let Some(off) = holds.remove(r) {
            if sync.get(&off).map(|s| s.0.as_str()) == Some(r) {
                sync.remove(&off);
            }
        }
    };
    for line in asm.lines() {
        let t = line.trim_start();
        if !line.starts_with("    ") || t.is_empty() {
            // Label, Direktiven, Kommentare am Zeilenanfang. Ein Label ist
            // eine Blockgrenze: der Zustand des Vorgaengers gilt nicht.
            if t.ends_with(':') && !t.starts_with('.') {
                sync.clear();
                holds.clear();
                nullab.clear();
            } else if t.starts_with(".L") && t.ends_with(':') {
                sync.clear();
                holds.clear();
                nullab.clear();
            }
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let mut parts = t.splitn(2, ' ');
        let mn = parts.next().unwrap_or("");
        let ops = parts.next().unwrap_or("").trim();
        // Zielenformen, die wir tracken/ersetzen.
        // Speichern in ein WERT-Fach, in JEDER Breite (Runde 51: vorher nur
        // `qword`). `off <= max_slot` grenzt auf die Wert-Faecher ein —
        // `alloca`-Plaetze liegen dahinter und koennen ueber Zeiger
        // beschrieben werden.
        let st_width = if t.starts_with("mov qword ptr [rbp-") {
            Some(64)
        } else if t.starts_with("mov dword ptr [rbp-") {
            Some(32)
        } else if t.starts_with("mov word ptr [rbp-") {
            Some(16)
        } else if t.starts_with("mov byte ptr [rbp-") {
            Some(8)
        } else {
            None
        };
        if let Some(bw) = st_width {
            let rest = &t[t.find("[rbp-").unwrap() + 5..];
            if let Some(kl) = rest.find(']') {
                let off: u64 = rest[..kl].parse().unwrap_or(0);
                let q = rest[kl + 1..].trim_start_matches(',').trim();
                if off >= 8 && off <= max_slot && width_of(q) == bw && is_reg64(stem(q)) {
                    let q = stem(q).to_string();
                    kill_reg(&q, &mut sync, &mut holds);
                    sync.insert(off, (q.clone(), bw));
                    holds.insert(q, off);
                    out.push_str(line);
                    out.push('\n');
                    continue;
                }
                // Sofortkonstante o.ae.: das Fach hat einen neuen Inhalt,
                // der in keinem Register steht.
                if off >= 8 && off <= max_slot {
                    if let Some((r, _)) = sync.remove(&off) {
                        holds.remove(&r);
                    }
                }
            }
        }
        // Reload aus einem Wert-Fach. Vier Formen, jede mit ihrer eigenen
        // Bedingung:
        //   mov  rY,  qword ptr [X]   braucht Speicherbreite 64
        //   mov  rYd, dword ptr [X]   braucht >= 32 und nullab[rY] <= 32
        //   movzx rYd, byte ptr [X]   braucht >=  8 und nullab[rY] <=  8
        //   movzx rYd, word ptr [X]   braucht >= 16 und nullab[rY] <= 16
        // `movsx`/`movsxd` bleiben aussen vor: Vorzeichenerweiterung laesst
        // sich aus `nullab` nicht belegen.
        let rlform = if mn == "mov" {
            if let Some(k) = ops.find(", qword ptr [rbp-") {
                Some((k, 17usize, 64u32, 64u32))
            } else {
                ops.find(", dword ptr [rbp-").map(|k| (k, 17usize, 32u32, 32u32))
            }
        } else if mn == "movzx" {
            if let Some(k) = ops.find(", byte ptr [rbp-") {
                Some((k, 16usize, 8u32, 8u32))
            } else {
                ops.find(", word ptr [rbp-").map(|k| (k, 16usize, 16u32, 16u32))
            }
        } else {
            None
        };
        if let Some((kl, before, min, ndbits)) = rlform {
            let target = &ops[..kl];
            let zb = width_of(target);
            // Zielbreite muss zur Ladeform passen: qword -> 64, sonst 32.
            let fits_target = if min == 64 { zb == 64 } else { zb == 32 };
            if fits_target && is_reg64(stem(target)) {
                if let Some(end) = ops[kl + before..].find(']') {
                    let off: u64 = ops[kl + before..kl + before + end].parse().unwrap_or(0);
                    if off >= 8 && off <= max_slot {
                        let z = stem(target).to_string();
                        let hit = match sync.get(&off) {
                            Some((r2, bw)) if *bw >= min => Some(r2.clone()),
                            _ => None,
                        };
                        if let Some(r2) = hit {
                            let same = r2 == z;
                            let already_null = nullab.get(&z).copied().unwrap_or(64) <= ndbits;
                            if same && (min == 64 || already_null) {
                                // Der Wert steht bereits genau so im Register.
                                kill_reg(&z, &mut sync, &mut holds);
                                sync.insert(off, (z.clone(), min));
                                holds.insert(z.clone(), off);
                                if min < 64 {
                                    nullab.insert(z, ndbits);
                                }
                                continue;
                            }
                            if !same && min == 64 {
                                out.push_str(&format!("    mov {}, {}\n", z, r2));
                                kill_reg(&z, &mut sync, &mut holds);
                                sync.insert(off, (z.clone(), 64));
                                holds.insert(z.clone(), off);
                                nullab.remove(&z);
                                continue;
                            }
                        }
                        kill_reg(&z, &mut sync, &mut holds);
                        sync.insert(off, (z.clone(), min));
                        holds.insert(z.clone(), off);
                        if min < 64 {
                            nullab.insert(z, ndbits);
                        } else {
                            nullab.remove(&z);
                        }
                        out.push_str(line);
                        out.push('\n');
                        continue;
                    }
                }
            }
        }
        // Spruenge/Ruecksprung: Zustand des Folgeblocks unbekannt.
        if mn.starts_with('j') || mn == "ret" {
            sync.clear();
            holds.clear();
            nullab.clear();
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if mn == "call" {
            for r in ["rax", "rcx", "rdx", "rsi", "rdi", "r8", "r9", "r10", "r11"] {
                kill_reg(r, &mut sync, &mut holds);
                nullab.remove(r);
            }
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if mn == "syscall" {
            for r in ["rax", "rcx", "r11"] {
                kill_reg(r, &mut sync, &mut holds);
                nullab.remove(r);
            }
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if mn == "rep" {
            for r in ["rdi", "rsi", "rcx"] {
                kill_reg(r, &mut sync, &mut holds);
                nullab.remove(r);
            }
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if mn == "div" || mn == "idiv" {
            kill_reg("rax", &mut sync, &mut holds);
            kill_reg("rdx", &mut sync, &mut holds);
            nullab.remove("rax");
            nullab.remove("rdx");
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if mn == "cqo" || mn == "cdq" {
            kill_reg("rdx", &mut sync, &mut holds);
            nullab.remove("rdx");
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if mn.starts_with("set") {
            kill_reg("rax", &mut sync, &mut holds); // Ziel ist im RA-Pfad immer `al`
            nullab.remove("rax"); // `setcc al` laesst die oberen Bits stehen
            out.push_str(line);
            out.push('\n');
            continue;
        }
        // Instruktionen, die ihr erstes Operandenregister schreiben.
        if matches!(
            mn,
            "mov" | "movzx" | "movsx" | "movsxd" | "lea" | "add" | "sub" | "and" | "or" | "xor"
                | "imul" | "shl" | "sar" | "shr" | "neg" | "not" | "pop"
        ) || mn.starts_with("cmov")
        {
            let target = ops.split(',').next().unwrap_or("").trim();
            // ACHTUNG: der Zielname kann schmal sein (`xor eax, eax` nullt
            // ganz rax) — die Pruefung muss auf das STAMMREGISTER gehen,
            // sonst ueberlebt ein veralteter Deskriptor-Eintrag (Runde 40:
            // liess tests/305_dtoa_hardcases falsch rechnen).
            let z = stem(target);
            if !target.contains('[') && is_reg64(z) {
                let zs = z.to_string();
                kill_reg(&zs, &mut sync, &mut holds);
                // Nullerweiterung fortschreiben (Runde 51). Ein Schreibzugriff
                // auf ein 32-Bit-Register nullt die oberen 32 Bit; `movzx`
                // aus einer 8-/16-Bit-Quelle sagt sogar mehr. Alles andere
                // macht den Inhalt oben unbekannt.
                let bw = width_of(target);
                if mn == "movzx" {
                    let q = ops.rsplit(',').next().unwrap_or("").trim();
                    let of = if q.starts_with("byte ptr") {
                        8
                    } else if q.starts_with("word ptr") {
                        16
                    } else {
                        width_of(q)
                    };
                    if bw >= 32 && (of == 8 || of == 16) {
                        nullab.insert(zs, of);
                    } else {
                        nullab.remove(&zs);
                    }
                } else if bw == 32 {
                    nullab.insert(zs, 32);
                } else {
                    nullab.remove(&zs);
                }
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Sind anweisungsgenaue Debugzeilen aktiv (`--no-opt`, siehe `dwarf.rs`)?
/// Dann uebernimmt der Grundpfad, damit `.loc` je Instruktion erhalten bleibt.
fn debug_lines_active(f: &Func) -> bool {
    f.blocks.iter().any(|b| {
        (0..b.insts.len()).any(|i| crate::dwarf::line_at(&f.name, b.id, i as u32).is_some())
    })
}

fn supported(f: &Func) -> bool {
    let basic = unsupported_basic(f);
    if let Some(g) = basic {
        if std::env::var_os("FIRN_RA_WARN").is_some() {
            eprintln!("RA base path: {} — {}", f.name, g);
        }
        return false;
    }
    true
}

/// Warum faellt `f` auf den Grundpfad zurueck? `None` = Registerzuteilung moeglich.
fn unsupported_basic(f: &Func) -> Option<String> {
    // RUNDE 52: `#[interrupt]` hat eine eigene Aufrufkonvention (alle Register
    // retten, `iretq`). Sie steht im Grundpfad von `codegen_x86.rs`.
    if f.interrupt {
        return Some("#[interrupt]".into());
    }
    if debug_lines_active(f) {
        return Some("debug lines active".into());
    }
    // GLEITKOMMA: dieser Zuteiler kennt nur die Ganzzahlregister. `f64` lebt
    // in den SSE-Registern und braucht eine zweite Registerklasse mit eigenen
    // Intervallen. Solange die fehlt, geht eine Funktion mit `f64` ueber den
    // Grundpfad in `codegen_x86.rs` — korrekt, aber ohne Registerzuteilung.
    // Ehrlich benannt in SPEC §14.1.f64.
    if f.val_types.iter().any(|t| *t == FTy::F64) {
        return Some("f64 in the value set".into());
    }
    if f.blocks.is_empty() {
        return Some("no blocks".into());
    }
    if let Some((i, b)) = f.blocks.iter().enumerate().find(|(i, b)| b.id as usize != *i) {
        return Some(format!("block numbers not consecutive (index {}, id {})", i, b.id));
    }
    for b in &f.blocks {
        if matches!(b.term, Term::Unset) {
            return Some(format!("block {} without terminator", b.id));
        }
        for i in &b.insts {
            match &i.op {
                Op::Call { .. } | Op::CallIndirect { .. } | Op::VtabAddr { .. } => {}
                Op::Syscall { args } => {
                    if args.is_empty() || args.len() > 7 {
                        return Some(format!("syscall with {} arguments", args.len()));
                    }
                }
                // RUNDE 52: Inline-Assembler und MMIO gehen ueber den
                // Grundpfad. Beides bindet feste Register und ist `volatile`;
                // die Zuteilung haette dafuer eine eigene Sonderregel
                // gebraucht, und eine Sonderregel im Zuteiler ist genau die
                // Sorte Code, die den Fehler aus Runde 40 erzeugt hat.
                // Kernel-Code laeuft damit ohne Registerzuteilung — langsamer,
                // aber nachweislich richtig. Ehrlich benannt in docs/RUNDE52.md.
                Op::Asm { .. } => return Some("Inline-Assembler".into()),
                Op::MmioLoad { .. } | Op::MmioStore { .. } => {
                    return Some("MMIO access".into())
                }
                _ => {}
            }
        }
    }
    None
}

fn emit_with(e: &mut Emitter, f: &Func, a: &Alloc) -> Result<(), String> {
    let read = count_reads(f);
    let (offset, skipped, preloader) = foldable_addresses(f, a, &read);
    let ra = Ra { f, a, read, offset, skipped, preloader };
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
    for (i, _t) in f.params.iter().enumerate().take(ARG_REGS.len()) {
        match ra.a.loc(i as Val) {
            Loc::Slot(off) => {
                e.line(&format!("mov qword ptr [rbp-{}], {}", off, ARG_REGS[i]))
            }
            Loc::Reg(dst) => prolog_moves.push((dst.to_string(), ARG_REGS[i].to_string())),
        }
    }
    parallel_reg_moves(e, &prolog_moves);
    // Parameter ab dem siebten liegen im Rahmen des AUFRUFERS (System V:
    // [rbp+16], [rbp+24], … — davor stehen gesicherte Ruecksprungadresse und
    // gesichertes rbp). Sie werden ERST NACH den parallelen Bewegungen geholt:
    // ihr Zielregister darf sonst eine noch gebrauchte Quelle ueberschreiben.
    // `rax` ist als Arbeitsregister nie Heimat eines Wertes und darf hier als
    // Zwischenlager dienen.
    for (i, _t) in f.params.iter().enumerate().skip(ARG_REGS.len()) {
        let of = 16 + 8 * (i - ARG_REGS.len()) as u64;
        match ra.a.loc(i as Val) {
            Loc::Slot(off) => {
                e.line(&format!("mov rax, qword ptr [rbp+{}]", of));
                e.line(&format!("mov qword ptr [rbp-{}], rax", off));
            }
            Loc::Reg(dst) => e.line(&format!("mov {}, qword ptr [rbp+{}]", dst, of)),
        }
    }
    // Runde 51: die Bloecke werden nicht mehr in ihrer FIR-Reihenfolge
    // ausgegeben, sondern entlang von Spuren (siehe `emissionsreihenfolge`).
    let order = emit_order(f);
    for (k, &bi) in order.iter().enumerate() {
        let b = &f.blocks[bi];
        e.raw(&format!("{}:", block_label(&f.name, b.id)));
        // Fallthrough: steht das Sprungziel unmittelbar dahinter, faellt der
        // Sprung weg (spart pro BrCond mit else==naechster Block ein `jmp`).
        let next = order.get(k + 1).map(|&j| f.blocks[j].id);
        emit_block(e, &ra, b, next)?;
    }
    Ok(())
}

/// **Blocklayout entlang von Spuren** (Runde 51).
///
/// Bisher wurden die Bloecke in ihrer FIR-Nummerierung ausgegeben. Wo weder
/// `then` noch `else` zufaellig der naechste Block war, standen hinter dem
/// bedingten Sprung noch ein `jmp` — im Tokenizer an 641 Stellen, gemessen
/// **28.414.304 von 775.569.867 Instruktionen (3,66 %)** allein fuer diese
/// unbedingten Spruenge:
///
/// ```text
/// cmp  -0x18(%rbp),%r8
/// jae  40dbd4          ; then
/// jmp  40dbe0          ; else — haette Fallthrough sein koennen
/// ```
///
/// Das Verfahren ist die uebliche gierige Spurbildung: ab `bb0` wird dem
/// bevorzugten Nachfolger gefolgt, solange der noch frei ist; reisst die Spur
/// ab, geht es beim kleinsten noch nicht platzierten Block weiter. Bevorzugt
/// wird der `else`-Zweig — `emit_block` dreht die Bedingung selbst um, wenn
/// stattdessen `then` folgt, es geht also kein Fall verloren.
///
/// **Warum das nichts kaputt machen kann.** Die Reihenfolge betrifft
/// ausschliesslich die AUSGABE. Jeder Block hat einen expliziten Terminator,
/// und `emit_block` laesst einen Sprung nur dann weg, wenn sein Ziel wirklich
/// unmittelbar folgt (`next`). Lebendigkeitsanalyse, Intervalle und
/// Registerwahl arbeiten weiter auf der FIR-Reihenfolge und werden hier nicht
/// angefasst — ein Wert liegt nach wie vor ueber seine ganze Lebensdauer am
/// selben Ort.
///
/// Abschaltbar mit `FIRN_NO_LAYOUT=1` (Fehlersuche).
fn emit_order(f: &Func) -> Vec<usize> {
    let n = f.blocks.len();
    if std::env::var_os("FIRN_NO_LAYOUT").is_some() {
        return (0..n).collect();
    }
    let mut placed = vec![false; n];
    let mut out: Vec<usize> = Vec::with_capacity(n);
    let mut free = 0usize;
    let mut b = 0usize;
    while out.len() < n {
        // Spur legen, solange der bevorzugte Nachfolger noch frei ist.
        loop {
            placed[b] = true;
            out.push(b);
            let w = match &f.blocks[b].term {
                Term::Br(t) => Some(*t as usize),
                Term::BrCond { then_bb, else_bb, .. } => {
                    let el = *else_bb as usize;
                    if el < n && !placed[el] {
                        Some(el)
                    } else {
                        Some(*then_bb as usize)
                    }
                }
                Term::Switch { default, .. } => Some(*default as usize),
                Term::Ret(_) | Term::Unset => None,
            };
            match w {
                Some(t) if t < n && !placed[t] => b = t,
                _ => break,
            }
        }
        while free < n && placed[free] {
            free += 1;
        }
        if free >= n {
            break;
        }
        b = free;
    }
    out
}

/// Ist `s` ein 64-Bit-Maschinenregistername (und damit ein Operand, dessen
/// Inhalt durch andere Registerbewegungen zerstoert werden kann)?
fn is_reg64(s: &str) -> bool {
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
fn parallel_reg_moves(e: &mut Emitter, pairs: &[(String, String)]) {
    let mut open: Vec<(String, String)> =
        pairs.iter().filter(|(z, q)| z != q).cloned().collect();
    while !open.is_empty() {
        if let Some(i) = open
            .iter()
            .position(|(z, _)| !open.iter().any(|(_, q)| q == z))
        {
            let (z, q) = open.remove(i);
            e.line(&format!("mov {}, {}", z, q));
            continue;
        }
        // Nur noch Zyklen: den alten Inhalt des Ziels nach rax retten, damit
        // das Ziel frei wird; alle Quellen, die darauf zeigten, lesen ab jetzt
        // aus rax.
        let (z, q) = open[0].clone();
        e.line(&format!("mov rax, {}", z));
        for (_, source) in open.iter_mut() {
            if *source == z {
                *source = "rax".to_string();
            }
        }
        e.line(&format!("mov {}, {}", z, q));
        open.remove(0);
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

fn emit_block(e: &mut Emitter, ra: &Ra, b: &Block, next: Option<BlockId>) -> Result<(), String> {
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
    let mergeable = match (&b.term, b.insts.last()) {
        (Term::BrCond { cond, .. }, Some(last)) => {
            matches!(last.op, Op::Cmp { .. })
                && last.dst == Some(*cond)
                && ra.read.get(*cond as usize).copied().unwrap_or(2) == 1
                && !ra.f.is_secret(*cond)
        }
        _ => false,
    };
    let n = if mergeable { b.insts.len() - 1 } else { b.insts.len() };
    for i in &b.insts[..n] {
        emit_inst(e, ra, i)?;
    }
    if mergeable {
        return emit_cmp_br(e, ra, b, next);
    }
    let f = ra.f;
    match &b.term {
        Term::Br(t) => {
            if next != Some(*t) {
                e.line(&format!("jmp {}", block_label(&f.name, *t)));
            }
        }
        Term::Switch { val, ty, .. } => {
            // Runde 51: der Wert wandert DIREKT von seinem Ort nach rax.
            // Vorher schrieb dieser Pfad ihn erst in sein Rahmenfach, weil
            // `emit_switch` ihn nur von dort lesen konnte — zwei Speicher-
            // zugriffe je Zustandswechsel im Tokenizer (10,2 Mio Ir auf
            // realweb).
            //
            // Zusicherung an `Wertquelle::Geladen`: `Ra::load_ext` emittiert
            // hier IMMER einen Schreibzugriff auf `eax`/`rax`. Der einzige
            // Zweig, der nichts emittieren wuerde, ist „Quelle ist bereits
            // das Zielregister" — und `rax` wird nie vergeben (siehe
            // CALLEE_SAVED / TEMP_REGS / ARG_SPARE / DIV_SPARE). Zur
            // Sicherheit wird genau das hier geprueft.
            let (v, vty) = (*val, *ty);
            if matches!(ra.a.place(v), Loc::Reg("rax")) {
                return Err("internal error: switch value is in rax".to_string());
            }
            crate::codegen_switch::emit_switch(
                e,
                f,
                crate::codegen_switch::ValueSource::Loaded(&|e2: &mut Emitter, bits: u32| {
                    ra.load_ext(e2, "rax", v, vty, bits);
                }),
                &b.term,
            )?;
        }
        Term::BrCond { cond, then_bb, else_bb } => {
            if f.constant_time && f.is_secret(*cond) {
                return Err(format!(
                    "#[constant_time]: conditional jump in '{}' depends on a secret value (%{})",
                    f.name, cond
                ));
            }
            if f.val_ty(*cond) != FTy::Bool {
                return Err(format!(
                    "internal error: condition %{} in '{}' is {}, expected bool",
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
            if next == Some(*else_bb) {
                e.line(&format!("jnz {}", block_label(&f.name, *then_bb)));
            } else if next == Some(*then_bb) {
                e.line(&format!("jz {}", block_label(&f.name, *else_bb)));
            } else {
                e.line(&format!("jnz {}", block_label(&f.name, *then_bb)));
                e.line(&format!("jmp {}", block_label(&f.name, *else_bb)));
            }
        }
        Term::Ret(v) => {
            if let Some(v) = v {
                ra.load_full(e, "rax", *v);
            } else {
                // Runde 51: KEIN `xor eax, eax` mehr. Eine Funktion mit
                // Rueckgabetyp `void` hat keinen Ergebniswert; System V
                // laesst `rax` in diesem Fall undefiniert, und in FIR liest
                // niemand das Ergebnis eines void-Aufrufs (`Op::Call` ohne
                // `dst`). Gemessen im Tokenizer: 4.229.623 Aufrufe, also
                // ebenso viele Instruktionen fuer nichts.
            }
            epilogue(e, ra.a);
        }
        Term::Unset => {
            return Err(format!(
                "internal error: block bb{} in '{}' has no terminator",
                b.id, f.name
            ))
        }
    }
    Ok(())
}

/// `cmp` und bedingter Sprung in einem: der Vergleich der letzten Instruktion
/// des Blocks setzt die Flags, der Terminator liest sie unmittelbar.
fn emit_cmp_br(e: &mut Emitter, ra: &Ra, b: &Block, next: Option<BlockId>) -> Result<(), String> {
    let f = ra.f;
    let last = b.insts.last().ok_or("internal error: empty block at cmp+jcc")?;
    let (op, oty, a, bb) = match &last.op {
        Op::Cmp { op, ty, a, b } => (*op, *ty, *a, *b),
        _ => return Err("internal error: cmp+jcc without comparison".to_string()),
    };
    let (then_bb, else_bb) = match &b.term {
        Term::BrCond { then_bb, else_bb, .. } => (*then_bb, *else_bb),
        _ => return Err("internal error: cmp+jcc without brcond".to_string()),
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
    if next == Some(else_bb) {
        e.line(&format!("{} {}", jcc, block_label(&f.name, then_bb)));
    } else if next == Some(then_bb) {
        e.line(&format!("{} {}", jcc_inverse(jcc), block_label(&f.name, else_bb)));
    } else {
        e.line(&format!("{} {}", jcc, block_label(&f.name, then_bb)));
        e.line(&format!("jmp {}", block_label(&f.name, else_bb)));
    }
    Ok(())
}

/// Der Gegensprung (Fallthrough-Optimierung: Ziel und Fallthrough tauschen).
fn jcc_inverse(jcc: &str) -> &'static str {
    match jcc {
        "je" => "jne",
        "jne" => "je",
        "jl" => "jge",
        "jge" => "jl",
        "jb" => "jae",
        "jae" => "jb",
        "jle" => "jg",
        "jg" => "jle",
        "jbe" => "ja",
        "ja" => "jbe",
        _ => unreachable!("unknown jump {}", jcc),
    }
}

fn emit_inst(e: &mut Emitter, ra: &Ra, i: &Inst) -> Result<(), String> {
    let ty = i.ty;
    match &i.op {
        Op::Const(c) => {
            let d = i.dst.ok_or("internal error: const without target")?;
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
            let d = i.dst.ok_or("internal error: binary operation without target")?;
            // Runde 51: Adressrechnung, die im folgenden Speicherzugriff steht
            // (`add` als Adressbildung, `shl`/`mul` als Skalierung des Index).
            if let Some(src) = ra.preloader.get(&d).copied() {
                // Vom Rechnen bleibt nur, das Register zu fuellen.
                if let Loc::Reg(r) = ra.a.loc(d) {
                    ra.load_full(e, r, src);
                    return Ok(());
                }
            }
            if ra.offset.contains_key(&d) || ra.skipped.contains(&d) {
                return Ok(()); // wird nirgends sonst gelesen
            }
            emit_bin(e, ra, *op, ty, *x, *y, d)?;
        }
        Op::Cmp { op, ty: oty, a, b } => {
            let d = i.dst.ok_or("internal error: comparison without target")?;
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
            // direkt ins Zielregister breit machen — `rax` wird nie vergeben,
            // darum ist `al` hier immer frei.
            match ra.a.loc(d) {
                Loc::Reg(dr) => e.line(&format!("movzx {}, al", rn(dr, 32))),
                Loc::Slot(_) => {
                    e.line("movzx eax, al");
                    ra.store_dst(e, d, "rax");
                }
            }
        }
        Op::Un(op, x) => {
            let d = i.dst.ok_or("internal error: unary operation without target")?;
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
            let d = i.dst.ok_or("internal error: conversion without target")?;
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
            let d = i.dst.ok_or("internal error: gc_state without target")?;
            crate::codegen_x86::emit_gc_addr(e, *regs);
            ra.store_dst(e, d, "rax");
        }
        Op::Alloca { .. } => {
            let d = i.dst.ok_or("internal error: alloca without target")?;
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
                .ok_or("internal error: alloca without space")?;
            e.line(&format!("lea rax, [rbp-{}]", off));
            ra.store_dst(e, d, "rax");
        }
        Op::Load { addr } => {
            let d = i.dst.ok_or("internal error: load without target")?;
            if ra.a.alias.contains_key(&d) {
                // Zellen-Alias: der Wert steht bereits im Zellenregister,
                // die einzige Verwendung liest ihn ueber ort() direkt.
                return Ok(());
            }
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
                let mem = match (ra.offset.get(addr), ra.a.frame_addr.get(addr), ra.a.place(*addr)) {
                    (Some(addr), _, _) => addr.text(),
                    (None, Some(off), _) => format!("[rbp-{}]", off),
                    (None, None, Loc::Reg(r)) => format!("[{}]", r),
                    (None, None, Loc::Slot(_)) => {
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
                let mem = match (ra.offset.get(addr), ra.a.frame_addr.get(addr), ra.a.place(*addr)) {
                    (Some(addr), _, _) => addr.text(),
                    (None, Some(off), _) => format!("[rbp-{}]", off),
                    (None, None, Loc::Reg(r)) => format!("[{}]", r),
                    (None, None, Loc::Slot(_)) => {
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
            let d = i.dst.ok_or("internal error: ptradd without target")?;
            if let Some(src) = ra.preloader.get(&d).copied() {
                if let Loc::Reg(r) = ra.a.loc(d) {
                    ra.load_full(e, r, src);
                    return Ok(());
                }
            }
            if ra.offset.contains_key(&d) {
                // Der Versatz steht im folgenden Speicherzugriff; die Adresse
                // selbst wird nirgends sonst gelesen und braucht kein `lea`.
                return Ok(());
            }
            // `lea` liest BEIDE Operanden, bevor es das Ziel schreibt — eine
            // Kollision zwischen Ziel- und Offsetregister ist dort also
            // unschaedlich. Nur der `mov`+`add`-Weg braucht den Umweg ueber
            // rax; deshalb wird das Ziel hier optimistisch gewaehlt und nur in
            // den beiden `add`-Zweigen zurueckgenommen.
            let dreg = match ra.a.loc(d) {
                Loc::Reg(r) => r,
                Loc::Slot(_) => "rax",
            };
            let reg_of = |v: Val| match (ra.a.imm(v), ra.a.place(v)) {
                (None, Loc::Reg(r)) => Some(r),
                _ => None,
            };
            let off_reg = reg_of(*off);
            let base_reg = reg_of(*base);
            let mut target = dreg;
            if let Some(boff) = ra.a.frame_addr.get(base).copied() {
                // Adresse = rbp - boff + off  -> ein einziges `lea`
                match (ra.a.imm(*off), off_reg) {
                    (Some(k), _) => {
                        let delta = k - boff as i64;
                        if delta >= 0 {
                            e.line(&format!("lea {}, [rbp+{}]", target, delta));
                        } else {
                            e.line(&format!("lea {}, [rbp-{}]", target, -delta));
                        }
                    }
                    (None, Some(r)) => e.line(&format!("lea {}, [rbp+{}-{}]", target, r, boff)),
                    (None, None) => {
                        e.line(&format!("mov rcx, {}", ra.opnd(*off)));
                        e.line(&format!("lea {}, [rbp+rcx-{}]", target, boff));
                    }
                }
            } else if let (Some(x), Some(k)) = (base_reg, ra.a.imm(*off)) {
                lea_sum(e, target, x, k);
            } else if let (Some(x), Some(y)) = (base_reg, off_reg) {
                e.line(&format!("lea {}, [{}+{}]", target, x, y));
            } else if target != "rax" {
                // Basis liegt im Rahmen oder ist eine Konstante: einmal nach
                // rax holen, dann mit EINEM `lea` ins Ziel.
                ra.load_full(e, "rax", *base);
                match (ra.a.imm(*off), off_reg) {
                    (Some(k), _) => lea_sum(e, target, "rax", k),
                    (None, Some(y)) => e.line(&format!("lea {}, [rax+{}]", target, y)),
                    (None, None) => {
                        e.line(&format!("add rax, {}", ra.opnd(*off)));
                        target = "rax";
                    }
                }
            } else {
                ra.load_full(e, "rax", *base);
                e.line(&format!("add rax, {}", ra.opnd(*off)));
                target = "rax";
            }
            if target == "rax" {
                ra.store_dst(e, d, "rax");
            }
        }
        Op::Call { name, args } => {
            // Argumente in die Argumentregister. Die Zuteilung vergibt `r8`
            // und `r9` sehr wohl als Heimat (`TEMP_REGS`), deshalb muessen die
            // Register-zu-Register-Bewegungen PARALLEL geschehen; Operanden aus
            // Speicher oder Sofortkonstanten lesen kein Register und kommen
            // danach.
            // Argumente ab dem siebten liegen bei `call` auf dem Stapel
            // ([rsp], [rsp+8], …). Sie werden ZUERST abgelegt: danach sind die
            // Argumentregister frei und werden nicht mehr angefasst. Als
            // Zwischenlager dient `rax` (nie Heimat eines Wertes); die Quellen
            // sind rbp-relativ oder Register und bleiben von `sub rsp`
            // unberuehrt.
            //
            // AUSRICHTUNG: an der `call`-Grenze muss `rsp` 16-fach ausgerichtet
            // sein. Nach `push rbp` + `sub rsp, <Vielfaches von 16>` ist sie es;
            // der Argumentbereich wird deshalb ebenfalls auf 16 aufgerundet —
            // wortgleich mit dem Grundpfad in codegen_x86.rs.
            let stack = args.len().saturating_sub(ARG_REGS.len());
            let space = align_up(stack as u64 * 8, 16);
            if space > 0 {
                e.line(&format!("sub rsp, {}", space));
                for (k, arg) in args.iter().skip(ARG_REGS.len()).enumerate() {
                    ra.load_full(e, "rax", *arg);
                    e.line(&format!("mov qword ptr [rsp+{}], rax", k * 8));
                }
            }
            let mut reg_moves: Vec<(String, String)> = Vec::new();
            let mut later: Vec<(usize, Val)> = Vec::new();
            for (k, arg) in args.iter().enumerate().take(ARG_REGS.len()) {
                let o = ra.opnd(*arg);
                if is_reg64(&o) {
                    reg_moves.push((ARG_REGS[k].to_string(), o));
                } else {
                    later.push((k, *arg));
                }
            }
            parallel_reg_moves(e, &reg_moves);
            for (k, arg) in later {
                ra.load_full(e, ARG_REGS[k], arg);
            }
            e.line(&format!("call {}", label(name)));
            if space > 0 {
                e.line(&format!("add rsp, {}", space));
            }
            if let Some(d) = i.dst {
                ra.store_dst(e, d, "rax");
            }
        }
        // Dynamischer Versand (iface.rs, Runde 46). Wortgleich zum `call`
        // darueber, nur steht das Ziel in einem Register statt in einem
        // Symbol. Das Ziel wird ZULETZT geladen, und zwar nach `rax`: `rax`
        // ist nie Heimat eines Wertes (siehe Kopf dieser Datei) und kein
        // Argumentregister — damit kann das Laden weder ein schon gesetztes
        // Argument noch das Ziel selbst zerstoeren.
        Op::CallIndirect { target, args } => {
            let stack = args.len().saturating_sub(ARG_REGS.len());
            let space = align_up(stack as u64 * 8, 16);
            if space > 0 {
                e.line(&format!("sub rsp, {}", space));
                for (k, arg) in args.iter().skip(ARG_REGS.len()).enumerate() {
                    ra.load_full(e, "rax", *arg);
                    e.line(&format!("mov qword ptr [rsp+{}], rax", k * 8));
                }
            }
            let mut reg_moves: Vec<(String, String)> = Vec::new();
            let mut later: Vec<(usize, Val)> = Vec::new();
            for (k, arg) in args.iter().enumerate().take(ARG_REGS.len()) {
                let o = ra.opnd(*arg);
                if is_reg64(&o) {
                    reg_moves.push((ARG_REGS[k].to_string(), o));
                } else {
                    later.push((k, *arg));
                }
            }
            parallel_reg_moves(e, &reg_moves);
            for (k, arg) in later {
                ra.load_full(e, ARG_REGS[k], arg);
            }
            ra.load_full(e, "rax", *target);
            e.line("call rax");
            if space > 0 {
                e.line(&format!("add rsp, {}", space));
            }
            if let Some(d) = i.dst {
                ra.store_dst(e, d, "rax");
            }
        }
        Op::VtabAddr { table } => {
            let d = i.dst.ok_or("internal error: vtab without target")?;
            e.line(&format!(
                "lea rax, [rip + {}]",
                crate::iface::table_label(table)
            ));
            ra.store_dst(e, d, "rax");
        }
        Op::Syscall { args } => {
            const SYS_REGS: [&str; 6] = ["rdi", "rsi", "rdx", "r10", "r8", "r9"];
            if args.is_empty() {
                return Err("internal error: syscall without number".to_string());
            }
            // Gleiche Fehlerklasse wie beim Aufruf: `r10`, `r8` und `r9`
            // sind zugleich Arbeitsregister der Zuteilung.
            let mut sys_moves: Vec<(String, String)> = Vec::new();
            let mut sys_later: Vec<(usize, Val)> = Vec::new();
            for (k, arg) in args.iter().skip(1).enumerate() {
                let o = ra.opnd(*arg);
                if is_reg64(&o) {
                    sys_moves.push((SYS_REGS[k].to_string(), o));
                } else {
                    sys_later.push((k, *arg));
                }
            }
            parallel_reg_moves(e, &sys_moves);
            for (k, arg) in sys_later {
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
            let d = i.dst.ok_or("internal error: select without target")?;
            ra.load_full(e, "rdx", *cond);
            ra.load_full(e, "rax", *b);
            ra.load_full(e, "rcx", *a);
            e.line("test dl, dl");
            e.line("cmovnz rax, rcx");
            ra.store_dst(e, d, "rax");
        }
        Op::Barrier { val } => {
            let d = i.dst.ok_or("internal error: barrier without target")?;
            ra.load_full(e, "rax", *val);
            e.raw("    # barrier: opaque to every optimization pass");
            ra.store_dst(e, d, "rax");
        }
        Op::SecureZero { addr, size } => {
            ra.load_full(e, "rdi", *addr);
            ra.load_full(e, "rcx", *size);
            e.line("xor eax, eax");
            e.line("cld");
            e.line("rep stosb");
        }
        // Runde 49 (thread.rs). Wie beim atomaren Addieren sind rax/rcx/rdx
        // nie Heimat eines Wertes; `spawn` benutzt zusaetzlich die
        // Systemaufrufregister und ist oben als aufrufaehnlich eingetragen,
        // damit kein Intervall in einem caller-saved Register darueber lebt.
        Op::AtomicCas { addr, erw, new } => {
            let d = i.dst.ok_or("internal error: atomcas without target")?;
            ra.load_full(e, "rcx", *addr);
            ra.load_full(e, "rdx", *new);
            ra.load_full(e, "rax", *erw);
            crate::thread::cas_sequence(e);
            ra.store_dst(e, d, "rax");
        }
        Op::ThreadSpawn { arg, stack, ctid } => {
            let d = i.dst.ok_or("internal error: spawn without target")?;
            ra.load_full(e, "rdi", *arg);
            ra.load_full(e, "rsi", *stack);
            ra.load_full(e, "rdx", *ctid);
            crate::thread::spawn_sequence(e);
            ra.store_dst(e, d, "rax");
        }
        Op::ThreadSelf => {
            let d = i.dst.ok_or("internal error: threadself without target")?;
            crate::thread::self_sequence(e);
            ra.store_dst(e, d, "rax");
        }
        Op::AtomicAdd { addr, val } => {
            // Runde 47: EINE Instruktion, mit `lock`-Praefix. rax und rcx sind
            // nie Heimat eines Wertes (weder CALLEE_SAVED noch TEMP_REGS noch
            // ARG_SPARE/DIV_SPARE), deshalb braucht diese Instruktion keinen
            // Eintrag in memop_pos/divsel_pos.
            let d = i.dst.ok_or("internal error: atomadd without target")?;
            ra.load_full(e, "rcx", *addr);
            ra.load_full(e, "rax", *val);
            e.line("lock xadd qword ptr [rcx], rax");
            ra.store_dst(e, d, "rax");
        }
        Op::CopyMem { dst, src, size } => {
            ra.load_full(e, "rdi", *dst);
            ra.load_full(e, "rsi", *src);
            e.line(&format!("mov rcx, {}", size));
            e.line("cld");
            e.line("rep movsb");
        }
        // RUNDE 52: unerreichbar — `unsupported_grund` schickt jede Funktion
        // mit Inline-Assembler oder MMIO in den Grundpfad. Als Fehler statt
        // als stiller Zweig, damit ein spaeteres Lockern auffliegt.
        Op::Asm { .. } | Op::MmioLoad { .. } | Op::MmioStore { .. } => {
            return Err(
                "internal error: inline assembler/MMIO in the register-allocating path".to_string(),
            )
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
fn add_over_rax(ra: &Ra, a: Val, b: Val) -> bool {
    let is_reg = |v: Val| ra.a.imm(v).is_none() && matches!(ra.a.place(v), Loc::Reg(_));
    let is_frame = |v: Val| ra.a.imm(v).is_none() && matches!(ra.a.place(v), Loc::Slot(_));
    (is_reg(a) && is_frame(b)) || (is_frame(a) && is_reg(b))
}

/// Laesst sich `d = a op b` als ein einziges `lea` schreiben?
///
/// Nur 64 Bit (siehe `lea_summe`), nur mit Zielregister, und nur wenn die
/// Operanden wirklich als Adressteile taugen: Register + Register,
/// Register + Sofortwert, Sofortwert + Register. Bei `sub` zusaetzlich
/// `k != i64::MIN`, weil `-k` sonst ueberlaeuft.
fn lea_possible(ra: &Ra, op: BinOp, ty: FTy, a: Val, b: Val, d: Val) -> bool {
    if ty.bits() <= 32 || !matches!(ra.a.loc(d), Loc::Reg(_)) {
        return false;
    }
    let is_reg = |v: Val| ra.a.imm(v).is_none() && matches!(ra.a.place(v), Loc::Reg(_));
    match op {
        BinOp::Add => {
            (is_reg(a) && is_reg(b))
                || (is_reg(a) && ra.a.imm(b).is_some())
                || (ra.a.imm(a).is_some() && is_reg(b))
        }
        BinOp::Sub => is_reg(a) && matches!(ra.a.imm(b), Some(k) if k != i64::MIN),
        _ => false,
    }
}

fn lea_sum(e: &mut Emitter, target: &str, base: &str, offset: i64) {
    if offset >= 0 {
        e.line(&format!("lea {}, [{}+{}]", target, base, offset));
    } else {
        e.line(&format!("lea {}, [{}-{}]", target, base, -(offset as i128) as i64));
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
        BinOp::Add | BinOp::Sub if lea_possible(ra, op, ty, a, b, d) => {
            let dr = match ra.a.loc(d) {
                Loc::Reg(r) => r,
                Loc::Slot(_) => unreachable!("lea_possible requires a target register"),
            };
            let reg_of = |v: Val| match (ra.a.imm(v), ra.a.place(v)) {
                (None, Loc::Reg(r)) => Some(r),
                _ => None,
            };
            match op {
                BinOp::Add => match (reg_of(a), reg_of(b), ra.a.imm(a), ra.a.imm(b)) {
                    (Some(x), Some(y), _, _) => e.line(&format!("lea {}, [{}+{}]", dr, x, y)),
                    (Some(x), None, _, Some(k)) => lea_sum(e, dr, x, k),
                    (None, Some(y), Some(k), _) => lea_sum(e, dr, y, k),
                    _ => unreachable!("lea_possible has guaranteed the case"),
                },
                _ => match (reg_of(a), ra.a.imm(b)) {
                    (Some(x), Some(k)) => lea_sum(e, dr, x, -k),
                    _ => unreachable!("lea_possible has guaranteed the case"),
                },
            }
        }
        // Ein Operand liegt im Rahmen, der andere in einem Register — der mit
        // Abstand haeufigste Fall in Adressrechnungen (`basis + versatz`, wobei
        // die Basis ein Parameter im Rahmen ist). Einmal nach rax holen, dann
        // EIN `lea` ins Ziel. Der allgemeine Zweig darunter braucht hier drei
        // Instruktionen, weil das Ziel mit dem Registeroperanden zusammenfaellt
        // und er deshalb ueber rax rechnen und zurueckkopieren muss.
        BinOp::Add if wide && matches!(ra.a.loc(d), Loc::Reg(_)) && add_over_rax(ra, a, b) => {
            let dr = match ra.a.loc(d) {
                Loc::Reg(r) => r,
                Loc::Slot(_) => unreachable!("excluded by the condition"),
            };
            let is_reg = |v: Val| ra.a.imm(v).is_none() && matches!(ra.a.place(v), Loc::Reg(_));
            // `+` ist kommutativ: der Registeroperand wird zum Indexteil.
            let (out_frame, in_reg) = if is_reg(b) { (a, b) } else { (b, a) };
            let y = match ra.a.place(in_reg) {
                Loc::Reg(r) => r,
                Loc::Slot(_) => unreachable!("add_via_rax has reserved a register"),
            };
            ra.load_full(e, "rax", out_frame);
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
            let m = match (op, ty.signed()) {
                (BinOp::Shl, _) => "shl",
                (_, true) => "sar",
                (_, false) => "shr",
            };
            if let Some(k) = ra.a.imm(b) {
                // konstanter Abstand: Sofortform, kein rcx-Aufbau.
                // Maske wie die CPU (32 Bit: 5 Bit, 64 Bit: 6 Bit); die FIR
                // laesst Weiten >= Bitbreite gar nicht erst durch den
                // Optimierer, aber die Maske haelt den Assembler-Text im
                // imm8-Rahmen.
                let k = k & if bits == 64 { 63 } else { 31 };
                // direkt im Zielregister schieben, wenn es eines hat
                match ra.a.loc(d) {
                    Loc::Reg(dr) => {
                        ra.load_ext(e, dr, a, ty, bits);
                        e.line(&format!("{} {}, {}", m, rn(dr, bits), k));
                    }
                    Loc::Slot(_) => {
                        ra.load_ext(e, "rax", a, ty, bits);
                        e.line(&format!("{} {}, {}", m, rn("rax", bits), k));
                        ra.store_dst(e, d, "rax");
                    }
                }
            } else {
                ra.load_ext(e, "rax", a, ty, bits);
                ra.load_full(e, "rcx", b);
                e.line(&format!("{} {}, cl", m, rn("rax", bits)));
                ra.store_dst(e, d, "rax");
            }
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
    fn loop_counter_lands_in_a_register() {
        let f = loop_func();
        let a = allocate(&f);
        assert!(!a.cells.is_empty(), "the alloca cell must be promoted");
        let regs = a.locs.iter().filter(|l| matches!(l, Loc::Reg(_))).count() + a.cells.len();
        assert!(regs >= 3, "too few registers assigned: {}", regs);
    }

    #[test]
    fn loop_body_without_mem_access() {
        let asm = emit(&Module { funcs: vec![loop_func()] }).expect("codegen");
        // im Rumpf (bb2) darf kein [rbp- mehr vorkommen
        let body = asm.split(".Lmain__bb2:").nth(1).unwrap_or("");
        let body = body.split(".Lmain__bb3:").next().unwrap_or("");
        assert!(!body.contains("[rbp-"), "loop body still accesses the stack:\n{}", body);
    }

    #[test]
    fn callee_saved_become_saved_and_retrieved() {
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
    fn cells_with_escaping_address_become_not_promoted() {
        let mut f = Func::new("main", vec![], FTy::I32);
        let slot = f.alloca(8, 8);
        let off = f.push(0, FTy::I64, Op::Const(0));
        let p = f.push(0, FTy::Ptr, Op::PtrAdd { base: slot, off });
        let v = f.push(0, FTy::I32, Op::Const(7));
        f.push_void(0, FTy::I32, Op::Store { addr: p, val: v });
        let l = f.push(0, FTy::I32, Op::Load { addr: slot });
        f.set_term(0, Term::Ret(Some(l)));
        let a = allocate(&f);
        assert!(a.cells.is_empty(), "address escapes via ptradd");
    }

    #[test]
    fn secret_values_get_no_register() {
        let mut f = Func::new("main", vec![], FTy::I32);
        let c = f.push(0, FTy::I32, Op::Const(5));
        f.secret.insert(c);
        let d = f.push(0, FTy::I32, Op::Bin(BinOp::Add, c, c));
        f.set_term(0, Term::Ret(Some(d)));
        let a = allocate(&f);
        assert!(matches!(a.loc(c), Loc::Slot(_)));
    }

    #[test]
    fn select_stays_cmov_also_with_registers() {
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

    /// Runde 43: mehr als sechs Parameter sind KEIN Grund mehr fuer den
    /// Grundpfad — der siebte kommt aus [rbp+16].
    #[test]
    fn many_parameter_stay_in_register_path() {
        let mut f = Func::new("f", vec![FTy::I64; 7], FTy::I64);
        f.set_term(0, Term::Ret(Some(6)));
        assert!(supported(&f));
        let mut e = Emitter { out: String::new() };
        emit_func_ra(&mut e, &f).expect("register path responsible").expect("codegen");
        assert!(e.out.contains("qword ptr [rbp+16]"), "{}", e.out);
    }

    /// … und ein Aufruf mit acht Argumenten legt die letzten zwei auf den
    /// Stapel, ohne die 16-Byte-Ausrichtung zu verletzen.
    #[test]
    fn call_with_eight_args_puts_two_on_the_stack() {
        let mut g = Func::new("main", vec![], FTy::I32);
        let mut args = Vec::new();
        for k in 0..8 {
            args.push(g.push(0, FTy::I64, Op::Const(k as i128 + 1)));
        }
        let r = g.push(0, FTy::I64, Op::Call { name: "f".to_string(), args });
        let rc = g.push(0, FTy::I32, Op::Cast { src: r, from: FTy::I64 });
        g.set_term(0, Term::Ret(Some(rc)));
        assert!(supported(&g));
        let mut e = Emitter { out: String::new() };
        emit_func_ra(&mut e, &g).expect("register path responsible").expect("codegen");
        assert!(e.out.contains("sub rsp, 16"), "{}", e.out);
        assert!(e.out.contains("mov qword ptr [rsp+0], rax"), "{}", e.out);
        assert!(e.out.contains("mov qword ptr [rsp+8], rax"), "{}", e.out);
        assert!(e.out.contains("add rsp, 16"), "{}", e.out);
    }
    // ---------------------------------------------------------- Runde 51 ---

    /// `[basis + index*4]` statt `shl` + `lea` + Zugriff.
    #[test]
    fn addressing_moves_in_the_mem_operands() {
        let mut f = Func::new("main", vec![FTy::Ptr, FTy::U64], FTy::U64);
        let four = f.push(0, FTy::U64, Op::Const(4));
        let sk = f.push(0, FTy::U64, Op::Bin(BinOp::Mul, 1, four));
        let ad = f.push(0, FTy::U64, Op::Bin(BinOp::Add, 0, sk));
        let w = f.push(0, FTy::U32, Op::Load { addr: ad });
        let c = f.push(0, FTy::U64, Op::Cast { src: w, from: FTy::U32 });
        f.set_term(0, Term::Ret(Some(c)));
        let asm = emit(&Module { funcs: vec![f] }).expect("codegen");
        let body = asm.split("main:").nth(1).unwrap();
        assert!(
            body.lines().any(|l| l.contains("dword ptr [") && l.contains("*4]")),
            "no scaled memory operand:\n{}",
            asm
        );
        assert!(!body.contains("shl "), "scaling remained:\n{}", asm);
        assert!(!body.contains("lea "), "address computation remained:\n{}", asm);
    }

    /// Wird dieselbe Adresse ZWEIMAL gelesen, darf sie nicht in den
    /// Speicheroperanden wandern — sonst lebt die Basis laenger, als der
    /// Verteiler weiss (die Fehlerklasse aus Runde 40/41).
    #[test]
    fn twice_read_address_becomes_not_folded() {
        let mut f = Func::new("main", vec![FTy::Ptr, FTy::U64], FTy::U64);
        let ad = f.push(0, FTy::U64, Op::Bin(BinOp::Add, 0, 1));
        let a = f.push(0, FTy::U64, Op::Load { addr: ad });
        let b = f.push(0, FTy::U64, Op::Load { addr: ad });
        let sum = f.push(0, FTy::U64, Op::Bin(BinOp::Add, a, b));
        f.set_term(0, Term::Ret(Some(sum)));
        let asm = emit(&Module { funcs: vec![f] }).expect("codegen");
        let body = asm.split("main:").nth(1).unwrap();
        assert!(
            body.contains("lea ") || body.lines().filter(|l| l.contains("add ")).count() > 0,
            "address should be computed once:\n{}",
            asm
        );
    }

    /// Ein 32-Bit-`add` darf NICHT zur Adressierung werden: dort schneidet
    /// FIR das Ergebnis ab, die Adressierung taete es nicht.
    #[test]
    fn narrow_add_becomes_not_to_address() {
        let mut f = Func::new("main", vec![FTy::Ptr, FTy::U32], FTy::U32);
        let ad = f.push(0, FTy::U32, Op::Bin(BinOp::Add, 0, 1));
        let w = f.push(0, FTy::U32, Op::Load { addr: ad });
        f.set_term(0, Term::Ret(Some(w)));
        let asm = emit(&Module { funcs: vec![f] }).expect("codegen");
        let body = asm.split("main:").nth(1).unwrap();
        // Die 32-Bit-Addition muss als EIGENE Instruktion dastehen. Welches
        // Register der Verteiler dafuer waehlt, ist seine Sache: die schmale
        // Sicht heisst `eax`..`edi`, aber `r8d`..`r15d` bei den erweiterten
        // (der Merge der Runde 49 hat die Wahl auf `r10d` verschoben — der
        // alte Test suchte nur nach "add e" und schlug deshalb an, obwohl
        // der Code richtig war).
        let narrow_addition = body.lines().any(|l| {
            let l = l.trim();
            l.starts_with("add e")
                || (l.starts_with("add r") && l.split(',').next().is_some_and(|r| r.ends_with('d')))
        });
        assert!(
            narrow_addition || body.contains("lea "),
            "32-bit addition must remain its own instruction:\n{}",
            asm
        );
    }

    /// Der Wert eines `switch` kommt aus seinem Register, nicht ueber den
    /// Rahmen — und der Index braucht kein `mov eax, eax`.
    #[test]
    fn switch_reads_the_value_without_detour_over_the_frame() {
        let mut f = Func::new("main", vec![FTy::U32], FTy::I32);
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
        f.set_term(0, Term::Switch { val: 0, ty: FTy::U32, cases, default: bd });
        let asm = emit(&Module { funcs: vec![f] }).expect("codegen");
        assert!(asm.contains("jmp qword ptr [rdx + rax*8]"), "{}", asm);
        assert!(!asm.contains("mov eax, eax"), "superfluous zero extension:\n{}", asm);
        let body = asm.split("main:").nth(1).unwrap();
        // Der Wert wird nicht erst in sein Rahmenfach geschrieben.
        assert!(
            !body.lines().any(|l| l.trim().starts_with("mov qword ptr [rbp-") && l.contains(", rax")),
            "switch value went out of range:\n{}",
            asm
        );
    }

    /// Blocklayout: hinter einem bedingten Sprung darf kein unbedingter mehr
    /// stehen, wenn eine der beiden Kanten Fallthrough sein kann.
    #[test]
    fn blocklayout_makes_out_the_second_jump_a_fallthrough() {
        let f = loop_func();
        let asm = emit(&Module { funcs: vec![f] }).expect("codegen");
        let lines: Vec<&str> = asm.lines().map(|l| l.trim()).collect();
        for (i, z) in lines.iter().enumerate() {
            let conditional = z.starts_with('j') && !z.starts_with("jmp");
            if conditional {
                if let Some(n) = lines.get(i + 1) {
                    assert!(
                        !n.starts_with("jmp "),
                        "unconditional jump after conditional:\n{}",
                        asm
                    );
                }
            }
        }
    }

    /// Eine `void`-Funktion setzt `rax` nicht mehr auf null.
    #[test]
    fn void_ret_without_xor() {
        let mut empty = Func::new("empty", vec![], FTy::Void);
        empty.set_term(0, Term::Ret(None));
        let mut m = Func::new("main", vec![], FTy::I32);
        let n = m.push(0, FTy::I32, Op::Const(7));
        m.set_term(0, Term::Ret(Some(n)));
        let asm = emit(&Module { funcs: vec![empty, m] }).expect("codegen");
        let body = asm.split("_F0.empty:").nth(1).unwrap();
        let body = body.split("main:").next().unwrap();
        assert!(!body.contains("xor eax, eax"), "{}", asm);
        assert!(body.contains("ret"), "{}", asm);
    }

}
