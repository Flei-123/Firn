// SPDX-License-Identifier: GPL-2.0-only
//! **Round BILLIG — register allocation for aarch64.**
//!
//! Up to this round `codegen_a64.rs` gave every FIR value its own eight-byte
//! slot in the frame and computed in `x9`–`x15`. Its own header says what
//! that is worth: *"That is not fast, and it is not meant to be — it is
//! checkable."* On x86 the linear scan of `regalloc.rs` (round 43) bought
//! **−26 % executed instructions and about +80 % throughput**. On ARM64 —
//! Justin's phone — that gain has been lying on the floor since round 80,
//! and it applies to the WHOLE browser, not only to JavaScript.
//!
//! ## Why this is a new file and not a second target inside `regalloc.rs`
//!
//! `regalloc.rs` is 5005 lines. Counted by function boundaries, about 1580
//! of them are the analysis (liveness, intervals, loop depth, promotable
//! cells, linear scan) and about 3000 are an **x86 assembler emitter** —
//! `rn()`, `Address::text()`, `emit_inst`, `emit_bin`, `emit_div_const`,
//! `descriptor_peephole`. The analysis is machine independent; the emitter
//! is not, and A64 already has one. So the useful half gets reused
//! (`regalloc::compute_live`, `loop_depth`, `promotable_cells`) and only
//! the register description and the scan are written here.
//!
//! ## The register set, and why it is exactly six
//!
//! AAPCS64 gives ten callee-saved general purpose registers, `x19`–`x28`.
//! Six of them are handed out here, `x19`–`x24`, and the reason is the
//! collector and not the ABI:
//!
//! > `codegen_a64::emit_gc_addr` writes **x19–x24** into the state block of
//! > the collector at every `Op::GcAddr { regs: true }` site, because
//! > SPEC §3.5.3 promises a conservative scan over stack **and registers**.
//! > The block has room for six words (`gc::REG_SAVE_OFF`, the same six the
//! > x86 side fills with `rbx`, `rbp`, `r12`–`r15`).
//!
//! A pointer in `x25`–`x28` would be invisible to that scan, the collector
//! would free an object that is still live, and the mistake would show up
//! as a corrupted heap somewhere else entirely. Six registers that are
//! already scanned are worth more than ten that are not. Widening the block
//! is a separate, deliberate change to three files (`gc.rs`, the runtime
//! text and `codegen_x86.rs`) and is not smuggled in here.
//!
//! Everything the scan hands out is callee-saved, so **no interval needs a
//! crossing analysis**: `bl`, `svc` and every helper sequence of this
//! backend leave `x19`–`x24` alone. That is the one place where ARM64 is
//! genuinely simpler than x86, where `r11`/`r10`/`rsi`/`rdi`/`rdx` may only
//! be handed to intervals that cross nothing (`regalloc.rs::exact_crossings`,
//! 90 lines of round 87).
//!
//! ## What deliberately does NOT get a register
//!
//! * `f32`/`f64`/`v128` values — they live in `d`/`q` registers and this
//!   round does not touch the floating point path.
//! * `secret` values (SPEC §9.2) — they stay in memory so that
//!   `SecureZero` can reach them.
//! * every value of a function with `#[interrupt]` — `INT_SAVE_A64` saves
//!   `x0`–`x18` and `x30` and relies on the statement *"x19–x28 are
//!   callee-saved and this backend never hands them out"*. This round makes
//!   that statement false, so interrupt handlers keep the old path.

use crate::fir::{FTy, Func, Inst, Op, Term, Val};
use std::collections::HashMap;

/// The registers handed out. Callee-saved (AAPCS64) AND covered by the
/// register save of `emit_gc_addr` — see the header.
pub(crate) const A64_POOL: [&str; 6] = ["x19", "x20", "x21", "x22", "x23", "x24"];

/// The result of the allocation. Empty (`!on`) means: everything as before.
#[derive(Default)]
pub(crate) struct RaA64 {
    /// ordinary value -> register
    pub regs: HashMap<Val, &'static str>,
    /// promoted `alloca` -> (register, access width)
    pub cells: HashMap<Val, (&'static str, FTy)>,
    /// which registers the prologue has to save (sorted, as in `A64_POOL`)
    pub saved: Vec<&'static str>,
    /// RUNDE BILLIG, DER BEFUND MIT DEM SAMMLER -- je `Op::GcAddr
    /// { regs: true }` (Wert des Ziels als Schluessel) die Register aus
    /// `saved`, die dort NICHTS LEBENDES mehr halten. `emit_gc_addr`
    /// schreibt fuer sie `xzr` in den Zustandsblock statt des Registers.
    ///
    /// WARUM ES DAS BRAUCHT, und es ist der Fund dieser Runde:
    /// `tests/901_dom_tree_gc.fi` ging mit `--no-opt` von 0 auf 3 --
    /// „nach dem Einsammeln sind noch zu viele Objekte am Leben". Kein
    /// falscher Code, sondern ein STAENDERER WURZELPUNKT: der Lauf des
    /// Sammlers ist konservativ, und ein toter Zeiger, der bis dahin in
    /// einem Rahmenplatz einer laengst zurueckgekehrten Funktion lag (also
    /// UNTER `sp` und damit ausserhalb des Laufs), liegt jetzt in einem
    /// aufrufergesicherten Register -- und das wird bei jedem Sicherungs-
    /// punkt mitgeschrieben. 4680 Knoten blieben so am Leben.
    ///
    /// Die Richtung ist sicher: geleert wird NUR ein Register aus `saved`
    /// (dessen Wert des Aufrufers also im eigenen Rahmen liegt und dort
    /// gefunden wird) und NUR dann, wenn kein Intervall die Stelle
    /// ueberdeckt. Intervalle sind die konvexe Huelle der Lebendigkeit,
    /// also eine OBERMENGE -- wer hier durchfaellt, ist wirklich tot.
    /// Erreicht der Datenfluss seinen Fixpunkt nicht, wird gar nichts
    /// geleert.
    pub safepoints: HashMap<Val, Vec<&'static str>>,
    /// RUNDE BILLIG, DIE ANDERE HAELFTE DESSELBEN BEFUNDS -- je Stelle
    /// (Block, Befehlsnummer) die Register, die DANACH mit `xzr`
    /// ueberschrieben werden, weil dort ein ZEIGER gestorben ist.
    ///
    /// Den Zustandsblock am Sicherungspunkt zu leeren reicht NICHT: der
    /// Vorspann JEDER gerufenen Funktion schreibt die aufrufergesicherten
    /// Register in ihren eigenen Rahmen, und dieser Rahmen liegt ueber `sp`
    /// und wird konservativ mitgelesen. Ein toter Zeiger in x19 pflanzt
    /// sich also die ganze Aufrufkette hinunter fort. Er muss dort
    /// verschwinden, wo er stirbt.
    ///
    /// Nur fuer `FTy::Ptr`. Eine tote Zahl in einem Register kostet den
    /// Sammler nichts (`__gc_block_of` findet fuer sie keinen Block); ein
    /// toter Zeiger kostet den ganzen daran haengenden Baum. Und nur, wenn
    /// das Register nicht sofort wieder gebraucht wird -- dann erledigt
    /// der naechste Schreiber es umsonst.
    pub clears: HashMap<(usize, usize), Vec<&'static str>>,
    /// RUNDE BILLIG -- values that are REBUILT instead of stored. See
    /// `cheap_const`: a constant that `imm_into` writes in ONE instruction
    /// costs nothing to make again, so it deserves neither a slot nor a
    /// register. Before this the constants of a function ate the register
    /// file: in the smoke test `summe` the four constants 0, 1, 2 and 3 held
    /// x20-x23 while the loop counter went to the stack.
    pub remat: HashMap<Val, i64>,
    /// is the register path active at all?
    pub on: bool,
}

impl RaA64 {
    /// Register of an ordinary value, if it has one.
    pub fn reg(&self, v: Val) -> Option<&'static str> {
        if !self.on {
            return None;
        }
        self.regs.get(&v).copied()
    }
    /// Die Register, die NACH diesem Befehl mit `xzr` zu ueberschreiben sind.
    pub fn clear_after(&self, bi: usize, ii: usize) -> &[&'static str] {
        if !self.on {
            return &[];
        }
        match self.clears.get(&(bi, ii)) {
            Some(v) => v.as_slice(),
            None => &[],
        }
    }
    /// Die Register, die an diesem Sicherungspunkt geleert werden duerfen.
    pub fn dead_at(&self, gcaddr: Val) -> &[&'static str] {
        if !self.on {
            return &[];
        }
        match self.safepoints.get(&gcaddr) {
            Some(v) => v.as_slice(),
            None => &[],
        }
    }
    /// The constant a value is, if it is one that gets rebuilt.
    pub fn imm(&self, v: Val) -> Option<i64> {
        if !self.on {
            return None;
        }
        self.remat.get(&v).copied()
    }
    /// Register and width of a promoted cell, if `addr` is one.
    pub fn cell(&self, addr: Val) -> Option<(&'static str, FTy)> {
        if !self.on {
            return None;
        }
        self.cells.get(&addr).copied()
    }
}

/// One live interval.
struct Iv {
    val: Val,
    start: usize,
    end: usize,
    weight: u64,
    is_cell: bool,
}

/// Does `codegen_a64::imm_into` write this value in exactly ONE
/// instruction? Then rebuilding it is never worse than reading it out of a
/// slot, and it is strictly better than holding a register for it.
///
/// The three one instruction forms, in the order `imm_into` tries them:
/// `mov r, xzr` (zero), `movz` (exactly one non-zero 16-bit chunk) and
/// `movn` (at most one chunk that is not 0xffff).
pub(crate) fn cheap_const(v: i64) -> bool {
    let u = v as u64;
    if u == 0 {
        return true;
    }
    let c = [u & 0xffff, (u >> 16) & 0xffff, (u >> 32) & 0xffff, (u >> 48) & 0xffff];
    let nz = c.iter().filter(|x| **x != 0).count();
    let nf = c.iter().filter(|x| **x != 0xffff).count();
    nz == 1 || nf <= 1
}

/// May a value of this type live in a general purpose register?
fn integral(t: FTy) -> bool {
    if t == FTy::Ptr && std::env::var_os("FIRN_A64_NO_PTR").is_some() {
        return false;
    }
    !matches!(t, FTy::F32 | FTy::F64 | FTy::V128 | FTy::Void)
}

/// Values whose address the emitter needs as a FRAME address rather than as
/// a value: none on A64 — `Op::Alloca` produces the address as an ordinary
/// value here. Kept as a function so the reason is written down once.
fn needs_frame_slot(f: &Func, v: Val) -> bool {
    f.is_secret(v)
}

pub(crate) fn allocate(f: &Func) -> RaA64 {
    let mut ra = RaA64::default();
    // Schalter zum Eingrenzen -- dasselbe Mittel, das die x86-Seite mit
    // `FIRN_RA_STATS`/`FIRN_RA_ROUGH` hat. Wer einen Fehler sucht, will
    // wissen, WELCHE der drei Stufen ihn macht, ohne uebersetzen zu muessen.
    if std::env::var_os("FIRN_A64_RA_OFF").is_some() {
        return ra;
    }
    if f.interrupt {
        return ra; // see the header: INT_SAVE_A64 does not save x19-x24
    }
    let nv = f.val_types.len();
    let nb = f.blocks.len();
    if nv == 0 || nb == 0 || nv.saturating_mul(nb) > 8_000_000 {
        return ra;
    }
    // The linear numbering of `compute_live` assumes block i has id i — the
    // same precondition `regalloc::allocate` checks.
    if f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return ra;
    }
    // AN `asm` BLOCK ENDS IT. The A64 inline assembler (round
    // ARM-FREESTANDING) lets the programmer name ANY register, x19-x28
    // included, as an operand or in the clobber list -- `core.rs:184` lists
    // them as legal names. A value of ours in such a register would be
    // destroyed without a trace. The x86 path draws the same line
    // (`regalloc.rs::unsupported_basic`); here it is drawn in one place
    // because A64 has only this one door.
    if f.blocks.iter().any(|b| b.insts.iter().any(|i| matches!(i.op, crate::fir::Op::Asm { .. }))) {
        return ra;
    }

    // ---- the constants that get rebuilt --------------------------------
    // ROUND 92 (x86, `immediate_consts`) states the precondition and it holds
    // here word for word: only a value with EXACTLY ONE definition IS the
    // constant its `const` instruction names. After `phi.rs` a value can be
    // written from several blocks.
    let mut defs: HashMap<Val, u32> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let Some(d) = i.dst {
                *defs.entry(d).or_insert(0) += 1;
            }
        }
    }
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Const(c)) = (i.dst, &i.op) {
                if defs.get(&d).copied().unwrap_or(0) != 1 || !integral(i.ty) || f.is_secret(d) {
                    continue;
                }
                let val = i.ty.truncate(*c) as i64;
                if cheap_const(val) && std::env::var_os("FIRN_A64_NO_REMAT").is_none() {
                    ra.remat.insert(d, val);
                }
            }
        }
    }

    let live = crate::regalloc::compute_live(f);
    let depth = crate::regalloc::loop_depth(f);
    // DIE BEFOERDERTE ZELLE BLEIBT AUS, UND DAS IST DER ZWEITE BEFUND
    // DIESER RUNDE -- gemessen, nicht befuerchtet.
    //
    // Eine befoerderte Zelle ist eine oertliche Groesse, die ihre GANZE
    // Funktion lang in einem aufrufergesicherten Register wohnt
    // (Intervall [0, letzter Zugriff], die Regel aus Runde 90). Und jedes
    // dieser Register schreibt `emit_gc_addr` bei JEDEM Sicherungspunkt in
    // den Zustandsblock des Sammlers, weil der Lauf konservativ ist. Eine
    // tote Baumwurzel in so einer Zelle haelt damit ihren ganzen Baum fest.
    //
    // Gemessen an `tests/901_dom_tree_gc.fi` mit `--no-opt` (die Stufe
    // ohne mem2reg, in der JEDE oertliche Groesse eine Zelle ist),
    // zurueckgehalten nach einem Einsammeln, das alles freigeben muesste:
    //
    //     mit Zellen      4600 von 4680 Knoten
    //     ohne Zellen        0
    //
    // Und es liegt NICHT am Zeigertyp: `Gc[T]` ist in FIR eine Zahl, kein
    // `FTy::Ptr` -- die Probe, nur `Ptr` auszuschliessen, hielt dieselben
    // 4600 Objekte fest. FIR kann eine Zahl, die ein Zeiger ist, gar nicht
    // von einer Zahl unterscheiden; das ist der Preis eines konservativen
    // Sammlers und keine Nachlaessigkeit.
    //
    // Auf x86 faellt derselbe Mechanismus weniger auf, weil dort vier
    // Registervorraete existieren und nur EINER (rbx/rbp/r12-r15) im
    // Zustandsblock landet; alles, was keinen Aufruf kreuzt, bekommt
    // r11/r10/rsi/rdi/rdx und ist fuer den Sammler unsichtbar. Auf ARM64
    // sind alle sechs ausgeteilten Register im Block.
    //
    // Der saubere Weg zurueck ist bekannt und steht im Bericht: x25-x28
    // als NICHT-sammlersichtbare Haelfte, sobald der Zustandsblock von
    // sechs auf zehn Woerter waechst. Bis dahin bleiben Zellen im Rahmen.
    // `FIRN_A64_CELLS=1` schaltet sie zum Messen wieder an.
    let cells = if std::env::var_os("FIRN_A64_CELLS").is_some() {
        crate::regalloc::promotable_cells(f)
    } else {
        HashMap::new()
    };

    // ---- intervals and weights ----------------------------------------
    let mut start = vec![usize::MAX; nv];
    let mut end = vec![0usize; nv];
    let mut weight = vec![0u64; nv];
    let mut touch = |v: Val, p: usize, w: u64, start: &mut Vec<usize>, end: &mut Vec<usize>, weight: &mut Vec<u64>| {
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
    let mut buf: Vec<Val> = Vec::new();
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
            let uses = buf.clone();
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
        let vv = v as Val;
        if start[v] == usize::MAX || cells.contains_key(&vv) || ra.remat.contains_key(&vv) {
            continue;
        }
        if needs_frame_slot(f, vv) {
            continue;
        }
        match f.val_types.get(v) {
            Some(t) if integral(*t) => {}
            _ => continue,
        }
        ivs.push(Iv { val: vv, start: start[v], end: end[v], weight: weight[v], is_cell: false });
    }
    for (&c, t) in cells.iter() {
        let cv = c as usize;
        if cv >= nv || start[cv] == usize::MAX || !integral(*t) {
            continue;
        }
        // A cell holds a variable: it has to sit in its register from the
        // start of the function to its last access, because a variable in a
        // loop is read again after the back edge. Same rule round 90 settled
        // on for x86 after measuring the tighter answer and throwing it away.
        ivs.push(Iv {
            val: c,
            start: 0,
            end: end[cv],
            weight: weight[cv].saturating_mul(2).max(1),
            is_cell: true,
        });
    }
    if ivs.is_empty() {
        ra.on = !ra.remat.is_empty();
        return ra;
    }
    ivs.sort_by_key(|i| (i.start, i.end, i.val));

    // ---- the linear scan ----------------------------------------------
    //
    // Poletto/Sarkar, with the one simplification the ABI hands us: every
    // register in the pool survives every call, so a register fits an
    // interval whenever it is free. No crossing mask, no four pools.
    if std::env::var_os("FIRN_A64_NO_REGS").is_some() {
        ra.on = !ra.remat.is_empty();
        return ra;
    }
    let mut free: Vec<&'static str> = A64_POOL.to_vec();
    let mut active: Vec<(usize, &'static str, usize)> = Vec::new(); // (end, reg, index in ivs)
    let mut got: Vec<Option<&'static str>> = vec![None; ivs.len()];
    let mut used: Vec<&'static str> = Vec::new();

    for k in 0..ivs.len() {
        // expire
        active.retain(|&(e, r, _)| {
            if e < ivs[k].start {
                free.push(r);
                false
            } else {
                true
            }
        });
        if let Some(r) = free.pop() {
            got[k] = Some(r);
            if !used.contains(&r) {
                used.push(r);
            }
            active.push((ivs[k].end, r, k));
            active.sort_by_key(|&(e, _, _)| e);
            continue;
        }
        // full: the interval that ends LAST is the candidate for the stack —
        // but only if the newcomer is worth more (uses, weighted by loop
        // depth). Otherwise the newcomer goes to the stack.
        if let Some(&(le, lr, li)) = active.last() {
            if le > ivs[k].end && ivs[k].weight > ivs[li].weight {
                got[li] = None;
                got[k] = Some(lr);
                active.pop();
                active.push((ivs[k].end, lr, k));
                active.sort_by_key(|&(e, _, _)| e);
            }
        }
    }

    for (k, iv) in ivs.iter().enumerate() {
        if let Some(r) = got[k] {
            if iv.is_cell {
                let t = cells.get(&iv.val).copied().unwrap_or(FTy::I64);
                ra.cells.insert(iv.val, (r, t));
            } else {
                ra.regs.insert(iv.val, r);
            }
        }
    }
    if ra.regs.is_empty() && ra.cells.is_empty() && ra.remat.is_empty() {
        return ra;
    }
    ra.saved = A64_POOL.iter().copied().filter(|r| used.contains(r)).collect();

    // ---- die Sicherungspunkte -------------------------------------------
    // Fuer jeden `Op::GcAddr { regs: true }`: welches Register aus `saved`
    // haelt an dieser Stelle nichts Lebendiges mehr? Siehe den langen Text
    // am Feld `safepoints`.
    if live.converged && !ra.saved.is_empty() {
        // Belegte Intervalle, nach Register sortiert.
        let mut je_reg: HashMap<&'static str, Vec<(usize, usize)>> = HashMap::new();
        for (k, iv) in ivs.iter().enumerate() {
            if let Some(r) = got[k] {
                je_reg.entry(r).or_default().push((iv.start, iv.end));
            }
        }
        for (bi, b) in f.blocks.iter().enumerate() {
            for (ii, i) in b.insts.iter().enumerate() {
                let d = match (i.dst, &i.op) {
                    (Some(d), crate::fir::Op::GcAddr { regs: true }) => d,
                    _ => continue,
                };
                let p = live.pos[bi][ii];
                let tot: Vec<&'static str> = ra
                    .saved
                    .iter()
                    .copied()
                    .filter(|r| match je_reg.get(r) {
                        Some(v) => !v.iter().any(|&(s, e)| s <= p && p <= e),
                        None => true,
                    })
                    .collect();
                if !tot.is_empty() {
                    ra.safepoints.insert(d, tot);
                }
            }
        }
    }
    // ---- wo ein Zeiger stirbt -------------------------------------------
    if live.converged {
        let mut je_reg: HashMap<&'static str, Vec<(usize, usize)>> = HashMap::new();
        for (k, iv) in ivs.iter().enumerate() {
            if let Some(r) = got[k] {
                je_reg.entry(r).or_default().push((iv.start, iv.end));
            }
        }
        // Stelle -> (Block, Befehlsnummer). Endet ein Intervall auf der
        // Stelle des Abschlusses eines Blocks, gehoert das Leeren hinter
        // den LETZTEN Befehl dieses Blocks -- der Abschluss selbst darf
        // nichts mehr dazwischen bekommen.
        let mut stelle: HashMap<usize, (usize, usize)> = HashMap::new();
        for (bi, b) in f.blocks.iter().enumerate() {
            for ii in 0..b.insts.len() {
                stelle.insert(live.pos[bi][ii], (bi, ii));
            }
            // NICHT die Stelle des Abschlusses. Das war der Fehler beim
            // ersten Anlauf, und er hat das Programm zum Haengen gebracht:
            // endet ein Intervall auf `block_end`, dann ist der Wert
            // LIVE-OUT dieses Blocks -- er faehrt ueber die Kante weiter,
            // haeufig ueber eine Rueckwaertskante an den Schleifenkopf.
            // Ein `mov r, xzr` davor loescht einen lebenden Wert.
            //
            // Umgekehrt gilt: liegt das Ende auf einer echten
            // Befehlsstelle, dann ist der Wert weder live-out dieses
            // Blocks noch live-in irgendeines spaeteren -- sonst haette
            // `touch(block_end)` bzw. `touch(block_start)` das Ende
            // dorthin geschoben. Genau dann ist das Leeren sicher.
        }
        for (k, iv) in ivs.iter().enumerate() {
            let r = match got[k] {
                Some(r) => r,
                None => continue,
            };
            if f.val_types.get(iv.val as usize) != Some(&FTy::Ptr) {
                continue;
            }
            // Wird das Register gleich danach wieder gebraucht, macht der
            // naechste Schreiber die Arbeit umsonst.
            let weiter = je_reg
                .get(r)
                .map(|v| v.iter().any(|&(s2, e2)| s2 <= iv.end + 1 && iv.end + 1 <= e2))
                .unwrap_or(false);
            if weiter {
                continue;
            }
            if let Some(&(bi, ii)) = stelle.get(&iv.end) {
                let e = ra.clears.entry((bi, ii)).or_default();
                if !e.contains(&r) {
                    e.push(r);
                }
            }
        }
    }
    ra.on = true;
    ra
}

/// Does this instruction read or write the given value? Used by the emitter
/// to decide whether a `str` into the slot may be skipped.
#[allow(dead_code)]
pub(crate) fn touches(i: &Inst, v: Val) -> bool {
    if i.dst == Some(v) {
        return true;
    }
    let mut b = Vec::new();
    i.op.uses(&mut b);
    b.contains(&v)
}

