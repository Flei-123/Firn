//! FIR — die eigene Zwischensprache (SPEC §8.1).
//!
//! Eigenschaften (Invarianten, siehe docs/FIR.md):
//!  * typisiert: jede Instruktion hat einen Ergebnistyp `FTy`
//!  * SSA-artig: jede Instruktion definiert hoechstens EINEN neuen Wert `%n`,
//!    jeder Wert wird genau einmal definiert
//!  * Basisbloecke mit GENAU EINEM Terminator am Ende (`br`, `brcond`, `ret`)
//!  * keine Phi-Knoten: veraenderliche Variablen liegen in `alloca`-Slots und
//!    werden mit `load`/`store` angesprochen
//!  * keine x86-Eigenheiten: Register, Stackrahmen und Aufrufkonvention
//!    entstehen erst im Backend

use std::fmt::Write as _;

pub type Val = u32;
pub type BlockId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FTy {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    Bool,
    /// Zeiger (immer 64 Bit, untypisiert in FIR)
    Ptr,
    /// kein Wert
    Void,
}

impl FTy {
    pub fn bits(self) -> u32 {
        match self {
            FTy::I8 | FTy::U8 | FTy::Bool => 8,
            FTy::I16 | FTy::U16 => 16,
            FTy::I32 | FTy::U32 => 32,
            FTy::I64 | FTy::U64 | FTy::Ptr => 64,
            FTy::Void => 0,
        }
    }
    pub fn bytes(self) -> u64 {
        (self.bits() / 8) as u64
    }
    pub fn signed(self) -> bool {
        matches!(self, FTy::I8 | FTy::I16 | FTy::I32 | FTy::I64)
    }
    pub fn name(self) -> &'static str {
        match self {
            FTy::I8 => "i8",
            FTy::I16 => "i16",
            FTy::I32 => "i32",
            FTy::I64 => "i64",
            FTy::U8 => "u8",
            FTy::U16 => "u16",
            FTy::U32 => "u32",
            FTy::U64 => "u64",
            FTy::Bool => "bool",
            FTy::Ptr => "ptr",
            FTy::Void => "void",
        }
    }
    /// Wert auf die Breite/Signedness des Typs zurechtstutzen.
    pub fn truncate(self, v: i128) -> i128 {
        let bits = self.bits();
        if bits == 0 || bits >= 128 {
            return v;
        }
        let mask: i128 = (1i128 << bits) - 1;
        let m = v & mask;
        if self == FTy::Bool {
            return if m & 1 != 0 { 1 } else { 0 };
        }
        if self.signed() && (m >> (bits - 1)) & 1 != 0 {
            m - (1i128 << bits)
        } else {
            m
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    And,
    Or,
    Xor,
    Shl,
    Shr,
}

impl BinOp {
    pub fn name(self) -> &'static str {
        match self {
            BinOp::Add => "add",
            BinOp::Sub => "sub",
            BinOp::Mul => "mul",
            BinOp::Div => "div",
            BinOp::Rem => "rem",
            BinOp::And => "and",
            BinOp::Or => "or",
            BinOp::Xor => "xor",
            BinOp::Shl => "shl",
            BinOp::Shr => "shr",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl CmpOp {
    pub fn name(self) -> &'static str {
        match self {
            CmpOp::Eq => "eq",
            CmpOp::Ne => "ne",
            CmpOp::Lt => "lt",
            CmpOp::Le => "le",
            CmpOp::Gt => "gt",
            CmpOp::Ge => "ge",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    /// arithmetische Negation
    Neg,
    /// bitweises Nicht (Ganzzahl) bzw. logisches Nicht (bool)
    Not,
}

#[derive(Clone, Debug)]
pub enum Op {
    /// Konstante des Instruktionstyps
    Const(i128),
    Bin(BinOp, Val, Val),
    Cmp { op: CmpOp, ty: FTy, a: Val, b: Val },
    Un(UnOp, Val),
    /// Umwandlung; Zieltyp ist der Instruktionstyp
    Cast { src: Val, from: FTy },
    /// Stackspeicher in der Funktion (nur im Eintrittsblock zulaessig)
    Alloca { size: u64, align: u64 },
    Load { addr: Val },
    /// kein Ergebniswert; `ty` der Instruktion ist der gespeicherte Typ
    Store { addr: Val, val: Val },
    /// Zeiger + Byte-Offset (Offset ist i64/u64)
    PtrAdd { base: Val, off: Val },
    Call { name: String, args: Vec<Val> },
    /// Linux-Syscall: args[0] = Nummer, danach bis zu 6 Argumente
    Syscall { args: Vec<Val> },
    /// Blockweise Kopie (Structs/Arrays); kein Ergebniswert
    CopyMem { dst: Val, src: Val, size: u64 },
    /// Datenunabhaengige Auswahl (SPEC §9.3): `cond ? a : b`, im Backend `cmov`.
    /// KEIN Durchgang darf daraus eine Verzweigung machen (SPEC §9.2).
    Select { cond: Val, a: Val, b: Val },
    /// Undurchsichtige Sperre (`barrier(inout x)`): liefert den Wert unveraendert
    /// zurueck, gilt aber fuer jeden Durchgang als undurchschaubar.
    Barrier { val: Val },
    /// `secure_zero(inout buf)`: nullt `size` Bytes ab `addr`. Gilt NIE als tot.
    SecureZero { addr: Val, size: Val },
}

impl Op {
    /// Rein = ohne Seiteneffekt, darf entfernt werden, wenn das Ergebnis
    /// unbenutzt ist.
    pub fn is_pure(&self) -> bool {
        match self {
            Op::Const(_)
            | Op::Bin(..)
            | Op::Cmp { .. }
            | Op::Un(..)
            | Op::Cast { .. }
            | Op::PtrAdd { .. }
            | Op::Load { .. }
            | Op::Alloca { .. }
            | Op::Select { .. } => true,
            Op::Store { .. }
            | Op::Call { .. }
            | Op::Syscall { .. }
            | Op::CopyMem { .. }
            | Op::Barrier { .. }
            | Op::SecureZero { .. } => false,
        }
    }

    /// Alle gelesenen Werte.
    pub fn uses(&self, out: &mut Vec<Val>) {
        match self {
            Op::Const(_) | Op::Alloca { .. } => {}
            Op::Bin(_, a, b) => {
                out.push(*a);
                out.push(*b);
            }
            Op::Cmp { a, b, .. } => {
                out.push(*a);
                out.push(*b);
            }
            Op::Un(_, a) => out.push(*a),
            Op::Cast { src, .. } => out.push(*src),
            Op::Load { addr } => out.push(*addr),
            Op::Store { addr, val } => {
                out.push(*addr);
                out.push(*val);
            }
            Op::PtrAdd { base, off } => {
                out.push(*base);
                out.push(*off);
            }
            Op::Call { args, .. } | Op::Syscall { args } => out.extend_from_slice(args),
            Op::CopyMem { dst, src, .. } => {
                out.push(*dst);
                out.push(*src);
            }
            Op::Select { cond, a, b } => {
                out.push(*cond);
                out.push(*a);
                out.push(*b);
            }
            Op::Barrier { val } => out.push(*val),
            Op::SecureZero { addr, size } => {
                out.push(*addr);
                out.push(*size);
            }
        }
    }

}

#[derive(Clone, Debug)]
pub struct Inst {
    pub dst: Option<Val>,
    pub ty: FTy,
    pub op: Op,
}

#[derive(Clone, Debug)]
pub enum Term {
    Br(BlockId),
    BrCond { cond: Val, then_bb: BlockId, else_bb: BlockId },
    /// Mehrfachverzweigung ueber einen Ganzzahlwert (SPEC §6.3, `P4`).
    /// `cases` ist nach Marke aufsteigend sortiert und duplikatfrei; jeder
    /// nicht genannte Wert geht nach `default`. Das Backend darf daraus eine
    /// Sprungtabelle machen, muss aber nicht.
    Switch { val: Val, ty: FTy, cases: Vec<(i128, BlockId)>, default: BlockId },
    Ret(Option<Val>),
    /// Nur waehrend des Aufbaus; darf am Ende des Lowerings nicht mehr
    /// vorkommen (Invariante: jeder Block hat einen echten Terminator).
    Unset,
}

impl Term {
    pub fn successors(&self) -> Vec<BlockId> {
        match self {
            Term::Br(b) => vec![*b],
            Term::BrCond { then_bb, else_bb, .. } => vec![*then_bb, *else_bb],
            Term::Switch { cases, default, .. } => {
                let mut v: Vec<BlockId> = cases.iter().map(|(_, b)| *b).collect();
                v.push(*default);
                v
            }
            Term::Ret(_) | Term::Unset => vec![],
        }
    }
}

#[derive(Clone, Debug)]
pub struct Block {
    pub id: BlockId,
    pub insts: Vec<Inst>,
    pub term: Term,
}

#[derive(Clone, Debug)]
pub struct Func {
    pub name: String,
    pub params: Vec<FTy>,
    pub ret: FTy,
    pub blocks: Vec<Block>,
    /// Typ jedes jemals vergebenen Wertes, indiziert mit der Val-Id.
    pub val_types: Vec<FTy>,
    /// Werte, die aus `secret[T]` stammen (SPEC §9.2). Der Optimierer und der
    /// Codegenerator behandeln sie gesondert: keine Verzweigung, kein
    /// datenabhaengiger Zugriff, kein Entfernen von Schreibvorgaengen.
    pub secret: std::collections::HashSet<Val>,
    /// `#[constant_time]`: der Codegenerator bricht ab, wenn ein bedingter
    /// Sprung von einem `secret`-Wert abhaengt.
    pub constant_time: bool,
}

impl Func {
    /// Legt eine Funktion mit Eintrittsblock `bb0` an. Die Parameter erhalten
    /// die Werte `%0 .. %(n-1)`.
    pub fn new(name: &str, params: Vec<FTy>, ret: FTy) -> Func {
        let val_types = params.clone();
        Func {
            name: name.to_string(),
            params,
            ret,
            blocks: vec![Block { id: 0, insts: Vec::new(), term: Term::Unset }],
            val_types,
            secret: std::collections::HashSet::new(),
            constant_time: false,
        }
    }

    pub fn entry(&self) -> BlockId {
        0
    }

    pub fn param_val(&self, i: usize) -> Val {
        i as Val
    }

    pub fn add_block(&mut self) -> BlockId {
        let id = self.blocks.len() as BlockId;
        self.blocks.push(Block { id, insts: Vec::new(), term: Term::Unset });
        id
    }

    fn new_val(&mut self, ty: FTy) -> Val {
        let v = self.val_types.len() as Val;
        self.val_types.push(ty);
        v
    }

    /// Haengt eine wertliefernde Instruktion an das Ende von `b` an.
    pub fn push(&mut self, b: BlockId, ty: FTy, op: Op) -> Val {
        let v = self.new_val(ty);
        self.blocks[b as usize].insts.push(Inst { dst: Some(v), ty, op });
        v
    }

    /// Haengt eine Instruktion ohne Ergebnis an (`store`, `copymem`, void-Call).
    pub fn push_void(&mut self, b: BlockId, ty: FTy, op: Op) {
        self.blocks[b as usize].insts.push(Inst { dst: None, ty, op });
    }

    /// Fuegt eine Alloca vorne im Eintrittsblock ein (Invariante: alle Allocas
    /// stehen im Eintrittsblock).
    pub fn alloca(&mut self, size: u64, align: u64) -> Val {
        let v = self.new_val(FTy::Ptr);
        let inst = Inst { dst: Some(v), ty: FTy::Ptr, op: Op::Alloca { size, align } };
        let n = self.blocks[0].insts.iter().take_while(|i| matches!(i.op, Op::Alloca { .. })).count();
        self.blocks[0].insts.insert(n, inst);
        v
    }

    pub fn set_term(&mut self, b: BlockId, t: Term) {
        let blk = &mut self.blocks[b as usize];
        if matches!(blk.term, Term::Unset) {
            blk.term = t;
        }
    }

    pub fn is_terminated(&self, b: BlockId) -> bool {
        !matches!(self.blocks[b as usize].term, Term::Unset)
    }

    /// Markiert einen Wert als geheim (SPEC §9.1).
    ///
    /// Es gibt bewusst KEINE Hilfsmethode dafuer, solange das Frontend keine
    /// `secret`-Werte erzeugen kann (`secret[T]` ist nicht umgesetzt, SPEC
    /// §14.1): eine Methode, die nur Tests aufrufen, waere toter Code. Die
    /// Menge `secret` ist oeffentlich; Tests schreiben direkt hinein, und alle
    /// Durchgaenge lesen sie ueber `is_secret`.

    pub fn is_secret(&self, v: Val) -> bool {
        self.secret.contains(&v)
    }

    pub fn val_ty(&self, v: Val) -> FTy {
        self.val_types.get(v as usize).copied().unwrap_or(FTy::Void)
    }

    /// Anzahl aller Instruktionen (fuer Optimierungstests).
    pub fn inst_count(&self) -> usize {
        self.blocks.iter().map(|b| b.insts.len()).sum()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Module {
    pub funcs: Vec<Func>,
}

impl Module {
    pub fn new() -> Module {
        Module::default()
    }
    pub fn inst_count(&self) -> usize {
        self.funcs.iter().map(|f| f.inst_count()).sum()
    }
    pub fn block_count(&self) -> usize {
        self.funcs.iter().map(|f| f.blocks.len()).sum()
    }

    /// Menschenlesbares, stabiles Textformat (siehe docs/FIR.md).
    pub fn to_text(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "; FIR v0");
        for f in &self.funcs {
            let ps: Vec<String> = f
                .params
                .iter()
                .enumerate()
                .map(|(i, t)| format!("%{}: {}", i, t.name()))
                .collect();
            let _ = writeln!(s, "fn @{}({}) -> {} {{", f.name, ps.join(", "), f.ret.name());
            for b in &f.blocks {
                let _ = writeln!(s, "bb{}:", b.id);
                for i in &b.insts {
                    let _ = writeln!(s, "  {}", fmt_inst(i));
                }
                let _ = writeln!(s, "  {}", fmt_term(&b.term));
            }
            let _ = writeln!(s, "}}");
        }
        s
    }
}

fn vlist(vs: &[Val]) -> String {
    vs.iter().map(|v| format!("%{}", v)).collect::<Vec<_>>().join(", ")
}

fn fmt_inst(i: &Inst) -> String {
    let head = match i.dst {
        Some(d) => format!("%{} = ", d),
        None => String::new(),
    };
    let t = i.ty.name();
    let body = match &i.op {
        Op::Const(c) => format!("const.{} {}", t, c),
        Op::Bin(op, a, b) => format!("{}.{} %{}, %{}", op.name(), t, a, b),
        Op::Cmp { op, ty, a, b } => format!("cmp.{}.{} %{}, %{}", op.name(), ty.name(), a, b),
        Op::Un(op, a) => match op {
            UnOp::Neg => format!("neg.{} %{}", t, a),
            UnOp::Not => format!("not.{} %{}", t, a),
        },
        Op::Cast { src, from } => format!("cast.{}.{} %{}", from.name(), t, src),
        Op::Alloca { size, align } => format!("alloca.ptr size={} align={}", size, align),
        Op::Load { addr } => format!("load.{} %{}", t, addr),
        Op::Store { addr, val } => format!("store.{} %{}, %{}", t, val, addr),
        Op::PtrAdd { base, off } => format!("ptradd.ptr %{}, %{}", base, off),
        Op::Call { name, args } => format!("call.{} @{}({})", t, name, vlist(args)),
        Op::Syscall { args } => format!("syscall.{} {}", t, vlist(args)),
        Op::CopyMem { dst, src, size } => format!("copymem %{}, %{}, size={}", dst, src, size),
        Op::Select { cond, a, b } => format!("select.{} %{}, %{}, %{}", t, cond, a, b),
        Op::Barrier { val } => format!("barrier.{} %{}", t, val),
        Op::SecureZero { addr, size } => format!("secure_zero %{}, %{}", addr, size),
    };
    format!("{}{}", head, body)
}

fn fmt_term(t: &Term) -> String {
    match t {
        Term::Br(b) => format!("br bb{}", b),
        Term::BrCond { cond, then_bb, else_bb } => {
            format!("brcond %{}, bb{}, bb{}", cond, then_bb, else_bb)
        }
        Term::Switch { val, ty, cases, default } => {
            let arms: Vec<String> =
                cases.iter().map(|(k, b)| format!("{} => bb{}", k, b)).collect();
            format!("switch.{} %{} [{}] default bb{}", ty.name(), val, arms.join(", "), default)
        }
        Term::Ret(Some(v)) => format!("ret %{}", v),
        Term::Ret(None) => "ret".to_string(),
        Term::Unset => "<unset>".to_string(),
    }
}
