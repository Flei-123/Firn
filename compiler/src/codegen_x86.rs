// SPDX-License-Identifier: GPL-2.0-only
//! x86_64 code generator: FIR -> GNU assembler text (Intel syntax) for `as`/`ld`.
//! No LLVM, no Cranelift, no C — every instruction is chosen here by hand.
//!
//! INTERFACE (fixed):
//!   `pub fn emit(m: &fir::Module) -> Result<String, String>`
//!
//! Model of the register allocation (deliberately naive, yet correct):
//!   * Every FIR value `%n` gets its own 8-byte stack slot in the frame.
//!   * Computing happens exclusively at the scratch registers rax/rcx (rdx for
//!     division/remainder, rdi/rsi/rcx additionally for `copymem`).
//!   * Thereby rbx, rbp, r12-r15 (callee-saved) are never touched; every
//!     register used is caller-saved, and across a `call` no value lives in
//!     a register.
//!
//! Frame (System V AMD64):
//!   At entry rsp % 16 == 8 holds (the return address sits on top).
//!   `push rbp` makes rsp 16-aligned, `sub rsp, FRAME` with FRAME % 16 == 0
//!   keeps it. The stack is thereby 16-aligned at EVERY call site.

use crate::config;
use crate::dwarf;
use crate::fir::{BinOp, Block, CmpOp, FTy, Func, Inst, Module, Op, Term, UnOp, Val, WrapSatKind};
use std::fmt::Write as _;

/// Argument registers of the System V AMD64 calling convention.
pub(crate) const ARG_REGS: [&str; 6] = ["rdi", "rsi", "rdx", "rcx", "r8", "r9"];
/// **ROUND 71** — argument registers of the SSE class. `f32` and `f64` travel
/// here, not in the integer registers any more (SPEC §14.1.f32).
pub(crate) const SSE_REGS: [&str; 8] = [
    "xmm0", "xmm1", "xmm2", "xmm3", "xmm4", "xmm5", "xmm6", "xmm7",
];
/// Argument registers of the Linux syscall ABI (after the number at rax).
const SYS_REGS: [&str; 6] = ["rdi", "rsi", "rdx", "r10", "r8", "r9"];

/// Register name at the width `bits` (needed for rax/rcx/rdx only).
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

/// Size word for memory operands.
pub(crate) fn size_word(bits: u32) -> &'static str {
    match bits {
        8 => "byte ptr",
        16 => "word ptr",
        32 => "dword ptr",
        _ => "qword ptr",
    }
}

/// Registers that one `#[interrupt]` function rescues — every general purpose
/// register except `rsp` (the processor) and `rbp` (the ordinary prologue).
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

/// Frame partition of a function.
pub(crate) struct Frame {
    /// slot offset per value id (address = rbp - off).
    pub(crate) slot: Vec<u64>,
    /// offset of the storage per `alloca` value (address = rbp - off).
    pub(crate) alloca_off: Vec<Option<u64>>,
    pub(crate) size: u64,
}

fn layout(f: &Func) -> Frame {
    let n = f.val_types.len();
    let mut slot = vec![0u64; n];
    let mut cursor = 0u64;
    for (idx, s) in slot.iter_mut().enumerate() {
        // ROUND 82: a `v128` value needs SIXTEEN octets, and it wants them
        // sixteen byte aligned so that `movdqa` may reach them. `rbp` is
        // 16-aligned, so an offset that is a multiple of sixteen suffices.
        if f.val_types.get(idx) == Some(&FTy::V128) {
            cursor = align_up(cursor + 16, 16);
        } else {
            cursor += 8;
        }
        *s = cursor;
    }
    let mut alloca_off: Vec<Option<u64>> = vec![None; n];
    for b in &f.blocks {
        // Invariant of FIR: all `alloca` stand in the entry block. Anything
        // else would be a frame of variable size — stage 0 cannot do that.
        if b.id != f.entry() && b.insts.iter().any(|i| matches!(i.op, Op::Alloca { .. })) {
            continue;
        }
        for i in &b.insts {
            if let Op::Alloca { size, align } = i.op {
                if let Some(d) = i.dst {
                    // address = rbp - cursor; bring cursor to `align`, so that
                    // the address is aligned (rbp is 16-aligned).
                    let a = if align == 0 { 1 } else { align.min(16) };
                    cursor = align_up(cursor + size.max(1), a);
                    alloca_off[d as usize] = Some(cursor);
                }
            }
        }
    }
    Frame { slot, alloca_off, size: align_up(cursor, 16) }
}

#[derive(Default)]
pub(crate) struct Emitter {
    pub(crate) out: String,
    /// **ROUND 90** — the COLD half of the current function.
    ///
    /// A checked operation is two things: an instruction, and an arm that
    /// prints a message and never comes back. Until round 90 both stood in
    /// the same instruction stream, so the loop that a `release-safe` build
    /// really executes read
    ///
    ///     push rax / push rcx / <op> / jc site / add rsp, 16 / jmp ok
    ///     site: pop rcx / pop rdx / lea rdi,msg / mov esi / mov r8 / mov r9 / jmp panic
    ///     ok:
    ///
    /// -- four instructions and two memory writes of pure overhead per
    /// arithmetic operation, plus six instructions of message building
    /// sitting in the middle of the hot instruction cache lines. Measured:
    /// the checks cost 1.90x in the median (tools/bench90), `matmul` 7.01x.
    ///
    /// Everything after `jc` belongs here instead. The hot path becomes
    /// `<op> / jc site` -- one conditional branch, forward, not taken, which
    /// is the cheapest shape x86 has -- and the whole panic arm is flushed
    /// out behind the `ret` of the function it belongs to.
    pub(crate) cold: String,
    /// **ROUND 82** — the `xmm` value cache of the base path (`simd.rs`).
    /// Empty and harmless in every function without a `v128` in it.
    pub(crate) xmm: crate::simd::XmmCache,
    /// Round 64: everything the debugger needs about the functions
    /// emitted so far. Empty as soon as no debug information is produced.
    pub(crate) debug_funcs: Vec<crate::dwarf_info::FnInfo>,
    /// **ROUND 94** — the position the last `.loc` announced. A `.loc` holds
    /// until the next one, so repeating it would only blow up the line
    /// program; leaving one out where the position CHANGED would be a lie.
    pub(crate) last_loc: Option<crate::fir::Loc>,
    /// **ROUND 94** — the position of the instruction being emitted. The
    /// panic arms of the cold half (`panic_rt.rs`) read it: they are the code
    /// of exactly that source line, only moved behind the `ret`.
    pub(crate) here: crate::fir::Loc,
}

impl Emitter {
    /// **ROUND 94** — announce the source position of what comes next.
    ///
    /// Three rules, and every one of them exists so that the line table can
    /// be believed:
    ///   * a position of `line == 0` (runtime code, frame bookkeeping) emits
    ///     NOTHING and leaves `here` alone — the assembler keeps the previous
    ///     line, which is what an interleaved runtime helper deserves;
    ///   * the same position twice emits one `.loc`, not two;
    ///   * a changed position always emits, at every build level. This is the
    ///     difference to round 64, where the whole line table was switched
    ///     off as soon as the optimizer ran.
    pub(crate) fn loc_at(&mut self, l: crate::fir::Loc) {
        if l.is_none() || !dwarf::with_lines() {
            return;
        }
        self.here = l;
        if self.last_loc == Some(l) {
            return;
        }
        self.last_loc = Some(l);
        let _ = writeln!(self.out, "    .loc {} {} {}", l.file + 1, l.line, l.col);
    }
    /// **ROUND 94** — the same for the cold half. The cold buffer is flushed
    /// behind the function, so its `.loc` has to be written out in full: the
    /// assembler reads the text in the order it stands in.
    pub(crate) fn cold_loc_here(&mut self) {
        let l = self.here;
        if l.is_none() {
            return;
        }
        let _ = writeln!(self.cold, "    .loc {} {} {}", l.file + 1, l.line, l.col);
    }
    /// **ROUND 94** — after the cold half the assembler's idea of the current
    /// line is whatever the last panic arm said. Forget ours, so the next
    /// function announces its own position instead of assuming it.
    pub(crate) fn forget_loc(&mut self) {
        self.last_loc = None;
        self.here = crate::fir::Loc::NONE;
    }
    pub(crate) fn line(&mut self, s: &str) {
        let _ = writeln!(self.out, "    {}", s);
    }
    pub(crate) fn raw(&mut self, s: &str) {
        let _ = writeln!(self.out, "{}", s);
    }
    /// ROUND 90 — one instruction of the COLD half (see the field).
    pub(crate) fn cold_line(&mut self, s: &str) {
        let _ = writeln!(self.cold, "    {}", s);
    }
    /// ROUND 90 — a label of the COLD half.
    pub(crate) fn cold_raw(&mut self, s: &str) {
        let _ = writeln!(self.cold, "{}", s);
    }
    /// ROUND 90 — put the cold half behind the function it belongs to. Has
    /// to be called at the END of every function, from both backends; the
    /// blocks in it are reached only by an explicit jump, never by falling
    /// through, so their position is free.
    pub(crate) fn flush_cold(&mut self) {
        if self.cold.is_empty() {
            return;
        }
        let c = std::mem::take(&mut self.cold);
        self.out.push_str(&c);
        self.forget_loc();
    }
}

/// Linker symbol of a function name.
///
/// **The only** place at which an internal name becomes a symbol — the
/// scheme itself stands at `modules.rs` (`SYMBOL_SCHEMA`, DESIGN_GOALS.md §4).
/// Internal block labels (`block_label`) deliberately do NOT pass through
/// here: they are file local (`.L…`) and never appear in the symbol table.
///
/// **Round 75 (SPEC §14.5)** — two escape hatches from the `_F0.` scheme,
/// both looked up in `extfn.rs`:
///   * `name` is an `extern fn` — `Op::Call { name, .. }` reaches here from
///     a CALLER's body and has to produce the symbol the C side defines,
///     unmangled.
///   * `name` carries `#[export_c]` — the function itself is emitted here
///     (`emit_func`'s `.globl label(&f.name)` / `label(&f.name):`) and has
///     to come out under its bare Firn name so C can call it.
/// Every other name keeps going through `modules::symbol` exactly as
/// before — this is additive, nothing about the existing scheme changed.
pub(crate) fn label(name: &str) -> String {
    if let Some(link) = crate::extfn::extern_link_name(name) {
        // ROUND WINDOWS: a call that LEAVES the program has to speak Win64,
        // and it does so through the thunk `win.rs` writes for it. The
        // symbol of the DLL function itself never appears in a `call` --
        // it lives in the import address table.
        if crate::target::windows() {
            if let Some(t) = crate::win::note(&link) {
                return t;
            }
        }
        return link;
    }
    if let Some(export) = crate::extfn::export_link_name(name) {
        return export;
    }
    crate::modules::symbol(name, None)
}

pub(crate) fn block_label(fname: &str, b: u32) -> String {
    // Round 58: like `symbol` — the `#` of a generated closure name is no
    // assembler character and becomes a dot.
    format!(".L{}__bb{}", fname.replace('#', "."), b)
}

pub fn emit(m: &Module) -> Result<String, String> {
    let mut e = Emitter {
        out: String::new(),
        cold: String::new(),
        debug_funcs: Vec::new(),
        xmm: Default::default(),
        last_loc: None,
        here: crate::fir::Loc::NONE,
    };
    e.raw(&format!(
        "# generated by {} {} — own x86_64 code generator (no LLVM)",
        config::compiler_name(),
        config::VERSION
    ));
    e.raw(".intel_syntax noprefix");
    // Source files for .debug_line (dwarf.rs); empty = no debug info.
    let files = dwarf::file_directives();
    if !files.is_empty() {
        e.out.push_str(&files);
    }
    e.raw(".text");
    // ROUND 52 (SPEC §2): under the profile `kernel` there is NO entry point and
    // no runtime prologue. The result is an object file that a boot loader or
    // a linker script pulls in — `_start`, setting up `rsp` and the
    // `exit` system call would be wrong there.
    let freestanding = crate::prof::is_kernel();
    // ROUND WINDOWS: the entry point is a different one. The loader hands
    // over no stack block, `exit` is not a system call, and the standard
    // handles have to be fetched before anything can report an error --
    // `win.rs::start_asm` writes the four calls that do it.
    if !freestanding && crate::target::windows() {
        let gc = if crate::gc::runtime_active() {
            Some(label(crate::gc::FN_INIT))
        } else {
            None
        };
        e.raw(&crate::win::start_asm(
            gc.as_deref(),
            &label(crate::win_seam::INIT_FN),
            &label(crate::win_seam::ARGV_FN),
            &label("main"),
        ));
    }
    if !freestanding && !crate::target::windows() {
    e.raw(".globl _start");
    e.raw("_start:");
    e.line("xor rbp, rbp");
    // HOOK gc (ROUND 88): THE COLLECTOR STARTS ITSELF.
    //
    // Whoever writes `let b = a + " and more"` allocates on the GC heap --
    // and up to round 87 the program then died with exit code 70 and
    // `firn-gc: gc_init() was not called`, because the collector waited for
    // a call the beginner had never read about anywhere. The compiler knows
    // at this point that it has linked the runtime in (gc.rs, the token
    // signal of SPEC 8.0); so it also writes the setup.
    //
    // HERE and not in `main`: this is the first instruction of the process,
    // before the first instruction of the user, exactly once, and no source
    // text has to be rewritten for it. `gc_init` is idempotent (`S_INIT` in
    // lib/gc/gc.fi) -- an explicit `if !gc_init() { … }` in the program
    // keeps working and just gets `true` back. `gc_set_max_bytes` afterwards
    // keeps working too; it writes its own word of the state block.
    //
    // Under the kernel profile nothing of this arises: there is no `_start`
    // there at all (`freestanding` above), and hence no collector.
    if crate::gc::runtime_active() {
        e.line(&format!("call {}", label(crate::gc::FN_INIT)));
    }
    // START BLOCK AT `main`: at process start `rsp` points to
    //   [argc][argv0]..[argvN][0][envp0]..[0][auxv..]
    // That pointer goes to `rdi` — that is, to the FIRST parameter of `main`.
    // A program with `fn main() -> i32` notices nothing of it (it never reads
    // `rdi`); one with `fn main(start: u64) -> i32` reaches its call arguments
    // that way. Without it `firnc1` could accept no file name
    // (docs/SELF_HOSTING.md §2, point 3).
    e.line("mov rdi, rsp");
    e.line("and rsp, -16");
    e.line(&format!("call {}", label("main")));
    e.line("mov edi, eax");
    e.line("mov eax, 60");
    e.line("syscall");
    e.line("hlt");
    }

    if !freestanding && !m.funcs.iter().any(|f| f.name == "main") {
        return Err("no entry point: 'fn main() -> i32' is missing".to_string());
    }

    for f in &m.funcs {
        emit_func(&mut e, f)?;
    }
    // ROUND 64: the address range of the compilation unit.
    if dwarf::with_variables() {
        e.raw(".Ltext_end:");
    }
    // HOOK gc: type table (.rodata) and state block (.data) of the collector —
    // only when the program contains a `gc class` at all (gc.rs).
    // Round 49: a program WITHOUT `gc class` that uses threads needs the state
    // block as well — the thread table and the locks sit in it.
    if crate::gc::has_classes() || crate::gc::runtime_active() {
        e.raw(&crate::gc::ty_table_asm());
    }
    // HOOK iface: the method tables (.rodata) — only when the program
    // implements an interface at all (iface.rs, round 46).
    if crate::iface::has_interfaces() {
        e.raw(&crate::iface::tables_asm());
    }
    // HOOK fnval: the function records (.rodata) — only when the program
    // takes a function as a value at all (fnval.rs, round 58).
    if crate::fnval::has_records() {
        e.raw(&crate::fnval::records_asm());
    }
    // HOOK panic_rt: the shared out-of-line panic trampoline and its message
    // table (round 72, SPEC §13) — only when the program contains at least
    // one checked arithmetic operation at all. A program built
    // `--opt-level=release-fast`, or one that never reached a checked path
    // in lowering, carries neither one byte of this.
    if crate::panic_rt::any_registered() {
        e.raw(&crate::panic_rt::rodata_asm());
        // ROUND 94: the shared trampoline is RUNTIME, not program text. It
        // used to inherit the last `.loc` of the function in front of it, so
        // `gdb` attributed the panic formatter to some innocent source line.
        // Line 0 is DWARF's way of saying "no source line here".
        if dwarf::with_lines() {
            e.raw("    .loc 1 0 0");
            e.forget_loc();
        }
        e.raw(&crate::panic_rt::trampoline_asm());
    }
    // HOOK statics: `.bss`/`.data`/`.rodata` of the global variables
    // (round 89, SPEC 14.1.statics) — only when the program declares a
    // `static` at all.
    if crate::statics::any() {
        e.raw(&crate::statics::data_asm());
    }
    // ROUND 64: `.debug_abbrev` and `.debug_info` of our own -- names, types
    // and variables. The line table stays with the assembler (`.loc`).
    if dwarf::with_variables() && !e.debug_funcs.is_empty() {
        let files = dwarf::files();
        let unit = files.first().cloned().unwrap_or_default();
        let dir = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".to_string());
        let producer = format!("{} {}", config::compiler_name(), config::VERSION);
        let start = if freestanding {
            e.debug_funcs
                .first()
                .map(|f| f.start_label.clone())
                .unwrap_or_else(|| ".Ltext_end".to_string())
        } else {
            "_start".to_string()
        };
        let funcs = std::mem::take(&mut e.debug_funcs);
        let (abbrev, info) =
            crate::dwarf_info::build(&producer, &unit, &dir, &start, ".Ltext_end", &funcs);
        e.raw(&abbrev);
        e.raw(&info);
        e.raw(".text");
    }
    if crate::target::windows() {
        // The thunks, the stack probe and the import table -- everything the
        // PE image needs and nothing that a Linux build would carry.
        e.raw(&crate::win::runtime_asm());
    } else {
        e.raw(".section .note.GNU-stack,\"\",@progbits");
    }
    Ok(e.out)
}

/// `Op::GcAddr` — address of the state block of the collector in `rax`.
///
/// With `regs` the callee-saved registers get rescued into the block first.
/// Without that step the promise "CONSERVATIVE stack AND register scan"
/// (SPEC §3.5.3) would be false: the register allocation (`regalloc.rs`)
/// keeps values across calls in `rbx`/`r12`–`r15`.
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

/// **The frame — and on Windows the walk down to it.**
///
/// `sub rsp, N` is all a Linux frame needs: the kernel grows the stack at
/// the first touch, wherever that touch lands. Windows does not. A thread
/// stack there is reserved address space with a PAGE_GUARD page at its
/// lower end; only touching THAT page commits one more and moves the guard
/// down. A function that drops `rsp` by 40 KiB in one instruction and then
/// writes at the bottom of its frame steps clean over the guard into
/// reserved memory, and the process dies at a place that has nothing to do
/// with the mistake.
///
/// So every frame of a page or more walks down page by page first
/// (`win.rs::chkstk_asm`). This is not a corner case: `let buf: [u8; 8192]`
/// is one, and every parser in this repository has one.
pub(crate) fn emit_frame(e: &mut Emitter, size: u64) {
    if crate::target::windows() && size >= crate::win::PAGE {
        crate::win::note_probe();
        // `rax` is free here: the arguments are in the argument registers
        // and no value has been placed yet.
        e.line(&format!("mov rax, {}", size));
        e.line(&format!("call {}", crate::win::CHKSTK));
    }
    e.line(&format!("sub rsp, {}", size));
}

fn emit_func(e: &mut Emitter, f: &Func) -> Result<(), String> {
    // HOOK opt: register allocation (compiler/src/regalloc.rs). Once the
    // register aware path yields `None`, the base path below takes over.
    if let Some(r) = crate::regalloc::emit_func_ra(e, f) {
        return r;
    }
    let fr = layout(f);
    // ROUND 82: which vector value may lose its register when, without
    // being written back (simd.rs). One pass over the instructions.
    crate::simd::xplan(e, f, &fr);
    e.raw("");
    e.raw(&format!(".globl {}", label(&f.name)));
    e.raw(&format!("{}:", label(&f.name)));
    e.forget_loc();
    if let Some((file, line)) = dwarf::fn_line(&f.name) {
        e.loc_at(crate::fir::Loc { file, line, col: 0 });
    }
    // ROUND 52 (SPEC §2): `#[interrupt]` — a calling convention of its own. On
    // entry the processor has rescued NOTHING except the interrupt frame
    // (ss:rsp, rflags, cs:rip); everything else belongs to the interrupted
    // thread and has to travel here and back.
    if f.interrupt {
        e.raw("    # interrupt: save all general purpose registers");
        for r in INT_SAVE {
            e.line(&format!("push {}", r));
        }
    }
    e.line("push rbp");
    e.line("mov rbp, rsp");
    if fr.size > 0 {
        emit_frame(e, fr.size);
    }
    // Save the parameters into their slots: the first six integer words come
    // from the argument registers, all further ones off the stack of the
    // caller (System V: [rbp+16], [rbp+24], ... — in front of those sit the
    // saved return address and the saved rbp).
    // ROUND 71: the same placement rule as at the call site, read from the
    // other side. `place_args` is asked with the parameter VALUES (%0, %1,
    // ...), whose types are exactly `f.params`.
    let pvals: Vec<Val> = (0..f.params.len() as Val).collect();
    let (spot, _stack) = place_args(f, &pvals);
    let mut stack_i = 0usize;
    for (i, _t) in f.params.iter().enumerate() {
        match spot[i] {
            Some(r) if r.starts_with("xmm") => {
                // ROUND 82: a whole vector register, not a scalar word.
                if f.params[i] == FTy::V128 {
                    crate::simd::slot_from_reg(e, &fr, i as Val, r);
                    continue;
                }
                if f.params[i] == FTy::F32 {
                    e.line(&format!("movd eax, {}", r));
                } else {
                    e.line(&format!("movq rax, {}", r));
                }
                e.line(&format!("mov qword ptr [rbp-{}], rax", fr.slot[i]));
            }
            Some(r) => {
                e.line(&format!("mov qword ptr [rbp-{}], {}", fr.slot[i], r));
            }
            None => {
                let off = 16 + 8 * stack_i as u64;
                stack_i += 1;
                e.line(&format!("mov rax, qword ptr [rbp+{}]", off));
                e.line(&format!("mov qword ptr [rbp-{}], rax", fr.slot[i]));
            }
        }
    }

    // ROUND 72: one label counter per function, so every checked site in
    // it gets its own label pair (`panic_rt.rs::SiteCounter`).
    let mut site = crate::panic_rt::SiteCounter::new(&f.name);
    for b in &f.blocks {
        e.raw(&format!("{}:", block_label(&f.name, b.id)));
        emit_block(e, f, &fr, b, &mut site)?;
    }
    // ROUND 64: `DW_AT_high_pc` needs an address at the end of the function,
    // and the frame offsets of the declared names are only known HERE --
    // `layout` runs per function.
    if dwarf::with_variables() {
        let end = format!(".Lfunc_end_{}", label(&f.name));
        e.raw(&format!("{}:", end));
        let vars: Vec<(dwarf::VarNote, u64)> = dwarf::vars_of(&f.name)
            .into_iter()
            .filter_map(|v| {
                fr.alloca_off
                    .get(v.val as usize)
                    .and_then(|o| *o)
                    .map(|o| (v, o))
            })
            .collect();
        let (file, line) = dwarf::fn_line(&f.name).unwrap_or((0, 0));
        e.debug_funcs.push(crate::dwarf_info::FnInfo {
            name: f.name.clone(),
            start_label: label(&f.name),
            end_label: end,
            file,
            line,
            ret: dwarf::ret_of(&f.name).unwrap_or(crate::dwarf::DType::Void),
            vars,
        });
    }
    // ROUND 90: the panic arms of this function's checked operations.
    e.flush_cold();
    Ok(())
}

fn emit_block(
    e: &mut Emitter,
    f: &Func,
    fr: &Frame,
    b: &Block,
    site: &mut crate::panic_rt::SiteCounter,
) -> Result<(), String> {
    // ROUND 82: a block may be reached from anywhere; nothing from the
    // predecessor may be believed. Everything it had is in its slots, because
    // every block flushes in front of its terminator (three lines below).
    crate::simd::xclear(e);
    for (idx, i) in b.insts.iter().enumerate() {
        // ROUND 94: the position travels ON the instruction (`fir::Loc`), so
        // it is right no matter which pass moved the instruction here.
        e.loc_at(i.loc);
        emit_inst(e, f, fr, i, site)?;
        crate::simd::xretire(e, b.id, idx as u32);
    }
    // ROUND 82: everything the xmm cache still holds goes into its home slot
    // before the block is left — the successor knows nothing of it.
    crate::simd::xflush(e, fr);
    match &b.term {
        Term::Br(t) => e.line(&format!("jmp {}", block_label(&f.name, *t))),
        Term::Switch { .. } => crate::codegen_switch::emit_switch(
            e,
            f,
            crate::codegen_switch::ValueSource::Frame(fr),
            &b.term,
        )?,
        Term::BrCond { cond, then_bb, else_bb } => {
            // SPEC §9.2: inside `#[constant_time]` functions no conditional jump
            // may depend on a secret value — hard abort.
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
            e.line(&format!("mov al, byte ptr [rbp-{}]", fr.slot[*cond as usize]));
            e.line("test al, al");
            e.line(&format!("jnz {}", block_label(&f.name, *then_bb)));
            e.line(&format!("jmp {}", block_label(&f.name, *else_bb)));
        }
        Term::Ret(v) => {
            if let Some(v) = v {
                // ROUND 71: a floating point result goes into `xmm0`, as
                // System V prescribes -- and as every other compiler on this
                // platform expects it.
                if f.ret == FTy::V128 {
                    // ROUND 82: System V hands a 128-bit vector back in xmm0.
                    crate::simd::reg_from_slot(e, fr, "xmm0", *v);
                } else if f.ret.is_float() {
                    if f.ret == FTy::F32 {
                        e.line(&format!("mov eax, dword ptr [rbp-{}]", fr.slot[*v as usize]));
                        e.line("movd xmm0, eax");
                    } else {
                        e.line(&format!("mov rax, qword ptr [rbp-{}]", fr.slot[*v as usize]));
                        e.line("movq xmm0, rax");
                    }
                } else {
                e.line(&format!("mov rax, qword ptr [rbp-{}]", fr.slot[*v as usize]));
                }
            } else {
                // Round 51: NO `xor eax, eax` any more. A function with
                // return type `void` has no result value; System V
                // leaves `rax` undefined in that case, and in FIR
                // nobody reads the result of a void call (`Op::Call` without
                // `dst`). Measured in the tokenizer: 4.229.623 calls, so
                // just as many instructions for nothing.
                //
                // Merge R51+R52: the condition `!f.interrupt` of round 52
                // thereby falls away by itself — it served only to spare the
                // interrupt handlers the zeroing of rax.
                // Now nobody zeroes it any more, which is strictly stronger.
            }
            e.line("mov rsp, rbp");
            e.line("pop rbp");
            if f.interrupt {
                // Restore backwards, then `iretq`: that instruction alone
                // restores rflags, cs and rsp of the interrupted thread —
                // `ret` would mangle the frame.
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
                "internal error: block bb{} in '{}' has no terminator",
                b.id, f.name
            ))
        }
    }
    Ok(())
}

/// Loads the complete 8-byte slot of a value into a register.
pub(crate) fn load_full(e: &mut Emitter, fr: &Frame, r: &str, v: Val) {
    e.line(&format!("mov {}, qword ptr [rbp-{}]", r, fr.slot[v as usize]));
}

/// Loads a value sign/zero extended to `to_bits` (32 or 64).
pub(crate) fn load_ext(e: &mut Emitter, fr: &Frame, r: &str, v: Val, ty: FTy, to_bits: u32) {
    let off = fr.slot[v as usize];
    let bits = ty.bits().max(8);
    if bits >= to_bits {
        // Already at least that wide: the lower `to_bits` bits suffice.
        e.line(&format!("mov {}, {} [rbp-{}]", reg(r, to_bits), size_word(to_bits), off));
        return;
    }
    match (ty.signed(), bits) {
        (true, 8) => e.line(&format!("movsx {}, byte ptr [rbp-{}]", reg(r, to_bits), off)),
        (true, 16) => e.line(&format!("movsx {}, word ptr [rbp-{}]", reg(r, to_bits), off)),
        (true, _) => e.line(&format!("movsxd {}, dword ptr [rbp-{}]", reg(r, to_bits), off)),
        (false, 8) => e.line(&format!("movzx {}, byte ptr [rbp-{}]", reg(r, to_bits.min(32)), off)),
        (false, 16) => e.line(&format!("movzx {}, word ptr [rbp-{}]", reg(r, to_bits.min(32)), off)),
        // 32 bits unsigned: `mov e_x` zeroes the upper 32 bits automatically.
        (false, _) => e.line(&format!("mov {}, dword ptr [rbp-{}]", reg(r, 32), off)),
    }
}

/// Writes rax (full) into the slot of the target value.
pub(crate) fn store_dst(e: &mut Emitter, fr: &Frame, d: Val, r: &str) {
    e.line(&format!("mov qword ptr [rbp-{}], {}", fr.slot[d as usize], r));
}

/// **ROUND 71** — a floating point value out of its frame slot into an xmm
/// register. `rax` is the ferry: the slot is 8 bytes wide and holds the bit
/// pattern, `movq` brings all 8 of them over. For an `f32` only the lower 4
/// count, and those are exactly the ones the SSE instructions read.
fn load_xmm(e: &mut Emitter, fr: &Frame, x: &str, v: Val, single: bool) {
    if single {
        // 32 bits: `movd` zeroes the rest of the register, so what is
        // standing there is exactly the bit pattern and nothing else.
        e.line(&format!("mov eax, dword ptr [rbp-{}]", fr.slot[v as usize]));
        e.line(&format!("movd {}, eax", x));
        return;
    }
    load_full(e, fr, "rax", v);
    e.line(&format!("movq {}, rax", x));
}

/// The way back: xmm register -> `rax` -> frame slot of `d`. For `f32`
/// through `movd`/`eax`: that zeroes the upper half of the word, so the
/// slot holds a DEFINED value and not the leftovers of an earlier
/// instruction.
fn store_xmm(e: &mut Emitter, fr: &Frame, d: Val, x: &str, single: bool) {
    if single {
        e.line(&format!("movd eax, {}", x));
    } else {
        e.line(&format!("movq rax, {}", x));
    }
    store_dst(e, fr, d, "rax");
}

/// **ROUND 71** — WHERE does argument number `k` sit?
///
/// System V hands out two register files independently of each other:
/// integer words go to `rdi, rsi, rdx, rcx, r8, r9`, floating point words to
/// `xmm0`-`xmm7`. Anything for which no register of ITS class is left travels
/// on the stack -- in the order of writing, together with the others.
///
/// The result is one entry per argument: `Some(register)` or `None` for the
/// stack. The FIR type decides the class, and nothing else; that is what
/// makes a `{ f32, f32 }` (loaded as one SSE word by the lowering) land in
/// `xmm0` all by itself.
fn place_args(f: &Func, args: &[Val]) -> (Vec<Option<&'static str>>, Vec<Val>) {
    let mut int_i = 0usize;
    let mut sse_i = 0usize;
    let mut spot: Vec<Option<&'static str>> = Vec::with_capacity(args.len());
    let mut stack: Vec<Val> = Vec::new();
    for a in args {
        // ROUND 82: `v128` travels in the SSE class as well — System V puts
        // every 128-bit vector into `xmm0`-`xmm7`, then on the stack.
        if f.val_ty(*a).is_float() || f.val_ty(*a) == FTy::V128 {
            if sse_i < SSE_REGS.len() {
                spot.push(Some(SSE_REGS[sse_i]));
                sse_i += 1;
                continue;
            }
        } else if int_i < ARG_REGS.len() {
            spot.push(Some(ARG_REGS[int_i]));
            int_i += 1;
            continue;
        }
        spot.push(None);
        stack.push(*a);
    }
    (spot, stack)
}

/// Loads the arguments into their registers. SSE first: `rax` is the ferry
/// into an xmm register and must not overwrite an integer argument that is
/// already sitting in place.
fn load_args(
    e: &mut Emitter,
    f: &Func,
    fr: &Frame,
    args: &[Val],
    spot: &[Option<&'static str>],
) {
    for (k, a) in args.iter().enumerate() {
        if let Some(r) = spot[k] {
            if r.starts_with("xmm") {
                if f.val_ty(*a) == FTy::V128 {
                    crate::simd::reg_from_slot(e, fr, r, *a);
                } else {
                    load_xmm(e, fr, r, *a, f.val_ty(*a) == FTy::F32);
                }
            }
        }
    }
    for (k, a) in args.iter().enumerate() {
        if let Some(r) = spot[k] {
            if !r.starts_with("xmm") {
                load_full(e, fr, r, *a);
            }
        }
    }
}

fn emit_inst(
    e: &mut Emitter,
    f: &Func,
    fr: &Frame,
    i: &Inst,
    site: &mut crate::panic_rt::SiteCounter,
) -> Result<(), String> {
    let ty = i.ty;
    match &i.op {
        Op::Const(c) => {
            let d = i.dst.ok_or("internal error: const without target")?;
            let bits = ty.truncate(*c) as i64;
            if bits == 0 {
                e.line("xor eax, eax");
            } else {
                e.line(&format!("mov rax, {}", bits));
            }
            store_dst(e, fr, d, "rax");
        }
        Op::Bin(op, a, b) => {
            let d = i.dst.ok_or("internal error: binary operation without target")?;
            emit_bin(e, fr, *op, ty, *a, *b, d)?;
        }
        // ROUND 72 -- checked "+ - *" (SPEC section 13, item L9). Both
        // operands sign/zero extended to a full 64 bits first (`load_ext`
        // with `to_bits=64` already produces exactly that for any `ty`);
        // the actual check computes at `ty`'s own bit width inside
        // `panic_rt.rs`, so a value that only fits into 8 or 16 bits is
        // caught even though the registers underneath are wider.
        Op::CheckedBin { op, a, b, msg } => {
            let d = i.dst.ok_or("internal error: checked binary operation without target")?;
            load_ext(e, fr, "rax", *a, ty, 64);
            load_ext(e, fr, "rcx", *b, ty, 64);
            crate::panic_rt::emit_checked_bin(e, *op, ty, msg, site, &|e: &mut Emitter| {
                // ROUND 90: the failure arm reloads the two originals from
                // their frame slots -- the same two loads that filled
                // rax/rcx above. `rcx` first: `a` may live where `b` does
                // not, but never the other way round after this order.
                load_ext(e, fr, "rcx", *b, ty, 64);
                load_ext(e, fr, "rdx", *a, ty, 64);
            });
            store_dst(e, fr, d, "rax");
        }
        Op::CheckedDiv { op, a, b, msg_zero, msg_range } => {
            let d = i.dst.ok_or("internal error: checked division without target")?;
            load_ext(e, fr, "rax", *a, ty, 64);
            load_ext(e, fr, "rcx", *b, ty, 64);
            crate::panic_rt::emit_checked_div(e, *op, ty, msg_zero, msg_range, site, &|e: &mut Emitter| {
                load_ext(e, fr, "rcx", *b, ty, 64);
                load_ext(e, fr, "rdx", *a, ty, 64);
            });
            let res = if *op == BinOp::Div { "rax" } else { "rdx" };
            store_dst(e, fr, d, res);
        }
        Op::CheckedCast { src, from, msg } => {
            let d = i.dst.ok_or("internal error: checked cast without target")?;
            load_ext(e, fr, "rax", *src, *from, 64);
            crate::panic_rt::emit_checked_cast(e, *from, ty, msg, site, &|e: &mut Emitter| {
                load_ext(e, fr, "rdx", *src, *from, 64);
            });
            store_dst(e, fr, d, "rax");
        }
        // ROUND 72 -- explicit "+% -% *%" / "+| -| *|" (SPEC section 13,
        // item L9). Never checked, always the caller's own well-defined
        // fallback for when overflow is the point (hashes, checksums,
        // timestamps).
        // ROUND 89 -- the checked ARRAY INDEX (SPEC section 13, item L9).
        // The index is a `usize`, so ONE unsigned comparison against the
        // length decides both ends at once.
        Op::CheckedIdx { idx, len, msg } => {
            let d = i.dst.ok_or("internal error: checked index without target")?;
            load_ext(e, fr, "rax", *idx, ty, 64);
            crate::panic_rt::emit_checked_idx(e, *len, msg, site);
            store_dst(e, fr, d, "rax");
        }
        Op::BinWrapSat { kind, op, a, b } => {
            let d = i.dst.ok_or("internal error: wrap/sat binary operation without target")?;
            emit_wrap_sat(e, fr, *kind, *op, ty, *a, *b, d, site)?;
        }
        Op::Cmp { op, ty: oty, a, b } => {
            let d = i.dst.ok_or("internal error: comparison without target")?;
            // FLOATING POINT: `ucomisd` sets the flags like an UNSIGNED
            // comparison (CF/ZF), hence `setb`/`seta` rather than `setl`/`setg`.
            // comparison. For NaN, PF is set and ZF/CF as well — that makes every
            // comparison except `!=` false, exactly as IEEE-754 demands.
            if oty.is_float() {
                // For NaN `ucomisd` sets ZF=PF=CF=1 — the unordered case
                // therefore looks like "less or equal". IEEE-754 demands,
                // though, that EVERY ordering comparison with NaN be false.
                //
                // `seta`/`setae` check `CF=0 [and ZF=0]` and are therefore
                // right by themselves. For `<` and `<=` the OPERANDS ARE
                // SWAPPED (`a < b` becomes `b > a`) instead of computing
                // on the parity flag afterwards.
                let swap = matches!(op, CmpOp::Lt | CmpOp::Le);
                let (first, second) = if swap { (*b, *a) } else { (*a, *b) };
                let single = *oty == FTy::F32;
                load_xmm(e, fr, "xmm0", first, single);
                load_xmm(e, fr, "xmm1", second, single);
                // ROUND 71: `ucomiss` for binary32 — the same flag rules,
                // the same swap trick, only the width differs.
                e.line(if single {
                    "ucomiss xmm0, xmm1"
                } else {
                    "ucomisd xmm0, xmm1"
                });
                let cc = match op {
                    CmpOp::Eq => "sete",
                    CmpOp::Ne => "setne",
                    CmpOp::Lt | CmpOp::Gt => "seta",
                    CmpOp::Le | CmpOp::Ge => "setae",
                };
                e.line(&format!("{} al", cc));
                if matches!(op, CmpOp::Eq) {
                    // NaN == NaN: ZF is set, but PF is too. `setnp`
                    // masks the unordered case out.
                    e.line("setnp cl");
                    e.line("and al, cl");
                }
                if matches!(op, CmpOp::Ne) {
                    // Mirror image: for NaN `!=` is true.
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
            let d = i.dst.ok_or("internal error: unary operation without target")?;
            // FLOATING POINT: the sign is ONE bit. `neg` would treat the whole
            // bit pattern as two's complement — wrong. That is why only bit 63
            // is flipped.
            if ty.is_float() {
                if !matches!(op, UnOp::Neg) {
                    return Err(format!(
                        "internal error: '!' is not defined for {}",
                        ty.name()
                    ));
                }
                load_full(e, fr, "rax", *a);
                if ty == FTy::F32 {
                    // ROUND 71: for binary32 the sign is bit 31. The 32-bit
                    // `xor` zeroes the upper half of `rax` by itself, which
                    // is what keeps the slot clean.
                    e.line("mov ecx, -2147483648");
                    e.line("xor eax, ecx");
                } else {
                    e.line("mov rcx, -9223372036854775808");
                    e.line("xor rax, rcx");
                }
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
            let d = i.dst.ok_or("internal error: conversion without target")?;
            // FLOATING POINT CONVERSIONS
            //
            // ROUND 71: first the two widths among each other. They are the
            // only pair in which BOTH sides are floating point, and neither
            // direction is a reinterpretation -- `cvtsd2ss` really rounds.
            if ty.is_float() && from.is_float() {
                if ty == *from {
                    load_full(e, fr, "rax", *src);
                    store_dst(e, fr, d, "rax");
                    return Ok(());
                }
                load_xmm(e, fr, "xmm0", *src, *from == FTy::F32);
                e.line(if ty == FTy::F64 {
                    "cvtss2sd xmm0, xmm0"
                } else {
                    "cvtsd2ss xmm0, xmm0"
                });
                store_xmm(e, fr, d, "xmm0", ty == FTy::F32);
                return Ok(());
            }
            if ty == FTy::F32 && !from.is_float() {
                // Integer -> f32, same reservation about unsigned 64-bit
                // values above 2^63 as with `cvtsi2sd`.
                load_ext(e, fr, "rax", *src, *from, 64);
                e.line("cvtsi2ss xmm0, rax");
                store_xmm(e, fr, d, "xmm0", true);
                return Ok(());
            }
            if *from == FTy::F32 && !ty.is_float() {
                // f32 -> integer, cutting towards zero, as in C.
                load_xmm(e, fr, "xmm0", *src, true);
                e.line("cvttss2si rax, xmm0");
                store_dst(e, fr, d, "rax");
                return Ok(());
            }
            if ty == FTy::F64 && *from != FTy::F64 {
                // Integer -> f64. Signed with `cvtsi2sd`; unsigned 64-bit
                // values above 2^63 are beyond this instruction, so the value
                // is brought to 64 bits beforehand and the special case is
                // stated honestly (SPEC §14.1.f64).
                load_ext(e, fr, "rax", *src, *from, 64);
                e.line("cvtsi2sd xmm0, rax");
                e.line("movq rax, xmm0");
                store_dst(e, fr, d, "rax");
                return Ok(());
            }
            if *from == FTy::F64 && ty != FTy::F64 {
                // f64 -> integer, cutting (towards zero), as in C.
                load_full(e, fr, "rax", *src);
                e.line("movq xmm0, rax");
                e.line("cvttsd2si rax, xmm0");
                store_dst(e, fr, d, "rax");
                return Ok(());
            }
            if ty == FTy::Bool {
                // Safety net: bool contains 0/1 only.
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
            let d = i.dst.ok_or("internal error: gc_state without target")?;
            emit_gc_addr(e, *regs);
            store_dst(e, fr, d, "rax");
        }
        Op::Alloca { .. } => {
            let d = i.dst.ok_or("internal error: alloca without target")?;
            let off = fr.alloca_off[d as usize].ok_or("internal error: alloca without space")?;
            e.line(&format!("lea rax, [rbp-{}]", off));
            store_dst(e, fr, d, "rax");
        }
        Op::Load { addr } => {
            let d = i.dst.ok_or("internal error: load without target")?;
            // ROUND 82: sixteen octets through a pointer out of the program.
            if ty == FTy::V128 {
                // ...unless the pointer is a promoted cell, and then it is a
                // register move.
                if let Some(off) = crate::simd::cell_of(e, *addr) {
                    crate::simd::emit_cell_load(e, fr, d, *addr, off);
                    return Ok(());
                }
                crate::simd::emit_ptr_load(e, fr, d, *addr);
                return Ok(());
            }
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
            if ty == FTy::V128 {
                if let Some(off) = crate::simd::cell_of(e, *addr) {
                    crate::simd::emit_cell_store(e, fr, *addr, off, *val);
                    return Ok(());
                }
                crate::simd::emit_ptr_store(e, fr, *addr, *val);
                return Ok(());
            }
            load_full(e, fr, "rcx", *addr);
            load_full(e, fr, "rax", *val);
            let bits = ty.bits().max(8);
            e.line(&format!("mov {} [rcx], {}", size_word(bits), reg("rax", bits)));
        }
        Op::PtrAdd { base, off } => {
            let d = i.dst.ok_or("internal error: ptradd without target")?;
            load_full(e, fr, "rax", *base);
            load_full(e, fr, "rcx", *off);
            e.line("add rax, rcx");
            store_dst(e, fr, d, "rax");
        }
        Op::Call { name, args } => {
            // ROUND 82: every one of the sixteen xmm registers is CALLER
            // saved on System V — a call destroys the whole cache.
            crate::simd::xflush(e, fr);
            // Arguments for which no register of their class is left: they sit
            // immediately in front of the `call` at [rsp+8k]. The 16-byte
            // alignment survives (an odd number of words gets a padding word).
            let (spot, stack) = place_args(f, args);
            let mut adjust = 8 * stack.len() as u64;
            if stack.len() % 2 == 1 {
                adjust += 8;
            }
            if adjust > 0 {
                e.line(&format!("sub rsp, {}", adjust));
                for (k, a) in stack.iter().enumerate() {
                    load_full(e, fr, "rax", *a);
                    e.line(&format!("mov qword ptr [rsp+{}], rax", 8 * k));
                }
            }
            load_args(e, f, fr, args, &spot);
            e.line(&format!("call {}", label(name)));
            if adjust > 0 {
                e.line(&format!("add rsp, {}", adjust));
            }
            if let Some(d) = i.dst {
                // ROUND 71: a floating point result comes back in `xmm0`.
                // ROUND 82: a `v128` result likewise, only sixteen octets wide.
                if ty == FTy::V128 {
                    crate::simd::slot_from_reg(e, fr, d, "xmm0");
                } else if ty.is_float() {
                    store_xmm(e, fr, d, "xmm0", ty == FTy::F32);
                } else {
                    store_dst(e, fr, d, "rax");
                }
            }
        }
        // Dynamic dispatch (iface.rs, round 46): like `Op::Call`, only the target
        // sits in a register. `rax` is a pure scratch register on the base path
        // and no argument register — it is loaded LAST.
        Op::CallIndirect { target, args } => {
            crate::simd::xflush(e, fr);
            let (spot, stack) = place_args(f, args);
            let mut adjust = 8 * stack.len() as u64;
            if stack.len() % 2 == 1 {
                adjust += 8;
            }
            if adjust > 0 {
                e.line(&format!("sub rsp, {}", adjust));
                for (k, a) in stack.iter().enumerate() {
                    load_full(e, fr, "rax", *a);
                    e.line(&format!("mov qword ptr [rsp+{}], rax", 8 * k));
                }
            }
            load_args(e, f, fr, args, &spot);
            load_full(e, fr, "rax", *target);
            e.line("call rax");
            if adjust > 0 {
                e.line(&format!("add rsp, {}", adjust));
            }
            if let Some(d) = i.dst {
                if ty == FTy::V128 {
                    crate::simd::slot_from_reg(e, fr, d, "xmm0");
                } else if ty.is_float() {
                    store_xmm(e, fr, d, "xmm0", ty == FTy::F32);
                } else {
                    store_dst(e, fr, d, "rax");
                }
            }
        }
        Op::VtabAddr { table } => {
            let d = i.dst.ok_or("internal error: vtab without target")?;
            e.line(&format!(
                "lea rax, [rip + {}]",
                crate::iface::table_label(table)
            ));
            store_dst(e, fr, d, "rax");
        }
        // Round 58 (fnval.rs): a named function as a value — the address of
        // its function record.
        Op::FnRef { name } => {
            let d = i.dst.ok_or("internal error: fnref without target")?;
            e.line(&format!(
                "lea rax, [rip + {}]",
                crate::fnval::record_label(name)
            ));
            store_dst(e, fr, d, "rax");
        }
        // Round 89 (statics.rs): the address of a global variable.
        Op::GlobalAddr { name } => {
            let d = i.dst.ok_or("internal error: globaladdr without target")?;
            e.line(&format!(
                "lea rax, [rip + {}]",
                crate::statics::label_of(name)
            ));
            store_dst(e, fr, d, "rax");
        }
        Op::Syscall { args } => {
            crate::simd::xflush(e, fr);
            if args.is_empty() {
                return Err("internal error: syscall without number".to_string());
            }
            if args.len() > 7 {
                return Err("syscall with more than 6 arguments".to_string());
            }
            // ROUND WINDOWS: there is no `syscall` instruction a program may
            // use here. The very same seven values become an ordinary call
            // to the seam (`win_seam.rs`), which does the work over Win32.
            if crate::target::windows() {
                e.line("sub rsp, 16");
                if args.len() >= 7 {
                    load_full(e, fr, "rax", args[6]);
                } else {
                    e.line("xor eax, eax");
                }
                e.line("mov qword ptr [rsp], rax");
                for k in 0..ARG_REGS.len() {
                    if k < args.len() {
                        load_full(e, fr, ARG_REGS[k], args[k]);
                    } else {
                        e.line(&format!("mov {}, 0", ARG_REGS[k]));
                    }
                }
                e.line(&format!("call {}", label(crate::win_seam::SYSCALL_FN)));
                e.line("add rsp, 16");
                if let Some(d) = i.dst {
                    store_dst(e, fr, d, "rax");
                }
                return Ok(());
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
            // Data independent choice: `cmov`, never a jump (SPEC §9.2).
            let d = i.dst.ok_or("internal error: select without target")?;
            load_full(e, fr, "rdx", *cond);
            load_full(e, fr, "rax", *b);
            load_full(e, fr, "rcx", *a);
            e.line("test dl, dl");
            e.line("cmovnz rax, rcx");
            store_dst(e, fr, d, "rax");
        }
        // ROUND 92 -- the copy that `phi.rs` leaves behind. On the base path
        // every value lives in the frame, so this is one load and one store;
        // the register aware path in `regalloc.rs` turns it into a single
        // `mov` between registers, or into nothing at all when both ends
        // already share one.
        Op::Copy { src } => {
            let d = i.dst.ok_or("internal error: copy without target")?;
            load_full(e, fr, "rax", *src);
            store_dst(e, fr, d, "rax");
        }
        // ROUND 92 -- `phi.rs` runs before every backend, so a phi can only
        // get here if somebody wired a new code path past it.
        Op::Phi { .. } => {
            return Err("internal error: phi in the code generator (phi.rs did not run)".into())
        }
        Op::Barrier { val } => {
            // Opaque: the value passes through an empty asm needle eye.
            let d = i.dst.ok_or("internal error: barrier without target")?;
            load_full(e, fr, "rax", *val);
            e.raw("    # barrier: opaque to every optimization pass");
            store_dst(e, fr, d, "rax");
        }
        Op::SecureZero { addr, size } => {
            // Zeroing byte by byte; must never be removed (SPEC §9.3 C3).
            load_full(e, fr, "rdi", *addr);
            load_full(e, fr, "rcx", *size);
            e.line("xor eax, eax");
            e.line("cld");
            e.line("rep stosb");
        }
        Op::AtomicAdd { addr, val } => {
            // Round 47 (atomic.rs): `lock xadd` — one instruction, the result
            // is the OLD value.
            let d = i.dst.ok_or("internal error: atomadd without target")?;
            load_full(e, fr, "rcx", *addr);
            load_full(e, fr, "rax", *val);
            e.line("lock xadd qword ptr [rcx], rax");
            store_dst(e, fr, d, "rax");
        }
        // Round 49 (thread.rs): compare-and-swap, thread creation, self pointer.
        Op::AtomicCas { addr, erw, new } => {
            let d = i.dst.ok_or("internal error: atomcas without target")?;
            load_full(e, fr, "rcx", *addr);
            load_full(e, fr, "rdx", *new);
            load_full(e, fr, "rax", *erw);
            crate::thread::cas_sequence(e);
            store_dst(e, fr, d, "rax");
        }
        Op::ThreadSpawn { arg, stack, ctid } => {
            // ROUND WINDOWS: `clone(2)` has no Win32 equivalent this round
            // can honour -- `CreateThread` hands the child a stack of its
            // own and a different entry convention, and the collector's
            // thread table (lib/gc/gc.fi) is built on the Linux shape.
            //
            // A COMPILE ERROR would be the wrong answer, and the round
            // measured why: `lib/gc/gc.fi` CONTAINS a spawn, so every
            // program that links the collector would be refused -- 93 of
            // 309 cases, almost none of which ever start a thread. So the
            // instruction becomes what the seam does with a system call it
            // cannot serve: `-38` (ENOSYS), the value `thread_spawn`
            // already knows as a failure.
            if crate::target::windows() {
                crate::thread::spawn_unsupported(e);
                if let Some(d) = i.dst {
                    store_dst(e, fr, d, "rax");
                }
                return Ok(());
            }
            crate::simd::xflush(e, fr);
            let d = i.dst.ok_or("internal error: spawn without target")?;
            load_full(e, fr, "rdi", *arg);
            load_full(e, fr, "rsi", *stack);
            load_full(e, fr, "rdx", *ctid);
            crate::thread::spawn_sequence(e);
            store_dst(e, fr, d, "rax");
        }
        Op::ThreadSelf => {
            let d = i.dst.ok_or("internal error: threadself without target")?;
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
        // ROUND 52 (core.rs, SPEC §2): inline assembler. ALWAYS volatile —
        // the lines stand exactly once and exactly here.
        Op::Asm { template, out, in_regs, ins, out_regs, outs, clobber } => {
            // ROUND 82: hand written assembler may name any register at all.
            crate::simd::xflush(e, fr);
            e.raw("    # asm (volatile): must be neither removed nor moved");
            for (r, v) in in_regs.iter().zip(ins.iter()) {
                let stem = crate::core::stem(r)
                    .ok_or_else(|| format!("unknown asm register '{}'", r))?;
                load_full(e, fr, stem, *v);
            }
            for line in template.split('\n') {
                e.line(line);
            }
            // The VALUE form goes into its frame slot FIRST. `store_dst` is a
            // plain `mov [rbp-N], reg` and needs no scratch register — after
            // it, rax and rcx are free for the memory outputs below.
            if let Some(r) = out {
                let stem = crate::core::stem(r)
                    .ok_or_else(|| format!("unknown asm register '{}'", r))?;
                let d = i.dst.ok_or("internal error: asm with out but without target")?;
                store_dst(e, fr, d, stem);
            }
            // ROUND 68 — the memory outputs. Every result register goes on
            // the STACK first; only then are rax/rcx used to write them out.
            // Doing it the other way round would destroy a result that is
            // still to be written: an address has to be loaded into a
            // register, and that register may itself be an output.
            if !outs.is_empty() {
                for r in out_regs.iter() {
                    let stem = crate::core::stem(r)
                        .ok_or_else(|| format!("unknown asm register '{}'", r))?;
                    e.line(&format!("push {}", stem));
                }
                for k in (0..outs.len()).rev() {
                    e.line("pop rax");
                    load_full(e, fr, "rcx", outs[k]);
                    e.line("mov qword ptr [rcx], rax");
                }
            }
            if !clobber.is_empty() {
                e.raw(&format!("    # asm clobber: {}", clobber.join(", ")));
            }
        }
        // ROUND 52: MMIO — exactly ONE memory access per source line.
        Op::MmioLoad { addr } => {
            let d = i.dst.ok_or("internal error: mmio_load without target")?;
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
        // ROUND 82 (simd.rs, SPEC 8.7): one vector or crypto instruction.
        Op::Simd { .. } => crate::simd::emit(e, fr, i)?,
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
    // FLOATING POINT runs over the SSE unit. Computing happens in xmm0/xmm1,
    // reading and writing happens through rax — an `f64` sits in the frame
    // as an ordinary 64-bit word (its bit pattern), which is why no separate
    // memory path is needed here.
    if ty.is_float() {
        // ROUND 71: the scalar single instructions sit right next to the
        // double ones — same encoding family, one letter apart.
        let m = match (op, ty) {
            (BinOp::Add, FTy::F32) => "addss",
            (BinOp::Sub, FTy::F32) => "subss",
            (BinOp::Mul, FTy::F32) => "mulss",
            (BinOp::Div, FTy::F32) => "divss",
            (BinOp::Add, _) => "addsd",
            (BinOp::Sub, _) => "subsd",
            (BinOp::Mul, _) => "mulsd",
            (BinOp::Div, _) => "divsd",
            _ => {
                return Err(format!(
                    "internal error: operator '{:?}' is not defined for {}",
                    op,
                    ty.name()
                ))
            }
        };
        let single = ty == FTy::F32;
        load_xmm(e, fr, "xmm0", a, single);
        load_xmm(e, fr, "xmm1", b, single);
        e.line(&format!("{} xmm0, xmm1", m));
        store_xmm(e, fr, d, "xmm0", single);
        return Ok(());
    }
    match op {
        BinOp::Add | BinOp::Sub | BinOp::And | BinOp::Or | BinOp::Xor | BinOp::Mul => {
            // The low order bits are independent of the width for these
            // operations; that is why computing happens at 32/64 bits and the
            // result is cut to the type width while reading.
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
            // Bring the operands exactly to the computing width (upper bits in
            // the slot are not guaranteed), then idiv/div to match the sign.
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
            // Widen the left operand exactly, so that `shr`/`sar` pull the right
            // bits along for 8/16-bit types too.
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

/// **ROUND 72** — `+% -% *%` (wrapping) and `+| -| *|` (saturating), SPEC
/// §13 item L9. Precondition/postcondition exactly like `emit_bin`'s
/// `Add`/`Sub`/`Mul` arm (compute at 32/64 bits, narrow on store) — wrapping
/// needs nothing else at all: two's complement wrapping IS what `add`/
/// `sub`/`imul` already do when nobody looks at the flag, so `Wrap` reuses
/// the unchecked instruction sequence outright. Saturating clamps the
/// wrapped result back into range with `cmov` when the flag fired — the
/// same overflow test `panic_rt.rs` uses, only the ending differs.
#[allow(clippy::too_many_arguments)]
fn emit_wrap_sat(
    e: &mut Emitter,
    fr: &Frame,
    kind: WrapSatKind,
    op: BinOp,
    ty: FTy,
    a: Val,
    b: Val,
    d: Val,
    site: &mut crate::panic_rt::SiteCounter,
) -> Result<(), String> {
    if kind == WrapSatKind::Wrap {
        // Bit for bit the unchecked path: wrapping on overflow is exactly
        // what plain two's complement arithmetic already means.
        return emit_bin(e, fr, op, ty, a, b, d);
    }
    let bits = ty.bits();
    load_ext(e, fr, "rax", a, ty, 64);
    load_ext(e, fr, "rcx", b, ty, 64);
    e.line("mov r10, rax");
    e.line("mov r11, rcx");
    let narrow = |name: &str, bits: u32| -> &'static str {
        match (name, bits) {
            ("rax", 8) => "al",
            ("rax", 16) => "ax",
            ("rax", 32) => "eax",
            ("rax", _) => "rax",
            ("rcx", 8) => "cl",
            ("rcx", 16) => "cx",
            ("rcx", 32) => "ecx",
            (_, _) => "rcx",
        }
    };
    match op {
        BinOp::Add => e.line(&format!("add {}, {}", narrow("rax", bits), narrow("rcx", bits))),
        BinOp::Sub => e.line(&format!("sub {}, {}", narrow("rax", bits), narrow("rcx", bits))),
        // 8 bit has no two operand `imul` (see panic_rt.rs::emit_checked_bin).
        BinOp::Mul if ty.signed() && bits == 8 => e.line("imul cl"),
        BinOp::Mul if ty.signed() => {
            e.line(&format!("imul {}, {}", narrow("rax", bits), narrow("rcx", bits)))
        }
        BinOp::Mul => e.line(&format!("mul {}", narrow("rcx", bits))),
        _ => return Err("internal error: wrap/sat only defined for + - *".to_string()),
    }
    // Same reasoning as panic_rt.rs::emit_checked_bin: unsigned overflow
    // is a CF condition for every one of + - *, not only Mul -- `200u8 +
    // 100u8` sets CF but leaves OF clear (as i8 the sum still fits), so an
    // `Add`/`Sub` clamp gated on OF alone silently returned the wrapped
    // value instead of saturating (round 72's own bug, found testing
    // `+|` on `u8`).
    let jcc = if ty.signed() { "jo" } else { "jc" };
    // ROUND 72, second pass: the label suffix comes from the FUNCTION's own
    // site counter, not from the three value numbers. Value numbers restart
    // at 0 in every function, so `.Lsatclamp0_1_6` was emitted twice the
    // moment two functions saturated at the same place in their own
    // numbering and `as` refused the file ("symbol already defined") --
    // exactly the collision `SiteCounter` was introduced for on the checked
    // side, missed here (found compiling five one-line `+|` functions).
    let uid = site.next();
    let clamp = format!(".Lsatclamp{}", uid);
    let done = format!(".Lsatdone{}", uid);
    e.line(&format!("{} {}", jcc, clamp));
    e.line(&format!("jmp {}", done));
    e.raw(&format!("{}:", clamp));
    // Which bound? For `Sub` a negative result that overflowed a signed
    // type means the true result was below MIN — the ONE case where the
    // "did it go negative" test alone is not enough (unsigned subtraction
    // always saturates to 0 on overflow, no sign question at all).
    let (min_lit, max_lit): (i128, i128) = match ty {
        FTy::I8 => (i8::MIN as i128, i8::MAX as i128),
        FTy::I16 => (i16::MIN as i128, i16::MAX as i128),
        FTy::I32 => (i32::MIN as i128, i32::MAX as i128),
        FTy::I64 => (i64::MIN as i128, i64::MAX as i128),
        FTy::U8 => (0, u8::MAX as i128),
        FTy::U16 => (0, u16::MAX as i128),
        FTy::U32 => (0, u32::MAX as i128),
        _ => (0, u64::MAX as i128),
    };
    if !ty.signed() {
        // Unsigned: `Add`/`Mul` overflow means "too big" (clamp to MAX);
        // `Sub` overflow means "went below zero" (clamp to 0).
        if op == BinOp::Sub {
            e.line(&format!("mov rax, {}", min_lit as u64));
        } else {
            e.line(&format!("mov rax, {}", max_lit as u64));
        }
    } else {
        // Signed: the sign of ONE original operand (for Add/Sub) or the
        // XOR of both signs (for Mul) says which bound was crossed.
        match op {
            BinOp::Add => {
                // a + b overflowed: if a is negative, the true sum was
                // below MIN; otherwise above MAX.
                e.line("cmp r10, 0");
                e.line(&format!("mov rax, {}", max_lit));
                e.line(&format!("mov rdx, {}", min_lit));
                e.line("cmovl rax, rdx");
            }
            BinOp::Sub => {
                // a - b overflowed: if a is negative and b positive, below
                // MIN; if a positive and b negative, above MAX. Equivalent
                // to: sign of a differs from sign of (a - b)'s true value,
                // which is exactly sign of b for this purpose — if b is
                // negative the true difference is larger, so it is the
                // ABOVE-MAX case.
                e.line("cmp r11, 0");
                e.line(&format!("mov rax, {}", min_lit));
                e.line(&format!("mov rdx, {}", max_lit));
                e.line("cmovl rax, rdx");
            }
            BinOp::Mul => {
                // a * b overflowed: same sign -> above MAX, different sign
                // -> below MIN.
                e.line("mov rax, r10");
                e.line("xor rax, r11");
                e.line("cmp rax, 0");
                e.line(&format!("mov rax, {}", max_lit));
                e.line(&format!("mov rdx, {}", min_lit));
                e.line("cmovl rax, rdx");
            }
            _ => unreachable!("guarded above"),
        }
    }
    e.raw(&format!("{}:", done));
    store_dst(e, fr, d, "rax");
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
        // Depending on the register allocation 'mov rax, 42' or 'mov eax, 42'.
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

    /// More than six parameters: the further ones sit on the stack of the
    /// caller, the 16-byte alignment survives (abi.rs, SPEC §13).
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
        // Since round 43 the register path covers this case as well; the calling
        // convention is the same on BOTH paths, which is why the test checks
        // only that and no longer the producing path.
        assert!(asm.contains("qword ptr [rbp+16]"), "{}", asm);
        assert!(asm.contains("qword ptr [rbp+24]"), "{}", asm);
        assert!(asm.contains("sub rsp, 16"), "{}", asm);
        assert!(asm.contains("mov qword ptr [rsp+0], rax"), "{}", asm);
        assert!(asm.contains("mov qword ptr [rsp+8], rax"), "{}", asm);
        assert!(asm.contains("add rsp, 16"), "{}", asm);
    }
}
