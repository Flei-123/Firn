//! Optimierer auf FIR: Konstantenfaltung und Entfernen toten Codes.
//!
//! SCHNITTSTELLE (fest):
//!   `pub fn optimize(m: &mut fir::Module) -> OptStats`
//! Regel: Die Optimierung darf das Programmverhalten NIE aendern. Die
//! Testsuite faehrt jedes Programm mit und ohne `--no-opt` und vergleicht.
//!
//! Durchgefuehrte Umformungen (alle verhaltenserhaltend):
//!  1. **Konstantenfaltung** ueber `Op::Bin`, `Op::Cmp`, `Op::Un` und
//!     `Op::Cast`, wenn alle Operanden `Op::Const` sind. Das Ergebnis wird mit
//!     `FTy::truncate` auf Breite und Vorzeichen des Ergebnistyps normalisiert
//!     und ersetzt die Instruktion durch `Op::Const` — die `Val`-Id bleibt
//!     gleich, alle Verwendungen bleiben gueltig.
//!     NICHT gefaltet werden: Division/Rest durch null, der Ueberlauffall
//!     `MIN / -1` bzw. `MIN % -1` (beides loest auf der CPU eine Ausnahme aus)
//!     und Verschiebungen mit einer Weite >= Bitbreite (auf x86 undefiniert).
//!  2. **Vereinfachung von `brcond`** mit konstanter Bedingung (oder gleichen
//!     Zielen) zu `br`. Erst dadurch entsteht unerreichbarer Code.
//!  3. **Toter Code**: unerreichbare Bloecke (Erreichbarkeit ab `bb0` ueber
//!     `Term::successors`) werden entfernt und die verbleibenden Bloecke
//!     luecklos neu nummeriert (Invariante `blocks[i].id == i` bleibt erhalten,
//!     alle Terminatoren werden umgeschrieben). Unbenutzte REINE Instruktionen
//!     (kein `store`/`call`/`syscall`/`copymem`) werden entfernt; `alloca` nur
//!     dann, wenn ihr Zeiger nirgends mehr verwendet wird.
//!
//! Es wird bis zum Fixpunkt iteriert, aber hoechstens `MAX_ROUNDS` mal, damit
//! der Optimierer unter keinen Umstaenden haengen bleibt.

use crate::fir::{BinOp, BlockId, CmpOp, FTy, Func, Module, Op, Term, UnOp, Val};
use std::collections::{HashMap, HashSet};

/// harte Obergrenze der Fixpunkt-Iterationen
const MAX_ROUNDS: u32 = 50;

#[derive(Clone, Copy, Debug, Default)]
pub struct OptStats {
    /// Anzahl zu Konstanten gefalteter Instruktionen
    pub folded: usize,
    /// entfernte Instruktionen (tot/unbenutzt und rein)
    pub removed_insts: usize,
    /// entfernte, unerreichbare Basisbloecke
    pub removed_blocks: usize,
    /// aufgeloeste `load`s (mem2reg + lokale Speicherweiterleitung)
    pub promoted_loads: usize,
    /// fortgepflanzte Kopien / algebraische Identitaeten
    pub copies: usize,
    /// entfernte gemeinsame Teilausdruecke
    pub cse: usize,
    /// verschmolzene bzw. ueberbrueckte Bloecke
    pub merged_blocks: usize,
    /// eingebettete Aufrufe (inline.rs)
    pub inlined: usize,
    /// entfernte, beweisbar immer erfuellte Bereichspruefungen
    pub removed_checks: usize,
    /// schleifeninvariante Instruktionen, die in den Vorkopf gewandert sind
    pub hoisted: usize,
}

// ------------------------------------------------ Durchgangsregister ---
//
// DESIGNZIELE.md §5 und §10.4 Punkt 4: Jeder Optimierungsdurchgang hat einen
// NAMEN, einen SCHALTER und ein ETIKETT `debugerhaltend ja/nein`. Nur so laesst
// sich spaeter die Baustufe `--dev-fast` bauen (schnell, aber debuggbar), ohne
// jeden Durchgang anzufassen. Das Register ist die einzige Wahrheit darueber,
// welche Durchgaenge es gibt — `--list-passes` gibt es aus.

/// Baustufe. `DevFast` ist die Voreinstellung (DESIGNZIELE.md §5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    /// gar keine Optimierung (`--no-opt`) — nur zur Compilerfehlersuche
    Dev,
    /// nur debugerhaltende Durchgaenge — der Alltagsmodus
    DevFast,
    /// alle Durchgaenge (Pruefungen bleiben an, sobald es welche gibt)
    ReleaseSafe,
    /// alle Durchgaenge
    ReleaseFast,
}

impl Level {
    pub fn from_str(s: &str) -> Option<Level> {
        match s {
            "dev" => Some(Level::Dev),
            "dev-fast" => Some(Level::DevFast),
            "release-safe" => Some(Level::ReleaseSafe),
            "release-fast" => Some(Level::ReleaseFast),
            _ => None,
        }
    }
    /// Laeuft in dieser Stufe auch nicht-debugerhaltendes?
    fn allows_all(self) -> bool {
        matches!(self, Level::ReleaseSafe | Level::ReleaseFast)
    }
}

/// Wirkungsbereich eines Durchgangs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    /// arbeitet auf einer einzelnen Funktion, laeuft in der Fixpunktschleife
    Func,
    /// arbeitet auf dem ganzen Modul, laeuft einmal
    Module,
}

/// Beschreibung eines Durchgangs.
pub struct PassInfo {
    /// Schaltername fuer `--no-pass=<name>`
    pub name: &'static str,
    pub scope: Scope,
    /// **Etikett.** `true` = jede benannte Variable zeigt an jedem Haltepunkt
    /// weiterhin ihren korrekten Wert; der Aufrufstapel bleibt lesbar.
    /// `false` = der Durchgang zerstoert das Debugbild und laeuft nur in den
    /// Release-Stufen.
    pub debug_preserving: bool,
    pub what: &'static str,
}

/// Alle Durchgaenge. Reihenfolge = Ausfuehrungsreihenfolge innerhalb einer Runde.
pub const PASSES: &[PassInfo] = &[
    PassInfo {
        name: "fold",
        scope: Scope::Func,
        debug_preserving: true,
        what: "Konstantenfaltung (Bin/Cmp/Un/Cast mit konstanten Operanden)",
    },
    PassInfo {
        name: "mem2reg",
        scope: Scope::Func,
        debug_preserving: true,
        what: "Stapelfaecher zu Werten, Weiterleitung lokaler loads, tote stores",
    },
    PassInfo {
        name: "copyprop",
        scope: Scope::Func,
        debug_preserving: true,
        what: "Kopien fortpflanzen, algebraische Identitaeten",
    },
    PassInfo {
        name: "cse",
        scope: Scope::Func,
        debug_preserving: true,
        what: "gemeinsame Teilausdruecke zusammenfassen",
    },
    PassInfo {
        name: "licm",
        scope: Scope::Func,
        debug_preserving: true,
        what: "schleifeninvariante Berechnungen in den Vorkopf ziehen",
    },
    PassInfo {
        name: "bce",
        scope: Scope::Func,
        debug_preserving: true,
        what: "beweisbar immer erfuellte Bereichspruefungen entfernen",
    },
    PassInfo {
        name: "simplify-term",
        scope: Scope::Func,
        debug_preserving: true,
        what: "brcond mit konstanter Bedingung zu br vereinfachen",
    },
    PassInfo {
        name: "merge-blocks",
        scope: Scope::Func,
        debug_preserving: true,
        what: "leere und einfach verkettete Basisbloecke verschmelzen",
    },
    PassInfo {
        name: "dce",
        scope: Scope::Func,
        debug_preserving: true,
        what: "unerreichbare Bloecke und unbenutzte reine Instruktionen entfernen",
    },
    PassInfo {
        name: "inline",
        scope: Scope::Module,
        debug_preserving: false,
        what: "Aufrufe einbetten (Groessenheuristik) — macht den Aufrufstapel unlesbar",
    },
];

/// Was in einem Lauf ausgefuehrt werden soll.
#[derive(Clone, Debug)]
pub struct OptConfig {
    pub level: Level,
    /// einzeln abgeschaltete Durchgaenge (`--no-pass=<name>`)
    pub disabled: Vec<String>,
}

impl Default for OptConfig {
    fn default() -> Self {
        OptConfig { level: Level::ReleaseFast, disabled: Vec::new() }
    }
}

impl OptConfig {
    /// Laeuft dieser Durchgang?
    pub fn runs(&self, name: &str) -> bool {
        if self.level == Level::Dev {
            return false;
        }
        if self.disabled.iter().any(|d| d == name) {
            return false;
        }
        match PASSES.iter().find(|p| p.name == name) {
            Some(p) => p.debug_preserving || self.level.allows_all(),
            // Unbekannter Name kann nicht vorkommen (nur interne Aufrufer),
            // wird aber konservativ ausgefuehrt statt still uebersprungen.
            None => true,
        }
    }
    /// Gibt es diesen Durchgangsnamen ueberhaupt?
    pub fn is_known(name: &str) -> bool {
        PASSES.iter().any(|p| p.name == name)
    }
}

/// Register als Text (fuer `--list-passes`).
pub fn passes_text() -> String {
    let mut out = String::from(
        "Optimierungsdurchgaenge (Reihenfolge = Ausfuehrungsreihenfolge)\n\nNAME            BEREICH  DEBUGERHALTEND  BESCHREIBUNG\n",
    );
    for p in PASSES {
        out.push_str(&format!(
            "{:<15} {:<8} {:<15} {}\n",
            p.name,
            match p.scope {
                Scope::Func => "Funktion",
                Scope::Module => "Modul",
            },
            if p.debug_preserving { "ja" } else { "NEIN" },
            p.what
        ));
    }
    out.push_str(
        "\nBaustufen: --opt-level=dev | dev-fast | release-safe | release-fast\n'dev-fast' fuehrt nur die debugerhaltenden Durchgaenge aus.\nEinzeln abschalten: --no-pass=<name> (mehrfach erlaubt).\n",
    );
    out
}

// ------------------------------------------------------- Ausfuehrung ---

/// Volle Optimierung (`Level::ReleaseFast`) — Kurzform fuer die Modultests.
#[cfg(test)]
pub fn optimize(m: &mut Module) -> OptStats {
    optimize_with(m, &OptConfig::default())
}

pub fn optimize_with(m: &mut Module, cfg: &OptConfig) -> OptStats {
    let mut st = OptStats::default();
    if cfg.level == Level::Dev {
        return st;
    }
    // Erst je Funktion aufraeumen, damit die Groessenheuristik des Inliners
    // auf bereits vereinfachten Rumpfen arbeitet.
    for f in m.funcs.iter_mut() {
        optimize_func(f, &mut st, cfg);
    }
    if cfg.runs("inline") {
        st.inlined += crate::inline::inline_module(m);
        for f in m.funcs.iter_mut() {
            optimize_func(f, &mut st, cfg);
        }
    }
    st
}

fn optimize_func(f: &mut Func, st: &mut OptStats, cfg: &OptConfig) {
    let mut round = 0;
    loop {
        round += 1;
        let mut changed = false;
        if cfg.runs("fold") {
            changed |= fold_constants(f, st);
        }
        if cfg.runs("mem2reg") {
            let p =
                crate::mem2reg::promote_single_store(f) + crate::mem2reg::forward_local_loads(f);
            let ds = crate::mem2reg::remove_dead_stores(f);
            st.removed_insts += ds;
            changed |= ds > 0;
            st.promoted_loads += p;
            changed |= p > 0;
        }
        if cfg.runs("copyprop") {
            let c = crate::mem2reg::copy_propagate(f);
            st.copies += c;
            changed |= c > 0;
        }
        if cfg.runs("cse") {
            let e = cse(f);
            st.cse += e;
            changed |= e > 0;
        }
        if cfg.runs("licm") {
            let h = crate::licm::hoist_loop_invariants(f);
            st.hoisted += h;
            changed |= h > 0;
        }
        if cfg.runs("bce") {
            let r = remove_redundant_checks(f);
            st.removed_checks += r;
            changed |= r > 0;
        }
        if cfg.runs("simplify-term") {
            changed |= simplify_terminators(f);
        }
        if cfg.runs("merge-blocks") {
            let mb = crate::mem2reg::merge_blocks(f);
            st.merged_blocks += mb;
            changed |= mb > 0;
        }
        if cfg.runs("dce") {
            changed |= remove_unreachable_blocks(f, st);
            changed |= remove_dead_insts(f, st);
        }
        if !changed || round >= MAX_ROUNDS {
            break;
        }
    }
}

// ------------------------------------------- gemeinsame Teilausdruecke (CSE) ---

/// Schluessel eines reinen, wiederverwendbaren Ausdrucks.
#[derive(PartialEq, Eq, Hash, Clone)]
enum Key {
    Const(u8, i128),
    Bin(u8, u8, Val, Val),
    Cmp(u8, u8, Val, Val),
    Un(u8, u8, Val),
    Cast(u8, u8, Val),
    PtrAdd(Val, Val),
}

/// Kennzahl eines FIR-Typs (fir::FTy leitet `Hash` nicht ab).
fn tyk(t: FTy) -> u8 {
    match t {
        FTy::I8 => 1,
        FTy::I16 => 2,
        FTy::I32 => 3,
        FTy::I64 => 4,
        FTy::U8 => 5,
        FTy::U16 => 6,
        FTy::U32 => 7,
        FTy::U64 => 8,
        FTy::Bool => 9,
        FTy::Ptr => 10,
        FTy::Void => 11,
    }
}

fn bink(o: BinOp) -> u8 {
    match o {
        BinOp::Add => 1,
        BinOp::Sub => 2,
        BinOp::Mul => 3,
        BinOp::Div => 4,
        BinOp::Rem => 5,
        BinOp::And => 6,
        BinOp::Or => 7,
        BinOp::Xor => 8,
        BinOp::Shl => 9,
        BinOp::Shr => 10,
    }
}

fn cmpk(o: CmpOp) -> u8 {
    match o {
        CmpOp::Eq => 1,
        CmpOp::Ne => 2,
        CmpOp::Lt => 3,
        CmpOp::Le => 4,
        CmpOp::Gt => 5,
        CmpOp::Ge => 6,
    }
}

fn unk(o: UnOp) -> u8 {
    match o {
        UnOp::Neg => 1,
        UnOp::Not => 2,
    }
}

fn key_of(i: &crate::fir::Inst) -> Option<Key> {
    match &i.op {
        Op::Const(c) => Some(Key::Const(tyk(i.ty), *c)),
        Op::Bin(o, a, b) => Some(Key::Bin(tyk(i.ty), bink(*o), *a, *b)),
        Op::Cmp { op, ty, a, b } => Some(Key::Cmp(tyk(*ty), cmpk(*op), *a, *b)),
        Op::Un(o, a) => Some(Key::Un(tyk(i.ty), unk(*o), *a)),
        Op::Cast { src, from } => Some(Key::Cast(tyk(i.ty), tyk(*from), *src)),
        Op::PtrAdd { base, off } => Some(Key::PtrAdd(*base, *off)),
        // `load` haengt vom Speicher ab, `alloca` liefert je Instruktion eine
        // eigene Adresse, `select`/`barrier`/`secure_zero` sind unantastbar.
        _ => None,
    }
}

/// Entfernt mehrfach berechnete reine Ausdruecke entlang des Dominatorbaums:
/// ein Ausdruck darf nur durch einen Wert ersetzt werden, dessen Definition
/// die Verwendung dominiert.
fn cse(f: &mut Func) -> usize {
    if f.blocks.len() > 512 || f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return 0;
    }
    let dom = crate::mem2reg::dominators(f);
    let n = f.blocks.len();
    let mut avail: HashMap<Key, Vec<(usize, Val)>> = HashMap::new();
    let mut map: HashMap<Val, Val> = HashMap::new();
    for bi in 0..n {
        for ii in 0..f.blocks[bi].insts.len() {
            let inst = &f.blocks[bi].insts[ii];
            let d = match inst.dst {
                Some(d) => d,
                None => continue,
            };
            if f.is_secret(d) {
                continue;
            }
            let k = match key_of(inst) {
                Some(k) => k,
                None => continue,
            };
            let e = avail.entry(k).or_default();
            let mut hit = None;
            for &(ob, ov) in e.iter() {
                // Dominanz: gleicher Block (frueher) oder dominierender Block
                if (ob == bi || dom[bi][ob]) && !f.is_secret(ov) {
                    hit = Some(ov);
                    break;
                }
            }
            match hit {
                Some(ov) => {
                    map.insert(d, ov);
                }
                None => e.push((bi, d)),
            }
        }
    }
    if map.is_empty() {
        return 0;
    }
    let cnt = map.len();
    crate::mem2reg::replace_uses(f, &map);
    cnt
}

// ------------------------------------------------------- Bereichspruefungen ---

/// Entfernt beweisbar immer erfuellte Bereichspruefungen: ein `brcond` auf
/// `i < n`, das von einem dominierenden `brcond` mit derselben Bedingung
/// bereits als wahr (bzw. falsch) entschieden wurde, wird zum unbedingten
/// Sprung. Damit verschwindet die doppelte Pruefung, die beim Zugriff auf ein
/// Feld innerhalb einer bereits gepruefte Schleife entsteht.
/// Liefert die Anzahl entfernter Pruefungen.
fn remove_redundant_checks(f: &mut Func) -> usize {
    if f.blocks.len() > 512 || f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return 0;
    }
    let preds = crate::mem2reg::preds(f);
    let n = f.blocks.len();
    // Wissen wird ausschliesslich entlang von Ketten mit GENAU EINEM Vorgaenger
    // fortgeschrieben. Eine solche Kette kann keinen Zyklus enthalten (ein
    // wiederbetretener Block haette einen zweiten Vorgaenger), also ist der
    // Wert der Bedingung auf dem tatsaechlich gelaufenen Weg unveraendert.
    let mut known: Vec<HashMap<Val, bool>> = vec![HashMap::new(); n];
    for bi in 0..n {
        let mut cur = bi;
        let mut facts: HashMap<Val, bool> = HashMap::new();
        for _ in 0..64 {
            if preds[cur].len() != 1 {
                break;
            }
            let p = preds[cur][0];
            if p == cur {
                break;
            }
            if let Term::BrCond { cond, then_bb, else_bb } = f.blocks[p].term {
                if (then_bb as usize == cur) != (else_bb as usize == cur) {
                    facts.entry(cond).or_insert(then_bb as usize == cur);
                }
            }
            cur = p;
        }
        known[bi] = facts;
    }
    let mut removed = 0usize;
    for bi in 0..n {
        if let Term::BrCond { cond, then_bb, else_bb } = f.blocks[bi].term {
            if f.is_secret(cond) {
                continue; // SPEC §9.2: geheime Bedingungen bleiben unberuehrt
            }
            if let Some(&v) = known[bi].get(&cond) {
                f.blocks[bi].term = Term::Br(if v { then_bb } else { else_bb });
                removed += 1;
            }
        }
    }
    removed
}

// ---------------------------------------------------------------- Faltung ---

/// Sammelt alle bekannten Konstantenwerte der Funktion.
fn const_map(f: &Func) -> HashMap<Val, i128> {
    let mut m = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Const(c)) = (i.dst, &i.op) {
                m.insert(d, *c);
            }
        }
    }
    m
}

fn fold_constants(f: &mut Func, st: &mut OptStats) -> bool {
    let mut consts = const_map(f);
    let mut changed = false;
    for bi in 0..f.blocks.len() {
        for ii in 0..f.blocks[bi].insts.len() {
            let (ty, op, dst) = {
                let i = &f.blocks[bi].insts[ii];
                (i.ty, i.op.clone(), i.dst)
            };
            let dst = match dst {
                Some(d) => d,
                None => continue,
            };
            let folded = match op {
                Op::Bin(bop, a, b) => match (consts.get(&a), consts.get(&b)) {
                    (Some(&x), Some(&y)) => fold_bin(ty, bop, x, y),
                    _ => None,
                },
                Op::Cmp { op, ty: oty, a, b } => match (consts.get(&a), consts.get(&b)) {
                    (Some(&x), Some(&y)) => Some(fold_cmp(oty, op, x, y)),
                    _ => None,
                },
                Op::Un(uop, a) => consts.get(&a).map(|&x| fold_un(ty, uop, x)),
                Op::Cast { src, from } => match consts.get(&src) {
                    Some(&x) => fold_cast(ty, from, x),
                    None => None,
                },
                _ => None,
            };
            if let Some(v) = folded {
                f.blocks[bi].insts[ii].op = Op::Const(v);
                consts.insert(dst, v);
                st.folded += 1;
                changed = true;
            }
        }
    }
    changed
}

fn fold_bin(ty: FTy, op: BinOp, a: i128, b: i128) -> Option<i128> {
    if ty == FTy::Void || ty.bits() == 0 {
        return None;
    }
    let a = ty.truncate(a);
    let b = ty.truncate(b);
    let bits = ty.bits() as i128;
    let min_signed: i128 = if ty.signed() { -(1i128 << (ty.bits() - 1)) } else { 0 };
    let r = match op {
        BinOp::Add => a + b,
        BinOp::Sub => a - b,
        BinOp::Mul => a * b,
        BinOp::Div => {
            if b == 0 || (ty.signed() && a == min_signed && b == -1) {
                return None;
            }
            a / b
        }
        BinOp::Rem => {
            if b == 0 || (ty.signed() && a == min_signed && b == -1) {
                return None;
            }
            a % b
        }
        BinOp::And => a & b,
        BinOp::Or => a | b,
        BinOp::Xor => a ^ b,
        BinOp::Shl => {
            if b < 0 || b >= bits {
                return None;
            }
            a << b
        }
        BinOp::Shr => {
            if b < 0 || b >= bits {
                return None;
            }
            // `a` ist bereits vorzeichenrichtig normalisiert: bei
            // vorzeichenlosen Typen nicht negativ (-> logische Verschiebung),
            // bei vorzeichenbehafteten arithmetisch.
            a >> b
        }
    };
    Some(ty.truncate(r))
}

fn fold_cmp(ty: FTy, op: CmpOp, a: i128, b: i128) -> i128 {
    let a = ty.truncate(a);
    let b = ty.truncate(b);
    let r = match op {
        CmpOp::Eq => a == b,
        CmpOp::Ne => a != b,
        CmpOp::Lt => a < b,
        CmpOp::Le => a <= b,
        CmpOp::Gt => a > b,
        CmpOp::Ge => a >= b,
    };
    if r {
        1
    } else {
        0
    }
}

fn fold_un(ty: FTy, op: UnOp, a: i128) -> i128 {
    let a = ty.truncate(a);
    match op {
        UnOp::Neg => ty.truncate(-a),
        UnOp::Not => {
            if ty == FTy::Bool {
                if a & 1 != 0 {
                    0
                } else {
                    1
                }
            } else {
                ty.truncate(!a)
            }
        }
    }
}

fn fold_cast(to: FTy, from: FTy, a: i128) -> Option<i128> {
    if to == FTy::Void || from == FTy::Void {
        return None;
    }
    // Ganzzahl -> bool ist keine reine Bitoperation (Vergleich mit 0 gegenueber
    // "unterstes Bit"); das ueberlaesst der Optimierer dem Backend.
    if to == FTy::Bool && from != FTy::Bool {
        return None;
    }
    Some(to.truncate(from.truncate(a)))
}

// ---------------------------------------------------------- Terminatoren ---

fn simplify_terminators(f: &mut Func) -> bool {
    let consts = const_map(f);
    let mut changed = false;
    for b in f.blocks.iter_mut() {
        if let Term::BrCond { cond, then_bb, else_bb } = b.term {
            if then_bb == else_bb {
                b.term = Term::Br(then_bb);
                changed = true;
            } else if let Some(&c) = consts.get(&cond) {
                b.term = Term::Br(if c != 0 { then_bb } else { else_bb });
                changed = true;
            }
        } else if let Term::Switch { val, cases, default, .. } = &b.term {
            // Konstante Marke: direkt zum passenden Zweig springen.
            if let Some(&c) = consts.get(val) {
                let t = cases.iter().find(|(k, _)| *k == c).map(|(_, t)| *t).unwrap_or(*default);
                b.term = Term::Br(t);
                changed = true;
            } else if cases.iter().all(|(_, t)| *t == *default) {
                let d = *default;
                b.term = Term::Br(d);
                changed = true;
            }
        }
    }
    changed
}

// ------------------------------------------------------------- toter Code ---

fn collect_uses(f: &Func, blocks: &[usize]) -> HashSet<Val> {
    let mut used = HashSet::new();
    let mut buf = Vec::new();
    for &bi in blocks {
        let b = &f.blocks[bi];
        for i in &b.insts {
            buf.clear();
            i.op.uses(&mut buf);
            for v in buf.iter() {
                used.insert(*v);
            }
        }
        match &b.term {
            Term::BrCond { cond, .. } => {
                used.insert(*cond);
            }
            Term::Ret(Some(v)) => {
                used.insert(*v);
            }
            Term::Switch { val, .. } => {
                used.insert(*val);
            }
            Term::Br(_) | Term::Ret(None) | Term::Unset => {}
        }
    }
    used
}

fn remove_unreachable_blocks(f: &mut Func, st: &mut OptStats) -> bool {
    if f.blocks.is_empty() {
        return false;
    }
    let mut index_of: HashMap<BlockId, usize> = HashMap::new();
    for (i, b) in f.blocks.iter().enumerate() {
        index_of.insert(b.id, i);
    }
    // Erreichbarkeit ab dem Eintrittsblock
    let mut reachable = vec![false; f.blocks.len()];
    let mut stack = vec![0usize];
    reachable[0] = true;
    while let Some(bi) = stack.pop() {
        for s in f.blocks[bi].term.successors() {
            if let Some(&si) = index_of.get(&s) {
                if !reachable[si] {
                    reachable[si] = true;
                    stack.push(si);
                }
            }
        }
    }
    if reachable.iter().all(|&r| r) {
        return false;
    }

    // Sicherheitsnetz: Wird ein in einem unerreichbaren Block definierter Wert
    // noch aus erreichbarem Code heraus gelesen (das waere ein Verstoss gegen
    // die SSA-Dominanz), wird NICHTS entfernt — lieber toter Code als eine
    // baumelnde Val-Id.
    let live_idx: Vec<usize> = (0..f.blocks.len()).filter(|&i| reachable[i]).collect();
    let used = collect_uses(f, &live_idx);
    for (i, b) in f.blocks.iter().enumerate() {
        if reachable[i] {
            continue;
        }
        for inst in &b.insts {
            if let Some(d) = inst.dst {
                if used.contains(&d) {
                    return false;
                }
            }
        }
    }

    let removed_insts: usize =
        f.blocks.iter().enumerate().filter(|(i, _)| !reachable[*i]).map(|(_, b)| b.insts.len()).sum();
    let removed_blocks = reachable.iter().filter(|&&r| !r).count();

    // luecklos neu nummerieren, Reihenfolge bleibt erhalten
    let mut new_id: HashMap<BlockId, BlockId> = HashMap::new();
    let mut kept = Vec::with_capacity(live_idx.len());
    for (n, &i) in live_idx.iter().enumerate() {
        new_id.insert(f.blocks[i].id, n as BlockId);
        kept.push(f.blocks[i].clone());
    }
    for (n, b) in kept.iter_mut().enumerate() {
        b.id = n as BlockId;
        b.term = match &b.term {
            Term::Br(t) => Term::Br(new_id[t]),
            Term::BrCond { cond, then_bb, else_bb } => {
                Term::BrCond { cond: *cond, then_bb: new_id[then_bb], else_bb: new_id[else_bb] }
            }
            Term::Switch { val, ty, cases, default } => Term::Switch {
                val: *val,
                ty: *ty,
                cases: cases.iter().map(|(k, t)| (*k, new_id[t])).collect(),
                default: new_id[default],
            },
            other => other.clone(),
        };
    }
    f.blocks = kept;
    st.removed_blocks += removed_blocks;
    st.removed_insts += removed_insts;
    true
}

fn remove_dead_insts(f: &mut Func, st: &mut OptStats) -> bool {
    let mut changed = false;
    let mut round = 0;
    loop {
        round += 1;
        let all: Vec<usize> = (0..f.blocks.len()).collect();
        let used = collect_uses(f, &all);
        let mut removed = 0usize;
        for b in f.blocks.iter_mut() {
            let before = b.insts.len();
            b.insts.retain(|i| {
                if !i.op.is_pure() {
                    return true;
                }
                match i.dst {
                    Some(d) => used.contains(&d),
                    // reine Instruktion ohne Ergebnis: wirkungslos
                    None => false,
                }
            });
            removed += before - b.insts.len();
        }
        if removed == 0 {
            break;
        }
        st.removed_insts += removed;
        changed = true;
        if round >= MAX_ROUNDS {
            break;
        }
    }
    changed
}

// ------------------------------------------------------------------ Tests ---

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fir::{Inst, Term};

    fn consts_in(f: &Func) -> Vec<i128> {
        let mut v = Vec::new();
        for b in &f.blocks {
            for i in &b.insts {
                if let Op::Const(c) = i.op {
                    v.push(c);
                }
            }
        }
        v
    }

    #[test]
    fn faltet_arithmetik_und_entfernt_zwischenwerte() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let a = f.push(0, FTy::I32, Op::Const(20));
        let b = f.push(0, FTy::I32, Op::Const(2));
        let m = f.push(0, FTy::I32, Op::Bin(BinOp::Mul, a, b));
        let c = f.push(0, FTy::I32, Op::Const(2));
        let s = f.push(0, FTy::I32, Op::Bin(BinOp::Add, m, c));
        f.set_term(0, Term::Ret(Some(s)));
        let before = f.inst_count();
        let mut m0 = Module::new();
        m0.funcs.push(f);
        let st = optimize(&mut m0);
        let f = &m0.funcs[0];
        assert!(st.folded >= 2, "es muss gefaltet werden: {:?}", st);
        assert!(f.inst_count() < before, "{} -> {}", before, f.inst_count());
        assert_eq!(f.inst_count(), 1);
        assert_eq!(consts_in(f), vec![42]);
        assert!(matches!(f.blocks[0].term, Term::Ret(Some(_))));
    }

    #[test]
    fn division_durch_null_bleibt_stehen() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let a = f.push(0, FTy::I32, Op::Const(7));
        let b = f.push(0, FTy::I32, Op::Const(0));
        let d = f.push(0, FTy::I32, Op::Bin(BinOp::Div, a, b));
        f.set_term(0, Term::Ret(Some(d)));
        let mut m = Module::new();
        m.funcs.push(f);
        let st = optimize(&mut m);
        assert_eq!(st.folded, 0);
        assert_eq!(m.funcs[0].inst_count(), 3);
        assert!(matches!(m.funcs[0].blocks[0].insts[2].op, Op::Bin(BinOp::Div, _, _)));
    }

    #[test]
    fn zu_breite_verschiebung_bleibt_stehen() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let a = f.push(0, FTy::I32, Op::Const(1));
        let b = f.push(0, FTy::I32, Op::Const(32));
        let s = f.push(0, FTy::I32, Op::Bin(BinOp::Shl, a, b));
        f.set_term(0, Term::Ret(Some(s)));
        let mut m = Module::new();
        m.funcs.push(f);
        let st = optimize(&mut m);
        assert_eq!(st.folded, 0);
        assert!(matches!(m.funcs[0].blocks[0].insts[2].op, Op::Bin(BinOp::Shl, _, _)));
    }

    #[test]
    fn ueberlauf_wird_korrekt_zurechtgestutzt() {
        let mut f = Func::new("t", vec![], FTy::I8);
        let a = f.push(0, FTy::I8, Op::Const(100));
        let b = f.push(0, FTy::I8, Op::Const(100));
        let s = f.push(0, FTy::I8, Op::Bin(BinOp::Add, a, b));
        f.set_term(0, Term::Ret(Some(s)));
        let mut m = Module::new();
        m.funcs.push(f);
        optimize(&mut m);
        assert_eq!(consts_in(&m.funcs[0]), vec![-56]); // 200 mod 256 als i8
    }

    #[test]
    fn unsignierte_verschiebung_und_cast() {
        let mut f = Func::new("t", vec![], FTy::U64);
        let a = f.push(0, FTy::U8, Op::Const(200));
        let c = f.push(0, FTy::U64, Op::Cast { src: a, from: FTy::U8 });
        let sh = f.push(0, FTy::U64, Op::Const(1));
        let r = f.push(0, FTy::U64, Op::Bin(BinOp::Shr, c, sh));
        f.set_term(0, Term::Ret(Some(r)));
        let mut m = Module::new();
        m.funcs.push(f);
        optimize(&mut m);
        assert_eq!(consts_in(&m.funcs[0]), vec![100]);

        // vorzeichenbehaftete Verkuerzung/Erweiterung
        let mut f = Func::new("t2", vec![], FTy::I64);
        let a = f.push(0, FTy::I8, Op::Const(-1));
        let c = f.push(0, FTy::I64, Op::Cast { src: a, from: FTy::I8 });
        f.set_term(0, Term::Ret(Some(c)));
        let mut m = Module::new();
        m.funcs.push(f);
        optimize(&mut m);
        assert_eq!(consts_in(&m.funcs[0]), vec![-1]);

        // i8 -1 -> u32 = 4294967295
        let mut f = Func::new("t3", vec![], FTy::U32);
        let a = f.push(0, FTy::I8, Op::Const(-1));
        let c = f.push(0, FTy::U32, Op::Cast { src: a, from: FTy::I8 });
        f.set_term(0, Term::Ret(Some(c)));
        let mut m = Module::new();
        m.funcs.push(f);
        optimize(&mut m);
        assert_eq!(consts_in(&m.funcs[0]), vec![4294967295]);
    }

    #[test]
    fn vergleich_und_zweig_falten_unerreichbaren_block_weg() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let then_bb = f.add_block();
        let else_bb = f.add_block();
        let a = f.push(0, FTy::I32, Op::Const(3));
        let b = f.push(0, FTy::I32, Op::Const(4));
        let c = f.push(0, FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::I32, a, b });
        f.set_term(0, Term::BrCond { cond: c, then_bb, else_bb });
        let x = f.push(then_bb, FTy::I32, Op::Const(1));
        f.set_term(then_bb, Term::Ret(Some(x)));
        let y = f.push(else_bb, FTy::I32, Op::Const(2));
        f.set_term(else_bb, Term::Ret(Some(y)));
        let blocks_before = f.blocks.len();
        let mut m = Module::new();
        m.funcs.push(f);
        let st = optimize(&mut m);
        let f = &m.funcs[0];
        // 3 < 4 ist wahr: der else-Zweig faellt weg, der then-Zweig wird in
        // bb0 verschmolzen — uebrig bleibt EIN Block mit `ret 1`.
        assert_eq!(st.removed_blocks, 2);
        assert!(f.blocks.len() < blocks_before);
        assert_eq!(f.blocks.len(), 1);
        // Block-Ids bleiben lueckenlos und passen zu ihrer Position
        for (i, b) in f.blocks.iter().enumerate() {
            assert_eq!(b.id, i as u32);
        }
        assert!(matches!(f.blocks[0].term, Term::Ret(Some(_))));
        assert_eq!(consts_in(f), vec![1]);
    }

    #[test]
    fn seiteneffekte_bleiben_erhalten() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let slot = f.alloca(4, 4);
        let v = f.push(0, FTy::I32, Op::Const(5));
        f.push_void(0, FTy::I32, Op::Store { addr: slot, val: v });
        let n = f.push(0, FTy::I64, Op::Const(60));
        let arg = f.push(0, FTy::I64, Op::Const(0));
        let sc = f.push(0, FTy::I64, Op::Syscall { args: vec![n, arg] });
        let unused = f.push(0, FTy::I32, Op::Const(99));
        let _ = unused;
        let call = f.push(0, FTy::I32, Op::Call { name: "f".into(), args: vec![] });
        let _ = call;
        let ld = f.push(0, FTy::I32, Op::Load { addr: slot });
        f.set_term(0, Term::Ret(Some(ld)));
        let _ = sc;
        let mut m = Module::new();
        m.funcs.push(f);
        let st = optimize(&mut m);
        let f = &m.funcs[0];
        // Entfernt werden: die unbenutzte Konstante 99, dazu (neu in Runde 2)
        // die tote lokale Zelle — der `load` wird auf den gespeicherten Wert
        // weitergeleitet, danach liest niemand mehr aus der `alloca`.
        // Syscall und Call MUESSEN stehen bleiben.
        assert!(st.removed_insts >= 1);
        let kinds: Vec<&str> = f.blocks[0]
            .insts
            .iter()
            .map(|i: &Inst| match &i.op {
                Op::Alloca { .. } => "alloca",
                Op::Const(_) => "const",
                Op::Store { .. } => "store",
                Op::Syscall { .. } => "syscall",
                Op::Call { .. } => "call",
                Op::Load { .. } => "load",
                _ => "?",
            })
            .collect();
        assert_eq!(kinds, vec!["const", "const", "const", "syscall", "call"]);
        assert!(matches!(f.blocks[0].term, Term::Ret(Some(v)) if v == 1 + 0));
    }

    #[test]
    fn unbenutzte_alloca_verschwindet_kettenweise() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let slot = f.alloca(8, 8);
        let off = f.push(0, FTy::I64, Op::Const(4));
        let p = f.push(0, FTy::Ptr, Op::PtrAdd { base: slot, off });
        let _ld = f.push(0, FTy::I32, Op::Load { addr: p });
        let r = f.push(0, FTy::I32, Op::Const(0));
        f.set_term(0, Term::Ret(Some(r)));
        let mut m = Module::new();
        m.funcs.push(f);
        let st = optimize(&mut m);
        assert_eq!(m.funcs[0].inst_count(), 1);
        assert_eq!(st.removed_insts, 4);
    }

    #[test]
    fn schleife_bleibt_unangetastet_und_terminiert() {
        // while (i < 10) { i = i + 1 }  — nichts davon ist konstant faltbar,
        // der Optimierer darf hier nichts entfernen und muss anhalten.
        let mut f = Func::new("t", vec![], FTy::I32);
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
        let before = f.inst_count();
        let blocks = f.blocks.len();
        let mut m = Module::new();
        m.funcs.push(f);
        let st = optimize(&mut m);
        assert_eq!(st.folded, 0);
        assert_eq!(st.removed_insts, 0);
        assert_eq!(st.removed_blocks, 0);
        assert_eq!(m.funcs[0].inst_count(), before);
        assert_eq!(m.funcs[0].blocks.len(), blocks);
    }

    #[test]
    fn kette_wird_bis_zum_fixpunkt_gefaltet() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let mut v = f.push(0, FTy::I32, Op::Const(1));
        for _ in 0..10 {
            let one = f.push(0, FTy::I32, Op::Const(1));
            v = f.push(0, FTy::I32, Op::Bin(BinOp::Add, v, one));
        }
        let neg = f.push(0, FTy::I32, Op::Un(UnOp::Neg, v));
        f.set_term(0, Term::Ret(Some(neg)));
        let mut m = Module::new();
        m.funcs.push(f);
        optimize(&mut m);
        assert_eq!(m.funcs[0].inst_count(), 1);
        assert_eq!(consts_in(&m.funcs[0]), vec![-11]);
    }
}
