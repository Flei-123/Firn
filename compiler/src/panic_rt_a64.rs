// SPDX-License-Identifier: MIT
//! **Round 83** — the checked arithmetic of round 72 on the SECOND machine
//! (`docs/ROUND80.md` §7, SPEC §13 item `L9`).
//!
//! Round 72 built checked `+ - * / %` and a checked narrowing `as` for
//! x86-64 (`panic_rt.rs`). Round 80 gave the compiler an aarch64 backend
//! but was written while round 72 sat on an unmerged branch, so the A64
//! code generator had no arm for the four operations at all. This file is
//! that arm — the same promise on both machines, or the promise is worth
//! nothing.
//!
//! ## What is shared with the x86 side and what is not
//!
//! SHARED: the message table. `panic_rt::intern` is what `lower.rs` calls
//! while building the FIR, long before a target is chosen, and
//! `panic_rt::rodata_asm` writes `.ascii` — machine independent text. Both
//! backends emit the identical `.rodata`, which is why a program's panic
//! MESSAGE is the same octet sequence on both machines.
//!
//! NOT SHARED: everything that is instructions. The trampoline below is
//! A64, the flag conditions are A64's (`b.vs` for signed overflow, `b.cs`
//! / `b.cc` for the unsigned carry/borrow), and the multiplication asks
//! `smulh`/`umulh` for the upper half instead of reading a flag x86 sets
//! for free.
//!
//! ## Widths
//!
//! A64 has no 8- and 16-bit arithmetic and therefore no 8- or 16-bit
//! flags. `adds w9, w9, w10` on two `i8` values cannot overflow 32 bits at
//! all, so the answer for those widths is not a flag but a RANGE CHECK on
//! the exact result (`sxtb`/`uxtb`/`sxth`/`uxth` and compare). 32 and 64
//! bit use the flags, exactly as x86 does. A 32-bit multiplication is
//! computed at 64 bits (`mul` of two extended operands is the exact
//! product) and range checked the same way; only the 64-bit
//! multiplication needs `smulh`/`umulh`.
//!
//! ## Calling convention of the trampoline
//!
//! `x0` = message pointer, `x1` = message length, `x2` = a, `x3` = b,
//! `x4` = panic kind code (the `panic_rt::PANIC_*` numbers, for a kernel
//! that wants to tell them apart), `x5` = 1 when the two values are to be
//! read as UNSIGNED. It never returns.
//!
//! `profile kernel` does not reach this file: `codegen_a64.rs::emit`
//! refuses the kernel profile on aarch64 outright (round 80), so the
//! `osum_panic` hand-off has no aarch64 form yet and this file does not
//! invent one.

use crate::codegen_x86::Emitter;
use crate::fir::{BinOp, FTy};
use crate::panic_rt::{
    intern, SiteCounter, PANIC_CAST, PANIC_DIV0, PANIC_DIV_OVERFLOW, TRAMPOLINE,
};

/// Scratch registers of this file. They are the same ones
/// `codegen_a64.rs` uses for `A`/`B`/`C`/`T1`, plus `x15`/`x16`/`x17`,
/// which no value of that backend ever lives in (every FIR value sits in
/// the frame there).
const A: &str = "x9";
const B: &str = "x10";
const T: &str = "x15";
const U: &str = "x16";

fn wn(r: &str) -> String {
    format!("w{}", &r[1..])
}

/// The register at the computing width — `w` below 33 bits, `x` above.
fn rw(r: &str, bits: u32) -> String {
    if bits <= 32 {
        wn(r)
    } else {
        r.to_string()
    }
}

/// The shared out-of-line trampoline, written ONCE per object file
/// (`codegen_a64.rs::emit`, guarded by `panic_rt::any_registered`).
///
/// Builds `<message> (a=<N> b=<M>)\n`, writes it to file descriptor 2 and
/// calls `exit_group(101)` — the same text and the same exit code the x86
/// trampoline produces, because `tools/checked/run.sh` compares them
/// octet for octet and a second machine that says it differently has not
/// kept the promise.
pub(crate) fn trampoline_asm() -> String {
    let mut s = String::new();
    // ROUND 89: a program with a `#[panic_handler]` gets one `call` per
    // entry point and none of the formatter below. The trampoline's
    // register convention (x0..x4) IS the AAPCS64 one for five arguments,
    // so there is nothing to shuffle.
    if let Some(h) = crate::panic_rt::handler() {
        for (label, _, _) in entries() {
            s.push_str(&format!("{}:\n", label));
            s.push_str(&format!("    bl {}\n", crate::codegen_x86::label(&h)));
            if crate::prof::is_kernel() {
                // ROUND ARM-FREESTANDING: it came back anyway, and there is
                // no `exit_group` here to end it with. `brk #0` is what
                // `ud2` is on the other machine -- a deliberate trap, not a
                // fall-through into whatever `.text` holds next.
                s.push_str("    // a panic handler is not supposed to come back.\n");
                s.push_str("    brk #0\n");
                continue;
            }
            s.push_str("    mov x0, #101\n");
            s.push_str("    mov x8, #94\n");
            s.push_str("    svc #0\n");
            s.push_str("    brk #0\n");
        }
        return s;
    }
    // ROUND ARM-FREESTANDING -- the kernel ending, the A64 twin of
    // `panic_rt.rs`'s. There is no runtime here: no `write` to print the
    // message with and no `exit_group` to stop with. SPEC section 2 already
    // promised the answer ("calls osum_panic, configurable"), and the
    // argument registers x0..x4 ARE the AAPCS64 order the trampoline
    // already hands its five values in, so nothing has to be shuffled --
    // which is the one place where A64 is easier than x86-64 here (there
    // the trampoline's rdi/esi/rdx/rcx/r9 needed r9 moved into r8 first).
    //
    // `osum_panic` stays UNDEFINED in the object file on purpose. A kernel
    // that never defines it gets a link error, and a link error is the
    // honest outcome; quietly returning into code that has just proved its
    // own arithmetic wrong is not.
    if crate::prof::is_kernel() {
        for (label, _, _) in entries() {
            s.push_str(&format!("{}:\n", label));
            // x5 carries "read the two values as unsigned" and is the fifth
            // argument by AAPCS64 anyway -- see the register list in the
            // header of this file. The x86 side hands it in r9 and has to
            // move it; here it already sits where the callee looks.
            s.push_str(&format!("    bl {}\n", crate::panic_rt::OSUM_PANIC));
            s.push_str("    // osum_panic is not supposed to come back; running\n");
            s.push_str("    // into whatever comes next in .text would be silently\n");
            s.push_str("    // wrong, so this traps instead of guessing.\n");
            s.push_str("    brk #0\n");
        }
        return s;
    }
    for (label, open, mid) in entries() {

        s.push_str(&format!("{}:\n", label));
        // Nothing is saved: this never returns. x19/x20 are callee-saved and
        // are used all the same — the process is already dying.
        s.push_str("    mov x19, x0\n"); // message pointer
        s.push_str("    mov x20, x1\n"); // message length
        s.push_str("    mov x21, x2\n"); // a
        s.push_str("    mov x22, x3\n"); // b
        s.push_str("    mov x23, x5\n"); // 1 = read the two values as unsigned
        // 176 octets of buffer: the longest possible tail is
        // " (a=-9223372036854775808 b=-9223372036854775808)\n", 49 octets.
        s.push_str("    sub sp, sp, #176\n");
        s.push_str("    mov x24, sp\n"); // start of the buffer
        s.push_str("    mov x25, sp\n"); // write pointer
        for c in open.bytes().map(u32::from) {
            s.push_str(&format!("    mov w26, #{}\n", c));
            s.push_str("    strb w26, [x25], #1\n");
        }
        s.push_str("    mov x0, x21\n");
        s.push_str("    bl .Lpanic_a64_dec\n");
        for c in mid.bytes().map(u32::from) {
            s.push_str(&format!("    mov w26, #{}\n", c));
            s.push_str("    strb w26, [x25], #1\n");
        }
        s.push_str("    mov x0, x22\n");
        s.push_str("    bl .Lpanic_a64_dec\n");
        for c in [41u32, 10] {
            // ")\n"
            s.push_str(&format!("    mov w26, #{}\n", c));
            s.push_str("    strb w26, [x25], #1\n");
        }
        // write(2, message, length)
        s.push_str("    mov x0, #2\n");
        s.push_str("    mov x1, x19\n");
        s.push_str("    mov x2, x20\n");
        s.push_str("    mov x8, #64\n");
        s.push_str("    svc #0\n");
        // write(2, buffer, write pointer - start)
        s.push_str("    mov x0, #2\n");
        s.push_str("    mov x1, x24\n");
        s.push_str("    sub x2, x25, x24\n");
        s.push_str("    mov x8, #64\n");
        s.push_str("    svc #0\n");
        // exit_group(101) — the same number the x86 trampoline exits with.
        s.push_str("    mov x0, #101\n");
        s.push_str("    mov x8, #94\n");
        s.push_str("    svc #0\n");
        s.push_str("    brk #0\n");
    }
    // ---------------------------------------------------------------
    // Appends the decimal text of `x0` at `[x25]` and advances `x25`.
    // `x23` = 1 means the value is UNSIGNED and never gets a sign.
    // Two's complement makes `neg` on `i64::MIN` produce the right
    // MAGNITUDE as an unsigned bit pattern, so the unsigned `udiv`
    // afterwards is correct for that value too — the same argument the
    // x86 routine makes.
    s.push_str(".Lpanic_a64_dec:\n");
    s.push_str("    cbnz x23, .Lpanic_a64_dec_digits\n");
    s.push_str("    tbz x0, #63, .Lpanic_a64_dec_digits\n");
    s.push_str("    mov w26, #45\n");
    s.push_str("    strb w26, [x25], #1\n");
    s.push_str("    neg x0, x0\n");
    s.push_str(".Lpanic_a64_dec_digits:\n");
    s.push_str("    mov x27, x25\n"); // first digit written
    s.push_str("    mov x28, #10\n");
    s.push_str("    cbnz x0, .Lpanic_a64_dec_loop\n");
    s.push_str("    mov w26, #48\n");
    s.push_str("    strb w26, [x25], #1\n");
    s.push_str("    b .Lpanic_a64_dec_rev\n");
    s.push_str(".Lpanic_a64_dec_loop:\n");
    s.push_str("    cbz x0, .Lpanic_a64_dec_rev\n");
    s.push_str("    udiv x17, x0, x28\n");
    s.push_str("    msub x16, x17, x28, x0\n"); // remainder = x0 - (x0/10)*10
    s.push_str("    add w16, w16, #48\n");
    s.push_str("    strb w16, [x25], #1\n");
    s.push_str("    mov x0, x17\n");
    s.push_str("    b .Lpanic_a64_dec_loop\n");
    // The digits came out least significant first; turn them around.
    s.push_str(".Lpanic_a64_dec_rev:\n");
    s.push_str("    sub x16, x25, #1\n");
    s.push_str(".Lpanic_a64_dec_revloop:\n");
    s.push_str("    cmp x27, x16\n");
    s.push_str("    b.ge .Lpanic_a64_dec_done\n");
    s.push_str("    ldrb w17, [x27]\n");
    s.push_str("    ldrb w26, [x16]\n");
    s.push_str("    strb w26, [x27]\n");
    s.push_str("    strb w17, [x16]\n");
    s.push_str("    add x27, x27, #1\n");
    s.push_str("    sub x16, x16, #1\n");
    s.push_str("    b .Lpanic_a64_dec_revloop\n");
    s.push_str(".Lpanic_a64_dec_done:\n");
    s.push_str("    ret\n");
    s
}

/// The two entry points and the literal words they differ in — the same
/// list the x86 side builds (`panic_rt::TRAMPOLINE`/`TRAMPOLINE_INDEX`), so
/// a bounds panic reads `index=`/`len=` on both machines.
fn entries() -> Vec<(&'static str, &'static str, &'static str)> {
    let mut v = vec![(TRAMPOLINE, " (a=", " b=")];
    if crate::panic_rt::index_used() {
        v.push((crate::panic_rt::TRAMPOLINE_INDEX, " (index=", " len="));
    }
    v
}

/// Branches into the trampoline with the convention it expects. `a_reg`
/// and `b_reg` hold the two ORIGINAL operand values (64 bits, extended
/// the way the type reads them).
fn trampoline_jump(
    e: &mut Emitter,
    code: u64,
    msg_label: &str,
    msg_len: usize,
    a_reg: &str,
    b_reg: &str,
    unsigned: bool,
) {
    // The order matters: x2/x3 are written from registers that may BE x2
    // or x3 — they are not, in this backend (x9..x17), but the message
    // pointer is built last all the same so that nothing is read after it
    // was overwritten.
    e.line(&format!("mov x2, {}", a_reg));
    e.line(&format!("mov x3, {}", b_reg));
    e.line(&format!("adrp x0, {}", msg_label));
    e.line(&format!("add x0, x0, :lo12:{}", msg_label));
    crate::codegen_a64::imm_into(e, "x1", msg_len as i64);
    crate::codegen_a64::imm_into(e, "x4", code as i64);
    crate::codegen_a64::imm_into(e, "x5", i64::from(unsigned));
    e.line(&format!("b {}", TRAMPOLINE));
}

/// Like `trampoline_jump`, into a NAMED entry point (round 89).
#[allow(clippy::too_many_arguments)]
fn trampoline_jump_to(
    e: &mut Emitter,
    entry: &str,
    code: u64,
    msg_label: &str,
    msg_len: usize,
    a_reg: &str,
    b_reg: &str,
    unsigned: bool,
) {
    e.line(&format!("mov x2, {}", a_reg));
    e.line(&format!("mov x3, {}", b_reg));
    e.line(&format!("adrp x0, {}", msg_label));
    e.line(&format!("add x0, x0, :lo12:{}", msg_label));
    crate::codegen_a64::imm_into(e, "x1", msg_len as i64);
    crate::codegen_a64::imm_into(e, "x4", code as i64);
    crate::codegen_a64::imm_into(e, "x5", i64::from(unsigned));
    e.line(&format!("b {}", entry));
}

fn panic_code_of(op: BinOp) -> u64 {
    match op {
        BinOp::Add => crate::panic_rt::PANIC_ADD,
        BinOp::Sub => crate::panic_rt::PANIC_SUB,
        BinOp::Mul => crate::panic_rt::PANIC_MUL,
        _ => unreachable!("panic_code_of only ever sees Add/Sub/Mul"),
    }
}

/// `dst = the value in `src` cut to `ty`'s width and read back the way
/// `ty` reads it`. The one operation the range check is built out of.
fn extend_to(e: &mut Emitter, dst: &str, src: &str, ty: FTy) {
    match (ty.signed(), ty.bits()) {
        (true, 8) => e.line(&format!("sxtb {}, {}", dst, wn(src))),
        (true, 16) => e.line(&format!("sxth {}, {}", dst, wn(src))),
        (true, 32) => e.line(&format!("sxtw {}, {}", dst, wn(src))),
        (false, 8) => e.line(&format!("uxtb {}, {}", wn(dst), wn(src))),
        (false, 16) => e.line(&format!("uxth {}, {}", wn(dst), wn(src))),
        (false, 32) => e.line(&format!("mov {}, {}", wn(dst), wn(src))),
        (_, _) => {
            if dst != src {
                e.line(&format!("mov {}, {}", dst, src))
            }
        }
    }
}

/// Emits `+ - *`, CHECKED. Precondition: `x9` = a and `x10` = b, both
/// already extended to 64 bits the way `ty` reads them (`load_ext(.., 64)`
/// in `codegen_a64.rs` produces exactly that). The result is left in `x9`.
pub(crate) fn emit_checked_bin(
    e: &mut Emitter,
    op: BinOp,
    ty: FTy,
    msg: &str,
    site: &mut SiteCounter,
) {
    let bits = ty.bits();
    let label = intern(msg);
    let uid = site.next();
    let ok = format!(".Lchkok{}", uid);
    let bad = format!(".Lchksite{}", uid);
    // The two originals have to survive the operation: the message names
    // them. x11/x13 are scratch of the A64 backend and hold no FIR value.
    e.line(&format!("mov x11, {}", A));
    e.line(&format!("mov x13, {}", B));
    if bits == 64 && op == BinOp::Mul {
        // 64 bits: the upper half decides. Signed overflow means the high
        // word is not the sign extension of the low one, unsigned means it
        // is not zero. x86 reads a flag `imul`/`mul` set for free; A64
        // computes the half and asks.
        if ty.signed() {
            e.line(&format!("smulh {}, {}, {}", U, A, B));
            e.line(&format!("mul {}, {}, {}", A, A, B));
            e.line(&format!("cmp {}, {}, asr #63", U, A));
            e.line(&format!("b.ne {}", bad));
        } else {
            e.line(&format!("umulh {}, {}, {}", U, A, B));
            e.line(&format!("mul {}, {}, {}", A, A, B));
            e.line(&format!("cbnz {}, {}", U, bad));
        }
    } else if bits == 32 && op == BinOp::Mul {
        // Two 32-bit operands, each already extended to 64: their 64-bit
        // product is EXACT, so the question is a range question and needs
        // no flag at all.
        e.line(&format!("mul {}, {}, {}", A, A, B));
        range_check(e, ty, &bad);
    } else if bits >= 32 {
        let (aw, bw) = (rw(A, bits), rw(B, bits));
        match op {
            BinOp::Add => e.line(&format!("adds {}, {}, {}", aw, aw, bw)),
            BinOp::Sub => e.line(&format!("subs {}, {}, {}", aw, aw, bw)),
            _ => unreachable!("emit_checked_bin only ever sees Add/Sub/Mul"),
        }
        // `adds`/`subs` set V for the SIGNED reading. Unsigned addition
        // overflows when a carry comes out (C=1), unsigned subtraction
        // when a borrow was needed (C=0) -- x86 answers both with one
        // `jc`, A64 spells the borrow the other way round.
        let cond = if ty.signed() {
            "vs"
        } else if op == BinOp::Sub {
            "cc"
        } else {
            "cs"
        };
        e.line(&format!("b.{} {}", cond, bad));
    } else {
        // 8 and 16 bits: A64 has no arithmetic at those widths and
        // therefore no flags for them. Two values that fit into 8 or 16
        // bits cannot overflow a 32-bit operation, so the exact result is
        // there and only its RANGE is in question.
        let (aw, bw) = (wn(A), wn(B));
        match op {
            BinOp::Add => e.line(&format!("add {}, {}, {}", aw, aw, bw)),
            BinOp::Sub => e.line(&format!("sub {}, {}, {}", aw, aw, bw)),
            BinOp::Mul => e.line(&format!("mul {}, {}, {}", aw, aw, bw)),
            _ => unreachable!("emit_checked_bin only ever sees Add/Sub/Mul"),
        }
        // Read the 32-bit result as a full 64-bit number before comparing
        // it against its own truncation -- otherwise a negative sum and
        // its sign extension differ in the upper half for a reason that
        // has nothing to do with overflow.
        e.line(&format!("sxtw {}, {}", A, wn(A)));
        range_check(e, ty, &bad);
    }
    e.line(&format!("b {}", ok));
    e.raw(&format!("{}:", bad));
    trampoline_jump(e, panic_code_of(op), &label, msg.len(), "x11", "x13", !ty.signed());
    e.raw(&format!("{}:", ok));
}

/// `x9` cut to `ty` and read back: differs from `x9` exactly when the
/// value does not fit. Branches to `bad` in that case.
fn range_check(e: &mut Emitter, ty: FTy, bad: &str) {
    if ty.bits() >= 64 {
        return;
    }
    extend_to(e, T, A, ty);
    e.line(&format!("cmp {}, {}", A, T));
    e.line(&format!("b.ne {}", bad));
}

/// Emits `/` and `%`, CHECKED: division by zero, and for a signed type the
/// `MIN / -1` case. Precondition: `x9` = a, `x10` = b, extended to 64
/// bits. The RESULT (quotient for `Div`, remainder for `Rem`) is left in
/// `x9`.
pub(crate) fn emit_checked_div(
    e: &mut Emitter,
    op: BinOp,
    ty: FTy,
    msg_zero: &str,
    msg_range: &str,
    site: &mut SiteCounter,
) {
    let bits = ty.bits().max(32);
    let uid = site.next();
    let past_zero = format!(".Lchkdivz{}", uid);
    let site_zero = format!(".Lchksitez{}", uid);
    let past_range = format!(".Lchkdivr{}", uid);
    let site_range = format!(".Lchksiter{}", uid);
    e.line(&format!("mov x11, {}", A));
    e.line(&format!("mov x13, {}", B));
    e.line(&format!("cbnz {}, {}", rw(B, bits), past_zero));
    e.raw(&format!("{}:", site_zero));
    let label0 = intern(msg_zero);
    trampoline_jump(e, PANIC_DIV0, &label0, msg_zero.len(), "x11", "x13", !ty.signed());
    e.raw(&format!("{}:", past_zero));
    if ty.signed() {
        let min_val: i64 = match ty {
            FTy::I8 => i8::MIN as i64,
            FTy::I16 => i16::MIN as i64,
            FTy::I32 => i32::MIN as i64,
            _ => i64::MIN,
        };
        crate::codegen_a64::imm_into(e, T, min_val);
        e.line(&format!("cmp {}, {}", A, T));
        e.line(&format!("b.ne {}", past_range));
        crate::codegen_a64::imm_into(e, T, -1);
        e.line(&format!("cmp {}, {}", B, T));
        e.line(&format!("b.ne {}", past_range));
        e.raw(&format!("{}:", site_range));
        let label_r = intern(msg_range);
        trampoline_jump(e, PANIC_DIV_OVERFLOW, &label_r, msg_range.len(), "x11", "x13", false);
    }
    e.raw(&format!("{}:", past_range));
    // Both operands are already at the computing width and correctly
    // extended; A64 divides in one instruction and gets its remainder out
    // of `msub`, exactly as the unchecked path does.
    let d = if ty.signed() { "sdiv" } else { "udiv" };
    let (aw, bw) = (rw(A, bits), rw(B, bits));
    if op == BinOp::Rem {
        e.line(&format!("{} {}, {}, {}", d, rw(T, bits), aw, bw));
        e.line(&format!("msub {}, {}, {}, {}", aw, rw(T, bits), bw, aw));
    } else {
        e.line(&format!("{} {}, {}, {}", d, aw, aw, bw));
    }
}

/// Emits a checked (narrowing) `as`. Precondition: `x9` holds the source
/// value, extended to 64 bits the way `from` reads it. Leaves the
/// converted value in `x9`.
pub(crate) fn emit_checked_cast(
    e: &mut Emitter,
    from: FTy,
    to: FTy,
    msg: &str,
    site: &mut SiteCounter,
) {
    let uid = site.next();
    let ok = format!(".Lchkcast{}", uid);
    let bad = format!(".Lchksitec{}", uid);
    // The original, for the comparison and for the message.
    e.line(&format!("mov x11, {}", A));
    e.line(&format!("mov x13, {}", A));
    // Narrow to `to`, then widen BACK the way `from` reads it: anything
    // the conversion lost shows up as a difference against the original.
    extend_to(e, A, A, to);
    if from.bits() > to.bits() {
        extend_to(e, A, A, from);
    }
    e.line(&format!("cmp {}, x11", A));
    e.line(&format!("b.eq {}", ok));
    e.raw(&format!("{}:", bad));
    let label = intern(msg);
    trampoline_jump(e, PANIC_CAST, &label, msg.len(), "x11", "x13", !from.signed());
    e.raw(&format!("{}:", ok));
    // Nothing was lost — but the value in `x9` was widened back to
    // `from`'s width for the comparison. Cut it to `to` once more, so the
    // caller's `store_dst` sees exactly what the unchecked cast produces.
    e.line(&format!("mov {}, x11", A));
    extend_to(e, A, A, to);
}

/// Emits `+% -% *%` (wrap) and `+| -| *|` (saturate). Never checked, in
/// any build level. Precondition and result register as above.
pub(crate) fn emit_wrap_sat(
    e: &mut Emitter,
    kind: crate::fir::WrapSatKind,
    op: BinOp,
    ty: FTy,
    site: &mut SiteCounter,
) -> Result<(), String> {
    let bits = ty.bits();
    let wide = bits.max(32);
    if kind == crate::fir::WrapSatKind::Wrap {
        // Wrapping IS what two's complement arithmetic does; the store
        // narrows to the type's width exactly as for the unchecked path.
        let (aw, bw) = (rw(A, wide), rw(B, wide));
        match op {
            BinOp::Add => e.line(&format!("add {}, {}, {}", aw, aw, bw)),
            BinOp::Sub => e.line(&format!("sub {}, {}, {}", aw, aw, bw)),
            BinOp::Mul => e.line(&format!("mul {}, {}, {}", aw, aw, bw)),
            _ => return Err("internal error: wrap/sat only defined for + - *".to_string()),
        }
        return Ok(());
    }
    let uid = site.next();
    let clamp = format!(".Lsatclamp{}", uid);
    let done = format!(".Lsatdone{}", uid);
    // The two originals decide WHICH bound was crossed.
    e.line(&format!("mov x11, {}", A));
    e.line(&format!("mov x13, {}", B));
    let (min_lit, max_lit): (i64, i64) = match ty {
        FTy::I8 => (i8::MIN as i64, i8::MAX as i64),
        FTy::I16 => (i16::MIN as i64, i16::MAX as i64),
        FTy::I32 => (i32::MIN as i64, i32::MAX as i64),
        FTy::I64 => (i64::MIN, i64::MAX),
        FTy::U8 => (0, u8::MAX as i64),
        FTy::U16 => (0, u16::MAX as i64),
        FTy::U32 => (0, u32::MAX as i64),
        _ => (0, -1), // u64::MAX as a bit pattern
    };
    if bits >= 32 {
        let (aw, bw) = (rw(A, bits), rw(B, bits));
        match op {
            BinOp::Add => e.line(&format!("adds {}, {}, {}", aw, aw, bw)),
            BinOp::Sub => e.line(&format!("subs {}, {}, {}", aw, aw, bw)),
            BinOp::Mul if bits == 32 => {
                e.line(&format!("mul {}, {}, {}", A, A, B));
                fits_or(e, ty, &clamp);
                e.line(&format!("b {}", done));
                clamp_arm(e, &clamp, op, ty, min_lit, max_lit);
                e.raw(&format!("{}:", done));
                return Ok(());
            }
            BinOp::Mul => {
                if ty.signed() {
                    e.line(&format!("smulh {}, {}, {}", U, A, B));
                    e.line(&format!("mul {}, {}, {}", A, A, B));
                    e.line(&format!("cmp {}, {}, asr #63", U, A));
                    e.line(&format!("b.ne {}", clamp));
                } else {
                    e.line(&format!("umulh {}, {}, {}", U, A, B));
                    e.line(&format!("mul {}, {}, {}", A, A, B));
                    e.line(&format!("cbnz {}, {}", U, clamp));
                }
                e.line(&format!("b {}", done));
                clamp_arm(e, &clamp, op, ty, min_lit, max_lit);
                e.raw(&format!("{}:", done));
                return Ok(());
            }
            _ => return Err("internal error: wrap/sat only defined for + - *".to_string()),
        }
        let cond = if ty.signed() {
            "vs"
        } else if op == BinOp::Sub {
            "cc"
        } else {
            "cs"
        };
        e.line(&format!("b.{} {}", cond, clamp));
    } else {
        let (aw, bw) = (wn(A), wn(B));
        match op {
            BinOp::Add => e.line(&format!("add {}, {}, {}", aw, aw, bw)),
            BinOp::Sub => e.line(&format!("sub {}, {}, {}", aw, aw, bw)),
            BinOp::Mul => e.line(&format!("mul {}, {}, {}", aw, aw, bw)),
            _ => return Err("internal error: wrap/sat only defined for + - *".to_string()),
        }
        e.line(&format!("sxtw {}, {}", A, wn(A)));
        fits_or(e, ty, &clamp);
    }
    e.line(&format!("b {}", done));
    clamp_arm(e, &clamp, op, ty, min_lit, max_lit);
    e.raw(&format!("{}:", done));
    Ok(())
}

/// Branches to `clamp` when the value in `x9` does not fit `ty`.
fn fits_or(e: &mut Emitter, ty: FTy, clamp: &str) {
    if ty.bits() >= 64 {
        return;
    }
    extend_to(e, T, A, ty);
    e.line(&format!("cmp {}, {}", A, T));
    e.line(&format!("b.ne {}", clamp));
}

/// The saturating arm: which of the two bounds the result is clamped to.
fn clamp_arm(e: &mut Emitter, clamp: &str, op: BinOp, ty: FTy, min_lit: i64, max_lit: i64) {
    e.raw(&format!("{}:", clamp));
    if !ty.signed() {
        // Unsigned: `+`/`*` overflow means too big, `-` means below zero.
        let v = if op == BinOp::Sub { min_lit } else { max_lit };
        crate::codegen_a64::imm_into(e, A, v);
        return;
    }
    // Signed: the sign of one original operand (Add), of the second one
    // (Sub) or of the two XORed (Mul) says which end was crossed — the
    // same three rules the x86 side uses, with `csel` where x86 has
    // `cmovl`.
    crate::codegen_a64::imm_into(e, T, max_lit);
    crate::codegen_a64::imm_into(e, U, min_lit);
    match op {
        BinOp::Add => e.line("cmp x11, #0"),
        BinOp::Sub => {
            // a - b out of range: b negative means the true difference was
            // ABOVE MAX, b positive means below MIN — the opposite way
            // round from Add, so the two bounds are swapped instead of the
            // comparison.
            e.line("cmp x13, #0");
            e.line(&format!("csel {}, {}, {}, lt", A, T, U));
            return;
        }
        BinOp::Mul => {
            e.line(&format!("eor {}, x11, x13", A));
            e.line(&format!("cmp {}, #0", A));
        }
        _ => unreachable!("guarded by the caller"),
    }
    e.line(&format!("csel {}, {}, {}, lt", A, U, T));
}

/// **ROUND 89** — the checked ARRAY INDEX on aarch64 (SPEC §13, `L9`).
///
/// Precondition: `x9` = the index, zero extended to 64 bits. The same two
/// instructions the x86 side needs when nothing is wrong: one `cmp` and one
/// not-taken `b.lo` (unsigned "below"). `len` is a compile time number and
/// goes into `x10` through the backend's own immediate builder, which
/// already knows how to make a 64-bit constant out of `movz`/`movk`.
pub(crate) fn emit_checked_idx(e: &mut Emitter, len: u64, msg: &str, site: &mut SiteCounter) {
    crate::panic_rt::note_index_site();
    let label = intern(msg);
    let uid = site.next();
    let ok = format!(".Lchkidx{}", uid);
    crate::codegen_a64::imm_into(e, B, len as i64);
    e.line(&format!("cmp {}, {}", A, B));
    e.line(&format!("b.lo {}", ok));
    trampoline_jump_to(
        e,
        crate::panic_rt::TRAMPOLINE_INDEX,
        crate::panic_rt::PANIC_INDEX,
        &label,
        msg.len(),
        A,
        B,
        true,
    );
    e.raw(&format!("{}:", ok));
}
