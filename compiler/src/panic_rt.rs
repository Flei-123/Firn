//! **Round 72** — checked arithmetic: the panic path (SPEC §13, `L9`).
//!
//! `release-safe` promised to CHECK integer arithmetic and did not: the
//! build level existed, `+ - *` never aborted on overflow in it. This file
//! is the missing half — everything a checked operation needs once the
//! overflow flag is set, wherever `codegen_x86.rs`/`regalloc.rs` decide to
//! test it (both backends call [`emit_checked_bin`]/[`emit_checked_div`]/
//! [`emit_checked_cast`] with the operands already sitting in known
//! registers; the trampoline itself is written ONCE per object file by
//! [`rodata_asm`]/[`trampoline_asm`]).
//!
//! ## Shape
//!
//! One shared out-of-line trampoline per object file, [`TRAMPOLINE`]. A
//! checked instruction that finds itself out of range jumps there with:
//!   * `rdi` = address of the pre-built message text (`.rodata`, ready to
//!     print: file, line, the operator, English words),
//!   * `esi` = length of that text,
//!   * `rdx` = the first operand's value (sign extended to 64 bits),
//!   * `r9` = 1 when the two values are to be read as UNSIGNED, 0 when
//!     signed. Round 72 shipped without it and printed every value through
//!     the SIGNED digit routine, so `u64::MAX + 1` reported `(a=-1 b=1)` --
//!     a number that does not occur anywhere in the program the reader is
//!     looking at. Found while making `tests/690_lowering_core.fi` pass.
//!   * `rcx` = the second operand's value -- for a checked cast (`as`)
//!     there IS only one value, so `rcx` carries the same one again
//!     rather than a misleading 0 (`emit_checked_cast` below jumps in
//!     with `a_reg`/`b_reg` both set to the source value); the message
//!     text itself never mentions "b=" for a cast (`lower.rs::cast_msg`),
//!     the repeated number in the printed "(a=N b=N)" is a harmless
//!     redundancy, not a bug -- there was no natural second value to put
//!     there instead.
//!
//! Two very different endings, decided once by `prof::is_kernel()`:
//!
//! * **`app`** — there is a runtime (`write`, `exit_group`): the trampoline
//!   turns `rdx`/`rcx` into decimal digits on a small stack buffer, writes
//!   `<message> (a=<N> b=<M>)\n` to file descriptor 2 and calls
//!   `exit_group(101)`. No allocation, no `std.*` — this has to run inside
//!   a plain `app` program that imports nothing at all.
//! * **`kernel`** — there is neither. SPEC §2's own table already promises
//!   *"Panic on an out-of-range index: calls `osum_panic`, configurable"*
//!   for the one runtime panic that already existed on paper (bounds
//!   checks, not yet built). Checked arithmetic is the very same shape of
//!   problem and gets the same answer: the trampoline hands off to an
//!   EXTERNAL symbol `osum_panic(msg_ptr, msg_len, a, b, code)` that the
//!   kernel must define itself — the freestanding profile has no `write`,
//!   no `exit`, and this file cannot invent an ending on its own behalf.
//!   Leaving it undefined is a **link error**, not a quiet no-op: `ld`
//!   refuses the image, which is the whole point.
//!
//! `code` distinguishes the panic kinds for a kernel that wants to log or
//! recover differently (`PANIC_ADD` .. `PANIC_CAST` below); an `app`
//! program never sees the number, only the text.
//!
//! ## A register that is not free to use
//!
//! `regalloc.rs`'s `TEMP_REGS` includes `r10`/`r11` as ordinary VALUE
//! registers — a live value can sit there across the checked instruction,
//! to be read again afterwards. Only `rax`/`rcx`/`rdx` are ever pure
//! scratch at a checked site (the same three the UNCHECKED arithmetic path
//! already computes in). That is why the two original operand values are
//! rescued on the STACK (`push`/`push`, popped back in the success case
//! with a plain `add rsp, 16`, or popped INTO the message registers in the
//! failure case) rather than in a spare register: the stack is the one
//! storage neither backend's allocator ever hands out to a FIR value.

use std::cell::RefCell;

use crate::codegen_x86::Emitter;
use crate::fir::{BinOp, FTy};

/// One prepared panic message, interned once per distinct text so that two
/// checked additions that share a source line and an operator do not
/// duplicate the string in `.rodata`.
#[derive(Default)]
struct Table {
    /// message text -> its `.rodata` label index
    msgs: Vec<String>,
}

thread_local! {
    static TABLE: RefCell<Table> = RefCell::new(Table::default());
}

/// Resets the interning table — only between compilations of the SAME
/// process (module tests, `--package`).
pub fn reset() {
    TABLE.with(|t| *t.borrow_mut() = Table::default());
}

/// Registers a message text and returns its `.rodata` label
/// (`.Lpanicmsg<N>`). Called from `lower.rs` while building `Op::CheckedBin`
/// et al. — the label is baked into the FIR text (`fir.rs::fmt_inst`) only
/// as part of the human readable dump; the backends look the text up again
/// through this SAME table, keyed by content, so both compilers produce the
/// identical `.rodata` layout from identical FIR text.
pub fn intern(msg: &str) -> String {
    TABLE.with(|t| {
        let mut t = t.borrow_mut();
        if let Some(i) = t.msgs.iter().position(|m| m == msg) {
            return label_of(i);
        }
        let i = t.msgs.len();
        t.msgs.push(msg.to_string());
        label_of(i)
    })
}

fn label_of(i: usize) -> String {
    format!(".Lpanicmsg{}", i)
}

fn msg_len(msg: &str) -> usize {
    msg.len()
}

/// Is at least one checked operation anywhere in this compilation? Only
/// then is the trampoline and the `.rodata` table emitted at all — a
/// program compiled `--opt-level=release-fast` (or one that simply never
/// triggers a checked path) pays not one byte for the feature (SPEC §13,
/// cost measured in `docs/ROUND72.md`).
pub fn any_registered() -> bool {
    TABLE.with(|t| !t.borrow().msgs.is_empty())
}

/// Panic kind codes handed to `osum_panic` under `profile kernel`. Stable
/// numbers (part of the ABI a kernel author codes against), not FIR-facing.
pub const PANIC_ADD: u64 = 1;
pub const PANIC_SUB: u64 = 2;
pub const PANIC_MUL: u64 = 3;
pub const PANIC_DIV0: u64 = 4;
pub const PANIC_DIV_OVERFLOW: u64 = 5;
pub const PANIC_CAST: u64 = 6;

/// Label of the shared trampoline.
pub const TRAMPOLINE: &str = ".Lpanic_arith";
/// External symbol a `profile kernel` program must define itself.
pub const OSUM_PANIC: &str = "osum_panic";

/// `.rodata` — every distinct message text as raw octets (no NUL
/// terminator, exactly the SPEC §8 `Str` convention: length travels
/// alongside, not baked into the bytes).
pub fn rodata_asm() -> String {
    let mut out = String::new();
    TABLE.with(|t| {
        let t = t.borrow();
        if t.msgs.is_empty() {
            return;
        }
        out.push_str(".section .rodata\n");
        for (i, m) in t.msgs.iter().enumerate() {
            out.push_str(&format!("{}:\n", label_of(i)));
            out.push_str(&format!("    .ascii \"{}\"\n", crate::fir::asm_escape(m)));
        }
        out.push_str(".text\n");
    });
    out
}

/// The shared out-of-line trampoline, written ONCE per object file
/// (`codegen_x86.rs::emit`, guarded by [`any_registered`]).
///
/// Calling convention on entry: `rdi`=msg ptr, `esi`=msg len, `rdx`=a,
/// `rcx`=b, `r8`=panic kind code.
pub fn trampoline_asm() -> String {
    if crate::prof::is_kernel() {
        // No runtime at all. Somebody else decides what a panic means —
        // that is exactly the SPEC §2 promise ("osum_panic, configurable").
        // `osum_panic` is an external symbol; an undefined reference at
        // link time is the honest outcome when a kernel never defines it.
        let mut s = String::new();
        s.push_str(&format!("{}:\n", TRAMPOLINE));
        s.push_str(&format!("    call {}\n", OSUM_PANIC));
        s.push_str("    # osum_panic is not supposed to come back; running\n");
        s.push_str("    # into whatever comes next in .text would be silently\n");
        s.push_str("    # wrong, so this traps instead of guessing.\n");
        s.push_str("    ud2\n");
        s
    } else {
        // `app`: build "<msg> (a=<N> b=<M>)\n" on the stack and write(2, ., .)
        // then exit_group(101). No malloc, no std.* — this is the one place
        // in the compiler allowed to hand-roll a decimal formatter, because
        // it must work even in a program that imports nothing at all.
        let mut s = String::new();
        s.push_str(&format!("{}:\n", TRAMPOLINE));
        s.push_str("    push rbx\n");
        s.push_str("    push r12\n");
        s.push_str("    push r13\n");
        s.push_str("    push r14\n");
        s.push_str("    push r15\n");
        s.push_str("    mov r12, rdi\n");
        s.push_str("    mov r13, rsi\n");
        // ROUND 72, second pass: `r9` says whether the two numbers are to be
        // read as unsigned. It has to survive both calls to
        // `.Lpanic_i64_dec` (which clobbers rax/rcx/rdx/r8/r9), so it moves
        // into a register the trampoline saved itself.
        s.push_str("    mov r15, r9\n");
        // `b` (rcx) is rescued BEFORE the first call to `.Lpanic_i64_dec`:
        // that routine uses rcx/rdx itself as its own division scratch, so
        // reading rcx again after the FIRST call (for `a`) would read
        // whatever the digit loop left behind, not the real value of `b`.
        s.push_str("    mov r14, rcx\n");
        s.push_str("    sub rsp, 160\n");
        s.push_str("    mov rbx, rsp\n");
        s.push_str("    mov byte ptr [rbx], 32\n");
        s.push_str("    mov byte ptr [rbx+1], 40\n");
        s.push_str("    mov byte ptr [rbx+2], 97\n");
        s.push_str("    mov byte ptr [rbx+3], 61\n");
        s.push_str("    add rbx, 4\n");
        s.push_str("    mov rax, rdx\n");
        s.push_str("    call .Lpanic_i64_dec\n");
        s.push_str("    mov byte ptr [rbx], 32\n");
        s.push_str("    mov byte ptr [rbx+1], 98\n");
        s.push_str("    mov byte ptr [rbx+2], 61\n");
        s.push_str("    add rbx, 3\n");
        s.push_str("    mov rax, r14\n");
        s.push_str("    call .Lpanic_i64_dec\n");
        s.push_str("    mov byte ptr [rbx], 41\n");
        s.push_str("    mov byte ptr [rbx+1], 10\n");
        s.push_str("    add rbx, 2\n");
        s.push_str("    mov rax, 1\n");
        s.push_str("    mov rdi, 2\n");
        s.push_str("    mov rsi, r12\n");
        s.push_str("    mov rdx, r13\n");
        s.push_str("    syscall\n");
        s.push_str("    mov rax, 1\n");
        s.push_str("    mov rdi, 2\n");
        s.push_str("    lea rsi, [rsp]\n");
        s.push_str("    mov rdx, rbx\n");
        s.push_str("    sub rdx, rsp\n");
        s.push_str("    syscall\n");
        // r14 is popped only AFTER both writes -- popping it earlier would
        // shift rsp by 8 while rbx (the buffer's fixed base) stays where it
        // was, throwing the second write's start address and length off by
        // exactly that much (the bug this comment replaces).
        s.push_str("    pop r15\n");
        s.push_str("    pop r14\n");
        s.push_str("    mov rax, 231\n");
        s.push_str("    mov rdi, 101\n");
        s.push_str("    syscall\n");
        s.push_str("    hlt\n");
        // Helper: append the decimal (signed) text of `rax` at `[rbx]`,
        // advance `rbx` past it. Clobbers rax/rcx/rdx/r8/r9. Two's
        // complement makes `neg` on `i64::MIN` produce the right MAGNITUDE
        // as an unsigned bit pattern (`-MIN mod 2^64 == 2^63`), so an
        // unsigned `div` afterwards is correct for every possible `i64`,
        // that one value included.
        s.push_str(".Lpanic_i64_dec:\n");
        // An UNSIGNED type never has a minus sign and never negates: its
        // bit pattern IS the number. Only the signed reading looks at bit 63.
        s.push_str("    test r15, r15\n");
        s.push_str("    jnz .Lpanic_dec_nonneg\n");
        s.push_str("    test rax, rax\n");
        s.push_str("    jns .Lpanic_dec_nonneg\n");
        s.push_str("    mov byte ptr [rbx], 45\n");
        s.push_str("    inc rbx\n");
        s.push_str("    neg rax\n");
        s.push_str(".Lpanic_dec_nonneg:\n");
        s.push_str("    mov r8, rbx\n");
        s.push_str("    mov rcx, 10\n");
        s.push_str("    test rax, rax\n");
        s.push_str("    jnz .Lpanic_dec_loop\n");
        s.push_str("    mov byte ptr [rbx], 48\n");
        s.push_str("    inc rbx\n");
        s.push_str("    jmp .Lpanic_dec_rev\n");
        s.push_str(".Lpanic_dec_loop:\n");
        s.push_str("    test rax, rax\n");
        s.push_str("    jz .Lpanic_dec_rev\n");
        s.push_str("    xor rdx, rdx\n");
        s.push_str("    div rcx\n");
        s.push_str("    add rdx, 48\n");
        s.push_str("    mov byte ptr [rbx], dl\n");
        s.push_str("    inc rbx\n");
        s.push_str("    jmp .Lpanic_dec_loop\n");
        s.push_str(".Lpanic_dec_rev:\n");
        s.push_str("    mov rcx, rbx\n");
        s.push_str("    dec rcx\n");
        s.push_str(".Lpanic_dec_revloop:\n");
        s.push_str("    cmp r8, rcx\n");
        s.push_str("    jge .Lpanic_dec_done\n");
        s.push_str("    mov al, byte ptr [r8]\n");
        s.push_str("    mov dl, byte ptr [rcx]\n");
        s.push_str("    mov byte ptr [r8], dl\n");
        s.push_str("    mov byte ptr [rcx], al\n");
        s.push_str("    inc r8\n");
        s.push_str("    dec rcx\n");
        s.push_str("    jmp .Lpanic_dec_revloop\n");
        s.push_str(".Lpanic_dec_done:\n");
        s.push_str("    ret\n");
        s
    }
}

// --------------------------------------------------------- shared codegen ---
//
// Both backends (`codegen_x86.rs`'s plain one and `regalloc.rs`'s allocator
// aware one) reach the exact same instructions through the three functions
// below — checked arithmetic gets no separate "fast" and "careful" path,
// there is only one correct way to test the flag. What differs between the
// backends is only how the two OPERANDS get into `rax`/`rcx` beforehand
// (their APIs for that are not the same); these functions assume the values
// are there already, sign/zero extended to 64 bits — exactly what
// `load_ext(.., 64)` already produces in both backends.
//
// `site` is a small counter the caller owns (one per function; both
// backends keep their own) so that two checked operations in the same
// function get two distinct label pairs.
//
// BUG FOUND while writing docs/ROUND72.md, fixed in this same round: the
// counter alone was NOT enough. Two DIFFERENT functions each start their
// own `SiteCounter` at 0 (by design, see above) -- but the labels built
// from just that number (`.Lchksite0`, `.Lchkok0`, ...) collided the
// moment a program had two functions that BOTH used checked arithmetic:
// `as` (GNU assembler) rejects the file outright ("symbol already
// defined"), which `test.sh` would have caught immediately had a single
// one of its 143 test programs happened to declare more than one function
// with `+ - *` in it. `block_label`/`label` already fold the FUNCTION name
// into every OTHER label this compiler emits (`.Lmain__bb0`, `main:`); the
// checked-site labels were the one place that forgot to.

/// A monotonically increasing counter, reset once per function, so that
/// every checked site in that function gets its own label pair. Carries
/// the function's own name so the labels it hands out are unique ACROSS
/// functions too, not only within one.
#[derive(Default)]
pub(crate) struct SiteCounter {
    fn_name: String,
    next_id: u64,
}

impl SiteCounter {
    pub(crate) fn new(fn_name: &str) -> SiteCounter {
        // ROUND 58's own rule, missed when this struct was written (found
        // compiling `tests/875_closure_callback.fi`): a generated closure
        // name carries a `#` (`codegen_x86.rs::block_label` already turns
        // it into a `.` for exactly this reason) -- `#` is a COMMENT
        // character to the GNU assembler, so a label built straight from
        // the function name truncated itself at the first one and `as`
        // rejected what was left as an unknown pseudo-op.
        SiteCounter { fn_name: fn_name.replace('#', "."), next_id: 0 }
    }

    /// A fresh label suffix, unique both within this function (the
    /// counter) and across every other function in the same object file
    /// (the name folded in) -- `.Lchksite{suffix}` / `.Lchkok{suffix}`
    /// never collide with another function's the way a bare number did.
    pub(crate) fn next(&mut self) -> String {
        let v = self.next_id;
        self.next_id += 1;
        format!("{}_{}", self.fn_name, v)
    }
}

/// Jumps into the shared trampoline with the calling convention it expects.
/// `code` is one of the `PANIC_*` constants; `msg_label` names a string
/// already interned with [`intern`]; `a_reg`/`b_reg` hold the two original
/// (64-bit sign extended) operand values.
#[allow(clippy::too_many_arguments)]
fn emit_trampoline_jump(
    e: &mut Emitter,
    code: u64,
    msg_label: &str,
    msg_text: &str,
    a_reg: &str,
    b_reg: &str,
    unsigned: bool,
) {
    if a_reg != "rdx" {
        e.line(&format!("mov rdx, {}", a_reg));
    }
    if b_reg != "rcx" {
        e.line(&format!("mov rcx, {}", b_reg));
    }
    e.line(&format!("lea rdi, [rip + {}]", msg_label));
    e.line(&format!("mov esi, {}", msg_len(msg_text)));
    e.line(&format!("mov r8, {}", code));
    e.line(&format!("mov r9, {}", u32::from(unsigned)));
    e.line(&format!("jmp {}", TRAMPOLINE));
}

fn panic_code_of(op: BinOp) -> u64 {
    match op {
        BinOp::Add => PANIC_ADD,
        BinOp::Sub => PANIC_SUB,
        BinOp::Mul => PANIC_MUL,
        _ => unreachable!("panic_code_of only ever sees Add/Sub/Mul"),
    }
}

/// Register name at width `bits` for `rax`/`rcx` — its OWN tiny table
/// rather than reusing either backend's (`codegen_x86::reg` is private to
/// its file, `regalloc::rn` returns an owned `String` instead of a
/// `&'static str`).
fn narrow(name: &str, bits: u32) -> &'static str {
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
}

/// Emits `+ - *`, CHECKED (SPEC §13, `L9`). Precondition: `rax`=a, `rcx`=b,
/// both already sign/zero extended to `ty`'s own signedness at 64 bits.
/// Computes at `ty`'s OWN bit width, so the CPU's overflow/carry flag means
/// what `ty` means — a value that only fits into 8 or 16 bits is caught
/// even though the registers underneath are wider, which is why this does
/// NOT reuse the "compute at 32/64 bits regardless, narrow the result on
/// store" shortcut the unchecked `Op::Bin` path takes.
///
/// Leaves the result in `rax` (the caller's `store_dst` narrows it exactly
/// as for `Op::Bin`).
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
    let site_label = format!(".Lchksite{}", uid);
    // The two ORIGINAL values go on the stack — neither backend's register
    // allocator ever spills a live FIR value there behind our back at this
    // exact point, unlike r10/r11 (see the module header).
    e.line("push rax");
    e.line("push rcx");
    match op {
        BinOp::Add => e.line(&format!("add {}, {}", narrow("rax", bits), narrow("rcx", bits))),
        BinOp::Sub => e.line(&format!("sub {}, {}", narrow("rax", bits), narrow("rcx", bits))),
        // `imul r, r/m` (the two operand form) exists for 16, 32 and 64
        // bits ONLY -- `imul al, cl` is not an instruction, and `as`
        // rejected the whole file the first time a program multiplied two
        // `i8` under a checked build level (round 72 shipped that way; no
        // test happened to do it). The ONE operand form is the 8 bit
        // answer: `imul cl` computes ax = al * cl and sets OF exactly when
        // the product does not fit back into al -- which is the question
        // being asked.
        BinOp::Mul if ty.signed() && bits == 8 => e.line("imul cl"),
        BinOp::Mul if ty.signed() => {
            e.line(&format!("imul {}, {}", narrow("rax", bits), narrow("rcx", bits)))
        }
        // one-operand `mul`: rdx:rax (or the dx:ax / high:low split at
        // narrower widths, still addressed through eax/ax/al) is the full
        // product; CF=OF=1 exactly when the upper half is not all zero
        // bits — precisely unsigned overflow.
        BinOp::Mul => e.line(&format!("mul {}", narrow("rcx", bits))),
        _ => unreachable!("emit_checked_bin only ever sees Add/Sub/Mul"),
    }
    // `add`/`sub` set OF for the SIGNED interpretation only; an unsigned
    // type that overflows its width sets CF instead (OF can even stay
    // clear, as `200u8 + 100u8` shows -- as i8 that sum fits). `imul`'s
    // one-operand `mul` form sets CF=OF together for unsigned overflow, so
    // testing CF for every unsigned op (not just Mul) is correct across
    // the board; signed stays on OF, its own meaning of "out of range".
    e.line(&format!(
        "j{} {}",
        if ty.signed() { "o" } else { "c" },
        site_label
    ));
    // Success: drop the two rescued words (their values are not needed
    // again — the caller already has the answer in rax) and continue.
    e.line("add rsp, 16");
    e.line(&format!("jmp {}", ok));
    e.raw(&format!("{}:", site_label));
    e.line("pop rcx");
    e.line("pop rdx");
    emit_trampoline_jump(e, panic_code_of(op), &label, msg, "rdx", "rcx", !ty.signed());
    e.raw(&format!("{}:", ok));
}

/// Emits `/` and `%`, CHECKED (SPEC §13, `L9`): division by zero, and for
/// SIGNED types the `MIN / -1` (`MIN % -1`) special case that would
/// otherwise raise `SIGFPE` with no message of our own at all. Precondition:
/// `rax`=a, `rcx`=b, both sign/zero extended to 64 bits (the caller loads
/// them exactly once here, unlike the unchecked `BinOp::Div`/`Rem` path
/// which loads straight into the computing width — the check needs the
/// ORIGINAL 64-bit values available for the message even after `cqo`/`cdq`
/// destroy `rdx`).
pub(crate) fn emit_checked_div(
    e: &mut Emitter,
    op: BinOp,
    ty: FTy,
    msg_zero: &str,
    msg_range: &str,
    site: &mut SiteCounter,
) {
    let bits = ty.bits().max(32);
    let wide = ty.bits() > 32;
    let uid = site.next();
    let past_zero = format!(".Lchkdivz{}", uid);
    let past_range = format!(".Lchkdivr{}", uid);
    let site_zero = format!(".Lchksitez{}", uid);
    let site_range = format!(".Lchksiter{}", uid);
    e.line("push rax");
    e.line("push rcx");
    e.line(&format!("test {}, {}", narrow("rcx", bits), narrow("rcx", bits)));
    e.line(&format!("jz {}", site_zero));
    e.line(&format!("jmp {}", past_zero));
    e.raw(&format!("{}:", site_zero));
    e.line("pop rcx");
    e.line("pop rdx");
    let label0 = intern(msg_zero);
    emit_trampoline_jump(e, PANIC_DIV0, &label0, msg_zero, "rdx", "rcx", !ty.signed());
    e.raw(&format!("{}:", past_zero));
    if ty.signed() {
        // MIN / -1: two compares with a shared "definitely fine" target —
        // either one failing to match already rules the special case out.
        let min_val: i64 = match ty {
            FTy::I8 => i8::MIN as i64,
            FTy::I16 => i16::MIN as i64,
            FTy::I32 => i32::MIN as i64,
            _ => i64::MIN,
        };
        // `cmp r64, imm32` is the only immediate FORM x86-64 has for a
        // 64-bit register compare -- the assembler sign-extends a 32-bit
        // literal, which cannot represent `i64::MIN` at all (found
        // compiling firnc1.fi itself: `as` rejected "operand type
        // mismatch" the moment an actual `i64` division reached this
        // path, something none of this round's own hand-written `.fi`
        // test files happened to exercise at 64 bits). `rdx` is free
        // scratch here regardless of width: the rescued pair still sits
        // on the stack UNTOUCHED (the `add rsp, 16` below is the first
        // thing to consume it), and the `site_range` arm below already
        // overwrites `rdx` with `pop rdx` before it is read again.
        if bits > 32 {
            e.line(&format!("mov rdx, {}", min_val));
            e.line("cmp rax, rdx");
        } else {
            e.line(&format!("cmp {}, {}", narrow("rax", bits), min_val));
        }
        e.line(&format!("jne {}", past_range));
        e.line(&format!("cmp {}, -1", narrow("rcx", bits)));
        e.line(&format!("jne {}", past_range));
        e.line(&format!("jmp {}", site_range));
        e.raw(&format!("{}:", site_range));
        e.line("pop rcx");
        e.line("pop rdx");
        let label_r = intern(msg_range);
        emit_trampoline_jump(e, PANIC_DIV_OVERFLOW, &label_r, msg_range, "rdx", "rcx", false);
    }
    e.raw(&format!("{}:", past_range));
    // Nothing went out of range: the stack still holds the two rescued
    // words underneath the (untouched) rax/rcx — drop them and divide.
    e.line("add rsp, 16");
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
}

/// Emits a checked (narrowing only) `as` (SPEC §13, `L9`). Precondition:
/// `rax` holds `src`, sign/zero extended to `from`'s own width at 64 bits.
/// Narrows into `to`'s width, widens the result BACK to `from`'s width
/// (matching `from`'s own signedness, exactly as the value arrived) and
/// compares against the untouched original — any difference means the
/// conversion lost information.
pub(crate) fn emit_checked_cast(
    e: &mut Emitter,
    from: FTy,
    to: FTy,
    msg: &str,
    site: &mut SiteCounter,
) {
    let uid = site.next();
    let ok = format!(".Lchkcast{}", uid);
    let site_label = format!(".Lchksitec{}", uid);
    let from_bits = from.bits();
    let to_bits = to.bits();
    e.line("push rax");
    let narrow_to_then_widen_from = |e: &mut Emitter| {
        if to_bits < 64 {
            if to_bits == 32 {
                if to.signed() {
                    e.line("movsxd rax, eax");
                } else {
                    e.line("mov eax, eax");
                }
            } else {
                let ext = if to.signed() { "movsx" } else { "movzx" };
                e.line(&format!("{} rax, {}", ext, narrow("rax", to_bits)));
            }
        }
        if from_bits > to_bits && from_bits < 64 {
            if from_bits == 32 {
                if from.signed() {
                    e.line("movsxd rax, eax");
                } else {
                    e.line("mov eax, eax");
                }
            } else {
                let ext = if from.signed() { "movsx" } else { "movzx" };
                e.line(&format!("{} rax, {}", ext, narrow("rax", from_bits)));
            }
        }
    };
    narrow_to_then_widen_from(e);
    e.line("cmp qword ptr [rsp], rax");
    e.line(&format!("je {}", ok));
    e.line(&format!("jmp {}", site_label));
    e.raw(&format!("{}:", site_label));
    e.line("pop rdx");
    let label = intern(msg);
    emit_trampoline_jump(e, PANIC_CAST, &label, msg, "rdx", "rdx", !from.signed());
    e.raw(&format!("{}:", ok));
    // Nothing was lost: restore the ORIGINAL value from the stack (the
    // comparison above widened it past `to`'s own width again) and narrow
    // it once more, cleanly, to `to`'s width — the caller's `store_dst`
    // takes it from there exactly as the unchecked `Op::Cast` path does.
    e.line("pop rax");
    if to_bits < 64 {
        if to_bits == 32 {
            if to.signed() {
                e.line("movsxd rax, eax");
            } else {
                e.line("mov eax, eax");
            }
        } else {
            let ext = if to.signed() { "movsx" } else { "movzx" };
            e.line(&format!("{} rax, {}", ext, narrow("rax", to_bits)));
        }
    }
}
