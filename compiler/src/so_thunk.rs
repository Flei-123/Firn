// SPDX-License-Identifier: MPL-2.0
//! Entry thunks of a Linux shared object (`firnc --shared`, r558): C ABI -> Firn ABI for exports that take or return
//! structs by value in a shape where the two differ (`extfn::so_differs`).
//!
//! Firn places a call like this (`codegen_x86::place_args`, `codegen_a64::place_args`): every aggregate is a row of
//! eightbyte WORDS, each word goes to the next register of its class (integer `rdi..r9` / `x0-x7`, float `xmm0-7` /
//! `d0-d7`) or else to the stack, 8 octets per slot, in order of writing. A struct over 16 octets is a pointer to a copy
//! (one integer word). A result over 8 octets comes through a hidden pointer in the first integer register and is handed
//! back in `rax` / `x0`.
//!
//! The C side differs:
//!  * System V x86-64: a struct of up to 16 octets travels in registers only if ALL its words find a register, else all of
//!    it is copied to the stack (the other registers stay free for later arguments); a struct over 16 octets is a copy on
//!    the stack; a result of 9..16 octets comes back in `rax:rdx` / `xmm0:xmm1` (by word class).
//!  * AAPCS64: a homogeneous float aggregate (HFA, 1..=4 members) takes one `s`/`d` register PER MEMBER, any other struct
//!    of up to 16 octets takes integer registers (floats included); if they do not fit, the register file is closed and
//!    the struct is copied to the stack. A struct over 16 octets that is no HFA is a pointer (like Firn). Results: HFA in
//!    `s0..s3` / `d0..d3`, up to 16 octets in `x0:x1`, more through a hidden pointer in `x8`.
//!
//! The thunk parks every incoming register in its own frame, then loads each Firn word from memory (so no source is
//! clobbered by the placement), calls the Firn function under its internal symbol and converts the result.

use crate::abi::Hfa;
use crate::extfn::{AggRet, SoKind};

fn nwords(size: u32) -> usize {
    ((size + 7) / 8) as usize
}

/// All thunks of the program (Linux `.so`); empty when no export needs one.
pub fn runtime_asm() -> Result<String, String> {
    let a64 = matches!(crate::target::active(), crate::target::Target::Aarch64);
    let mut s = String::new();
    for th in crate::extfn::so_thunk_info(a64) {
        let sym = crate::codegen_x86::label(&th.internal);
        let body = if a64 { a64_thunk(&th.c_name, &sym, &th.params, &th.ret) } else { sysv_thunk(&th.c_name, &sym, &th.params, &th.ret) };
        s.push_str(&body.map_err(|e| format!("--shared: export '{}': {}", th.c_name, e))?);
    }
    Ok(s)
}

// ------------------------------------------------------------------------------------------------------------ x86-64

/// where a Firn word comes from, as a displacement from `rbp`
#[derive(Clone, Copy)]
enum XSrc {
    /// 8 octets in memory
    Mem(i32),
    /// 16 octets in memory (a `v128`)
    Q(i32),
    /// the ADDRESS of the memory (a pointer to a copy)
    Addr(i32),
}

fn xm(off: i32) -> String {
    if off < 0 { format!("[rbp-{}]", -off) } else { format!("[rbp+{}]", off) }
}

const X_INT: [&str; 6] = ["rdi", "rsi", "rdx", "rcx", "r8", "r9"];
/// parked integer argument register `i`
fn x_park_int(i: usize) -> i32 {
    -8 * (i as i32 + 1)
}
/// parked `xmm` argument register `k` (16 octets)
fn x_park_sse(k: usize) -> i32 {
    -64 - 16 * k as i32
}
/// 16 octets for a struct result that C returns in registers
const X_BUF: i32 = -192;

/// System V x86-64 entry `c_name` -> Firn function `sym`.
fn sysv_thunk(c_name: &str, sym: &str, params: &[SoKind], ret: &AggRet) -> Result<String, String> {
    let mut items: Vec<(bool, XSrc)> = Vec::new();
    let (mut ni, mut nx, mut st) = (0usize, 0usize, 0i32);
    let stack = |w: i32| 16 + 8 * w;
    let mut any_q = false;
    if let AggRet::Sret { size, .. } = ret {
        if *size > 16 {
            items.push((false, XSrc::Mem(x_park_int(0))));
            ni = 1;
        } else {
            items.push((false, XSrc::Addr(X_BUF)));
        }
    }
    for k in params {
        match k {
            SoKind::Int => {
                if ni < 6 {
                    items.push((false, XSrc::Mem(x_park_int(ni))));
                    ni += 1;
                } else {
                    items.push((false, XSrc::Mem(stack(st))));
                    st += 1;
                }
            }
            SoKind::F32 | SoKind::F64 => {
                if nx < 8 {
                    items.push((true, XSrc::Mem(x_park_sse(nx))));
                    nx += 1;
                } else {
                    items.push((true, XSrc::Mem(stack(st))));
                    st += 1;
                }
            }
            SoKind::V128 => {
                if nx >= 8 {
                    // C: sixteen octets on the stack at a sixteen octet boundary (the area starts 16-aligned at rbp+16)
                    st = (st + 1) / 2 * 2;
                    items.push((true, XSrc::Q(stack(st))));
                    st += 2;
                } else {
                    any_q = true;
                    items.push((true, XSrc::Q(x_park_sse(nx))));
                    nx += 1;
                }
            }
            SoKind::Words { sse, .. } => {
                let need_i = sse.iter().filter(|x| !**x).count();
                let need_x = sse.len() - need_i;
                if ni + need_i <= 6 && nx + need_x <= 8 {
                    for is_sse in sse {
                        if *is_sse {
                            items.push((true, XSrc::Mem(x_park_sse(nx))));
                            nx += 1;
                        } else {
                            items.push((false, XSrc::Mem(x_park_int(ni))));
                            ni += 1;
                        }
                    }
                } else {
                    for (w, is_sse) in sse.iter().enumerate() {
                        items.push((*is_sse, XSrc::Mem(stack(st + w as i32))));
                    }
                    st += sse.len() as i32;
                }
            }
            SoKind::Ref { size, .. } => {
                items.push((false, XSrc::Addr(stack(st))));
                st += nwords(*size) as i32;
            }
        }
    }
    // Firn placement: integers in rdi.., floats in xmm0-7, the rest on the stack in order of writing
    let (mut fi, mut fx) = (0usize, 0usize);
    let mut loads = String::new();
    let mut spill: Vec<XSrc> = Vec::new();
    for (sse, src) in &items {
        if *sse && fx < 8 {
            match src {
                XSrc::Mem(o) => loads.push_str(&format!("    movsd xmm{}, qword ptr {}\n", fx, xm(*o))),
                XSrc::Q(o) => loads.push_str(&format!("    movdqu xmm{}, xmmword ptr {}\n", fx, xm(*o))),
                XSrc::Addr(_) => return Err("internal: address in a float register".to_string()),
            }
            fx += 1;
        } else if !*sse && fi < 6 {
            match src {
                XSrc::Mem(o) => loads.push_str(&format!("    mov {}, qword ptr {}\n", X_INT[fi], xm(*o))),
                XSrc::Addr(o) => loads.push_str(&format!("    lea {}, {}\n", X_INT[fi], xm(*o))),
                XSrc::Q(_) => return Err("internal: vector in an integer register".to_string()),
            }
            fi += 1;
        } else {
            spill.push(*src);
        }
    }
    // Firn's outgoing stack area: eight octets per argument, a `v128` takes sixteen at a sixteen octet boundary
    // (`codegen_x86::stack_offsets`)
    let mut spill_at: Vec<usize> = Vec::new();
    let mut at = 0usize;
    for src in &spill {
        if matches!(src, XSrc::Q(_)) {
            at = (at + 15) / 16 * 16;
            spill_at.push(at);
            at += 16;
        } else {
            spill_at.push(at);
            at += 8;
        }
    }
    let room = (at + 15) / 16 * 16;
    let mut s = String::new();
    s.push_str(&format!("\n.globl {}\n{}:\n", c_name, c_name));
    s.push_str(&format!("    # System V -> Firn for {} (r558)\n", sym));
    s.push_str(&format!("    push rbp\n    mov rbp, rsp\n    sub rsp, {}\n", 192 + room));
    for (i, r) in X_INT.iter().enumerate() {
        s.push_str(&format!("    mov qword ptr {}, {}\n", xm(x_park_int(i)), r));
    }
    for k in 0..8usize {
        if any_q {
            s.push_str(&format!("    movdqu xmmword ptr {}, xmm{}\n", xm(x_park_sse(k)), k));
        } else {
            s.push_str(&format!("    movsd qword ptr {}, xmm{}\n", xm(x_park_sse(k)), k));
        }
    }
    for (j, src) in spill.iter().enumerate() {
        match src {
            XSrc::Mem(o) => s.push_str(&format!("    mov rax, qword ptr {}\n", xm(*o))),
            XSrc::Addr(o) => s.push_str(&format!("    lea rax, {}\n", xm(*o))),
            XSrc::Q(o) => {
                // xmm15 is no argument register
                s.push_str(&format!("    movdqu xmm15, xmmword ptr {}\n", xm(*o)));
                s.push_str(&format!("    movdqu xmmword ptr [rsp+{}], xmm15\n", spill_at[j]));
                continue;
            }
        }
        s.push_str(&format!("    mov qword ptr [rsp+{}], rax\n", spill_at[j]));
    }
    s.push_str(&loads);
    s.push_str(&format!("    call {}\n", sym));
    // A result of 9..16 octets: C wants it in rax:rdx / xmm0:xmm1 by word class. Over 16 octets both sides use the hidden pointer.
    if let AggRet::Sret { size, sse, .. } = ret {
        if *size <= 16 {
            s.push_str(&format!("    lea r10, {}\n", xm(X_BUF)));
            let (mut ri, mut rx) = (0usize, 0usize);
            for (w, is_sse) in sse.iter().enumerate() {
                if *is_sse {
                    s.push_str(&format!("    movsd xmm{}, qword ptr [r10+{}]\n", rx, 8 * w));
                    rx += 1;
                } else {
                    s.push_str(&format!("    mov {}, qword ptr [r10+{}]\n", ["rax", "rdx"][ri], 8 * w));
                    ri += 1;
                }
            }
        }
    }
    s.push_str("    leave\n    ret\n");
    Ok(s)
}

// ---------------------------------------------------------------------------------------------------------- aarch64

/// Where a value lives while the thunk runs.
#[derive(Clone, Copy)]
enum Loc {
    /// in the locals block (displacement from its start)
    Local(i32),
    /// in the caller's outgoing arguments (displacement from `x29`)
    Incoming(i32),
}

impl Loc {
    fn plus(self, d: i32) -> Loc {
        match self {
            Loc::Local(o) => Loc::Local(o + d),
            Loc::Incoming(o) => Loc::Incoming(o + d),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Use {
    /// 8 octets
    Word,
    /// 16 octets (`v128`)
    Vec,
    /// the address of the memory
    Addr,
}

// Locals above the outgoing stack arguments: x0-x7, x8, v0-v7 (16 octets each), a 32 octet result buffer, then 32 octets of
// staging per HFA that arrived in registers (one member per register, Firn wants the packed memory image).
const A_INT: i32 = 0;
const A_X8: i32 = 64;
const A_VEC: i32 = 80;
const A_BUF: i32 = 208;
const A_STAGE: i32 = 240;

/// the C-side placement state (AAPCS64 §6.4)
#[derive(Default)]
struct Cplace {
    ngrn: usize,
    nsrn: usize,
    st: i32,
    /// HFAs to repack: (shape, first v register, staging offset)
    stage: Vec<(Hfa, usize, i32)>,
}

impl Cplace {
    /// The memory image of a struct passed by value: staged from `s`/`d` registers (HFA), consecutive `x` registers (up to
    /// 16 octets) or the caller's stack when it does not fit (the register file is then closed for good).
    fn image(&mut self, size: u32, hfa: Option<Hfa>) -> Loc {
        let nw = nwords(size);
        if let Some(h) = hfa {
            if self.nsrn + h.n as usize <= 8 {
                let off = A_STAGE + 32 * self.stage.len() as i32;
                self.stage.push((h, self.nsrn, off));
                self.nsrn += h.n as usize;
                return Loc::Local(off);
            }
            self.nsrn = 8;
        } else {
            if self.ngrn + nw <= 8 {
                let l = Loc::Local(A_INT + 8 * self.ngrn as i32);
                self.ngrn += nw;
                return l;
            }
            self.ngrn = 8;
        }
        let l = Loc::Incoming(16 + 8 * self.st);
        self.st += nw as i32;
        l
    }
}

/// AAPCS64 entry `c_name` -> Firn function `sym`.
fn a64_thunk(c_name: &str, sym: &str, params: &[SoKind], ret: &AggRet) -> Result<String, String> {
    let mut items: Vec<(bool, Loc, Use)> = Vec::new();
    let mut c = Cplace::default();
    if let AggRet::Sret { size, hfa, .. } = ret {
        if hfa.is_none() && *size > 16 {
            items.push((false, Loc::Local(A_X8), Use::Word));
        } else {
            items.push((false, Loc::Local(A_BUF), Use::Addr));
        }
    }
    for k in params {
        match k {
            SoKind::Int | SoKind::Ref { hfa: None, .. } => {
                if c.ngrn < 8 {
                    items.push((false, Loc::Local(A_INT + 8 * c.ngrn as i32), Use::Word));
                    c.ngrn += 1;
                } else {
                    items.push((false, Loc::Incoming(16 + 8 * c.st), Use::Word));
                    c.st += 1;
                }
            }
            SoKind::F32 | SoKind::F64 => {
                if c.nsrn < 8 {
                    items.push((true, Loc::Local(A_VEC + 16 * c.nsrn as i32), Use::Word));
                    c.nsrn += 1;
                } else {
                    items.push((true, Loc::Incoming(16 + 8 * c.st), Use::Word));
                    c.st += 1;
                }
            }
            SoKind::V128 => {
                if c.nsrn >= 8 {
                    // AAPCS64: sixteen octets on the stack at a sixteen octet boundary (the area starts 16-aligned at x29+16)
                    c.st = (c.st + 1) / 2 * 2;
                    items.push((true, Loc::Incoming(16 + 8 * c.st), Use::Vec));
                    c.st += 2;
                } else {
                    items.push((true, Loc::Local(A_VEC + 16 * c.nsrn as i32), Use::Vec));
                    c.nsrn += 1;
                }
            }
            SoKind::Words { size, sse, hfa } => {
                let img = c.image(*size, *hfa);
                for (w, is_sse) in sse.iter().enumerate() {
                    items.push((*is_sse, img.plus(8 * w as i32), Use::Word));
                }
            }
            SoKind::Ref { size, hfa: Some(h) } => {
                let img = c.image(*size, Some(*h));
                items.push((false, img, Use::Addr));
            }
        }
    }
    // Firn placement: integers x0-x7, floats d0-d7, the rest on the stack in order of writing
    let (mut fi, mut fx) = (0usize, 0usize);
    let mut spill: Vec<(Loc, Use)> = Vec::new();
    // (target register text, source, use)
    let mut loads: Vec<(String, Loc, Use)> = Vec::new();
    for (sse, loc, u) in &items {
        if *sse && fx < 8 {
            let reg = if *u == Use::Vec { format!("q{}", fx) } else { format!("d{}", fx) };
            loads.push((reg, *loc, *u));
            fx += 1;
        } else if !*sse && fi < 8 {
            loads.push((format!("x{}", fi), *loc, *u));
            fi += 1;
        } else {
            spill.push((*loc, *u));
        }
    }
    // Firn's outgoing stack area: eight octets per argument, a `v128` sixteen at a sixteen octet boundary
    let mut spill_at: Vec<i32> = Vec::new();
    let mut at = 0i32;
    for (_, u) in &spill {
        if *u == Use::Vec {
            at = (at + 15) / 16 * 16;
            spill_at.push(at);
            at += 16;
        } else {
            spill_at.push(at);
            at += 8;
        }
    }
    let room = (at + 15) / 16 * 16;
    let total = room + A_STAGE + 32 * c.stage.len() as i32;
    let total = (total + 15) / 16 * 16;
    if total > 4000 {
        return Err("too many struct parameters for one thunk".to_string());
    }
    let at = |l: Loc| -> String {
        match l {
            Loc::Local(o) => format!("[sp, #{}]", room + o),
            Loc::Incoming(o) => format!("[x29, #{}]", o),
        }
    };
    let addr_of = |dst: &str, l: Loc| -> String {
        match l {
            Loc::Local(o) => format!("    add {}, sp, #{}\n", dst, room + o),
            Loc::Incoming(o) => format!("    add {}, x29, #{}\n", dst, o),
        }
    };
    let mut s = String::new();
    s.push_str(&format!("\n.globl {}\n{}:\n", c_name, c_name));
    s.push_str(&format!("    // AAPCS64 -> Firn for {} (r558)\n", sym));
    s.push_str("    stp x29, x30, [sp, #-16]!\n    mov x29, sp\n");
    s.push_str(&format!("    sub sp, sp, #{}\n", total));
    for i in 0..8 {
        s.push_str(&format!("    str x{}, {}\n", i, at(Loc::Local(A_INT + 8 * i))));
    }
    s.push_str(&format!("    str x8, {}\n", at(Loc::Local(A_X8))));
    for k in 0..8 {
        s.push_str(&format!("    str q{}, {}\n", k, at(Loc::Local(A_VEC + 16 * k))));
    }
    // HFAs that arrived one member per register: write them out as the packed struct
    for (h, first, off) in &c.stage {
        for j in 0..h.n as usize {
            let (r, d) = if h.double { ("x9", 8) } else { ("w9", 4) };
            s.push_str(&format!("    ldr {}, {}\n", r, at(Loc::Local(A_VEC + 16 * (*first + j) as i32))));
            s.push_str(&format!("    str {}, {}\n", r, at(Loc::Local(*off + d * j as i32))));
            let _ = d;
        }
    }
    for (j, (loc, u)) in spill.iter().enumerate() {
        if *u == Use::Vec {
            // v16 is no argument register
            s.push_str(&format!("    ldr q16, {}\n", at(*loc)));
            s.push_str(&format!("    str q16, [sp, #{}]\n", spill_at[j]));
            continue;
        }
        if *u == Use::Addr {
            s.push_str(&addr_of("x9", *loc));
        } else {
            s.push_str(&format!("    ldr x9, {}\n", at(*loc)));
        }
        s.push_str(&format!("    str x9, [sp, #{}]\n", spill_at[j]));
    }
    for (reg, loc, u) in &loads {
        if *u == Use::Addr {
            s.push_str(&addr_of(reg, *loc));
        } else {
            s.push_str(&format!("    ldr {}, {}\n", reg, at(*loc)));
        }
    }
    s.push_str(&format!("    bl {}\n", sym));
    let buf = at(Loc::Local(A_BUF));
    let buf_at = |d: i32| format!("[sp, #{}]", room + A_BUF + d);
    let members = |h: &Hfa| -> String {
        let mut m = String::new();
        for j in 0..h.n as i32 {
            m.push_str(&format!("    ldr {}{}, {}\n", if h.double { "d" } else { "s" }, j, buf_at(h.stride() as i32 * j)));
        }
        m
    };
    match ret {
        AggRet::Word { sse: true, hfa, .. } => match hfa {
            Some(h) if h.n == 1 => {}
            Some(h) => {
                s.push_str(&format!("    str d0, {}\n", buf));
                s.push_str(&members(h));
            }
            None => s.push_str("    fmov x0, d0\n"),
        },
        AggRet::Sret { hfa: Some(h), .. } => s.push_str(&members(h)),
        AggRet::Sret { size, .. } if *size <= 16 => {
            if *size > 8 {
                s.push_str(&format!("    ldr x1, {}\n", buf_at(8)));
            }
            s.push_str(&format!("    ldr x0, {}\n", buf));
        }
        _ => {}
    }
    s.push_str("    mov sp, x29\n    ldp x29, x30, [sp], #16\n    ret\n");
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hfa(double: bool, n: u32) -> Option<Hfa> {
        Some(Hfa { double, n })
    }

    /// 5 integers, a 16 octet struct (no registers left for both words), then an integer: C puts the struct on the stack and
    /// the last integer in `r9`; Firn splits the struct (`r9` + stack) and the integer goes to the stack.
    #[test]
    fn sysv_struct_that_does_not_fit_goes_to_the_stack() {
        let mut p = vec![SoKind::Int; 5];
        p.push(SoKind::Words { size: 16, sse: vec![false, false], hfa: None });
        p.push(SoKind::Int);
        let a = sysv_thunk("t", "sym", &p, &AggRet::None).unwrap();
        assert!(a.contains("mov r9, qword ptr [rbp+16]"), "{}", a);
        assert!(a.contains("mov rax, qword ptr [rbp+24]\n    mov qword ptr [rsp+0], rax"), "{}", a);
        assert!(a.contains("mov rax, qword ptr [rbp-48]\n    mov qword ptr [rsp+8], rax"), "{}", a);
    }

    #[test]
    fn sysv_big_struct_is_passed_as_the_address_of_the_stack_copy() {
        let p = vec![SoKind::Ref { size: 24, hfa: None }, SoKind::Int];
        let a = sysv_thunk("t", "sym", &p, &AggRet::None).unwrap();
        assert!(a.contains("lea rdi, [rbp+16]"), "{}", a);
        assert!(a.contains("mov rsi, qword ptr [rbp-8]"), "{}", a);
    }

    #[test]
    fn sysv_result_of_two_words_comes_back_in_registers() {
        let r = AggRet::Sret { size: 16, sse: vec![true, false], hfa: None };
        let a = sysv_thunk("t", "sym", &[SoKind::Int], &r).unwrap();
        assert!(a.contains("lea rdi, [rbp-192]"), "{}", a);
        assert!(a.contains("mov rsi, qword ptr [rbp-8]"), "{}", a);
        assert!(a.contains("movsd xmm0, qword ptr [r10+0]"), "{}", a);
        assert!(a.contains("mov rax, qword ptr [r10+8]"), "{}", a);
        // over 16 octets both sides use the hidden pointer: nothing to convert
        let big = AggRet::Sret { size: 24, sse: vec![], hfa: None };
        let a = sysv_thunk("t", "sym", &[], &big).unwrap();
        assert!(a.contains("mov rdi, qword ptr [rbp-8]") && !a.contains("r10"), "{}", a);
    }

    /// nine `v128` next to a struct that needs the thunk: the ninth comes from the stack (16 aligned) and goes there, too
    #[test]
    fn sysv_vector_beyond_xmm7_is_copied_to_the_stack() {
        let mut p = vec![SoKind::V128; 9];
        p.push(SoKind::Words { size: 16, sse: vec![true, false], hfa: None });
        let a = sysv_thunk("t", "sym", &p, &AggRet::None).unwrap();
        assert!(a.contains("movdqu xmm15, xmmword ptr [rbp+16]
    movdqu xmmword ptr [rsp+0], xmm15"), "{}", a);
    }

    /// an integer word before the ninth `v128` pushes it to the next sixteen octet boundary on both sides
    #[test]
    fn sysv_vector_on_the_stack_keeps_its_alignment() {
        let mut p = vec![SoKind::Int; 6];
        p.push(SoKind::Int);
        p.extend(vec![SoKind::V128; 9]);
        let a = sysv_thunk("t", "sym", &p, &AggRet::None).unwrap();
        // C: the seventh integer sits at rbp+16, the ninth vector at rbp+32; Firn: word at 0, vector at 16
        assert!(a.contains("mov rax, qword ptr [rbp+16]
    mov qword ptr [rsp+0], rax"), "{}", a);
        assert!(a.contains("movdqu xmm15, xmmword ptr [rbp+32]
    movdqu xmmword ptr [rsp+16], xmm15"), "{}", a);
    }

    /// `{f32, f32}`: C wants `s0`, `s1`; Firn takes one packed word in `d0`.
    #[test]
    fn a64_float_pair_is_repacked_into_one_word() {
        let p = vec![SoKind::Words { size: 8, sse: vec![true], hfa: hfa(false, 2) }, SoKind::Int];
        let a = a64_thunk("t", "sym", &p, &AggRet::None).unwrap();
        // staged member by member, then loaded as one word
        assert!(a.contains("ldr w9, [sp, #80]") && a.contains("ldr w9, [sp, #96]"), "{}", a);
        assert!(a.contains("ldr d0, [sp, #240]"), "{}", a);
        assert!(a.contains("ldr x0, [sp, #0]"), "{}", a);
    }

    #[test]
    fn a64_results_follow_the_aapcs64_rules() {
        let f = AggRet::Sret { size: 16, sse: vec![true, true], hfa: hfa(false, 4) };
        let a = a64_thunk("t", "sym", &[], &f).unwrap();
        assert!(a.contains("add x0, sp, #208") && a.contains("ldr s3, [sp, #220]"), "{}", a);
        let m = AggRet::Sret { size: 16, sse: vec![false, true], hfa: None };
        let a = a64_thunk("t", "sym", &[], &m).unwrap();
        assert!(a.contains("ldr x1, [sp, #216]") && a.contains("ldr x0, [sp, #208]"), "{}", a);
        let big = AggRet::Sret { size: 24, sse: vec![], hfa: None };
        let a = a64_thunk("t", "sym", &[], &big).unwrap();
        assert!(a.contains("ldr x0, [sp, #64]"), "x8 is Firn's first argument:\n{}", a);
    }

    /// a struct that finds no `x` registers closes the file: the following integer goes to the stack in C too
    #[test]
    fn a64_struct_that_does_not_fit_closes_the_integer_file() {
        let mut p = vec![SoKind::Int; 7];
        p.push(SoKind::Words { size: 16, sse: vec![false, false], hfa: None });
        p.push(SoKind::Int);
        let a = a64_thunk("t", "sym", &p, &AggRet::None).unwrap();
        assert!(a.contains("ldr x7, [x29, #16]"), "{}", a);
        assert!(a.contains("ldr x9, [x29, #24]\n    str x9, [sp, #0]"), "{}", a);
        assert!(a.contains("ldr x9, [x29, #32]\n    str x9, [sp, #8]"), "{}", a);
    }
}
