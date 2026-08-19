//! x86_64-Codegenerator: FIR -> GNU-Assembler-Text (Intel-Syntax) fuer `as`/`ld`.
//! Kein LLVM, kein Cranelift, kein C — jede Instruktion wird hier selbst gewaehlt.
//!
//! SCHNITTSTELLE (fest):
//!   `pub fn emit(m: &fir::Module) -> Result<String, String>`
//!
//! Modell der Registerzuteilung (bewusst naiv, aber korrekt):
//!   * Jeder FIR-Wert `%n` bekommt einen eigenen 8-Byte-Stack-Slot im Rahmen.
//!   * Gerechnet wird ausschliesslich in den Arbeitsregistern rax/rcx (rdx fuer
//!     Division/Rest, rdi/rsi/rcx zusaetzlich fuer `copymem`).
//!   * Damit sind rbx, rbp, r12-r15 (callee-saved) nie angetastet; alle
//!     benutzten Register sind caller-saved, ueber einen `call` hinweg lebt
//!     kein Wert in einem Register.
//!
//! Rahmen (System-V-AMD64):
//!   Bei Eintritt gilt rsp % 16 == 8 (die Ruecksprungadresse liegt oben).
//!   `push rbp` macht rsp 16-ausgerichtet, `sub rsp, FRAME` mit FRAME % 16 == 0
//!   erhaelt das. Damit ist der Stack an JEDER Aufrufstelle 16-ausgerichtet.

use crate::config;
use crate::dwarf;
use crate::fir::{BinOp, Block, CmpOp, FTy, Func, Inst, Module, Op, Term, UnOp, Val};
use std::fmt::Write as _;

/// Argumentregister der System-V-AMD64-Aufrufkonvention.
pub(crate) const ARG_REGS: [&str; 6] = ["rdi", "rsi", "rdx", "rcx", "r8", "r9"];
/// Argumentregister des Linux-Syscall-ABI (nach der Nummer in rax).
const SYS_REGS: [&str; 6] = ["rdi", "rsi", "rdx", "r10", "r8", "r9"];

/// Registername in der Breite `bits` (nur fuer rax/rcx/rdx noetig).
pub(crate) fn reg(name: &str, bits: u32) -> &'static str {
    match (name, bits) {
        ("rax", 8) => "al",
        ("rax", 16) => "ax",
        ("rax", 32) => "eax",
        ("rax", _) => "rax",
        ("rcx", 8) => "cl",
        ("rcx", 16) => "cx",
        ("rcx", 32) => "ecx",
        ("rcx", _) => "rcx",
        ("rdx", 8) => "dl",
        ("rdx", 16) => "dx",
        ("rdx", 32) => "edx",
        (_, _) => "rdx",
    }
}

/// Groessenwort fuer Speicheroperanden.
pub(crate) fn size_word(bits: u32) -> &'static str {
    match bits {
        8 => "byte ptr",
        16 => "word ptr",
        32 => "dword ptr",
        _ => "qword ptr",
    }
}

/// Register, die eine `#[interrupt]`-Funktion rettet — alle Universalregister
/// ausser `rsp` (der Prozessor) und `rbp` (der gewoehnliche Prolog).
pub(crate) const INT_SAVE: &[&str] = &[
    "rax", "rcx", "rdx", "rbx", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12",
    "r13", "r14", "r15",
];

fn align_up(x: u64, a: u64) -> u64 {
    if a <= 1 {
        x
    } else {
        (x + a - 1) / a * a
    }
}

/// Rahmenaufteilung einer Funktion.
pub(crate) struct Frame {
    /// Slot-Offset je Wert-Id (Adresse = rbp - off).
    pub(crate) slot: Vec<u64>,
    /// Offset des Speichers je `alloca`-Wert (Adresse = rbp - off).
    pub(crate) alloca_off: Vec<Option<u64>>,
    pub(crate) size: u64,
}

fn layout(f: &Func) -> Frame {
    let n = f.val_types.len();
    let mut slot = vec![0u64; n];
    let mut cursor = 0u64;
    for s in slot.iter_mut() {
        cursor += 8;
        *s = cursor;
    }
    let mut alloca_off: Vec<Option<u64>> = vec![None; n];
    for b in &f.blocks {
        // Invariante von FIR: alle `alloca` stehen im Eintrittsblock. Alles
        // andere waere ein variabel grosser Rahmen — den kann Stufe 0 nicht.
        if b.id != f.entry() && b.insts.iter().any(|i| matches!(i.op, Op::Alloca { .. })) {
            continue;
        }
        for i in &b.insts {
            if let Op::Alloca { size, align } = i.op {
                if let Some(d) = i.dst {
                    // Adresse = rbp - cursor; cursor auf `align` bringen, damit
                    // die Adresse ausgerichtet ist (rbp ist 16-ausgerichtet).
                    let a = if align == 0 { 1 } else { align.min(16) };
                    cursor = align_up(cursor + size.max(1), a);
                    alloca_off[d as usize] = Some(cursor);
                }
            }
        }
    }
    Frame { slot, alloca_off, size: align_up(cursor, 16) }
}

pub(crate) struct Emitter {
    pub(crate) out: String,
}

impl Emitter {
    pub(crate) fn line(&mut self, s: &str) {
        let _ = writeln!(self.out, "    {}", s);
    }
    pub(crate) fn raw(&mut self, s: &str) {
        let _ = writeln!(self.out, "{}", s);
    }
}

/// Linker-Symbol eines Funktionsnamens.
///
/// **Einzige** Stelle, an der aus einem internen Namen ein Symbol wird — das
/// Schema selbst steht in `modules.rs` (`SYMBOL_SCHEMA`, DESIGNZIELE.md §4).
/// Interne Blocklabels (`block_label`) gehen bewusst NICHT hier durch: sie sind
/// dateilokal (`.L…`) und erscheinen nie in der Symboltabelle.
pub(crate) fn label(name: &str) -> String {
    crate::modules::symbol(name, None)
}

pub(crate) fn block_label(fname: &str, b: u32) -> String {
    format!(".L{}__bb{}", fname, b)
}

pub fn emit(m: &Module) -> Result<String, String> {
    let mut e = Emitter { out: String::new() };
    e.raw(&format!(
        "# erzeugt von {} {} — eigener x86_64-Codegenerator (kein LLVM)",
        config::compiler_name(),
        config::VERSION
    ));
    e.raw(".intel_syntax noprefix");
    // Quelldateien fuer .debug_line (dwarf.rs); leer = keine Debuginfo.
    let files = dwarf::file_directives();
    if !files.is_empty() {
        e.out.push_str(&files);
    }
    e.raw(".text");
    // RUNDE 52 (SPEC §2): im Profil `kernel` gibt es KEINEN Einstiegspunkt und
    // keinen Laufzeitvorspann. Das Ergebnis ist eine Objektdatei, die ein
    // Bootlader bzw. ein Linkerskript einbindet — `_start`, das Aufsetzen von
    // `rsp` und der `exit`-Systemaufruf waeren dort falsch.
    let freestanding = crate::prof::is_kernel();
    if !freestanding {
    e.raw(".globl _start");
    e.raw("_start:");
    e.line("xor rbp, rbp");
    // STARTBLOCK AN `main`: beim Prozessstart zeigt `rsp` auf
    //   [argc][argv0]..[argvN][0][envp0]..[0][auxv..]
    // Dieser Zeiger geht in `rdi` — also in den ERSTEN Parameter von `main`.
    // Ein Programm mit `fn main() -> i32` merkt davon nichts (es liest `rdi`
    // nie); eines mit `fn main(start: u64) -> i32` kommt damit an seine
    // Aufrufargumente. Ohne das kann `firnc1` keinen Dateinamen entgegennehmen
    // (docs/SELBSTHOSTING.md §2, Punkt 3).
    e.line("mov rdi, rsp");
    e.line("and rsp, -16");
    e.line(&format!("call {}", label("main")));
    e.line("mov edi, eax");
    e.line("mov eax, 60");
    e.line("syscall");
    e.line("hlt");
    }

    if !freestanding && !m.funcs.iter().any(|f| f.name == "main") {
        return Err("kein Einstiegspunkt: 'fn main() -> i32' fehlt".to_string());
    }

    for f in &m.funcs {
        emit_func(&mut e, f)?;
    }
    // HOOK gc: Typtabelle (.rodata) und Zustandsblock (.data) des Sammlers —
    // nur, wenn das Programm ueberhaupt ein `gc class` enthaelt (gc.rs).
    // Runde 49: auch ein Programm OHNE `gc class`, das Faeden benutzt, braucht
    // den Zustandsblock — die Fadentafel und die Sperren liegen darin.
    if crate::gc::has_classes() || crate::gc::runtime_active() {
        e.raw(&crate::gc::ty_table_asm());
    }
    // HOOK iface: die Methodentafeln (.rodata) — nur, wenn das Programm
    // ueberhaupt eine Schnittstelle umsetzt (iface.rs, Runde 46).
    if crate::iface::has_interfaces() {
        e.raw(&crate::iface::tables_asm());
    }
    e.raw(".section .note.GNU-stack,\"\",@progbits");
    Ok(e.out)
}

/// `Op::GcAddr` — Adresse des Zustandsblocks des Sammlers in `rax`.
///
/// Mit `regs` werden vorher die callee-saved Register in den Block gerettet.
/// Ohne diesen Schritt waere die Zusage „KONSERVATIVER Stapel- UND
/// Registerscan" (SPEC §3.5.3) falsch: die Registerzuteilung (`regalloc.rs`)
/// haelt Werte ueber Aufrufe hinweg in `rbx`/`r12`–`r15`.
pub(crate) fn emit_gc_addr(e: &mut Emitter, regs: bool) {
    e.line(&format!("lea rax, [rip + {}]", crate::gc::STATE_LABEL));
    if !regs {
        return;
    }
    let off = crate::gc::REG_SAVE_OFF;
    for (i, r) in ["rbx", "rbp", "r12", "r13", "r14", "r15"].iter().enumerate() {
        e.line(&format!("mov qword ptr [rax+{}], {}", off + 8 * i as u64, r));
    }
}

fn emit_func(e: &mut Emitter, f: &Func) -> Result<(), String> {
    // HOOK opt: Registerzuteilung (compiler/src/regalloc.rs). Liefert der
    // registerbewusste Pfad `None`, uebernimmt der Grundpfad darunter.
    if let Some(r) = crate::regalloc::emit_func_ra(e, f) {
        return r;
    }
    let fr = layout(f);
    e.raw("");
    e.raw(&format!(".globl {}", label(&f.name)));
    e.raw(&format!("{}:", label(&f.name)));
    if let Some((file, line)) = dwarf::fn_line(&f.name) {
        e.line(&format!(".loc {} {} 0", file + 1, line));
    }
    // RUNDE 52 (SPEC §2): `#[interrupt]` — eigene Aufrufkonvention. Der
    // Prozessor hat beim Einsprung NICHTS gerettet ausser dem
    // Unterbrechungsrahmen (ss:rsp, rflags, cs:rip); alles andere gehoert
    // dem unterbrochenen Faden und muss hier hin und zurueck.
    if f.interrupt {
        e.raw("    # interrupt: alle universalregister retten");
        for r in INT_SAVE {
            e.line(&format!("push {}", r));
        }
    }
    e.line("push rbp");
    e.line("mov rbp, rsp");
    if fr.size > 0 {
        e.line(&format!("sub rsp, {}", fr.size));
    }
    // Parameter in ihre Slots sichern: die ersten sechs Ganzzahlwoerter kommen
    // aus den Argumentregistern, alle weiteren vom Stapel des Aufrufers
    // (System V: [rbp+16], [rbp+24], ... — davor liegen die gesicherte
    // Ruecksprungadresse und das gesicherte rbp).
    for (i, _t) in f.params.iter().enumerate() {
        if i < ARG_REGS.len() {
            e.line(&format!("mov qword ptr [rbp-{}], {}", fr.slot[i], ARG_REGS[i]));
        } else {
            let off = 16 + 8 * (i - ARG_REGS.len()) as u64;
            e.line(&format!("mov rax, qword ptr [rbp+{}]", off));
            e.line(&format!("mov qword ptr [rbp-{}], rax", fr.slot[i]));
        }
    }

    for b in &f.blocks {
        e.raw(&format!("{}:", block_label(&f.name, b.id)));
        emit_block(e, f, &fr, b)?;
    }
    Ok(())
}

fn emit_block(e: &mut Emitter, f: &Func, fr: &Frame, b: &Block) -> Result<(), String> {
    for (idx, i) in b.insts.iter().enumerate() {
        // Anweisungsgenaue Quellzeile (nur ohne Optimierer, siehe dwarf.rs)
        if let Some((file, line)) = dwarf::line_at(&f.name, b.id, idx as u32) {
            e.line(&format!(".loc {} {} 0", file + 1, line));
        }
        emit_inst(e, f, fr, i)?;
    }
    match &b.term {
        Term::Br(t) => e.line(&format!("jmp {}", block_label(&f.name, *t))),
        Term::Switch { .. } => crate::codegen_switch::emit_switch(
            e,
            f,
            crate::codegen_switch::ValueSource::Frame(fr),
            &b.term,
        )?,
        Term::BrCond { cond, then_bb, else_bb } => {
            // SPEC §9.2: in `#[constant_time]`-Funktionen darf kein bedingter
            // Sprung von einem geheimen Wert abhaengen — harter Abbruch.
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
            e.line(&format!("mov al, byte ptr [rbp-{}]", fr.slot[*cond as usize]));
            e.line("test al, al");
            e.line(&format!("jnz {}", block_label(&f.name, *then_bb)));
            e.line(&format!("jmp {}", block_label(&f.name, *else_bb)));
        }
        Term::Ret(v) => {
            if let Some(v) = v {
                e.line(&format!("mov rax, qword ptr [rbp-{}]", fr.slot[*v as usize]));
            } else {
                // Runde 51: KEIN `xor eax, eax` mehr. Eine Funktion mit
                // Rueckgabetyp `void` hat keinen Ergebniswert; System V
                // laesst `rax` in diesem Fall undefiniert, und in FIR liest
                // niemand das Ergebnis eines void-Aufrufs (`Op::Call` ohne
                // `dst`). Gemessen im Tokenizer: 4.229.623 Aufrufe, also
                // ebenso viele Instruktionen fuer nichts.
                //
                // Merge R51+R52: die Bedingung `!f.interrupt` aus Runde 52
                // entfaellt damit von selbst — sie diente nur dazu, den
                // Unterbrechungsbehandlern das rax-Nullen zu ersparen.
                // Jetzt nullt es niemand mehr, das ist strikt staerker.
            }
            e.line("mov rsp, rbp");
            e.line("pop rbp");
            if f.interrupt {
                // Rueckwaerts wiederherstellen, dann `iretq`: nur diese
                // Instruktion stellt rflags, cs und rsp des unterbrochenen
                // Fadens wieder her — `ret` wuerde den Rahmen verwuersteln.
                for r in INT_SAVE.iter().rev() {
                    e.line(&format!("pop {}", r));
                }
                e.line("iretq");
            } else {
                e.line("ret");
            }
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

/// Laedt den kompletten 8-Byte-Slot eines Wertes in ein Register.
pub(crate) fn load_full(e: &mut Emitter, fr: &Frame, r: &str, v: Val) {
    e.line(&format!("mov {}, qword ptr [rbp-{}]", r, fr.slot[v as usize]));
}

/// Laedt einen Wert vorzeichen-/nullerweitert auf `to_bits` (32 oder 64).
pub(crate) fn load_ext(e: &mut Emitter, fr: &Frame, r: &str, v: Val, ty: FTy, to_bits: u32) {
    let off = fr.slot[v as usize];
    let bits = ty.bits().max(8);
    if bits >= to_bits {
        // Bereits mindestens so breit: die unteren `to_bits` Bits genuegen.
        e.line(&format!("mov {}, {} [rbp-{}]", reg(r, to_bits), size_word(to_bits), off));
        return;
    }
    match (ty.signed(), bits) {
        (true, 8) => e.line(&format!("movsx {}, byte ptr [rbp-{}]", reg(r, to_bits), off)),
        (true, 16) => e.line(&format!("movsx {}, word ptr [rbp-{}]", reg(r, to_bits), off)),
        (true, _) => e.line(&format!("movsxd {}, dword ptr [rbp-{}]", reg(r, to_bits), off)),
        (false, 8) => e.line(&format!("movzx {}, byte ptr [rbp-{}]", reg(r, to_bits.min(32)), off)),
        (false, 16) => e.line(&format!("movzx {}, word ptr [rbp-{}]", reg(r, to_bits.min(32)), off)),
        // 32 Bit vorzeichenlos: `mov e_x` nullt die oberen 32 Bit automatisch.
        (false, _) => e.line(&format!("mov {}, dword ptr [rbp-{}]", reg(r, 32), off)),
    }
}

/// Schreibt rax (voll) in den Slot des Zielwertes.
pub(crate) fn store_dst(e: &mut Emitter, fr: &Frame, d: Val, r: &str) {
    e.line(&format!("mov qword ptr [rbp-{}], {}", fr.slot[d as usize], r));
}

fn emit_inst(e: &mut Emitter, f: &Func, fr: &Frame, i: &Inst) -> Result<(), String> {
    let ty = i.ty;
    match &i.op {
        Op::Const(c) => {
            let d = i.dst.ok_or("interner Fehler: const ohne Ziel")?;
            let bits = ty.truncate(*c) as i64;
            if bits == 0 {
                e.line("xor eax, eax");
            } else {
                e.line(&format!("mov rax, {}", bits));
            }
            store_dst(e, fr, d, "rax");
        }
        Op::Bin(op, a, b) => {
            let d = i.dst.ok_or("interner Fehler: Binaeroperation ohne Ziel")?;
            emit_bin(e, fr, *op, ty, *a, *b, d)?;
        }
        Op::Cmp { op, ty: oty, a, b } => {
            let d = i.dst.ok_or("interner Fehler: Vergleich ohne Ziel")?;
            // GLEITKOMMA: `ucomisd` setzt die Flags wie ein VORZEICHENLOSER
            // Vergleich (CF/ZF), deshalb `setb`/`seta` statt `setl`/`setg`.
            // Bei NaN wird PF gesetzt und ZF/CF ebenfalls — dadurch ist jeder
            // Vergleich ausser `!=` falsch, genau wie IEEE-754 es verlangt.
            if *oty == FTy::F64 {
                // `ucomisd` setzt bei NaN ZF=PF=CF=1 — der ungeordnete Fall
                // sieht also aus wie „kleiner oder gleich". IEEE-754 verlangt
                // aber, dass JEDER Ordnungsvergleich mit NaN falsch ist.
                //
                // `seta`/`setae` pruefen `CF=0 [und ZF=0]` und sind damit von
                // sich aus richtig. Fuer `<` und `<=` werden deshalb die
                // OPERANDEN VERTAUSCHT (`a < b` wird zu `b > a`), statt
                // hinterher am Paritaetsflag herumzurechnen.
                let swap = matches!(op, CmpOp::Lt | CmpOp::Le);
                let (first, second) = if swap { (*b, *a) } else { (*a, *b) };
                load_full(e, fr, "rax", first);
                e.line("movq xmm0, rax");
                load_full(e, fr, "rax", second);
                e.line("movq xmm1, rax");
                e.line("ucomisd xmm0, xmm1");
                let cc = match op {
                    CmpOp::Eq => "sete",
                    CmpOp::Ne => "setne",
                    CmpOp::Lt | CmpOp::Gt => "seta",
                    CmpOp::Le | CmpOp::Ge => "setae",
                };
                e.line(&format!("{} al", cc));
                if matches!(op, CmpOp::Eq) {
                    // NaN == NaN: ZF ist gesetzt, PF aber auch. `setnp`
                    // blendet den ungeordneten Fall aus.
                    e.line("setnp cl");
                    e.line("and al, cl");
                }
                if matches!(op, CmpOp::Ne) {
                    // Spiegelbildlich: bei NaN ist `!=` wahr.
                    e.line("setp cl");
                    e.line("or al, cl");
                }
                e.line("movzx eax, al");
                store_dst(e, fr, d, "rax");
                return Ok(());
            }
            let bits = oty.bits().max(8);
            load_full(e, fr, "rax", *a);
            load_full(e, fr, "rcx", *b);
            e.line(&format!("cmp {}, {}", reg("rax", bits), reg("rcx", bits)));
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
            store_dst(e, fr, d, "rax");
        }
        Op::Un(op, a) => {
            let d = i.dst.ok_or("interner Fehler: Unaeroperation ohne Ziel")?;
            // GLEITKOMMA: das Vorzeichen ist EIN Bit. `neg` wuerde das ganze
            // Bitmuster als Zweierkomplement behandeln — falsch. Gekippt wird
            // deshalb nur Bit 63.
            if ty == FTy::F64 {
                if !matches!(op, UnOp::Neg) {
                    return Err("interner Fehler: '!' ist fuer f64 nicht definiert".to_string());
                }
                load_full(e, fr, "rax", *a);
                e.line("mov rcx, -9223372036854775808");
                e.line("xor rax, rcx");
                store_dst(e, fr, d, "rax");
                return Ok(());
            }
            let bits = if ty.bits() > 32 { 64 } else { 32 };
            load_full(e, fr, "rax", *a);
            match op {
                UnOp::Neg => e.line(&format!("neg {}", reg("rax", bits))),
                UnOp::Not => {
                    if ty == FTy::Bool {
                        e.line("xor eax, 1");
                    } else {
                        e.line(&format!("not {}", reg("rax", bits)));
                    }
                }
            }
            store_dst(e, fr, d, "rax");
        }
        Op::Cast { src, from } => {
            let d = i.dst.ok_or("interner Fehler: Umwandlung ohne Ziel")?;
            // GLEITKOMMA-UMWANDLUNGEN
            if ty == FTy::F64 && *from != FTy::F64 {
                // Ganzzahl -> f64. Vorzeichenbehaftet mit `cvtsi2sd`;
                // vorzeichenlose 64-Bit-Werte ueber 2^63 kann diese Instruktion
                // nicht, deshalb wird der Wert vorher auf 64 Bit gebracht und
                // der Sonderfall ehrlich benannt (SPEC §14.1.f64).
                load_ext(e, fr, "rax", *src, *from, 64);
                e.line("cvtsi2sd xmm0, rax");
                e.line("movq rax, xmm0");
                store_dst(e, fr, d, "rax");
                return Ok(());
            }
            if *from == FTy::F64 && ty != FTy::F64 {
                // f64 -> Ganzzahl, abschneidend (Richtung null), wie in C.
                load_full(e, fr, "rax", *src);
                e.line("movq xmm0, rax");
                e.line("cvttsd2si rax, xmm0");
                store_dst(e, fr, d, "rax");
                return Ok(());
            }
            if ty == FTy::Bool {
                // Sicherheitsnetz: bool enthaelt nur 0/1.
                let bits = from.bits().max(8);
                load_full(e, fr, "rax", *src);
                e.line(&format!("test {}, {}", reg("rax", bits), reg("rax", bits)));
                e.line("setne al");
                e.line("movzx eax, al");
            } else {
                load_ext(e, fr, "rax", *src, *from, 64);
            }
            store_dst(e, fr, d, "rax");
        }
        Op::GcAddr { regs } => {
            let d = i.dst.ok_or("interner Fehler: gc_state ohne Ziel")?;
            emit_gc_addr(e, *regs);
            store_dst(e, fr, d, "rax");
        }
        Op::Alloca { .. } => {
            let d = i.dst.ok_or("interner Fehler: alloca ohne Ziel")?;
            let off = fr.alloca_off[d as usize].ok_or("interner Fehler: alloca ohne Platz")?;
            e.line(&format!("lea rax, [rbp-{}]", off));
            store_dst(e, fr, d, "rax");
        }
        Op::Load { addr } => {
            let d = i.dst.ok_or("interner Fehler: load ohne Ziel")?;
            load_full(e, fr, "rcx", *addr);
            let bits = ty.bits().max(8);
            match bits {
                8 => e.line("movzx eax, byte ptr [rcx]"),
                16 => e.line("movzx eax, word ptr [rcx]"),
                32 => e.line("mov eax, dword ptr [rcx]"),
                _ => e.line("mov rax, qword ptr [rcx]"),
            }
            store_dst(e, fr, d, "rax");
        }
        Op::Store { addr, val } => {
            load_full(e, fr, "rcx", *addr);
            load_full(e, fr, "rax", *val);
            let bits = ty.bits().max(8);
            e.line(&format!("mov {} [rcx], {}", size_word(bits), reg("rax", bits)));
        }
        Op::PtrAdd { base, off } => {
            let d = i.dst.ok_or("interner Fehler: ptradd ohne Ziel")?;
            load_full(e, fr, "rax", *base);
            load_full(e, fr, "rcx", *off);
            e.line("add rax, rcx");
            store_dst(e, fr, d, "rax");
        }
        Op::Call { name, args } => {
            // Stapelargumente ab dem siebten Wort: sie liegen unmittelbar vor
            // dem `call` bei [rsp+8k]. Die 16-Byte-Ausrichtung bleibt erhalten
            // (ungerade Wortzahl bekommt ein Fuellwort).
            let stack_args = args.len().saturating_sub(ARG_REGS.len());
            let mut adjust = 8 * stack_args as u64;
            if stack_args % 2 == 1 {
                adjust += 8;
            }
            if adjust > 0 {
                e.line(&format!("sub rsp, {}", adjust));
                for (k, a) in args.iter().skip(ARG_REGS.len()).enumerate() {
                    load_full(e, fr, "rax", *a);
                    e.line(&format!("mov qword ptr [rsp+{}], rax", 8 * k));
                }
            }
            for (k, a) in args.iter().take(ARG_REGS.len()).enumerate() {
                load_full(e, fr, ARG_REGS[k], *a);
            }
            e.line(&format!("call {}", label(name)));
            if adjust > 0 {
                e.line(&format!("add rsp, {}", adjust));
            }
            if let Some(d) = i.dst {
                store_dst(e, fr, d, "rax");
            }
        }
        // Dynamischer Versand (iface.rs, Runde 46): wie `Op::Call`, nur steht
        // das Ziel in einem Register. `rax` ist im Grundpfad reines
        // Arbeitsregister und kein Argumentregister — es wird ZULETZT geladen.
        Op::CallIndirect { target, args } => {
            let stack_args = args.len().saturating_sub(ARG_REGS.len());
            let mut adjust = 8 * stack_args as u64;
            if stack_args % 2 == 1 {
                adjust += 8;
            }
            if adjust > 0 {
                e.line(&format!("sub rsp, {}", adjust));
                for (k, a) in args.iter().skip(ARG_REGS.len()).enumerate() {
                    load_full(e, fr, "rax", *a);
                    e.line(&format!("mov qword ptr [rsp+{}], rax", 8 * k));
                }
            }
            for (k, a) in args.iter().take(ARG_REGS.len()).enumerate() {
                load_full(e, fr, ARG_REGS[k], *a);
            }
            load_full(e, fr, "rax", *target);
            e.line("call rax");
            if adjust > 0 {
                e.line(&format!("add rsp, {}", adjust));
            }
            if let Some(d) = i.dst {
                store_dst(e, fr, d, "rax");
            }
        }
        Op::VtabAddr { table } => {
            let d = i.dst.ok_or("interner Fehler: vtab ohne Ziel")?;
            e.line(&format!(
                "lea rax, [rip + {}]",
                crate::iface::table_label(table)
            ));
            store_dst(e, fr, d, "rax");
        }
        Op::Syscall { args } => {
            if args.is_empty() {
                return Err("interner Fehler: syscall ohne Nummer".to_string());
            }
            if args.len() > 7 {
                return Err("syscall mit mehr als 6 Argumenten".to_string());
            }
            for (k, a) in args.iter().skip(1).enumerate() {
                load_full(e, fr, SYS_REGS[k], *a);
            }
            load_full(e, fr, "rax", args[0]);
            e.line("syscall");
            if let Some(d) = i.dst {
                store_dst(e, fr, d, "rax");
            }
        }
        Op::Select { cond, a, b } => {
            // Datenunabhaengige Auswahl: `cmov`, niemals ein Sprung (SPEC §9.2).
            let d = i.dst.ok_or("interner Fehler: select ohne Ziel")?;
            load_full(e, fr, "rdx", *cond);
            load_full(e, fr, "rax", *b);
            load_full(e, fr, "rcx", *a);
            e.line("test dl, dl");
            e.line("cmovnz rax, rcx");
            store_dst(e, fr, d, "rax");
        }
        Op::Barrier { val } => {
            // Undurchsichtig: der Wert geht durch ein leeres asm-Nadeloehr.
            let d = i.dst.ok_or("interner Fehler: barrier ohne Ziel")?;
            load_full(e, fr, "rax", *val);
            e.raw("    # barrier: undurchsichtig fuer jeden Optimierungsdurchgang");
            store_dst(e, fr, d, "rax");
        }
        Op::SecureZero { addr, size } => {
            // Byteweises Nullen; darf nie entfernt werden (SPEC §9.3 C3).
            load_full(e, fr, "rdi", *addr);
            load_full(e, fr, "rcx", *size);
            e.line("xor eax, eax");
            e.line("cld");
            e.line("rep stosb");
        }
        Op::AtomicAdd { addr, val } => {
            // Runde 47 (atomic.rs): `lock xadd` — eine Instruktion, Ergebnis
            // ist der ALTE Wert.
            let d = i.dst.ok_or("interner Fehler: atomadd ohne Ziel")?;
            load_full(e, fr, "rcx", *addr);
            load_full(e, fr, "rax", *val);
            e.line("lock xadd qword ptr [rcx], rax");
            store_dst(e, fr, d, "rax");
        }
        // Runde 49 (thread.rs): Vergleichs-Tausch, Fadenerzeugung, Selbstzeiger.
        Op::AtomicCas { addr, erw, new } => {
            let d = i.dst.ok_or("interner Fehler: atomcas ohne Ziel")?;
            load_full(e, fr, "rcx", *addr);
            load_full(e, fr, "rdx", *new);
            load_full(e, fr, "rax", *erw);
            crate::thread::cas_sequence(e);
            store_dst(e, fr, d, "rax");
        }
        Op::ThreadSpawn { arg, stack, ctid } => {
            let d = i.dst.ok_or("interner Fehler: spawn ohne Ziel")?;
            load_full(e, fr, "rdi", *arg);
            load_full(e, fr, "rsi", *stack);
            load_full(e, fr, "rdx", *ctid);
            crate::thread::spawn_sequence(e);
            store_dst(e, fr, d, "rax");
        }
        Op::ThreadSelf => {
            let d = i.dst.ok_or("interner Fehler: threadself ohne Ziel")?;
            crate::thread::self_sequence(e);
            store_dst(e, fr, d, "rax");
        }
        Op::CopyMem { dst, src, size } => {
            load_full(e, fr, "rdi", *dst);
            load_full(e, fr, "rsi", *src);
            e.line(&format!("mov rcx, {}", size));
            e.line("cld");
            e.line("rep movsb");
        }
        // RUNDE 52 (core.rs, SPEC §2): Inline-Assembler. IMMER volatile —
        // die Zeilen stehen genau einmal und genau hier.
        Op::Asm { template, out, in_regs, ins, clobber } => {
            e.raw("    # asm (volatile): darf weder entfernt noch verschoben werden");
            for (r, v) in in_regs.iter().zip(ins.iter()) {
                let stem = crate::core::stem(r)
                    .ok_or_else(|| format!("unbekanntes asm-register '{}'", r))?;
                load_full(e, fr, stem, *v);
            }
            for line in template.split('\n') {
                e.line(line);
            }
            if let Some(r) = out {
                let stem = crate::core::stem(r)
                    .ok_or_else(|| format!("unbekanntes asm-register '{}'", r))?;
                let d = i.dst.ok_or("interner Fehler: asm mit out ohne Ziel")?;
                store_dst(e, fr, d, stem);
            }
            if !clobber.is_empty() {
                e.raw(&format!("    # asm clobber: {}", clobber.join(", ")));
            }
        }
        // RUNDE 52: MMIO — genau EIN Speicherzugriff je Quellzeile.
        Op::MmioLoad { addr } => {
            let d = i.dst.ok_or("interner Fehler: mmio_load ohne Ziel")?;
            load_full(e, fr, "rcx", *addr);
            let bits = ty.bits();
            match bits {
                8 | 16 => e.line(&format!("movzx eax, {} [rcx]", size_word(bits))),
                32 => e.line("mov eax, dword ptr [rcx]"),
                _ => e.line("mov rax, qword ptr [rcx]"),
            }
            store_dst(e, fr, d, "rax");
        }
        Op::MmioStore { addr, val } => {
            load_full(e, fr, "rcx", *addr);
            load_full(e, fr, "rax", *val);
            let bits = ty.bits();
            e.line(&format!(
                "mov {} [rcx], {}",
                size_word(bits),
                reg("rax", bits)
            ));
        }
    }
    let _ = f;
    Ok(())
}

fn emit_bin(
    e: &mut Emitter,
    fr: &Frame,
    op: BinOp,
    ty: FTy,
    a: Val,
    b: Val,
    d: Val,
) -> Result<(), String> {
    let wide = ty.bits() > 32;
    let bits = if wide { 64 } else { 32 };
    // GLEITKOMMA laeuft ueber die SSE-Einheit. Gerechnet wird in xmm0/xmm1,
    // gelesen und geschrieben wird ueber rax — ein `f64` liegt im Rahmen als
    // gewoehnliches 64-Bit-Wort (sein Bitmuster), deshalb braucht es hier
    // keinen eigenen Speicherpfad.
    if ty == FTy::F64 {
        let m = match op {
            BinOp::Add => "addsd",
            BinOp::Sub => "subsd",
            BinOp::Mul => "mulsd",
            BinOp::Div => "divsd",
            _ => {
                return Err(format!(
                    "interner Fehler: operator '{:?}' ist fuer f64 nicht definiert",
                    op
                ))
            }
        };
        load_full(e, fr, "rax", a);
        e.line("movq xmm0, rax");
        load_full(e, fr, "rax", b);
        e.line("movq xmm1, rax");
        e.line(&format!("{} xmm0, xmm1", m));
        e.line("movq rax, xmm0");
        store_dst(e, fr, d, "rax");
        return Ok(());
    }
    match op {
        BinOp::Add | BinOp::Sub | BinOp::And | BinOp::Or | BinOp::Xor | BinOp::Mul => {
            // Die niederwertigen Bits sind bei diesen Operationen unabhaengig
            // von der Breite; deshalb wird in 32/64 Bit gerechnet und beim
            // Lesen auf die Typbreite zurechtgeschnitten.
            load_full(e, fr, "rax", a);
            load_full(e, fr, "rcx", b);
            let m = match op {
                BinOp::Add => "add",
                BinOp::Sub => "sub",
                BinOp::And => "and",
                BinOp::Or => "or",
                BinOp::Xor => "xor",
                _ => "imul",
            };
            e.line(&format!("{} {}, {}", m, reg("rax", bits), reg("rcx", bits)));
            store_dst(e, fr, d, "rax");
        }
        BinOp::Div | BinOp::Rem => {
            // Operanden exakt auf die Rechenbreite bringen (obere Bits im Slot
            // sind nicht garantiert), dann idiv/div passend zum Vorzeichen.
            load_ext(e, fr, "rax", a, ty, bits);
            load_ext(e, fr, "rcx", b, ty, bits);
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
            store_dst(e, fr, d, res);
        }
        BinOp::Shl | BinOp::Shr => {
            // Linker Operand exakt erweitern, damit `shr`/`sar` auch bei
            // 8/16-Bit-Typen die richtigen Bits nachziehen.
            load_ext(e, fr, "rax", a, ty, bits);
            load_full(e, fr, "rcx", b);
            let m = match (op, ty.signed()) {
                (BinOp::Shl, _) => "shl",
                (_, true) => "sar",
                (_, false) => "shr",
            };
            e.line(&format!("{} {}, cl", m, reg("rax", bits)));
            store_dst(e, fr, d, "rax");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fir::{Func, Module, Op, Term};

    fn simple_module() -> Module {
        let mut f = Func::new("main", vec![], FTy::I32);
        let c = f.push(0, FTy::I32, Op::Const(42));
        f.set_term(0, Term::Ret(Some(c)));
        Module { funcs: vec![f] }
    }

    #[test]
    fn generated_start_and_prolog() {
        let s = emit(&simple_module()).expect("codegen");
        assert!(s.contains("_start:"));
        assert!(s.contains("push rbp"));
        // Je nach Registerzuteilung 'mov rax, 42' oder 'mov eax, 42'.
        assert!(s.contains("mov rax, 42") || s.contains("mov eax, 42"), "{}", s);
        assert!(s.contains("mov eax, 60"));
    }

    #[test]
    fn frame_is_16_aligned() {
        let mut f = Func::new("main", vec![], FTy::I32);
        let p = f.alloca(12, 4);
        let c = f.push(0, FTy::I32, Op::Const(1));
        f.push_void(0, FTy::I32, Op::Store { addr: p, val: c });
        f.set_term(0, Term::Ret(Some(c)));
        let fr = layout(&f);
        assert_eq!(fr.size % 16, 0);
        assert!(fr.size >= 12);
    }

    /// Mehr als sechs Parameter: die weiteren liegen auf dem Stapel des
    /// Aufrufers, die 16-Byte-Ausrichtung bleibt erhalten (abi.rs, SPEC §13).
    #[test]
    fn stack_args_ab_the_seventh_word() {
        let mut m = Module::new();
        let mut f = Func::new("f", vec![FTy::I64; 8], FTy::I64);
        let p7 = f.param_val(7);
        f.set_term(0, Term::Ret(Some(p7)));
        m.funcs.push(f);
        let mut g = Func::new("main", vec![], FTy::I32);
        let mut args = Vec::new();
        for k in 0..8 {
            args.push(g.push(0, FTy::I64, Op::Const(k as i128)));
        }
        let r = g.push(0, FTy::I64, Op::Call { name: "f".to_string(), args });
        let rc = g.push(0, FTy::I32, Op::Cast { src: r, from: FTy::I64 });
        g.set_term(0, Term::Ret(Some(rc)));
        m.funcs.push(g);
        let asm = emit(&m).expect("codegen");
        // Seit Runde 43 uebernimmt der Registerpfad auch diesen Fall; die
        // Aufrufkonvention ist in BEIDEN Pfaden dieselbe, deshalb prueft der
        // Test nur noch sie und nicht mehr den erzeugenden Pfad.
        assert!(asm.contains("qword ptr [rbp+16]"), "{}", asm);
        assert!(asm.contains("qword ptr [rbp+24]"), "{}", asm);
        assert!(asm.contains("sub rsp, 16"), "{}", asm);
        assert!(asm.contains("mov qword ptr [rsp+0], rax"), "{}", asm);
        assert!(asm.contains("mov qword ptr [rsp+8], rax"), "{}", asm);
        assert!(asm.contains("add rsp, 16"), "{}", asm);
    }
}
