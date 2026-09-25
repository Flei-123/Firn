// SPDX-License-Identifier: MPL-2.0
//! **ROUND WASM — the third code generator: FIR -> WebAssembly.**
//!
//! INTERFACE (the one the two machine backends have, with a binary result):
//!   `pub fn emit(m: &fir::Module) -> Result<Output, String>`
//!
//! `--target=wasm32-browser` ends here. The file reads exactly the
//! `fir::Module` the x86 and the aarch64 generators read; the frontend,
//! the type checker, the lowering and the optimizer do not know that it
//! exists. What comes out is a `.wasm` module that a browser (or `node`)
//! instantiates, plus its text form for `--emit=asm`.
//!
//! ## What a Firn value becomes
//!
//! | FIR type | WebAssembly | kept how |
//! |---|---|---|
//! | `i8 i16 i32 u8 u16 u32 bool` | `i32` | ALWAYS normalised: sign extended for the signed types, zero extended for the others, `bool` is 0 or 1 |
//! | `i64 u64 ptr` | `i64` | the full 64 bits |
//! | `f32` / `f64` | `f32` / `f64` | the IEEE value |
//! | `v128` | `v128` | the sixteen octets (round OPT-GENERAL; `FIRN_WASM_NO_SIMD=1` refuses it as before) |
//!
//! A pointer stays SIXTY-FOUR bits wide, in a register and in memory. That
//! is deliberate: every struct layout, every `size_of`, every offset the
//! frontend computed stays exactly what it is on x86-64, and a program that
//! prints a size prints the same number in the browser. Only at the moment
//! of a memory access is the address cut to the 32 bits of `wasm32`
//! (`i32.wrap_i64`). A Firn pointer into the linear memory therefore is its
//! offset in it — nothing else is needed to make `*p` work.
//!
//! The normalisation of the narrow types is the one place this backend
//! differs in METHOD from the two others: they keep garbage in the upper
//! bits of a slot and re-extend at every read (`load_ext`); here every
//! value is kept in its canonical form at all times. Both are the same
//! semantics — a consumer only ever looks at the low `bits()` of a value —
//! and the canonical form is what lets a WebAssembly comparison, division or
//! shift work on the value directly.
//!
//! ## The memory
//!
//! ```text
//!   0      .. 4096          never handed out (a null pointer lands here)
//!   4096   .. D             the data: statics, method tables, function
//!                           records, the collector's type table and state
//!                           block, panic messages, the argument block
//!   D      .. D + 8 MiB     the SHADOW STACK, growing downwards
//!   top    .. memory end    the heap: `mmap` becomes `memory.grow`
//! ```
//!
//! WebAssembly has its own call stack, but it is not addressable: a local
//! of a WebAssembly function has no address. Every `alloca` of FIR (every
//! variable whose address is taken, every struct on the stack) therefore
//! lives in a frame on the shadow stack in the linear memory, addressed
//! through the global `__sp`. A function that needs no such storage has no
//! frame at all.
//!
//! ## The collector without a stack scan — the shadow stack as the root set
//!
//! `lib/gc/gc.fi` finds its roots by scanning the machine stack
//! conservatively. WebAssembly cannot be scanned: its locals are invisible.
//! The answer chosen here keeps the collector UNCHANGED: every value that
//! could be a pointer (`i64`, `u64`, `ptr`) and that is still LIVE across
//! a call which may collect is written into a slot of the caller's shadow
//! frame right before that call. The collector's conservative scan of the
//! shadow stack (from `__gc_sp_below` to the stack top) then sees exactly
//! what the native scan sees in registers and stack slots. The liveness is
//! computed per function; a call that provably cannot reach the collector
//! (the call graph says so) spills nothing, and a program without the
//! collector spills nothing at all. The collector is non-moving, so the
//! value in the local stays valid after the call. `tools/wasm/gc_soak.sh`
//! runs the collector through thousands of collections and shows the
//! counter-check: with the spills switched off (`FIRN_WASM_NO_SPILL=1`) the
//! same program detects freed live objects.
//!
//! The one thing of the collector that cannot work unchanged is where the
//! stack BEGINS: `gc_init` reads it from `/proc/self/maps`. There are no
//! files in a browser. The two functions that read it
//! (`__gc_stack_bottom_maps`, `__gc_stack_bottom`) are therefore given a
//! body of one instruction here — the top of the shadow stack — and their
//! file access is never translated (`OVERRIDES`).
//!
//! ## System calls become imports
//!
//! `syscall(nr, ...)` is a Linux call; the browser has none. What it has is
//! a host: the JavaScript that instantiated the module. `syscalls.rs::wasm`
//! decides per call what it becomes — a call to one of six functions the
//! host provides (`firn.write`, `read`, `exit`, `clock_ns`, `random`,
//! `sleep_ns`), a function of the module itself (`mmap`/`munmap` on
//! `memory.grow`, `futex` with the semantics of a single thread), a
//! constant, or an ERROR AT COMPILE TIME that names the call and the path
//! through which `main` reaches it. Only functions reachable from the entry
//! points are translated at all, so a library function that opens a file is
//! no obstacle as long as nobody calls it.
//!
//! `extern fn` becomes an import from the module `env` under its link name
//! — that is how `lib/plat/web.fi` talks to the page — and `#[export_c]`
//! becomes an export under its bare name.
//!
//! ## Two switches for the tests, and only for them
//!
//!   * `FIRN_WASM_NO_SPILL=1` leaves the spills out. The program is then
//!     WRONG as soon as the collector runs -- that is the point: the soak
//!     test has to fail with it (`tools/wasm/gc_soak.sh`).
//!   * `FIRN_WASM_DISPATCH=1` translates every function through the
//!     dispatch loop that irreducible graphs need. Real programs almost
//!     never need it, so without the switch its correctness would rest on
//!     a handful of functions (`tools/wasm/run.sh` runs a series with it).
//!
//! ## What this generator does NOT do (stated, not hidden)
//!
//!   * threads (`__thread_start`) -- refused at compile time with the
//!     function that uses them.
//!   * the CRYPTO intrinsics (AES, SHA-256, carry-less multiply, crc32):
//!     WebAssembly has no such instructions. `__cpu_features()` answers
//!     SSE2 | SSE4.1 | SSSE3 -- the families this backend translates to
//!     WebAssembly SIMD (round OPT-GENERAL) -- and never AES/SHA/PCLMUL/
//!     SSE4.2, so a program that asks first (the house rule of round 82)
//!     takes its scalar path; one that does not traps with `unreachable`,
//!     as an x86 without the extension raises SIGILL.
//!   * inline assembler, `#[interrupt]`, the `kernel` profile — x86 texts
//!     and bare metal by nature.
//!   * files, sockets, processes, signals — refused at compile time, by
//!     name (`syscalls.rs`).
//!   * debug information.

use crate::fir::{BinOp, BlockId, CmpOp, FTy, Func, Inst, Module, Op, Term, UnOp, Val, WrapSatKind};
use crate::syscalls::{self, Wasm as Sys};
use crate::wasm_cfg::{self, Cfg};
use crate::wasm_enc::{self as w, Ins, VT};
use std::collections::{HashMap, HashSet, VecDeque};

/// What `emit` hands back.
pub struct Output {
    pub binary: Vec<u8>,
    pub text: String,
    /// functions translated, and how many needed the dispatch fallback
    pub funcs: usize,
    pub dispatch_funcs: usize,
}

// ------------------------------------------------------------ constants

/// The first address data may use. Everything below stays zero, so that a
/// write through a null pointer hits nothing that belongs to the program.
const DATA_BASE: u32 = 4096;
/// The shadow stack. Eight MiB is the default stack of a Linux process
/// (`ulimit -s`), so a recursion that fits natively fits here.
const STACK_SIZE: u32 = 8 << 20;
/// WebAssembly page.
const PAGE: u32 = 65536;

/// The functions of the collector runtime that get a body of their own on
/// this target (see the module comment). Nothing else is overridden.
const OVERRIDES: [&str; 2] = ["__gc_stack_bottom_maps", "__gc_stack_bottom"];

/// The imports the host provides, in index order.
const HOST: [(&str, &[VT], &[VT]); 6] = [
    ("write", &[VT::I32, VT::I32, VT::I32], &[VT::I32]),
    ("read", &[VT::I32, VT::I32, VT::I32], &[VT::I32]),
    ("exit", &[VT::I32], &[]),
    ("clock_ns", &[VT::I32], &[VT::I64]),
    ("random", &[VT::I32, VT::I32], &[VT::I32]),
    ("sleep_ns", &[VT::I64], &[VT::I32]),
];
const H_WRITE: u32 = 0;
const H_READ: u32 = 1;
const H_EXIT: u32 = 2;
const H_CLOCK: u32 = 3;
const H_RANDOM: u32 = 4;
const H_SLEEP: u32 = 5;

/// The globals, in index order.
const G_SP: u32 = 0;
const G_HEAP_TOP: u32 = 1;
const G_FREE: u32 = 2;

/// Round OPT-GENERAL: is WebAssembly SIMD switched on? (`FIRN_WASM_NO_SIMD=1`
/// restores the refusal of every `v128`, for an engine without SIMD.)
fn simd_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("FIRN_WASM_NO_SIMD").is_none())
}

/// What `__cpu_features()` answers here: SSE2 | SSE4.1 | SSSE3 (bits 0, 1
/// and 8 of `lib/std/cpu.fi`) -- the families translated to WebAssembly
/// SIMD. Without SIMD: nothing.
fn cpu_features_wasm() -> i64 {
    if simd_on() {
        1 | 2 | 256
    } else {
        0
    }
}

/// Is this intrinsic translated to WebAssembly (`Fx::simd`)? A kind added to
/// `simd.rs` later is refused at compile time, with its name, until it is.
fn wasm_simd_kind(k: crate::simd::SimdKind) -> bool {
    use crate::simd::SimdKind as K;
    matches!(
        k,
        K::Load | K::Store | K::Zero | K::FromU64 | K::GetU64 | K::GetU32 | K::GetU16 | K::SetU32
            | K::Xor | K::And | K::Or | K::AndNot | K::Add8 | K::Add32 | K::Add64 | K::Sub32
            | K::ShuffleB | K::Shuffle32 | K::AlignR | K::UnpackLo32 | K::UnpackHi32
            | K::UnpackLo64 | K::UnpackHi64 | K::ShlBytes | K::ShrBytes | K::Shl32 | K::Shr32
            | K::Shl64 | K::Shr64 | K::Blend16 | K::AesEnc | K::AesEncLast | K::AesDec
            | K::AesDecLast | K::AesImc | K::AesKeyGenAssist | K::Sha256Rnds2 | K::Sha256Msg1
            | K::Sha256Msg2 | K::Pclmul | K::Crc32U8 | K::Crc32U64 | K::CpuFeatures
            | K::Store64 | K::AddF32 | K::SubF32 | K::MulF32 | K::TruncF32I32 | K::CvtI32F32
            | K::CmpLtF32 | K::CmpLeF32 | K::CmpNltF32 | K::CmpGt32
    )
}

/// The class of a FIR type.
fn class(t: FTy) -> Option<VT> {
    match t {
        FTy::I8 | FTy::I16 | FTy::I32 | FTy::U8 | FTy::U16 | FTy::U32 | FTy::Bool => Some(VT::I32),
        FTy::I64 | FTy::U64 | FTy::Ptr => Some(VT::I64),
        FTy::F32 => Some(VT::F32),
        FTy::F64 => Some(VT::F64),
        FTy::V128 => {
            if simd_on() {
                Some(VT::V128)
            } else {
                None
            }
        }
        FTy::Void => None,
    }
}

/// Does a value of this type need re-normalising after a computation that
/// may set bits above its width?
fn narrow(t: FTy) -> bool {
    matches!(t, FTy::I8 | FTy::I16 | FTy::U8 | FTy::U16 | FTy::Bool)
}

fn align_up(x: u64, a: u64) -> u64 {
    if a <= 1 {
        x
    } else {
        (x + a - 1) / a * a
    }
}

// =================================================================== data

/// The data sections the other files of the compiler describe in GNU
/// assembler directives (`gc.rs::ty_table_asm`, `iface.rs::tables_asm`,
/// `fnval.rs::records_asm`, `statics.rs::data_asm`). They are READ here
/// rather than described a second time: the octets of a method table or of
/// the collector's type table are then the same on every target by
/// construction, and none of those files had to learn about WebAssembly.
#[derive(Default)]
struct Image {
    /// 0 = .rodata, 1 = .data, 2 = .bss, 3 = the runtime's own
    secs: [Vec<u8>; 4],
    labels: HashMap<String, (usize, u32)>,
    /// (section, offset, symbol, label of the enclosing function record)
    relocs: Vec<(usize, u32, String, Option<String>)>,
    /// method table label -> the function symbols in it, in order
    tables: HashMap<String, Vec<String>>,
    base: [u32; 4],
}

impl Image {
    fn parse(&mut self, text: &str) -> Result<(), String> {
        let mut sec: Option<usize> = None;
        let mut last_label: Option<String> = None;
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
                continue;
            }
            if line.ends_with(':') && !line.contains(char::is_whitespace) {
                let l = line[..line.len() - 1].to_string();
                let s = sec.ok_or_else(|| format!("wasm32: data label '{}' outside a section", l))?;
                self.labels.insert(l.clone(), (s, self.secs[s].len() as u32));
                last_label = Some(l);
                continue;
            }
            let (dir, rest) = match line.find(char::is_whitespace) {
                Some(i) => (&line[..i], line[i..].trim()),
                None => (line, ""),
            };
            match dir {
                ".text" => sec = None,
                ".section" => {
                    let name = rest.split(',').next().unwrap_or("").trim();
                    sec = match name {
                        ".rodata" => Some(0),
                        ".data" => Some(1),
                        ".bss" => Some(2),
                        ".text" => None,
                        other => return Err(format!("wasm32: unknown data section '{}'", other)),
                    };
                }
                ".align" | ".balign" | ".p2align" => {
                    let n: u64 = rest.parse().map_err(|_| format!("wasm32: bad alignment '{}'", line))?;
                    let a = if dir == ".p2align" { 1u64 << n } else { n };
                    if let Some(s) = sec {
                        let len = self.secs[s].len() as u64;
                        self.secs[s].resize(align_up(len, a) as usize, 0);
                    }
                }
                ".quad" => {
                    let s = sec.ok_or("wasm32: .quad outside a section")?;
                    for item in rest.split(',') {
                        let item = item.trim();
                        let off = self.secs[s].len() as u32;
                        match item.parse::<i64>() {
                            Ok(v) => self.secs[s].extend_from_slice(&v.to_le_bytes()),
                            Err(_) => match item.parse::<u64>() {
                                Ok(v) => self.secs[s].extend_from_slice(&v.to_le_bytes()),
                                Err(_) => {
                                    self.relocs.push((s, off, item.to_string(), last_label.clone()));
                                    if let Some(l) = &last_label {
                                        self.tables.entry(l.clone()).or_default().push(item.to_string());
                                    }
                                    self.secs[s].extend_from_slice(&[0u8; 8]);
                                }
                            },
                        }
                    }
                }
                ".zero" => {
                    let s = sec.ok_or("wasm32: .zero outside a section")?;
                    let n: usize = rest.parse().map_err(|_| format!("wasm32: bad .zero '{}'", line))?;
                    let len = self.secs[s].len();
                    self.secs[s].resize(len + n, 0);
                }
                ".byte" | ".short" => {
                    let s = sec.ok_or("wasm32: data outside a section")?;
                    for item in rest.split(',') {
                        let v: i64 = item.trim().parse().map_err(|_| format!("wasm32: bad number in '{}'", line))?;
                        if dir == ".byte" {
                            self.secs[s].push(v as u8);
                        } else {
                            self.secs[s].extend_from_slice(&(v as u16).to_le_bytes());
                        }
                    }
                }
                ".ascii" => {
                    let s = sec.ok_or("wasm32: .ascii outside a section")?;
                    let body = rest.trim_start_matches('"').trim_end_matches('"');
                    let mut it = body.chars();
                    while let Some(c) = it.next() {
                        if c == '\\' {
                            match it.next() {
                                Some('n') => self.secs[s].push(b'\n'),
                                Some('t') => self.secs[s].push(b'\t'),
                                Some('\\') => self.secs[s].push(b'\\'),
                                Some('"') => self.secs[s].push(b'"'),
                                Some(o) => return Err(format!("wasm32: unknown escape '\\{}'", o)),
                                None => {}
                            }
                        } else {
                            let mut b = [0u8; 4];
                            self.secs[s].extend_from_slice(c.encode_utf8(&mut b).as_bytes());
                        }
                    }
                }
                other => return Err(format!("wasm32: unknown data directive '{}'", other)),
            }
        }
        Ok(())
    }

    /// Appends runtime data to section 3 and returns its offset there.
    fn put(&mut self, bytes: &[u8], align: u64) -> u32 {
        let len = self.secs[3].len() as u64;
        self.secs[3].resize(align_up(len, align) as usize, 0);
        let off = self.secs[3].len() as u32;
        self.secs[3].extend_from_slice(bytes);
        off
    }

    fn addr(&self, label: &str) -> Option<u32> {
        self.labels.get(label).map(|(s, o)| self.base[*s] + o)
    }
}

// ============================================================ the module

/// Everything one function needs to know about the others.
struct Gen<'a> {
    m: &'a Module,
    /// FIR function name -> index into `m.funcs`
    by_name: HashMap<String, usize>,
    /// FIR function name -> WebAssembly function index (emitted ones only)
    fidx: HashMap<String, u32>,
    /// extern call name -> (import index, signature)
    externs: HashMap<String, (u32, w::FuncType)>,
    /// panic message -> address
    msg_addr: HashMap<String, u32>,
    gc_state: u32,
    stack_top: u32,
    stack_limit: u32,
    /// functions that may reach the collector
    may_collect: HashSet<String>,
    gc_active: bool,
    no_spill: bool,
    /// `FIRN_WASM_DISPATCH=1`: every function through the dispatch loop, so
    /// that the fallback for irreducible graphs is tested on real programs
    /// and not only on the few functions that need it
    all_dispatch: bool,
    rt: Rt,
    /// data label -> address (method tables, function records, statics)
    labels: HashMap<String, u32>,
    /// the signatures, shared by every function translated (call_indirect
    /// names its signature by index)
    types: std::sync::Mutex<Vec<w::FuncType>>,
    /// function name -> address of its function record
    records: HashMap<String, u32>,
}

impl<'a> Gen<'a> {
    fn type_index(&self, t: w::FuncType) -> u32 {
        let mut ts = match self.types.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if let Some(p) = ts.iter().position(|x| *x == t) {
            return p as u32;
        }
        ts.push(t);
        (ts.len() - 1) as u32
    }
}

/// The function indices of the runtime of this file.
#[derive(Default, Clone, Copy)]
struct Rt {
    start: u32,
    mmap: u32,
    munmap: u32,
    clock: u32,
    nanosleep: u32,
    futex: u32,
    panic: u32,
    dec: u32,
    overflow: u32,
}

/// Runtime data addresses.
#[derive(Default, Clone, Copy)]
struct RtData {
    argv: u32,
    buf: u32,
    lit_a: u32,
    lit_b: u32,
    lit_idx: u32,
    lit_len: u32,
    lit_close: u32,
    msg_overflow: (u32, u32),
    msg_futex: (u32, u32),
}

/// One reason not to translate a program, with the way to it.
struct Refusal {
    func: String,
    what: String,
}

pub fn emit(m: &Module) -> Result<Output, String> {
    if crate::prof::is_kernel() {
        return Err("--target=wasm32-browser does not support the kernel profile: a browser page is no bare machine".into());
    }
    if !m.funcs.iter().any(|f| f.name == "main") {
        return Err("no entry point: 'fn main() -> i32' is missing".to_string());
    }
    emit_inner(m)
}

fn emit_inner(m: &Module) -> Result<Output, String> {
    // ------------------------------------------------ 1. the data texts
    let mut img = Image::default();
    let gc_active = crate::gc::runtime_active();
    if crate::gc::has_classes() || gc_active {
        img.parse(&crate::gc::ty_table_asm())?;
    }
    if crate::iface::has_interfaces() {
        img.parse(&crate::iface::tables_asm())?;
    }
    if crate::fnval::has_records() {
        img.parse(&crate::fnval::records_asm())?;
    }
    if crate::statics::any_data() {
        img.parse(&crate::statics::data_asm())?;
    }
    let mut by_name: HashMap<String, usize> = HashMap::new();
    let mut sym2name: HashMap<String, String> = HashMap::new();
    for (i, f) in m.funcs.iter().enumerate() {
        by_name.insert(f.name.clone(), i);
        sym2name.insert(crate::codegen_x86::label(&f.name), f.name.clone());
    }

    // ------------------------------------------------ 2. reachability
    let mut roots: Vec<String> = vec!["main".to_string()];
    if gc_active && by_name.contains_key(crate::gc::FN_INIT) {
        roots.push(crate::gc::FN_INIT.to_string());
    }
    if let Some(h) = crate::panic_rt::handler() {
        if by_name.contains_key(&h) {
            roots.push(h);
        }
    }
    let mut exports: Vec<(String, String)> = Vec::new();
    for f in &m.funcs {
        if let Some(x) = crate::extfn::export_link_name(&f.name) {
            exports.push((x, f.name.clone()));
            roots.push(f.name.clone());
        }
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut parent: HashMap<String, String> = HashMap::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    for r in &roots {
        if seen.insert(r.clone()) {
            queue.push_back(r.clone());
        }
    }
    let mut refusals: Vec<Refusal> = Vec::new();
    let mut externs_sig: Vec<(String, w::FuncType)> = Vec::new();
    let mut fnref_targets: Vec<String> = Vec::new();
    let mut direct_targets: Vec<String> = Vec::new();
    let mut msgs: Vec<String> = Vec::new();
    let mut calls: HashMap<String, Vec<String>> = HashMap::new();
    let mut indirect: HashSet<String> = HashSet::new();
    while let Some(name) = queue.pop_front() {
        let f = &m.funcs[by_name[&name]];
        let mut callees: Vec<String> = Vec::new();
        if OVERRIDES.contains(&name.as_str()) {
            calls.insert(name, callees);
            continue;
        }
        if f.interrupt {
            refusals.push(Refusal { func: name.clone(), what: "'#[interrupt]' is a calling convention of bare x86 hardware".into() });
        }
        for p in &f.params {
            if class(*p).is_none() {
                refusals.push(Refusal { func: name.clone(), what: format!("a parameter of type '{}' (SIMD is not supported on wasm32 yet)", p.name()) });
            }
        }
        if f.ret == FTy::V128 && !simd_on() {
            refusals.push(Refusal { func: name.clone(), what: "a result of type 'v128' (SIMD is not supported on wasm32 yet)".into() });
        }
        let consts = single_consts(f);
        let reach = reachable_blocks(f);
        for b in &f.blocks {
            if !reach[b.id as usize] {
                continue;
            }
            for i in &b.insts {
                match &i.op {
                    Op::Call { name: callee, args } => {
                        if by_name.contains_key(callee) {
                            callees.push(callee.clone());
                        } else if crate::extfn::extern_link_name(callee).is_some() {
                            let sig = w::FuncType {
                                params: args.iter().filter_map(|a| class(f.val_ty(*a))).collect(),
                                results: class(i.ty).into_iter().collect(),
                            };
                            match externs_sig.iter().find(|(n, _)| n == callee) {
                                Some((_, s)) if *s != sig => refusals.push(Refusal {
                                    func: name.clone(),
                                    what: format!("the extern function '{}' is called with two different signatures", callee),
                                }),
                                Some(_) => {}
                                None => externs_sig.push((callee.clone(), sig)),
                            }
                        } else {
                            refusals.push(Refusal { func: name.clone(), what: format!("a call of the unknown function '{}'", callee) });
                        }
                    }
                    Op::CallIndirect { .. } => {
                        indirect.insert(name.clone());
                    }
                    Op::FnRef { name: t } => {
                        if by_name.contains_key(t) {
                            callees.push(t.clone());
                            if !fnref_targets.contains(t) {
                                fnref_targets.push(t.clone());
                            }
                        } else {
                            refusals.push(Refusal { func: name.clone(), what: format!("the address of the unknown function '{}'", t) });
                        }
                    }
                    Op::VtabAddr { table } => {
                        let l = crate::iface::table_label(table);
                        for sym in img.tables.get(&l).cloned().unwrap_or_default() {
                            match sym2name.get(&sym) {
                                Some(n) => {
                                    callees.push(n.clone());
                                    if !direct_targets.contains(n) {
                                        direct_targets.push(n.clone());
                                    }
                                }
                                None => refusals.push(Refusal { func: name.clone(), what: format!("a method table naming the unknown symbol '{}'", sym) }),
                            }
                        }
                    }
                    Op::Syscall { args } => {
                        match args.first().and_then(|a| consts.get(a)) {
                            None => refusals.push(Refusal { func: name.clone(), what: "a system call with a computed number (it is translated at compile time, see syscalls.rs)".into() }),
                            Some(nr) => {
                                let nr = *nr as i64;
                                match syscalls::wasm(nr) {
                                    None => refusals.push(Refusal { func: name.clone(), what: format!("system call {} -- it is not in the table of syscalls.rs", nr) }),
                                    Some((nm, Sys::Missing(why))) => refusals.push(Refusal { func: name.clone(), what: format!("system call '{}' ({}) -- {}", nm, nr, why) }),
                                    Some((nm, Sys::Mmap)) => {
                                        // A mapping of a FILE would be the one
                                        // shape of mmap without an equivalent.
                                        let fd = args.get(5).and_then(|a| consts.get(a)).map(|v| *v as i64);
                                        let fl = args.get(4).and_then(|a| consts.get(a)).map(|v| *v as i64);
                                        if let (Some(fd), Some(fl)) = (fd, fl) {
                                            if fd != -1 || fl & 0x20 == 0 {
                                                refusals.push(Refusal { func: name.clone(), what: format!("system call '{}' ({}) of a FILE -- there are no files in the browser, only anonymous memory", nm, nr) });
                                            }
                                        }
                                    }
                                    Some(_) => {}
                                }
                            }
                        }
                    }
                    Op::Simd { kind, .. } => {
                        if *kind != crate::simd::SimdKind::CpuFeatures && (!simd_on() || !wasm_simd_kind(*kind)) {
                            refusals.push(Refusal { func: name.clone(), what: format!("the SIMD instruction '{:?}' (SIMD is not supported on wasm32 yet)", kind) });
                        }
                    }
                    Op::Asm { .. } => refusals.push(Refusal { func: name.clone(), what: "inline assembler (it is x86 text)".into() }),
                    Op::ThreadSpawn { .. } => refusals.push(Refusal { func: name.clone(), what: "a thread start (threads are not supported on wasm32 yet)".into() }),
                    Op::CheckedBin { msg, .. } | Op::CheckedCast { msg, .. } | Op::CheckedIdx { msg, .. } => {
                        if !msgs.contains(msg) {
                            msgs.push(msg.clone());
                        }
                    }
                    Op::CheckedDiv { msg_zero, msg_range, .. } => {
                        for mm in [msg_zero, msg_range] {
                            if !msgs.contains(mm) {
                                msgs.push(mm.clone());
                            }
                        }
                    }
                    _ => {}
                }
                if let Some(d) = i.dst {
                    if f.val_ty(d) == FTy::V128 && !matches!(i.op, Op::Simd { .. }) && !simd_on() {
                        refusals.push(Refusal { func: name.clone(), what: "a 'v128' value (SIMD is not supported on wasm32 yet)".into() });
                    }
                }
            }
        }
        for c in &callees {
            if seen.insert(c.clone()) {
                parent.insert(c.clone(), name.clone());
                queue.push_back(c.clone());
            }
        }
        calls.insert(name, callees);
    }
    if !refusals.is_empty() {
        let mut out = String::new();
        let mut shown: HashSet<(String, String)> = HashSet::new();
        let mut n = 0;
        for r in &refusals {
            if !shown.insert((r.func.clone(), r.what.clone())) {
                continue;
            }
            n += 1;
            if n > 25 {
                continue;
            }
            let mut path = vec![r.func.clone()];
            let mut cur = r.func.clone();
            while let Some(p) = parent.get(&cur) {
                path.push(p.clone());
                cur = p.clone();
            }
            path.reverse();
            if !out.is_empty() {
                out.push_str("\nerror: ");
            }
            out.push_str(&format!(
                "wasm32-browser: '{}' uses {}\n  note: reached through {}",
                r.func,
                r.what,
                path.join(" -> ")
            ));
        }
        if n > 25 {
            out.push_str(&format!("\nnote: {} further reasons not shown", n - 25));
        }
        return Err(out);
    }
    // The emitted functions, in the order of the FIR module (deterministic).
    let emitted: Vec<usize> = (0..m.funcs.len()).filter(|i| seen.contains(&m.funcs[*i].name)).collect();

    // ------------------------------------------------ 3. function indices
    let mut wm = w::Module { names: true, ..Default::default() };
    for (nm, ps, rs) in HOST.iter() {
        let t = wm.type_index(w::FuncType { params: ps.to_vec(), results: rs.to_vec() });
        wm.imports.push(w::Import { module: "firn".into(), name: nm.to_string(), ty: t, sym: format!("firn.{}", nm) });
    }
    let mut externs: HashMap<String, (u32, w::FuncType)> = HashMap::new();
    for (nm, sig) in &externs_sig {
        let link = crate::extfn::extern_link_name(nm).unwrap_or_else(|| nm.clone());
        let t = wm.type_index(sig.clone());
        let idx = wm.imports.len() as u32;
        wm.imports.push(w::Import { module: "env".into(), name: link.clone(), ty: t, sym: format!("env.{}", link) });
        externs.insert(nm.clone(), (idx, sig.clone()));
    }
    let nimp = wm.imports.len() as u32;
    let mut fidx: HashMap<String, u32> = HashMap::new();
    for (k, i) in emitted.iter().enumerate() {
        fidx.insert(m.funcs[*i].name.clone(), nimp + k as u32);
    }
    let mut next = nimp + emitted.len() as u32;
    let mut take = || {
        let v = next;
        next += 1;
        v
    };
    let rt = Rt {
        start: take(),
        mmap: take(),
        munmap: take(),
        clock: take(),
        nanosleep: take(),
        futex: take(),
        panic: take(),
        dec: take(),
        overflow: take(),
    };
    // Function records point at a SHIM for every function that is not a
    // closure body: the call through a record passes the record as one
    // argument more (fnval.rs), and WebAssembly, unlike System V, checks
    // the argument count of an indirect call.
    let mut shims: Vec<(String, u32)> = Vec::new();
    for t in &fnref_targets {
        if !t.starts_with("__closure#") {
            shims.push((t.clone(), take()));
        }
    }

    // ------------------------------------------------ 4. the table
    let mut table: Vec<u32> = Vec::new();
    let mut t_direct: HashMap<String, u32> = HashMap::new();
    let mut t_fnref: HashMap<String, u32> = HashMap::new();
    wm.table_base = 1;
    for n in &direct_targets {
        t_direct.insert(n.clone(), 1 + table.len() as u32);
        table.push(fidx[n]);
    }
    for n in &fnref_targets {
        let fi = match shims.iter().find(|(s, _)| s == n) {
            Some((_, i)) => *i,
            None => fidx[n],
        };
        t_fnref.insert(n.clone(), 1 + table.len() as u32);
        table.push(fi);
    }
    wm.table = table;

    // ------------------------------------------------ 5. the data
    let mut rd = RtData::default();
    let mut argv = Vec::new();
    // [argc][argv0][NULL][envp: NULL][auxv: AT_NULL, 0] "wasm\0" -- the
    // same shape the kernel puts at the top of a native stack, so
    // `rt.arg_count(start)` reads 1 here too.
    for v in [1u64, 0, 0, 0, 0, 0] {
        argv.extend_from_slice(&v.to_le_bytes());
    }
    argv.extend_from_slice(b"wasm\0");
    let argv_off = img.put(&argv, 16);
    let buf_off = img.put(&[0u8; 256], 16);
    let la = img.put(b" (a=", 1);
    let lb = img.put(b" b=", 1);
    let li = img.put(b" (index=", 1);
    let ll = img.put(b" len=", 1);
    let lc = img.put(b")\n", 1);
    let mo_txt: &[u8] = b"firn: stack overflow (the wasm32 shadow stack of 8 MiB is exhausted)\n";
    let mo = img.put(mo_txt, 1);
    let mf_txt: &[u8] = b"firn: futex wait in a program with a single thread would block forever\n";
    let mf = img.put(mf_txt, 1);
    let mut msg_off: Vec<(String, u32)> = Vec::new();
    for s in &msgs {
        msg_off.push((s.clone(), img.put(s.as_bytes(), 1)));
    }
    // Lay the four sections out one behind the other.
    let mut at = DATA_BASE as u64;
    for s in 0..4 {
        at = align_up(at, 16);
        img.base[s] = at as u32;
        at += img.secs[s].len() as u64;
    }
    let data_end = align_up(at, 16) as u32;
    rd.argv = img.base[3] + argv_off;
    rd.buf = img.base[3] + buf_off;
    rd.lit_a = img.base[3] + la;
    rd.lit_b = img.base[3] + lb;
    rd.lit_idx = img.base[3] + li;
    rd.lit_len = img.base[3] + ll;
    rd.lit_close = img.base[3] + lc;
    rd.msg_overflow = (img.base[3] + mo, mo_txt.len() as u32);
    rd.msg_futex = (img.base[3] + mf, mf_txt.len() as u32);
    // argv[0] points at the name behind the block.
    {
        let name_addr = (rd.argv + 48) as u64;
        let o = argv_off as usize + 8;
        img.secs[3][o..o + 8].copy_from_slice(&name_addr.to_le_bytes());
    }
    let msg_addr: HashMap<String, u32> = msg_off.iter().map(|(s, o)| (s.clone(), img.base[3] + o)).collect();
    // Relocations: a data label is its address, a function is its TABLE
    // INDEX -- that is what a code address is in WebAssembly.
    let relocs = std::mem::take(&mut img.relocs);
    for (s, off, sym, rec) in &relocs {
        let v: u64 = if let Some(a) = img.addr(sym) {
            a as u64
        } else if let Some(n) = sym2name.get(sym) {
            let in_record = rec.as_deref().map(|r| r.starts_with(".L__fnv.")).unwrap_or(false);
            let t = if in_record { t_fnref.get(n) } else { t_direct.get(n) };
            // A table of a type nobody takes `as dyn` names functions that
            // were never emitted; its entry stays a null entry.
            t.copied().unwrap_or(0) as u64
        } else {
            return Err(format!("wasm32: the data names the unknown symbol '{}'", sym));
        };
        let o = *off as usize;
        img.secs[*s][o..o + 8].copy_from_slice(&v.to_le_bytes());
    }
    let gc_state = img.addr(crate::gc::STATE_LABEL).unwrap_or(0);
    let stack_limit = data_end;
    let stack_top = data_end + STACK_SIZE;
    let heap_base = align_up(stack_top as u64, PAGE as u64) as u32;
    wm.memory_pages = heap_base / PAGE;
    for s in 0..4 {
        let bytes = &img.secs[s];
        // Trailing zeros need no segment: the memory starts zeroed.
        let used = bytes.iter().rposition(|b| *b != 0).map(|p| p + 1).unwrap_or(0);
        if used > 0 {
            wm.data.push((img.base[s], bytes[..used].to_vec()));
        }
    }
    wm.globals.push(w::Global { vt: VT::I32, mutable: true, init: stack_top as i64, sym: "__sp".into() });
    wm.globals.push(w::Global { vt: VT::I32, mutable: true, init: heap_base as i64, sym: "__heap_top".into() });
    wm.globals.push(w::Global { vt: VT::I32, mutable: true, init: 0, sym: "__free_list".into() });

    // ------------------------------------------------ 6. who may collect
    let mut may_collect: HashSet<String> = HashSet::new();
    if gc_active {
        for r in ["gc_init", "gc_collect", "__gc_alloc_raw", "__gc_collect_now"] {
            if seen.contains(r) {
                may_collect.insert(r.to_string());
            }
        }
        // A function whose body calls through a pointer, or calls the host,
        // is counted as collecting: what it reaches is not known here.
        for (n, _) in calls.iter() {
            if indirect.contains(n) {
                may_collect.insert(n.clone());
            }
        }
        for (n, cs) in calls.iter() {
            let f = &m.funcs[by_name[n]];
            let host = f.blocks.iter().any(|b| b.insts.iter().any(|i| matches!(&i.op, Op::Call { name, .. } if externs.contains_key(name))));
            let _ = cs;
            if host {
                may_collect.insert(n.clone());
            }
        }
        let mut changed = true;
        while changed {
            changed = false;
            for (n, cs) in calls.iter() {
                if !may_collect.contains(n) && cs.iter().any(|c| may_collect.contains(c)) {
                    may_collect.insert(n.clone());
                    changed = true;
                }
            }
        }
    }

    let labels: HashMap<String, u32> = img.labels.keys().filter_map(|l| img.addr(l).map(|a| (l.clone(), a))).collect();
    // The symbol names are asked for HERE, on the thread that owns the
    // compiler's tables: `codegen_x86::label` reads the extern/export
    // registry of `extfn.rs`, which is thread local.
    let syms: Vec<String> = m.funcs.iter().map(|f| crate::codegen_x86::label(&f.name)).collect();
    let handler = crate::panic_rt::handler();
    let mut records: HashMap<String, u32> = HashMap::new();
    for t in &fnref_targets {
        if let Some(a) = img.addr(&crate::fnval::record_label(t)) {
            records.insert(t.clone(), a);
        }
    }
    let handler_ret = handler.as_ref().and_then(|h| by_name.get(h)).and_then(|i| class(m.funcs[*i].ret));
    let g = Gen {
        m,
        by_name,
        fidx,
        externs,
        msg_addr,
        gc_state,
        stack_top,
        stack_limit,
        may_collect,
        gc_active,
        no_spill: std::env::var("FIRN_WASM_NO_SPILL").map(|v| v == "1").unwrap_or(false),
        all_dispatch: std::env::var("FIRN_WASM_DISPATCH").map(|v| v == "1").unwrap_or(false),
        rt,
        labels,
        types: std::sync::Mutex::new(std::mem::take(&mut wm.types)),
        records,
    };

    // ------------------------------------------------ 7. the functions
    //
    // The translation recurses along the dominator tree, and a function of
    // ten thousand blocks is thousands of frames deep. A thread of its own
    // with a large stack keeps that from ever mattering; it only BORROWS
    // what the steps above prepared, and touches no thread local state.
    let translated: Result<(Vec<w::Func>, usize), String> = std::thread::scope(|sc| {
        let job = std::thread::Builder::new().stack_size(1 << 30).spawn_scoped(sc, || {
            let mut out: Vec<w::Func> = Vec::new();
            let mut dispatch = 0usize;
            for i in &emitted {
                let f = &m.funcs[*i];
                let sym = syms[*i].clone();
                let sig = w::FuncType {
                    params: f.params.iter().filter_map(|p| class(*p)).collect(),
                    results: class(f.ret).into_iter().collect(),
                };
                let param_vts = sig.params.clone();
                let ty = g.type_index(sig);
                if OVERRIDES.contains(&f.name.as_str()) {
                    let body = match class(f.ret) {
                        Some(VT::I64) => vec![Ins::I64Const(g.stack_top as i64)],
                        _ => return Err(format!("internal error: the override '{}' has no u64 result", f.name)),
                    };
                    out.push(w::Func { ty, locals: vec![], body, sym });
                    continue;
                }
                let mut fx = Fx::new(&g, f)?;
                if !fx.cfg.reducible || g.all_dispatch {
                    dispatch += 1;
                }
                fx.translate()?;
                let mut body = std::mem::take(&mut fx.out);
                crate::wasm_locals::branches(&mut body);
                let locals = crate::wasm_locals::pack(&param_vts, std::mem::take(&mut fx.locals), &mut body);
                crate::wasm_locals::copies(&mut body);
                out.push(w::Func { ty, locals, body, sym });
            }
            Ok((out, dispatch))
        });
        match job {
            Ok(h) => h.join().unwrap_or_else(|_| Err("internal error: the wasm32 code generator panicked".to_string())),
            Err(e) => Err(format!("wasm32: cannot start the code generator thread: {}", e)),
        }
    });
    let (funcs, dispatch) = translated?;
    wm.funcs = funcs;
    wm.types = match g.types.lock() {
        Ok(mut t) => std::mem::take(&mut *t),
        Err(p) => std::mem::take(&mut *p.into_inner()),
    };

    // ------------------------------------------------ 8. the runtime
    let main_f = &m.funcs[g.by_name["main"]];
    crate::wasm_rt::build(&mut wm, &crate::wasm_rt::Env {
        start: rt.start,
        mmap: rt.mmap,
        munmap: rt.munmap,
        clock: rt.clock,
        nanosleep: rt.nanosleep,
        futex: rt.futex,
        panic: rt.panic,
        dec: rt.dec,
        overflow: rt.overflow,
        h_write: H_WRITE,
        h_exit: H_EXIT,
        h_clock: H_CLOCK,
        h_sleep: H_SLEEP,
        g_heap_top: G_HEAP_TOP,
        g_free: G_FREE,
        heap_base,
        argv: rd.argv,
        buf: rd.buf,
        lit_a: rd.lit_a,
        lit_b: rd.lit_b,
        lit_idx: rd.lit_idx,
        lit_len: rd.lit_len,
        lit_close: rd.lit_close,
        msg_overflow: rd.msg_overflow,
        msg_futex: rd.msg_futex,
        gc_init: if gc_active { g.fidx.get(crate::gc::FN_INIT).copied() } else { None },
        main: g.fidx["main"],
        main_takes_start: !main_f.params.is_empty(),
        main_ret: class(main_f.ret),
        handler: handler.as_ref().and_then(|h| g.fidx.get(h).copied()),
        handler_ret,
    })?;
    for (n, _) in &shims {
        let f = &m.funcs[g.by_name[n]];
        let mut ps: Vec<VT> = f.params.iter().filter_map(|p| class(*p)).collect();
        let np = ps.len();
        ps.push(VT::I64);
        let ty = wm.type_index(w::FuncType { params: ps, results: class(f.ret).into_iter().collect() });
        let mut body = Vec::new();
        for k in 0..np {
            body.push(Ins::LocalGet(k as u32));
        }
        body.push(Ins::Call(g.fidx[n]));
        let sym = format!("{}$record", syms[g.by_name[n]]);
        wm.funcs.push(w::Func { ty, locals: vec![], body, sym });
    }

    // ------------------------------------------------ 9. exports
    wm.exports.push(w::Export { name: "memory".into(), kind: w::ExportKind::Memory, index: 0 });
    wm.exports.push(w::Export { name: "_start".into(), kind: w::ExportKind::Func, index: rt.start });
    for (x, n) in &exports {
        wm.exports.push(w::Export { name: x.clone(), kind: w::ExportKind::Func, index: g.fidx[n] });
    }
    let binary = wm.to_binary();
    let text = wm.to_text();
    Ok(Output { binary, text, funcs: emitted.len(), dispatch_funcs: dispatch })
}

/// Values that are defined exactly once, by a constant. After `phi.rs` a
/// value can be written from several blocks; only a single definition is
/// the constant its `const` names (the same rule `codegen_a64::layout`
/// follows for the system call number).
fn single_consts(f: &Func) -> HashMap<Val, i128> {
    let mut defs: HashMap<Val, u32> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let Some(d) = i.dst {
                *defs.entry(d).or_insert(0) += 1;
            }
        }
    }
    let mut out = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Const(c)) = (i.dst, &i.op) {
                if defs.get(&d).copied().unwrap_or(0) == 1 {
                    out.insert(d, i.ty.truncate(*c));
                }
            }
        }
    }
    out
}

/// Round OPT-GENERAL -- which values become expression trees.
///
/// A value used exactly once, in the same block, by an instruction whose
/// translation reads each operand once, is computed right at that use: its
/// instruction is emitted where the use reads it, and the result stays on
/// the operand stack. That removes a `local.set` and a `local.get` per value
/// and, more important for a one-pass engine, the local itself.
///
/// What may move: pure computations that cannot trap, and loads. A pure
/// computation may move past anything except what writes a local (a phi
/// copy) or calls (a call may run the collector, and a pointer the tree
/// still has to read must be in the shadow frame then). A load may only move
/// past other pure computations and loads.
///
/// Constants and data addresses defined once are simply computed again at
/// every use.
fn plan_trees(f: &Func, cfg: &Cfg) -> (HashMap<Val, (u32, usize)>, HashMap<Val, (u32, usize)>) {
    let mut inline: HashMap<Val, (u32, usize)> = HashMap::new();
    let mut remat: HashMap<Val, (u32, usize)> = HashMap::new();
    if std::env::var_os("FIRN_WASM_NO_TREES").is_some() {
        return (inline, remat);
    }
    let n = f.val_types.len();
    let mut ndef = vec![0u32; n];
    let mut nuse = vec![0u32; n];
    let mut use_at = vec![(u32::MAX, usize::MAX); n];
    let mut ops: Vec<Val> = Vec::new();
    for &b in &cfg.order {
        let blk = &f.blocks[b as usize];
        for (k, i) in blk.insts.iter().enumerate() {
            if let Some(d) = i.dst {
                if (d as usize) < n {
                    ndef[d as usize] += 1;
                }
            }
            ops.clear();
            i.op.uses(&mut ops);
            for &v in &ops {
                if (v as usize) < n {
                    nuse[v as usize] += 1;
                    use_at[v as usize] = (b, k);
                }
            }
        }
        let tv = match &blk.term {
            Term::BrCond { cond, .. } => Some(*cond),
            Term::Switch { val, .. } => Some(*val),
            Term::Ret(Some(v)) => Some(*v),
            _ => None,
        };
        if let Some(v) = tv {
            if (v as usize) < n {
                nuse[v as usize] += 1;
                use_at[v as usize] = (b, blk.insts.len());
            }
        }
    }
    for &b in &cfg.order {
        for (k, i) in f.blocks[b as usize].insts.iter().enumerate() {
            if let Some(d) = i.dst {
                if (d as usize) < n
                    && std::env::var_os("FIRN_WASM_NO_REMAT").is_none()
                    && ndef[d as usize] == 1
                    && class(i.ty).is_some()
                    && !f.is_secret(d)
                    && matches!(i.op, Op::Const(_) | Op::GlobalAddr { .. } | Op::FnRef { .. } | Op::VtabAddr { .. })
                {
                    remat.insert(d, (b, k));
                }
            }
        }
    }
    for &b in &cfg.order {
        let blk = &f.blocks[b as usize];
        let nb = blk.insts.len();
        let mut fpos: HashMap<usize, usize> = HashMap::new();
        for k in (0..nb).rev() {
            let i = &blk.insts[k];
            let d = match i.dst {
                Some(d) => d,
                None => continue,
            };
            let du = d as usize;
            if du >= n || remat.contains_key(&d) || ndef[du] != 1 || nuse[du] != 1 || f.is_secret(d) {
                continue;
            }
            let load = match &i.op {
                Op::Load { .. } if std::env::var_os("FIRN_WASM_TREE_NOLOAD").is_none() => true,
                op if tree_op(op, i.ty) => false,
                _ => continue,
            };
            let (ub, up) = use_at[du];
            if ub != b || up <= k || up == usize::MAX {
                continue;
            }
            let consumer = if up == nb {
                matches!(blk.term, Term::BrCond { .. } | Term::Ret(_) | Term::Switch { .. })
            } else {
                tree_consumer(&blk.insts[up].op)
            };
            if !consumer {
                continue;
            }
            let end = *fpos.get(&up).unwrap_or(&up);
            let mut blocked = false;
            let mut reads: Vec<Val> = Vec::new();
            i.op.uses(&mut reads);
            for j in (k + 1)..end.min(nb) {
                if fpos.contains_key(&j) {
                    continue;
                }
                let bj = &blk.insts[j];
                // Anything that writes a value the tree reads blocks it
                // (its own operands; deeper operands are checked by their
                // own trees over the same stretch). After phi elimination
                // that is not only a copy: `phi.rs` coalesces, so an
                // ordinary instruction may write the local of a phi whose
                // last use in FIR order came before it -- mandel.fi: the
                // new `zi` is computed straight into the phi of `zi` while
                // `zr*zr - zi*zi` still has to read the old one.
                if let Some(dd) = bj.dst {
                    if reads.contains(&dd) {
                        blocked = true;
                        break;
                    }
                }
                if let Op::Copy { .. } = &bj.op {
                    continue;
                }
                if tree_barrier(bj, load) {
                    blocked = true;
                    break;
                }
            }
            if blocked {
                continue;
            }
            if std::env::var_os("FIRN_WASM_TREE_TRACE").is_some() {
                eprintln!("tree: @{} bb{} %{} (#{}) -> #{} end #{}", f.name, b, d, k, up, end);
            }
            fpos.insert(k, end);
            inline.insert(d, (b, k));
        }
    }
    (inline, remat)
}

/// Round OPT-GENERAL -- the 64-bit values of which only the low 32 bits are
/// ever needed. Pointers stay 64 bits wide in memory and at every call (see
/// the module comment), but an address that is only computed to be used --
/// `base + i * 4`, cut to 32 bits by the load -- can be computed in 32 bits
/// from the start: the low 32 bits of a sum, difference, product, bit
/// operation or constant left shift depend only on the low 32 bits of the
/// operands. Greatest fixed point: start with every candidate and remove
/// each value one of whose uses needs more.
fn plan_low32(f: &Func, cfg: &Cfg) -> Vec<bool> {
    let n = f.val_types.len();
    let mut low = vec![false; n];
    if std::env::var_os("FIRN_WASM_NO_LOW32").is_some() {
        return low;
    }
    let np = f.params.len();
    let consts = single_consts(f);
    for b in &cfg.order {
        for i in &f.blocks[*b as usize].insts {
            if let Some(d) = i.dst {
                let du = d as usize;
                if du >= np && du < n && class(f.val_types[du]) == Some(VT::I64) && !f.is_secret(d) {
                    low[du] = true;
                }
            }
        }
    }
    let mut ops: Vec<Val> = Vec::new();
    loop {
        let mut changed = false;
        for b in &cfg.order {
            let blk = &f.blocks[*b as usize];
            for i in &blk.insts {
                ops.clear();
                i.op.uses(&mut ops);
                let dlow = i.dst.map(|d| (d as usize) < n && low[d as usize]).unwrap_or(false);
                for &u in &ops {
                    let uu = u as usize;
                    if uu >= n || !low[uu] {
                        continue;
                    }
                    let ok = match &i.op {
                        Op::Load { .. } | Op::MmioLoad { .. } | Op::CopyMem { .. } => true,
                        Op::Store { addr, val } | Op::MmioStore { addr, val } => *addr == u && *val != u,
                        Op::AtomicAdd { addr, val } => *addr == u && *val != u,
                        Op::AtomicCas { addr, erw, new } => *addr == u && *erw != u && *new != u,
                        Op::SecureZero { .. } => true,
                        Op::CallIndirect { target, args } => *target == u && !args.contains(&u),
                        Op::Bin(BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::And | BinOp::Or | BinOp::Xor, _, _) => dlow,
                        Op::BinWrapSat { kind: WrapSatKind::Wrap, op: BinOp::Add | BinOp::Sub | BinOp::Mul, .. } => dlow,
                        Op::Bin(BinOp::Shl, a, c) => {
                            dlow && *a == u && *c != u && consts.get(c).map(|k| (0..32).contains(k)).unwrap_or(false)
                        }
                        Op::PtrAdd { .. } => dlow,
                        Op::Copy { .. } => dlow,
                        Op::Cast { .. } => {
                            let to = i.ty;
                            if to.is_float() || to == FTy::Bool {
                                false
                            } else if class(to) == Some(VT::I32) {
                                true
                            } else {
                                dlow
                            }
                        }
                        _ => false,
                    };
                    if !ok {
                        low[uu] = false;
                        changed = true;
                    }
                }
            }
            let tv = match &blk.term {
                Term::BrCond { cond, .. } => Some(*cond),
                Term::Switch { val, .. } => Some(*val),
                Term::Ret(Some(v)) => Some(*v),
                _ => None,
            };
            if let Some(v) = tv {
                if (v as usize) < n && low[v as usize] {
                    low[v as usize] = false;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    low
}

/// A pure computation that cannot trap and reads each operand once.
fn tree_op(op: &Op, ty: FTy) -> bool {
    if class(ty).is_none() {
        return false;
    }
    match op {
        Op::Bin(o, _, _) => !matches!(o, BinOp::Div | BinOp::Rem),
        Op::BinWrapSat { kind: WrapSatKind::Wrap, op: o, .. } => !matches!(o, BinOp::Div | BinOp::Rem),
        Op::Cmp { .. } | Op::Un(..) | Op::Cast { .. } | Op::PtrAdd { .. } | Op::Alloca { .. } => true,
        _ => false,
    }
}

/// An instruction whose translation reads each operand exactly once.
fn tree_consumer(op: &Op) -> bool {
    matches!(
        op,
        Op::Load { .. }
            | Op::Store { .. }
            | Op::Bin(..)
            | Op::Cmp { .. }
            | Op::Un(..)
            | Op::Cast { .. }
            | Op::PtrAdd { .. }
            | Op::Copy { .. }
            | Op::Call { .. }
            | Op::Select { .. }
    ) || matches!(op, Op::BinWrapSat { kind: WrapSatKind::Wrap, .. })
}

/// May a tree (a load if `load`) move past this instruction?
fn tree_barrier(i: &Inst, load: bool) -> bool {
    match &i.op {
        Op::Copy { .. }
        | Op::Call { .. }
        | Op::CallIndirect { .. }
        | Op::Syscall { .. }
        | Op::Asm { .. }
        | Op::ThreadSpawn { .. }
        | Op::GcAddr { .. }
        | Op::Barrier { .. } => true,
        _ if !load => false,
        Op::Load { .. } | Op::Const(_) | Op::GlobalAddr { .. } | Op::FnRef { .. } | Op::VtabAddr { .. } => false,
        op => !tree_op(op, i.ty),
    }
}

fn reachable_blocks(f: &Func) -> Vec<bool> {
    let n = f.blocks.len();
    let mut r = vec![false; n];
    let mut stack = vec![0usize];
    if n == 0 {
        return r;
    }
    r[0] = true;
    while let Some(b) = stack.pop() {
        for s in f.blocks[b].term.successors() {
            let s = s as usize;
            if s < n && !r[s] {
                r[s] = true;
                stack.push(s);
            }
        }
    }
    r
}

// =============================================================== function

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ctx {
    If,
    Loop(BlockId),
    Block(BlockId),
    /// the loop around the dispatch fallback
    Dispatch,
}

struct Fx<'a> {
    g: &'a Gen<'a>,
    f: &'a Func,
    cfg: Cfg,
    out: Vec<Ins>,
    /// local index per value; `u32::MAX` = the value has none
    loc: Vec<u32>,
    locals: Vec<VT>,
    nparams: u32,
    /// frame pointer local (`u32::MAX` = no frame)
    fp: u32,
    frame: u32,
    alloca_off: HashMap<Val, u32>,
    spill_off: HashMap<Val, u32>,
    /// (block, instruction index) -> values to spill before that call
    spills: HashMap<(u32, usize), Vec<Val>>,
    consts: HashMap<Val, i128>,
    ctx: Vec<Ctx>,
    free_tmp: Vec<(VT, u32)>,
    /// dispatch fallback: the block number local and the block positions
    dispatch_local: u32,
    dispatch_pos: HashMap<BlockId, u32>,
    /// Round OPT-GENERAL: values computed right where their one use is
    /// (an expression tree on the operand stack instead of a local),
    /// by (block, instruction index) of the definition
    inline_def: HashMap<Val, (u32, usize)>,
    /// constants and data addresses: computed again at every use, never
    /// kept in a local
    remat: HashMap<Val, (u32, usize)>,
    /// the values whose definition is being emitted inline right now
    emitting: Vec<Val>,
    /// an error raised inside `get` (which cannot return one)
    err: Option<String>,
    /// Round OPT-GENERAL: 64-bit values of which every use needs only the
    /// low 32 bits (addresses, mostly) -- kept and computed as `i32`
    low32: Vec<bool>,
}

impl<'a> Fx<'a> {
    fn new(g: &'a Gen<'a>, f: &'a Func) -> Result<Fx<'a>, String> {
        let cfg = wasm_cfg::analyse(f);
        let n = f.val_types.len();
        let nparams = f.params.len() as u32;
        let mut loc = vec![u32::MAX; n];
        for (i, _) in f.params.iter().enumerate() {
            loc[i] = i as u32;
        }
        // Every value that appears in a reachable block gets a local,
        // grouped by type so the local declaration stays four runs long.
        let mut used = vec![false; n];
        let mut ops: Vec<Val> = Vec::new();
        for &b in &cfg.order {
            let blk = &f.blocks[b as usize];
            for i in &blk.insts {
                if let Some(d) = i.dst {
                    used[d as usize] = true;
                }
                ops.clear();
                i.op.uses(&mut ops);
                for v in &ops {
                    if (*v as usize) < n {
                        used[*v as usize] = true;
                    }
                }
            }
            match &blk.term {
                Term::BrCond { cond, .. } => used[*cond as usize] = true,
                Term::Switch { val, .. } => used[*val as usize] = true,
                Term::Ret(Some(v)) => used[*v as usize] = true,
                _ => {}
            }
        }
        let (inline_def, remat) = plan_trees(f, &cfg);
        let low32 = plan_low32(f, &cfg);
        let cls = |v: usize| -> Option<VT> {
            if low32[v] {
                Some(VT::I32)
            } else {
                class(f.val_types[v])
            }
        };
        let mut locals: Vec<VT> = Vec::new();
        for want in [VT::I32, VT::I64, VT::F32, VT::F64, VT::V128] {
            for v in nparams as usize..n {
                let vv = v as Val;
                if inline_def.contains_key(&vv) || remat.contains_key(&vv) {
                    continue;
                }
                if used[v] && cls(v) == Some(want) {
                    loc[v] = nparams + locals.len() as u32;
                    locals.push(want);
                }
            }
        }
        let consts = single_consts(f);
        let mut fx = Fx {
            g,
            f,
            cfg,
            out: Vec::new(),
            loc,
            locals,
            nparams,
            fp: u32::MAX,
            frame: 0,
            alloca_off: HashMap::new(),
            spill_off: HashMap::new(),
            spills: HashMap::new(),
            consts,
            ctx: Vec::new(),
            free_tmp: Vec::new(),
            dispatch_local: u32::MAX,
            dispatch_pos: HashMap::new(),
            inline_def,
            remat,
            emitting: Vec::new(),
            err: None,
            low32,
        };
        fx.plan_frame();
        Ok(fx)
    }

    // ------------------------------------------------------------ frame

    /// Allocas first, spill slots after them; the frame is a multiple of
    /// sixteen, so every alloca keeps its alignment (up to sixteen) as long
    /// as `__sp` does.
    fn plan_frame(&mut self) {
        let f = self.f;
        let mut cur: u64 = 0;
        for &b in &self.cfg.order {
            for i in &f.blocks[b as usize].insts {
                if let (Some(d), Op::Alloca { size, align }) = (i.dst, &i.op) {
                    let a = if *align == 0 { 1 } else { (*align).min(16) };
                    cur = align_up(cur, a);
                    self.alloca_off.insert(d, cur as u32);
                    cur += (*size).max(1);
                }
            }
        }
        if self.g.gc_active && !self.g.no_spill {
            self.plan_spills();
        }
        let mut spilled: Vec<Val> = self.spills.values().flatten().copied().collect();
        spilled.sort_unstable();
        spilled.dedup();
        cur = align_up(cur, 8);
        for v in spilled {
            self.spill_off.insert(v, cur as u32);
            cur += 8;
        }
        self.frame = align_up(cur, 16) as u32;
        if self.frame > 0 {
            self.fp = self.new_local(VT::I32);
        }
    }

    /// Does this instruction call something that may run the collector?
    fn collects(&self, i: &Inst) -> bool {
        match &i.op {
            Op::Call { name, .. } => self.g.may_collect.contains(name) || self.g.externs.contains_key(name),
            Op::CallIndirect { .. } => true,
            _ => false,
        }
    }

    /// The liveness of the pointer sized values, and from it the set of
    /// values each collecting call has to leave in the shadow frame.
    fn plan_spills(&mut self) {
        let f = self.f;
        let n = f.val_types.len();
        // Candidates: the values that can hold an address.
        let mut idx = vec![usize::MAX; n];
        let mut cands: Vec<Val> = Vec::new();
        for v in 0..n {
            if class(f.val_types[v]) == Some(VT::I64) && self.loc[v] != u32::MAX {
                idx[v] = cands.len();
                cands.push(v as Val);
            }
        }
        if cands.is_empty() {
            return;
        }
        let any_call = self.cfg.order.iter().any(|b| f.blocks[*b as usize].insts.iter().any(|i| self.collects(i)));
        if !any_call {
            return;
        }
        let words = cands.len().div_ceil(64);
        let nb = f.blocks.len();
        let mut live_in: Vec<Vec<u64>> = vec![vec![0u64; words]; nb];
        let mut live_out: Vec<Vec<u64>> = vec![vec![0u64; words]; nb];
        let set = |s: &mut Vec<u64>, v: Val| {
            let k = idx[v as usize];
            if k != usize::MAX {
                s[k / 64] |= 1u64 << (k % 64);
            }
        };
        let clr = |s: &mut Vec<u64>, v: Val| {
            let k = idx[v as usize];
            if k != usize::MAX {
                s[k / 64] &= !(1u64 << (k % 64));
            }
        };
        let term_uses = |t: &Term| -> Vec<Val> {
            match t {
                Term::BrCond { cond, .. } => vec![*cond],
                Term::Switch { val, .. } => vec![*val],
                Term::Ret(Some(v)) => vec![*v],
                _ => vec![],
            }
        };
        let mut ops: Vec<Val> = Vec::new();
        let mut changed = true;
        while changed {
            changed = false;
            for &b in self.cfg.order.iter().rev() {
                let bu = b as usize;
                let blk = &f.blocks[bu];
                let mut out = vec![0u64; words];
                for s in blk.term.successors() {
                    let su = s as usize;
                    if su < nb {
                        for k in 0..words {
                            out[k] |= live_in[su][k];
                        }
                    }
                }
                let mut live = out.clone();
                for v in term_uses(&blk.term) {
                    set(&mut live, v);
                }
                for i in blk.insts.iter().rev() {
                    if let Some(d) = i.dst {
                        clr(&mut live, d);
                    }
                    ops.clear();
                    i.op.uses(&mut ops);
                    for v in &ops {
                        set(&mut live, *v);
                    }
                }
                if out != live_out[bu] {
                    live_out[bu] = out;
                    changed = true;
                }
                if live != live_in[bu] {
                    live_in[bu] = live;
                    changed = true;
                }
            }
        }
        for &b in &self.cfg.order {
            let bu = b as usize;
            let blk = &f.blocks[bu];
            let mut live = live_out[bu].clone();
            for v in term_uses(&blk.term) {
                set(&mut live, v);
            }
            for (k, i) in blk.insts.iter().enumerate().rev() {
                if self.collects(i) {
                    let mut after = live.clone();
                    if let Some(d) = i.dst {
                        clr(&mut after, d);
                    }
                    let mut vs: Vec<Val> = Vec::new();
                    for (w, word) in after.iter().enumerate() {
                        let mut x = *word;
                        while x != 0 {
                            let bit = x.trailing_zeros() as usize;
                            x &= x - 1;
                            vs.push(cands[w * 64 + bit]);
                        }
                    }
                    if !vs.is_empty() {
                        self.spills.insert((b, k), vs);
                    }
                }
                if let Some(d) = i.dst {
                    clr(&mut live, d);
                }
                ops.clear();
                i.op.uses(&mut ops);
                for v in &ops {
                    set(&mut live, *v);
                }
            }
        }
    }

    // ------------------------------------------------------------ helpers

    fn new_local(&mut self, vt: VT) -> u32 {
        self.locals.push(vt);
        self.nparams + self.locals.len() as u32 - 1
    }

    fn tmp(&mut self, vt: VT) -> u32 {
        if let Some(p) = self.free_tmp.iter().position(|(t, _)| *t == vt) {
            return self.free_tmp.remove(p).1;
        }
        self.new_local(vt)
    }

    fn release(&mut self, vt: VT, l: u32) {
        self.free_tmp.push((vt, l));
    }

    fn ins(&mut self, i: Ins) {
        self.out.push(i);
    }

    fn num(&mut self, n: w::Num) {
        self.out.push(Ins::Num(n));
    }

    fn vt_of(&self, v: Val) -> Option<VT> {
        if self.low32.get(v as usize).copied().unwrap_or(false) {
            return Some(VT::I32);
        }
        class(self.f.val_ty(v))
    }

    /// Is the result of this instruction kept as a 32-bit value?
    fn dst_low(&self, i: &Inst) -> bool {
        i.dst.map(|d| self.low32.get(d as usize).copied().unwrap_or(false)).unwrap_or(false)
    }

    /// Converts the value on the stack from `have` to `want`. Between the
    /// integer classes this is a numeric widening or narrowing; between an
    /// integer and a float class it is a REINTERPRETATION of the bits --
    /// exactly what the native backends do when FIR stores an `f64` word
    /// through an `i64` store (`lower.rs::store_words`): the 64 bits of the
    /// slot travel unchanged.
    fn coerce(&mut self, have: VT, want: VT, signed: bool) {
        use VT::*;
        match (have, want) {
            (a, b) if a == b => {}
            (I32, I64) => self.num(if signed { w::I64_EXTEND_I32_S } else { w::I64_EXTEND_I32_U }),
            (I64, I32) => self.num(w::I32_WRAP_I64),
            (F32, I32) => self.num(w::I32_REINTERPRET_F32),
            (I32, F32) => self.num(w::F32_REINTERPRET_I32),
            (F64, I64) => self.num(w::I64_REINTERPRET_F64),
            (I64, F64) => self.num(w::F64_REINTERPRET_I64),
            (F32, I64) => {
                self.num(w::I32_REINTERPRET_F32);
                self.num(w::I64_EXTEND_I32_U);
            }
            (I64, F32) => {
                self.num(w::I32_WRAP_I64);
                self.num(w::F32_REINTERPRET_I32);
            }
            (F64, I32) => {
                self.num(w::I64_REINTERPRET_F64);
                self.num(w::I32_WRAP_I64);
            }
            (I32, F64) => {
                self.num(w::I64_EXTEND_I32_U);
                self.num(w::F64_REINTERPRET_I64);
            }
            (F32, F64) => {
                self.num(w::I32_REINTERPRET_F32);
                self.num(w::I64_EXTEND_I32_U);
                self.num(w::F64_REINTERPRET_I64);
            }
            (F64, F32) => {
                self.num(w::I64_REINTERPRET_F64);
                self.num(w::I32_WRAP_I64);
                self.num(w::F32_REINTERPRET_I32);
            }
            _ => {}
        }
    }

    /// The `i32` on the stack into the canonical form of `t`.
    fn normalize(&mut self, t: FTy) {
        match t {
            FTy::I8 => self.num(w::I32_EXTEND8_S),
            FTy::I16 => self.num(w::I32_EXTEND16_S),
            FTy::U8 => {
                self.ins(Ins::I32Const(0xFF));
                self.num(w::I32_AND);
            }
            FTy::U16 => {
                self.ins(Ins::I32Const(0xFFFF));
                self.num(w::I32_AND);
            }
            FTy::Bool => {
                self.ins(Ins::I32Const(1));
                self.num(w::I32_AND);
            }
            _ => {}
        }
    }

    /// Pushes a value, converted to `want`.
    fn get(&mut self, v: Val, want: VT) {
        if self.remat.contains_key(&v) {
            if let Some(&c) = self.consts.get(&v) {
                let t = self.f.val_ty(v);
                if !t.is_float() && matches!(class(t), Some(VT::I32) | Some(VT::I64)) {
                    // the constant as the class wants it: the low bits for
                    // i32, extended by the signedness of its type for i64
                    // (exactly what `coerce` would do to the pushed value)
                    let tc = t.truncate(c);
                    match want {
                        VT::I32 => {
                            self.ins(Ins::I32Const(tc as i64 as i32));
                            return;
                        }
                        VT::I64 => {
                            let x: i64 = if class(t) == Some(VT::I32) && !t.signed() {
                                (tc as i64) & 0xFFFF_FFFF
                            } else {
                                tc as i64
                            };
                            self.ins(Ins::I64Const(x));
                            return;
                        }
                        _ => {}
                    }
                }
            }
        }
        if let Some(&(b, k)) = self.inline_def.get(&v).or_else(|| self.remat.get(&v)) {
            let f = self.f;
            let inst = &f.blocks[b as usize].insts[k];
            self.emitting.push(v);
            if let Err(e) = self.inst(inst) {
                if self.err.is_none() {
                    self.err = Some(e);
                }
            }
            self.emitting.pop();
            if let Some(have) = self.vt_of(v) {
                let signed = self.f.val_ty(v).signed();
                self.coerce(have, want, signed);
            }
            return;
        }
        let l = self.loc.get(v as usize).copied().unwrap_or(u32::MAX);
        match self.vt_of(v) {
            Some(have) if l != u32::MAX => {
                self.ins(Ins::LocalGet(l));
                let signed = self.f.val_ty(v).signed();
                self.coerce(have, want, signed);
            }
            // A value without a local (never defined on a reachable path):
            // its content is undefined, zero is as good as any.
            _ => self.zero(want),
        }
    }

    fn zero(&mut self, vt: VT) {
        match vt {
            VT::I32 => self.ins(Ins::I32Const(0)),
            VT::I64 => self.ins(Ins::I64Const(0)),
            VT::F32 => self.ins(Ins::F32Const(0)),
            VT::F64 => self.ins(Ins::F64Const(0)),
            VT::V128 => self.ins(Ins::V128Const([0; 16])),
        }
    }

    /// Pushes a value in the canonical form of the type `t` -- for the
    /// instructions whose result depends on the bits above the width
    /// (division, right shift, comparison, widening).
    fn get_norm(&mut self, v: Val, t: FTy) {
        let vt = class(t).unwrap_or(VT::I64);
        self.get(v, vt);
        if vt == VT::I32 && self.f.val_ty(v) != t {
            self.normalize(t);
        }
    }

    /// Pushes a value as `i64`, sign or zero extended by `t` -- what
    /// `load_ext(.., t, 64)` does in the two native backends.
    fn get_ext64(&mut self, v: Val, t: FTy) {
        match class(t) {
            Some(VT::I32) => {
                self.get_norm(v, t);
                self.num(if t.signed() { w::I64_EXTEND_I32_S } else { w::I64_EXTEND_I32_U });
            }
            _ => self.get(v, VT::I64),
        }
    }

    /// Stores the value on the stack (class `have`, canonical for `t`) into
    /// the local of `d`.
    fn set(&mut self, d: Val, have: VT, t: FTy) {
        let dt = self.f.val_ty(d);
        if self.emitting.last() == Some(&d) {
            // an expression tree: the value stays on the operand stack, in
            // the class and canonical form a local of it would hold
            if let Some(want) = self.vt_of(d) {
                self.coerce(have, want, t.signed());
                if want == VT::I32 && dt != t && narrow(dt) {
                    self.normalize(dt);
                }
            }
            return;
        }
        let l = self.loc.get(d as usize).copied().unwrap_or(u32::MAX);
        match self.vt_of(d) {
            Some(want) if l != u32::MAX => {
                self.coerce(have, want, t.signed());
                if want == VT::I32 && dt != t && narrow(dt) {
                    self.normalize(dt);
                }
                self.ins(Ins::LocalSet(l));
            }
            _ => self.ins(Ins::Drop),
        }
    }

    /// The result of an instruction: into its value, or dropped.
    fn put(&mut self, i: &Inst, have: VT) {
        match i.dst {
            Some(d) => self.set(d, have, i.ty),
            None => self.ins(Ins::Drop),
        }
    }

    fn addr(&mut self, v: Val) {
        // `get` wraps an i64 value; a 32-bit value arrives as it is
        self.get(v, VT::I32);
    }

    /// The address of a load or store: a constant, non-negative part of an
    /// address that is computed only for this access goes into the offset
    /// of the instruction. Yields the offset.
    ///
    /// WebAssembly adds the offset to the 32-bit address without wrapping
    /// (an access past 4 GiB traps), where the i64 sum cut to 32 bits would
    /// wrap. The two differ only for an address that lies outside the
    /// linear memory either way.
    fn mem_addr(&mut self, addr: Val) -> u32 {
        let mut v = addr;
        let mut off: i64 = 0;
        let fold = std::env::var_os("FIRN_WASM_NO_OFFSET").is_none();
        for _ in 0..(if fold { 16 } else { 0 }) {
            let (b, k) = match self.inline_def.get(&v) {
                Some(x) => *x,
                None => break,
            };
            let inst = &self.f.blocks[b as usize].insts[k];
            let (base, c) = match &inst.op {
                Op::PtrAdd { base, off: o } => (*base, self.consts.get(o).copied()),
                Op::Bin(BinOp::Add, a, c) if class(inst.ty) == Some(VT::I64) => {
                    match (self.consts.get(c).copied(), self.consts.get(a).copied()) {
                        (Some(k), _) => (*a, Some(k)),
                        (None, Some(k)) => (*c, Some(k)),
                        _ => break,
                    }
                }
                _ => break,
            };
            match c {
                Some(c) if c >= 0 && off as i128 + c <= i32::MAX as i128 => {
                    off += c as i64;
                    v = base;
                }
                _ => break,
            }
        }
        self.addr(v);
        off as u32
    }

    fn label_addr(&self, l: &str) -> Result<u32, String> {
        self.g
            .labels
            .get(l)
            .copied()
            .ok_or_else(|| format!("internal error: wasm32: the data label '{}' was not laid out", l))
    }

    fn depth(&self, c: Ctx) -> Result<u32, String> {
        for (k, e) in self.ctx.iter().rev().enumerate() {
            if *e == c {
                return Ok(k as u32);
            }
        }
        Err(format!("internal error: wasm32: no enclosing label for a branch in '{}'", self.f.name))
    }

    // ------------------------------------------------------ translation

    fn translate(&mut self) -> Result<(), String> {
        if self.frame > 0 {
            // __sp -= frame; fp = __sp; a frame below the limit is fatal.
            self.ins(Ins::GlobalGet(G_SP));
            self.ins(Ins::I32Const(self.frame as i32));
            self.num(w::I32_SUB);
            self.ins(Ins::LocalTee(self.fp));
            self.ins(Ins::GlobalSet(G_SP));
            self.ins(Ins::LocalGet(self.fp));
            self.ins(Ins::I32Const(self.g.stack_limit as i32));
            self.num(w::I32_LT_U);
            self.ins(Ins::If);
            self.ins(Ins::Call(self.g.rt.overflow));
            self.ins(Ins::End);
        }
        if self.cfg.reducible && !self.g.all_dispatch {
            self.do_tree(0)?;
        } else {
            self.dispatch()?;
        }
        if let Some(e) = self.err.take() {
            return Err(e);
        }
        // Every path ended in a `return` or a branch; the end of the body is
        // never reached, but it has to type check.
        self.ins(Ins::Unreachable);
        Ok(())
    }

    fn do_tree(&mut self, x: BlockId) -> Result<(), String> {
        let xu = x as usize;
        let mut merges: Vec<BlockId> = self.cfg.children[xu].iter().copied().filter(|c| self.cfg.merge[*c as usize]).collect();
        merges.reverse();
        if self.cfg.loop_header[xu] {
            self.ins(Ins::Loop);
            self.ctx.push(Ctx::Loop(x));
            self.node_within(x, &merges)?;
            self.ctx.pop();
            self.ins(Ins::End);
        } else {
            self.node_within(x, &merges)?;
        }
        Ok(())
    }

    fn node_within(&mut self, x: BlockId, ys: &[BlockId]) -> Result<(), String> {
        match ys.split_first() {
            None => {
                self.block_code(x)?;
                self.terminator(x)
            }
            Some((y, rest)) => {
                self.ins(Ins::Block);
                self.ctx.push(Ctx::Block(*y));
                self.node_within(x, rest)?;
                self.ctx.pop();
                self.ins(Ins::End);
                self.do_tree(*y)
            }
        }
    }

    fn branch(&mut self, x: BlockId, y: BlockId) -> Result<(), String> {
        if self.dispatch_local != u32::MAX {
            let pos = self.dispatch_pos[&y];
            self.ins(Ins::I32Const(pos as i32));
            self.ins(Ins::LocalSet(self.dispatch_local));
            let d = self.depth(Ctx::Dispatch)?;
            self.ins(Ins::Br(d));
            return Ok(());
        }
        if self.cfg.rpo[y as usize] <= self.cfg.rpo[x as usize] {
            let d = self.depth(Ctx::Loop(y))?;
            self.ins(Ins::Br(d));
        } else if self.cfg.merge[y as usize] {
            let d = self.depth(Ctx::Block(y))?;
            self.ins(Ins::Br(d));
        } else {
            self.do_tree(y)?;
        }
        Ok(())
    }

    /// The label depth a `br_table` entry needs for the target `y`.
    fn label_of(&self, x: BlockId, y: BlockId) -> Result<u32, String> {
        if self.cfg.rpo[y as usize] <= self.cfg.rpo[x as usize] {
            self.depth(Ctx::Loop(y))
        } else {
            self.depth(Ctx::Block(y))
        }
    }

    /// The fallback for an irreducible graph: `loop { block* { br_table } }`,
    /// one `block` per basic block, the block number in a local.
    fn dispatch(&mut self) -> Result<(), String> {
        let order = self.cfg.order.clone();
        let n = order.len();
        for (k, b) in order.iter().enumerate() {
            self.dispatch_pos.insert(*b, k as u32);
        }
        self.dispatch_local = self.new_local(VT::I32);
        self.ins(Ins::I32Const(0));
        self.ins(Ins::LocalSet(self.dispatch_local));
        self.ins(Ins::Loop);
        self.ctx.push(Ctx::Dispatch);
        for k in (0..n).rev() {
            self.ins(Ins::Block);
            self.ctx.push(Ctx::Block(order[k]));
        }
        self.ins(Ins::LocalGet(self.dispatch_local));
        let labels: Vec<u32> = (0..n as u32).collect();
        self.ins(Ins::BrTable(labels, (n - 1) as u32));
        for b in order.iter() {
            self.ins(Ins::End);
            self.ctx.pop();
            self.block_code(*b)?;
            self.terminator(*b)?;
        }
        self.ctx.pop();
        self.ins(Ins::End);
        Ok(())
    }

    fn terminator(&mut self, x: BlockId) -> Result<(), String> {
        let f = self.f;
        match &f.blocks[x as usize].term {
            Term::Br(t) => self.branch(x, *t),
            Term::BrCond { cond, then_bb, else_bb } => {
                if f.constant_time && f.is_secret(*cond) {
                    return Err(format!("#[constant_time]: conditional jump in '{}' depends on a secret value (%{})", f.name, cond));
                }
                self.get(*cond, VT::I32);
                self.ins(Ins::If);
                self.ctx.push(Ctx::If);
                self.branch(x, *then_bb)?;
                self.ins(Ins::Else);
                self.branch(x, *else_bb)?;
                self.ctx.pop();
                self.ins(Ins::End);
                Ok(())
            }
            Term::Switch { val, ty, cases, default } => self.switch(x, *val, *ty, cases, *default),
            Term::Ret(v) => {
                if let Some(v) = v {
                    if let Some(want) = class(f.ret) {
                        self.get(*v, want);
                        if want == VT::I32 && f.val_ty(*v) != f.ret && narrow(f.ret) {
                            self.normalize(f.ret);
                        }
                    }
                } else if let Some(want) = class(f.ret) {
                    self.zero(want);
                }
                if self.frame > 0 {
                    self.ins(Ins::LocalGet(self.fp));
                    self.ins(Ins::I32Const(self.frame as i32));
                    self.num(w::I32_ADD);
                    self.ins(Ins::GlobalSet(G_SP));
                }
                self.ins(Ins::Return);
                Ok(())
            }
            Term::Unset => Err(format!("internal error: block bb{} in '{}' has no terminator", x, f.name)),
        }
    }

    fn switch(&mut self, x: BlockId, val: Val, ty: FTy, cases: &[(i128, BlockId)], default: BlockId) -> Result<(), String> {
        if self.f.constant_time && self.f.is_secret(val) {
            return Err(format!("#[constant_time]: switch in '{}' depends on a secret value (%{})", self.f.name, val));
        }
        let vt = class(ty).ok_or("internal error: switch over a value without a class")?;
        if cases.is_empty() {
            return self.branch(x, default);
        }
        let min = cases.iter().map(|(k, _)| *k).min().unwrap_or(0);
        let max = cases.iter().map(|(k, _)| *k).max().unwrap_or(0);
        let extent = max - min + 1;
        let table = self.dispatch_local == u32::MAX
            && cases.len() >= 4
            && extent > 0
            && extent <= 65536
            && (cases.len() as i128) * 100 / extent >= 30;
        if table {
            // index = value - min, out of range -> default; `br_table` does
            // the range check itself for the upper end once the index is
            // unsigned.
            self.get_norm(val, ty);
            if vt == VT::I64 {
                self.ins(Ins::I64Const(min as i64));
                self.num(w::I64_SUB);
                let t = self.tmp(VT::I64);
                self.ins(Ins::LocalTee(t));
                self.ins(Ins::I64Const(extent as i64));
                self.num(w::I64_GE_U);
                self.ins(Ins::If);
                self.ctx.push(Ctx::If);
                self.branch_label(x, default)?;
                self.ctx.pop();
                self.ins(Ins::End);
                self.ins(Ins::LocalGet(t));
                self.num(w::I32_WRAP_I64);
                self.release(VT::I64, t);
            } else {
                // value - min: inside the range it is the index, outside it
                // wraps to an unsigned number past the table, and
                // `br_table` sends every such number to the default.
                if min != 0 {
                    self.ins(Ins::I32Const(ty.truncate(min) as i32));
                    self.num(w::I32_SUB);
                }
            }
            let mut labels: Vec<u32> = Vec::with_capacity(extent as usize);
            let mut ci = 0usize;
            let mut k = min;
            while k <= max {
                while ci < cases.len() && cases[ci].0 < k {
                    ci += 1;
                }
                let target = if ci < cases.len() && cases[ci].0 == k { cases[ci].1 } else { default };
                labels.push(self.label_of(x, target)?);
                k += 1;
            }
            let d = self.label_of(x, default)?;
            self.ins(Ins::BrTable(labels, d));
            return Ok(());
        }
        // A chain of comparisons.
        let t = self.tmp(vt);
        self.get_norm(val, ty);
        self.ins(Ins::LocalSet(t));
        for (k, target) in cases {
            self.ins(Ins::LocalGet(t));
            if vt == VT::I64 {
                self.ins(Ins::I64Const(*k as i64));
                self.num(w::I64_EQ);
            } else {
                self.ins(Ins::I32Const(ty.truncate(*k) as i32));
                self.num(w::I32_EQ);
            }
            self.ins(Ins::If);
            self.ctx.push(Ctx::If);
            self.branch_label(x, *target)?;
            self.ctx.pop();
            self.ins(Ins::End);
        }
        self.release(vt, t);
        self.branch_label(x, default)
    }

    /// A branch to a target that is known to carry a label (switch targets
    /// always do, see `wasm_cfg.rs`).
    fn branch_label(&mut self, x: BlockId, y: BlockId) -> Result<(), String> {
        if self.dispatch_local != u32::MAX {
            return self.branch(x, y);
        }
        let d = self.label_of(x, y)?;
        self.ins(Ins::Br(d));
        Ok(())
    }

    fn block_code(&mut self, b: BlockId) -> Result<(), String> {
        let f = self.f;
        for (k, i) in f.blocks[b as usize].insts.iter().enumerate() {
            if let Some(vs) = self.spills.get(&(b, k)).cloned() {
                for v in vs {
                    let off = self.spill_off[&v];
                    self.ins(Ins::LocalGet(self.fp));
                    if self.vt_of(v) == Some(VT::I32) {
                        // an address kept in 32 bits: the collector wants
                        // the 64-bit word, zero extended
                        self.get(v, VT::I32);
                        self.num(w::I64_EXTEND_I32_U);
                    } else {
                        self.get(v, VT::I64);
                    }
                    self.ins(Ins::Store(w::I64_STORE, off));
                }
            }
            if let Some(d) = i.dst {
                if self.inline_def.contains_key(&d) || self.remat.contains_key(&d) {
                    continue; // emitted where it is used
                }
            }
            self.inst(i)?;
        }
        Ok(())
    }

    // ------------------------------------------------------ instructions

    fn inst(&mut self, i: &Inst) -> Result<(), String> {
        let f = self.f;
        let ty = i.ty;
        match &i.op {
            Op::Const(c) => {
                let vt = match class(ty) {
                    Some(v) => v,
                    None => return Err(format!("internal error: a constant of type {} in '{}'", ty.name(), f.name)),
                };
                match vt {
                    VT::I32 => self.ins(Ins::I32Const(ty.truncate(*c) as i32)),
                    VT::I64 => self.ins(Ins::I64Const(ty.truncate(*c) as i64)),
                    VT::F32 => self.ins(Ins::F32Const(*c as u32)),
                    VT::F64 => self.ins(Ins::F64Const(*c as u64)),
                    VT::V128 => self.ins(Ins::V128Const(c.to_le_bytes())),
                }
                self.put(i, vt);
            }
            Op::Bin(op, a, b) => self.bin(i, *op, *a, *b)?,
            Op::BinWrapSat { kind, op, a, b } => {
                if *kind == WrapSatKind::Wrap {
                    self.bin(i, *op, *a, *b)?;
                } else {
                    self.sat(i, *op, *a, *b)?;
                }
            }
            Op::Cmp { op, ty: oty, a, b } => {
                let n = match (class(*oty), op, oty.signed()) {
                    (Some(VT::F32), CmpOp::Eq, _) => w::F32_EQ,
                    (Some(VT::F32), CmpOp::Ne, _) => w::F32_NE,
                    (Some(VT::F32), CmpOp::Lt, _) => w::F32_LT,
                    (Some(VT::F32), CmpOp::Le, _) => w::F32_LE,
                    (Some(VT::F32), CmpOp::Gt, _) => w::F32_GT,
                    (Some(VT::F32), CmpOp::Ge, _) => w::F32_GE,
                    (Some(VT::F64), CmpOp::Eq, _) => w::F64_EQ,
                    (Some(VT::F64), CmpOp::Ne, _) => w::F64_NE,
                    (Some(VT::F64), CmpOp::Lt, _) => w::F64_LT,
                    (Some(VT::F64), CmpOp::Le, _) => w::F64_LE,
                    (Some(VT::F64), CmpOp::Gt, _) => w::F64_GT,
                    (Some(VT::F64), CmpOp::Ge, _) => w::F64_GE,
                    (Some(VT::I32), CmpOp::Eq, _) => w::I32_EQ,
                    (Some(VT::I32), CmpOp::Ne, _) => w::I32_NE,
                    (Some(VT::I32), CmpOp::Lt, true) => w::I32_LT_S,
                    (Some(VT::I32), CmpOp::Lt, false) => w::I32_LT_U,
                    (Some(VT::I32), CmpOp::Le, true) => w::I32_LE_S,
                    (Some(VT::I32), CmpOp::Le, false) => w::I32_LE_U,
                    (Some(VT::I32), CmpOp::Gt, true) => w::I32_GT_S,
                    (Some(VT::I32), CmpOp::Gt, false) => w::I32_GT_U,
                    (Some(VT::I32), CmpOp::Ge, true) => w::I32_GE_S,
                    (Some(VT::I32), CmpOp::Ge, false) => w::I32_GE_U,
                    (Some(VT::I64), CmpOp::Eq, _) => w::I64_EQ,
                    (Some(VT::I64), CmpOp::Ne, _) => w::I64_NE,
                    (Some(VT::I64), CmpOp::Lt, true) => w::I64_LT_S,
                    (Some(VT::I64), CmpOp::Lt, false) => w::I64_LT_U,
                    (Some(VT::I64), CmpOp::Le, true) => w::I64_LE_S,
                    (Some(VT::I64), CmpOp::Le, false) => w::I64_LE_U,
                    (Some(VT::I64), CmpOp::Gt, true) => w::I64_GT_S,
                    (Some(VT::I64), CmpOp::Gt, false) => w::I64_GT_U,
                    (Some(VT::I64), CmpOp::Ge, true) => w::I64_GE_S,
                    (Some(VT::I64), CmpOp::Ge, false) => w::I64_GE_U,
                    _ => return Err(format!("internal error: a comparison of {} values in '{}'", oty.name(), f.name)),
                };
                if oty.is_float() {
                    let vt = class(*oty).unwrap_or(VT::F64);
                    self.get(*a, vt);
                    self.get(*b, vt);
                } else {
                    self.get_norm(*a, *oty);
                    self.get_norm(*b, *oty);
                }
                self.num(n);
                match i.dst {
                    Some(d) => self.set(d, VT::I32, FTy::Bool),
                    None => self.ins(Ins::Drop),
                }
            }
            Op::Un(op, a) => {
                let vt = class(ty).ok_or("internal error: unary operation without a class")?;
                match (op, vt) {
                    (UnOp::Neg, VT::F32) => {
                        self.get(*a, VT::F32);
                        self.num(w::F32_NEG);
                    }
                    (UnOp::Neg, VT::F64) => {
                        self.get(*a, VT::F64);
                        self.num(w::F64_NEG);
                    }
                    (UnOp::Neg, VT::I32) => {
                        self.ins(Ins::I32Const(0));
                        self.get(*a, VT::I32);
                        self.num(w::I32_SUB);
                        self.normalize(ty);
                    }
                    (UnOp::Neg, VT::I64) => {
                        self.ins(Ins::I64Const(0));
                        self.get(*a, VT::I64);
                        self.num(w::I64_SUB);
                    }
                    (UnOp::Not, VT::I32) => {
                        self.get(*a, VT::I32);
                        if ty == FTy::Bool {
                            self.ins(Ins::I32Const(1));
                        } else {
                            self.ins(Ins::I32Const(-1));
                        }
                        self.num(w::I32_XOR);
                        if ty != FTy::Bool {
                            self.normalize(ty);
                        }
                    }
                    (UnOp::Not, VT::I64) => {
                        self.get(*a, VT::I64);
                        self.ins(Ins::I64Const(-1));
                        self.num(w::I64_XOR);
                    }
                    // Round GAPS: `__sqrt` is one instruction here as well.
                    (UnOp::Sqrt, VT::F32) => {
                        self.get(*a, VT::F32);
                        self.num(w::F32_SQRT);
                    }
                    (UnOp::Sqrt, VT::F64) => {
                        self.get(*a, VT::F64);
                        self.num(w::F64_SQRT);
                    }
                    // Round GAPS: `__bits` / `__f64_from_bits` / `__f32_from_bits`.
                    // The instruction type is the target, the operand keeps its
                    // own class of the same width -- a pure reinterpretation.
                    (UnOp::Bits, _) => {
                        let have = self.vt_of(*a).ok_or("internal error: bit cast of a value without a class")?;
                        self.get(*a, have);
                        self.coerce(have, vt, false);
                    }
                    _ => return Err(format!("internal error: unary {:?} is not defined for {}", op, ty.name())),
                }
                self.put(i, vt);
            }
            Op::Cast { src, from } => self.cast(i, *src, *from)?,
            Op::GcAddr { .. } => {
                // No callee saved registers exist here, so there is nothing
                // to rescue into the save area: every root is in the shadow
                // frames already (see the module comment). The area stays
                // zero and scanning it finds nothing.
                self.ins(Ins::I64Const(self.g.gc_state as i64));
                self.put(i, VT::I64);
            }
            Op::Alloca { .. } => {
                let d = i.dst.ok_or("internal error: alloca without target")?;
                let off = *self.alloca_off.get(&d).ok_or("internal error: alloca without space")?;
                self.ins(Ins::LocalGet(self.fp));
                if off != 0 {
                    self.ins(Ins::I32Const(off as i32));
                    self.num(w::I32_ADD);
                }
                if self.dst_low(i) {
                    self.put(i, VT::I32);
                } else {
                    self.num(w::I64_EXTEND_I32_U);
                    self.put(i, VT::I64);
                }
            }
            Op::Load { addr } if ty == FTy::V128 => {
                let off = self.mem_addr(*addr);
                self.ins(Ins::SimdMem(w::V128_LOAD, 4, off));
                self.put(i, VT::V128);
            }
            Op::Store { addr, val } if ty == FTy::V128 => {
                let off = self.mem_addr(*addr);
                self.get(*val, VT::V128);
                self.ins(Ins::SimdMem(w::V128_STORE, 4, off));
            }
            Op::Load { addr } | Op::MmioLoad { addr } => {
                let off = if matches!(i.op, Op::Load { .. }) { self.mem_addr(*addr) } else {
                    self.addr(*addr);
                    0
                };
                let (m, vt) = match ty {
                    FTy::I8 => (w::I32_LOAD8_S, VT::I32),
                    FTy::U8 | FTy::Bool => (w::I32_LOAD8_U, VT::I32),
                    FTy::I16 => (w::I32_LOAD16_S, VT::I32),
                    FTy::U16 => (w::I32_LOAD16_U, VT::I32),
                    FTy::I32 | FTy::U32 => (w::I32_LOAD, VT::I32),
                    FTy::I64 | FTy::U64 | FTy::Ptr => (w::I64_LOAD, VT::I64),
                    FTy::F32 => (w::F32_LOAD, VT::F32),
                    FTy::F64 => (w::F64_LOAD, VT::F64),
                    _ => return Err(format!("internal error: a load of type {} in '{}'", ty.name(), f.name)),
                };
                self.ins(Ins::Load(m, off));
                self.put(i, vt);
            }
            Op::Store { addr, val } | Op::MmioStore { addr, val } => {
                let off = if matches!(i.op, Op::Store { .. }) { self.mem_addr(*addr) } else {
                    self.addr(*addr);
                    0
                };
                let (m, vt) = match ty {
                    FTy::I8 | FTy::U8 | FTy::Bool => (w::I32_STORE8, VT::I32),
                    FTy::I16 | FTy::U16 => (w::I32_STORE16, VT::I32),
                    FTy::I32 | FTy::U32 => (w::I32_STORE, VT::I32),
                    FTy::I64 | FTy::U64 | FTy::Ptr => (w::I64_STORE, VT::I64),
                    FTy::F32 => (w::F32_STORE, VT::F32),
                    FTy::F64 => (w::F64_STORE, VT::F64),
                    _ => return Err(format!("internal error: a store of type {} in '{}'", ty.name(), f.name)),
                };
                self.get(*val, vt);
                self.ins(Ins::Store(m, off));
            }
            Op::PtrAdd { base, off } => {
                if self.dst_low(i) {
                    self.get(*base, VT::I32);
                    self.get(*off, VT::I32);
                    self.num(w::I32_ADD);
                    self.put(i, VT::I32);
                } else {
                    self.get(*base, VT::I64);
                    self.get(*off, VT::I64);
                    self.num(w::I64_ADD);
                    self.put(i, VT::I64);
                }
            }
            Op::Call { name, args } => self.call(i, name, args)?,
            Op::CallIndirect { target, args } => {
                let mut params = Vec::new();
                for a in args {
                    let vt = self.vt_of(*a).ok_or("internal error: an argument without a class")?;
                    params.push(vt);
                    self.get(*a, vt);
                }
                self.addr(*target);
                let results: Vec<VT> = class(ty).into_iter().collect();
                let t = self.g.type_index(w::FuncType { params, results: results.clone() });
                self.ins(Ins::CallIndirect(t));
                if let Some(vt) = results.first() {
                    self.put(i, *vt);
                }
            }
            // The three addresses of the data: a method table, a function
            // record, a global variable. All three are link time constants
            // natively, and here they are simply numbers -- the data layout
            // put them at a fixed place of the linear memory.
            Op::VtabAddr { table } => {
                let l = crate::iface::table_label(table);
                let a = self.label_addr(&l)?;
                self.ins(Ins::I64Const(a as i64));
                self.put(i, VT::I64);
            }
            Op::FnRef { name } => {
                let a = *self
                    .g
                    .records
                    .get(name)
                    .ok_or_else(|| format!("internal error: wasm32: no function record for '{}'", name))?;
                self.ins(Ins::I64Const(a as i64));
                self.put(i, VT::I64);
            }
            Op::GlobalAddr { name } => {
                let l = crate::statics::label_of(name);
                let a = self.label_addr(&l)?;
                self.ins(Ins::I64Const(a as i64));
                self.put(i, VT::I64);
            }
            Op::Syscall { args } => self.syscall(i, args)?,
            Op::Select { cond, a, b } => {
                let vt = class(ty).ok_or("internal error: select without a class")?;
                self.get(*a, vt);
                self.get(*b, vt);
                self.get(*cond, VT::I32);
                if vt == VT::V128 {
                    self.ins(Ins::SelectT(VT::V128));
                } else {
                    self.ins(Ins::Select);
                }
                self.put(i, vt);
            }
            Op::Copy { src } | Op::Barrier { val: src } => {
                let d = i.dst.ok_or("internal error: copy without target")?;
                let vt = match self.vt_of(d) {
                    Some(v) => v,
                    None => return Ok(()),
                };
                self.get(*src, vt);
                let st = f.val_ty(*src);
                self.set(d, vt, st);
            }
            Op::Phi { .. } => return Err("internal error: phi in the code generator (phi.rs did not run)".into()),
            Op::SecureZero { addr, size } => {
                // `memory.fill` is one instruction the engine cannot drop:
                // the store happens, whatever reads it afterwards.
                self.addr(*addr);
                self.ins(Ins::I32Const(0));
                self.get(*size, VT::I64);
                self.num(w::I32_WRAP_I64);
                self.ins(Ins::MemoryFill);
            }
            Op::AtomicAdd { addr, val } => {
                // One thread: a plain read-add-write IS atomic. The result is
                // the OLD value, as `lock xadd` gives it.
                let a = self.tmp(VT::I32);
                let old = self.tmp(VT::I64);
                self.addr(*addr);
                self.ins(Ins::LocalTee(a));
                self.ins(Ins::Load(w::I64_LOAD, 0));
                self.ins(Ins::LocalSet(old));
                self.ins(Ins::LocalGet(a));
                self.ins(Ins::LocalGet(old));
                self.get(*val, VT::I64);
                self.num(w::I64_ADD);
                self.ins(Ins::Store(w::I64_STORE, 0));
                self.ins(Ins::LocalGet(old));
                self.put(i, VT::I64);
                self.release(VT::I32, a);
                self.release(VT::I64, old);
            }
            Op::AtomicCas { addr, erw, new } => {
                let a = self.tmp(VT::I32);
                let old = self.tmp(VT::I64);
                self.addr(*addr);
                self.ins(Ins::LocalTee(a));
                self.ins(Ins::Load(w::I64_LOAD, 0));
                self.ins(Ins::LocalTee(old));
                self.get(*erw, VT::I64);
                self.num(w::I64_EQ);
                self.ins(Ins::If);
                self.ins(Ins::LocalGet(a));
                self.get(*new, VT::I64);
                self.ins(Ins::Store(w::I64_STORE, 0));
                self.ins(Ins::End);
                self.ins(Ins::LocalGet(old));
                self.put(i, VT::I64);
                self.release(VT::I32, a);
                self.release(VT::I64, old);
            }
            Op::ThreadSelf => {
                // No thread pointer exists -- the value `fs` has before any
                // `arch_prctl`, which is what `__thread_tcb` checks for.
                self.ins(Ins::I64Const(0));
                self.put(i, VT::I64);
            }
            Op::CopyMem { dst, src, size } => {
                if *size > 0 {
                    self.addr(*dst);
                    self.addr(*src);
                    self.ins(Ins::I32Const(*size as i32));
                    self.ins(Ins::MemoryCopy);
                }
            }
            Op::CheckedBin { op, a, b, msg } => self.checked_bin(i, *op, *a, *b, msg)?,
            Op::CheckedDiv { op, a, b, msg_zero, msg_range } => self.checked_div(i, *op, *a, *b, msg_zero, msg_range)?,
            Op::CheckedCast { src, from, msg } => self.checked_cast(i, *src, *from, msg)?,
            Op::CheckedIdx { idx, len, msg } => {
                let v = self.tmp(VT::I64);
                self.get_ext64(*idx, ty);
                self.ins(Ins::LocalTee(v));
                self.ins(Ins::I64Const(*len as i64));
                self.num(w::I64_GE_U);
                self.ins(Ins::If);
                self.ins(Ins::I32Const(1));
                self.msg(msg)?;
                self.ins(Ins::LocalGet(v));
                self.ins(Ins::I64Const(*len as i64));
                self.ins(Ins::I64Const(crate::panic_rt::PANIC_INDEX as i64));
                self.ins(Ins::I32Const(1));
                self.ins(Ins::Call(self.g.rt.panic));
                self.ins(Ins::End);
                self.get(*idx, class(ty).unwrap_or(VT::I64));
                self.put(i, class(ty).unwrap_or(VT::I64));
                self.release(VT::I64, v);
            }
            Op::Simd { kind, args, imm } if *kind != crate::simd::SimdKind::CpuFeatures && simd_on() => {
                self.simd(i, *kind, args, *imm)?;
            }
            Op::Simd { kind, .. } => {
                if *kind == crate::simd::SimdKind::CpuFeatures {
                    // The families translated to WebAssembly SIMD, and
                    // nothing else (see `cpu_features_wasm`).
                    self.ins(Ins::I64Const(cpu_features_wasm()));
                    let vt = class(ty).unwrap_or(VT::I64);
                    if vt == VT::I32 {
                        self.num(w::I32_WRAP_I64);
                    }
                    self.put(i, vt);
                } else {
                    return Err(format!("wasm32: SIMD instruction in '{}' (refused)", f.name));
                }
            }
            Op::Asm { .. } | Op::ThreadSpawn { .. } => {
                return Err(format!("wasm32: '{}' contains an instruction this target refuses", f.name))
            }
        }
        Ok(())
    }

    /// Round OPT-GENERAL -- one SIMD intrinsic (`simd.rs`) in WebAssembly
    /// SIMD, with exactly the x86 meaning `simd.rs` gives it.
    fn simd(&mut self, i: &Inst, kind: crate::simd::SimdKind, args: &[Val], imm: u8) -> Result<(), String> {
        use crate::simd::SimdKind as K;
        let v = VT::V128;
        let arg = |k: usize| -> Result<Val, String> {
            args.get(k).copied().ok_or_else(|| format!("internal error: SIMD {:?} without operand {}", kind, k))
        };
        // `i8x16.shuffle` over (first, second) with the given lane indices
        let shuf = |me: &mut Self, idx: [u8; 16]| me.ins(Ins::Shuffle(idx));
        match kind {
            K::Load => {
                self.addr(arg(0)?);
                self.ins(Ins::SimdMem(w::V128_LOAD, 4, 0));
                self.put(i, v);
            }
            K::Store => {
                self.addr(arg(0)?);
                self.get(arg(1)?, v);
                self.ins(Ins::SimdMem(w::V128_STORE, 4, 0));
            }
            K::Zero => {
                self.ins(Ins::V128Const([0; 16]));
                self.put(i, v);
            }
            K::FromU64 => {
                self.get(arg(0)?, VT::I64);
                self.ins(Ins::SimdOp(w::I64X2_SPLAT));
                self.get(arg(1)?, VT::I64);
                self.ins(Ins::SimdLane(w::I64X2_REPLACE_LANE, 1));
                self.put(i, v);
            }
            K::GetU64 => {
                self.get(arg(0)?, v);
                self.ins(Ins::SimdLane(w::I64X2_EXTRACT_LANE, imm & 1));
                self.put(i, VT::I64);
            }
            K::GetU32 => {
                self.get(arg(0)?, v);
                self.ins(Ins::SimdLane(w::I32X4_EXTRACT_LANE, imm & 3));
                self.put(i, VT::I32);
            }
            K::GetU16 => {
                self.get(arg(0)?, v);
                self.ins(Ins::SimdLane(w::I16X8_EXTRACT_LANE_U, imm & 7));
                self.put(i, VT::I32);
            }
            K::SetU32 => {
                self.get(arg(0)?, v);
                self.get(arg(1)?, VT::I32);
                self.ins(Ins::SimdLane(w::I32X4_REPLACE_LANE, imm & 3));
                self.put(i, v);
            }
            K::Xor | K::And | K::Or | K::Add8 | K::Add32 | K::Add64 | K::Sub32 => {
                self.get(arg(0)?, v);
                self.get(arg(1)?, v);
                let op = match kind {
                    K::Xor => w::V128_XOR,
                    K::And => w::V128_AND,
                    K::Or => w::V128_OR,
                    K::Add8 => w::I8X16_ADD,
                    K::Add32 => w::I32X4_ADD,
                    K::Add64 => w::I64X2_ADD,
                    _ => w::I32X4_SUB,
                };
                self.ins(Ins::SimdOp(op));
                self.put(i, v);
            }
            K::AndNot => {
                // `pandn`: ~a & b. WebAssembly's andnot(x, y) is x & ~y.
                self.get(arg(1)?, v);
                self.get(arg(0)?, v);
                self.ins(Ins::SimdOp(w::V128_ANDNOT));
                self.put(i, v);
            }
            K::ShuffleB => {
                // `pshufb`: lane = b & 0x80 ? 0 : a[b & 15]. `swizzle` gives
                // 0 for every index >= 16, so b & 0x8F says the same.
                self.get(arg(0)?, v);
                self.get(arg(1)?, v);
                self.ins(Ins::V128Const([0x8F; 16]));
                self.ins(Ins::SimdOp(w::V128_AND));
                self.ins(Ins::SimdOp(w::I8X16_SWIZZLE));
                self.put(i, v);
            }
            K::Shuffle32 => {
                // `pshufd`: lane l = src[(imm >> 2l) & 3]
                let mut idx = [0u8; 16];
                for l in 0..4 {
                    let s = (imm >> (2 * l)) & 3;
                    for k in 0..4 {
                        idx[l * 4 + k] = s * 4 + k as u8;
                    }
                }
                self.get(arg(0)?, v);
                self.get(arg(0)?, v);
                shuf(self, idx);
                self.put(i, v);
            }
            K::AlignR => {
                // `palignr a, b, imm`: the 32 octets b (low) : a (high),
                // shifted right by imm octets, the low sixteen of it
                let n = imm as usize;
                let mut idx = [0u8; 16];
                if n <= 16 {
                    for (k, x) in idx.iter_mut().enumerate() {
                        *x = (k + n) as u8; // 0..15 = b, 16..31 = a
                    }
                    self.get(arg(1)?, v);
                    self.get(arg(0)?, v);
                } else {
                    for (k, x) in idx.iter_mut().enumerate() {
                        let j = k + n - 16;
                        *x = if j < 16 { j as u8 } else { 16 };
                    }
                    self.get(arg(0)?, v);
                    self.ins(Ins::V128Const([0; 16]));
                }
                shuf(self, idx);
                self.put(i, v);
            }
            K::UnpackLo32 | K::UnpackHi32 | K::UnpackLo64 | K::UnpackHi64 => {
                // a = lanes 0..15, b = lanes 16..31
                let lanes: [u8; 4] = match kind {
                    K::UnpackLo32 => [0, 16, 4, 20],
                    K::UnpackHi32 => [8, 24, 12, 28],
                    K::UnpackLo64 => [0, 4, 16, 20],
                    _ => [8, 12, 24, 28],
                };
                let mut idx = [0u8; 16];
                for (q, base) in lanes.iter().enumerate() {
                    for k in 0..4 {
                        idx[q * 4 + k] = base + k as u8;
                    }
                }
                self.get(arg(0)?, v);
                self.get(arg(1)?, v);
                shuf(self, idx);
                self.put(i, v);
            }
            K::ShlBytes => {
                // `pslldq`: lane k = k >= imm ? v[k - imm] : 0
                let n = imm as usize;
                let mut idx = [0u8; 16];
                for (k, x) in idx.iter_mut().enumerate() {
                    *x = if k >= n { (16 + k - n) as u8 } else { 0 };
                }
                self.ins(Ins::V128Const([0; 16]));
                self.get(arg(0)?, v);
                shuf(self, idx);
                self.put(i, v);
            }
            K::ShrBytes => {
                // `psrldq`: lane k = k + imm < 16 ? v[k + imm] : 0
                let n = imm as usize;
                let mut idx = [0u8; 16];
                for (k, x) in idx.iter_mut().enumerate() {
                    *x = if k + n < 16 { (k + n) as u8 } else { 16 };
                }
                self.get(arg(0)?, v);
                self.ins(Ins::V128Const([0; 16]));
                shuf(self, idx);
                self.put(i, v);
            }
            K::Shl32 | K::Shr32 | K::Shl64 | K::Shr64 => {
                // the counts are literals below the lane width (simd.rs), so
                // WebAssembly's count modulo the width changes nothing
                self.get(arg(0)?, v);
                self.ins(Ins::I32Const(imm as i32));
                let op = match kind {
                    K::Shl32 => w::I32X4_SHL,
                    K::Shr32 => w::I32X4_SHR_U,
                    K::Shl64 => w::I64X2_SHL,
                    _ => w::I64X2_SHR_U,
                };
                self.ins(Ins::SimdOp(op));
                self.put(i, v);
            }
            K::Blend16 => {
                // `pblendw a, b, imm`: 16-bit lane l = imm bit l ? b : a
                let mut idx = [0u8; 16];
                for l in 0..8 {
                    let from_b = (imm >> l) & 1 != 0;
                    let base = if from_b { 16 } else { 0 } + (2 * l) as u8;
                    idx[2 * l] = base;
                    idx[2 * l + 1] = base + 1;
                }
                self.get(arg(0)?, v);
                self.get(arg(1)?, v);
                shuf(self, idx);
                self.put(i, v);
            }
            K::Store64 => {
                // the low eight octets only (two `f32`, TEMPO 7)
                self.addr(arg(0)?);
                self.get(arg(1)?, v);
                self.ins(Ins::SimdMemLane(w::V128_STORE64_LANE, 3, 0, 0));
            }
            K::AddF32 | K::SubF32 | K::MulF32 | K::CmpLtF32 | K::CmpLeF32 | K::CmpGt32 => {
                // lane for lane what the scalar instruction computes (TEMPO 4/5);
                // the comparisons give all ones / all zeros per lane, as
                // `cmpltps`/`cmpleps`/`pcmpgtd` do
                self.get(arg(0)?, v);
                self.get(arg(1)?, v);
                let op = match kind {
                    K::AddF32 => w::F32X4_ADD,
                    K::SubF32 => w::F32X4_SUB,
                    K::MulF32 => w::F32X4_MUL,
                    K::CmpLtF32 => w::F32X4_LT,
                    K::CmpLeF32 => w::F32X4_LE,
                    _ => w::I32X4_GT_S,
                };
                self.ins(Ins::SimdOp(op));
                self.put(i, v);
            }
            K::CmpNltF32 => {
                // NOT less-than: TRUE for an unordered pair (NaN), exactly
                // `cmpnltps`
                self.get(arg(0)?, v);
                self.get(arg(1)?, v);
                self.ins(Ins::SimdOp(w::F32X4_LT));
                self.ins(Ins::SimdOp(w::V128_NOT));
                self.put(i, v);
            }
            K::CvtI32F32 => {
                self.get(arg(0)?, v);
                self.ins(Ins::SimdOp(w::F32X4_CONVERT_I32X4_S));
                self.put(i, v);
            }
            K::TruncF32I32 => {
                // `cvttps2dq` answers 0x80000000 for NaN and for everything
                // out of range; `trunc_sat` saturates (NaN -> 0, too big ->
                // 0x7FFFFFFF). The lower side already agrees; NaN and the
                // upper side are put right with a mask.
                let x = self.tmp(v);
                self.get(arg(0)?, v);
                self.ins(Ins::LocalTee(x));
                self.ins(Ins::SimdOp(w::I32X4_TRUNC_SAT_F32X4_S));
                // bitselect(a = 0x80000000, b = trunc, mask)
                self.ins(Ins::V128Const([0, 0, 0, 0x80, 0, 0, 0, 0x80, 0, 0, 0, 0x80, 0, 0, 0, 0x80]));
                self.ins(Ins::LocalGet(x));
                self.ins(Ins::LocalGet(x));
                self.ins(Ins::SimdOp(w::F32X4_NE));
                self.ins(Ins::LocalGet(x));
                self.ins(Ins::F32Const(((1u64 << 31) as f32).to_bits()));
                self.ins(Ins::SimdOp(w::F32X4_SPLAT));
                self.ins(Ins::SimdOp(w::F32X4_GE));
                self.ins(Ins::SimdOp(w::V128_OR));
                // stack: trunc, MIN, mask -> bitselect(MIN, trunc, mask)
                let m = self.tmp(v);
                self.ins(Ins::LocalSet(m));
                let mn = self.tmp(v);
                self.ins(Ins::LocalSet(mn));
                let t = self.tmp(v);
                self.ins(Ins::LocalSet(t));
                self.ins(Ins::LocalGet(mn));
                self.ins(Ins::LocalGet(t));
                self.ins(Ins::LocalGet(m));
                self.ins(Ins::SimdOp(w::V128_BITSELECT));
                self.release(v, x);
                self.release(v, m);
                self.release(v, mn);
                self.release(v, t);
                self.put(i, v);
            }
            K::AesEnc
            | K::AesEncLast
            | K::AesDec
            | K::AesDecLast
            | K::AesImc
            | K::AesKeyGenAssist
            | K::Sha256Rnds2
            | K::Sha256Msg1
            | K::Sha256Msg2
            | K::Pclmul
            | K::Crc32U8
            | K::Crc32U64 => {
                // No such instruction in WebAssembly, and `__cpu_features`
                // says so (see the module comment): reaching it is the
                // program's error, like SIGILL on an x86 without it.
                self.ins(Ins::Unreachable);
                if let Some(d) = i.dst {
                    let vt = self.vt_of(d).unwrap_or(VT::I64);
                    self.put(i, vt);
                }
            }
            K::CpuFeatures => {
                self.ins(Ins::I64Const(cpu_features_wasm()));
                let vt = class(i.ty).unwrap_or(VT::I64);
                if vt == VT::I32 {
                    self.num(w::I32_WRAP_I64);
                }
                self.put(i, vt);
            }
            // a kind `simd.rs` gained after this table (refused before
            // translation by `wasm_simd_kind`, so never reached)
            #[allow(unreachable_patterns)]
            _ => return Err(format!("wasm32: the SIMD instruction '{:?}' in '{}' has no WebAssembly form yet", kind, self.f.name)),
        }
        Ok(())
    }

    /// Address and length of a panic message, as two `i32`.
    fn msg(&mut self, msg: &str) -> Result<(), String> {
        let a = *self.g.msg_addr.get(msg).ok_or("internal error: panic message without data")?;
        self.ins(Ins::I32Const(a as i32));
        self.ins(Ins::I32Const(msg.len() as i32));
        Ok(())
    }

    fn call(&mut self, i: &Inst, name: &str, args: &[Val]) -> Result<(), String> {
        let f = self.f;
        if let Some((idx, sig)) = self.g.externs.get(name).cloned() {
            for (k, a) in args.iter().enumerate() {
                if let Some(vt) = sig.params.get(k) {
                    self.get(*a, *vt);
                }
            }
            self.ins(Ins::Call(idx));
            match (sig.results.first(), i.dst) {
                (Some(vt), Some(d)) => self.set(d, *vt, i.ty),
                (Some(_), None) => self.ins(Ins::Drop),
                (None, Some(d)) => {
                    let vt = self.vt_of(d).unwrap_or(VT::I64);
                    self.zero(vt);
                    self.set(d, vt, f.val_ty(d));
                }
                (None, None) => {}
            }
            return Ok(());
        }
        let ci = *self.g.by_name.get(name).ok_or_else(|| format!("internal error: call of the unknown function '{}'", name))?;
        let callee = &self.g.m.funcs[ci];
        let idx = *self.g.fidx.get(name).ok_or_else(|| format!("internal error: '{}' was not emitted", name))?;
        for (k, pt) in callee.params.iter().enumerate() {
            let vt = class(*pt).ok_or("internal error: parameter without a class")?;
            match args.get(k) {
                Some(a) => {
                    self.get(*a, vt);
                    if vt == VT::I32 && f.val_ty(*a) != *pt && narrow(*pt) {
                        self.normalize(*pt);
                    }
                }
                // Fewer arguments than parameters: System V would hand over
                // whatever the register holds; zero is the defined choice.
                None => self.zero(vt),
            }
        }
        self.ins(Ins::Call(idx));
        match (class(callee.ret), i.dst) {
            (Some(vt), Some(d)) => self.set(d, vt, callee.ret),
            (Some(_), None) => self.ins(Ins::Drop),
            (None, Some(d)) => {
                let vt = self.vt_of(d).unwrap_or(VT::I64);
                self.zero(vt);
                self.set(d, vt, f.val_ty(d));
            }
            (None, None) => {}
        }
        Ok(())
    }

    fn bin(&mut self, i: &Inst, op: BinOp, a: Val, b: Val) -> Result<(), String> {
        let ty = i.ty;
        let vt = class(ty).ok_or("internal error: binary operation without a class")?;
        if ty.is_float() {
            let n = match (op, vt) {
                (BinOp::Add, VT::F32) => w::F32_ADD,
                (BinOp::Sub, VT::F32) => w::F32_SUB,
                (BinOp::Mul, VT::F32) => w::F32_MUL,
                (BinOp::Div, VT::F32) => w::F32_DIV,
                (BinOp::Add, _) => w::F64_ADD,
                (BinOp::Sub, _) => w::F64_SUB,
                (BinOp::Mul, _) => w::F64_MUL,
                (BinOp::Div, _) => w::F64_DIV,
                _ => return Err(format!("internal error: operator '{:?}' is not defined for {}", op, ty.name())),
            };
            self.get(a, vt);
            self.get(b, vt);
            self.num(n);
            self.put(i, vt);
            return Ok(());
        }
        let s = ty.signed();
        // Round OPT-GENERAL: a 64-bit result of which only the low 32 bits
        // are ever needed is computed in 32 bits -- the low bits of a sum,
        // difference, product, bit operation or left shift depend only on
        // the low bits of the operands (a shift only while the count is a
        // constant below 32, which `plan_low32` checks)
        let vt = if vt == VT::I64
            && self.dst_low(i)
            && matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::And | BinOp::Or | BinOp::Xor | BinOp::Shl)
        {
            VT::I32
        } else {
            vt
        };
        let wide = vt == VT::I64;
        let pick = |n32: w::Num, n64: w::Num| if wide { n64 } else { n32 };
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::And | BinOp::Or | BinOp::Xor | BinOp::Shl => {
                self.get(a, vt);
                self.get(b, vt);
                let n = match op {
                    BinOp::Add => pick(w::I32_ADD, w::I64_ADD),
                    BinOp::Sub => pick(w::I32_SUB, w::I64_SUB),
                    BinOp::Mul => pick(w::I32_MUL, w::I64_MUL),
                    BinOp::And => pick(w::I32_AND, w::I64_AND),
                    BinOp::Or => pick(w::I32_OR, w::I64_OR),
                    BinOp::Xor => pick(w::I32_XOR, w::I64_XOR),
                    _ => pick(w::I32_SHL, w::I64_SHL),
                };
                self.num(n);
                if !wide && matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Shl) {
                    self.normalize(ty);
                }
            }
            BinOp::Div | BinOp::Rem => {
                self.get_norm(a, ty);
                self.get_norm(b, ty);
                let n = match (op, s) {
                    (BinOp::Div, true) => pick(w::I32_DIV_S, w::I64_DIV_S),
                    (BinOp::Div, false) => pick(w::I32_DIV_U, w::I64_DIV_U),
                    (_, true) => pick(w::I32_REM_S, w::I64_REM_S),
                    (_, false) => pick(w::I32_REM_U, w::I64_REM_U),
                };
                self.num(n);
                if !wide {
                    self.normalize(ty);
                }
            }
            BinOp::Shr => {
                self.get_norm(a, ty);
                self.get(b, vt);
                self.num(if s { pick(w::I32_SHR_S, w::I64_SHR_S) } else { pick(w::I32_SHR_U, w::I64_SHR_U) });
            }
        }
        self.put(i, vt);
        Ok(())
    }

    /// The exact range of a narrow type, as `i64`.
    fn range(ty: FTy) -> (i64, i64) {
        match ty {
            FTy::I8 => (i8::MIN as i64, i8::MAX as i64),
            FTy::I16 => (i16::MIN as i64, i16::MAX as i64),
            FTy::I32 => (i32::MIN as i64, i32::MAX as i64),
            FTy::U8 => (0, u8::MAX as i64),
            FTy::U16 => (0, u16::MAX as i64),
            FTy::U32 | FTy::Bool => (0, u32::MAX as i64),
            _ => (i64::MIN, i64::MAX),
        }
    }

    /// Pushes 1 if `a op b` does not fit into `ty`, and leaves the wrapped
    /// result in `r` (an i64 local). `a`/`b` are i64 locals holding the
    /// operands extended by `ty`.
    fn overflow(&mut self, op: BinOp, ty: FTy, a: u32, b: u32, r: u32) -> Result<(), String> {
        let wide = ty.bits() == 64;
        let s = ty.signed();
        // r = a op b (exact for the narrow types, wrapped for 64 bits)
        self.ins(Ins::LocalGet(a));
        self.ins(Ins::LocalGet(b));
        self.num(match op {
            BinOp::Add => w::I64_ADD,
            BinOp::Sub => w::I64_SUB,
            BinOp::Mul => w::I64_MUL,
            _ => return Err("internal error: overflow test for an operator other than + - *".into()),
        });
        self.ins(Ins::LocalSet(r));
        if !wide {
            // The exact result is in r; it overflows when it leaves the range.
            let (lo, hi) = Self::range(ty);
            self.ins(Ins::LocalGet(r));
            self.ins(Ins::I64Const(lo));
            self.num(w::I64_LT_S);
            self.ins(Ins::LocalGet(r));
            self.ins(Ins::I64Const(hi));
            self.num(if s { w::I64_GT_S } else { w::I64_GT_U });
            self.num(w::I32_OR);
            return Ok(());
        }
        match (op, s) {
            (BinOp::Add, true) => {
                // ((a ^ r) & (b ^ r)) < 0
                self.ins(Ins::LocalGet(a));
                self.ins(Ins::LocalGet(r));
                self.num(w::I64_XOR);
                self.ins(Ins::LocalGet(b));
                self.ins(Ins::LocalGet(r));
                self.num(w::I64_XOR);
                self.num(w::I64_AND);
                self.ins(Ins::I64Const(0));
                self.num(w::I64_LT_S);
            }
            (BinOp::Sub, true) => {
                // ((a ^ b) & (a ^ r)) < 0
                self.ins(Ins::LocalGet(a));
                self.ins(Ins::LocalGet(b));
                self.num(w::I64_XOR);
                self.ins(Ins::LocalGet(a));
                self.ins(Ins::LocalGet(r));
                self.num(w::I64_XOR);
                self.num(w::I64_AND);
                self.ins(Ins::I64Const(0));
                self.num(w::I64_LT_S);
            }
            (BinOp::Add, false) => {
                // r < a (unsigned)
                self.ins(Ins::LocalGet(r));
                self.ins(Ins::LocalGet(a));
                self.num(w::I64_LT_U);
            }
            (BinOp::Sub, false) => {
                self.ins(Ins::LocalGet(a));
                self.ins(Ins::LocalGet(b));
                self.num(w::I64_LT_U);
            }
            (_, false) => {
                // WebAssembly has no high half of a product. Division
                // answers the same question: the product wrapped exactly
                // when r / a != b (for a != 0).
                let flag = self.tmp(VT::I32);
                self.ins(Ins::I32Const(0));
                self.ins(Ins::LocalSet(flag));
                self.ins(Ins::LocalGet(a));
                self.num(w::I64_EQZ);
                self.num(w::I32_EQZ);
                self.ins(Ins::If);
                self.ins(Ins::LocalGet(r));
                self.ins(Ins::LocalGet(a));
                self.num(w::I64_DIV_U);
                self.ins(Ins::LocalGet(b));
                self.num(w::I64_NE);
                self.ins(Ins::LocalSet(flag));
                self.ins(Ins::End);
                self.ins(Ins::LocalGet(flag));
                self.release(VT::I32, flag);
            }
            (_, true) => {
                // The same through the signed division, with its one trap
                // taken out first: a == -1 overflows exactly when b == MIN,
                // and MIN / -1 must not be executed at all.
                let flag = self.tmp(VT::I32);
                self.ins(Ins::I32Const(0));
                self.ins(Ins::LocalSet(flag));
                self.ins(Ins::LocalGet(a));
                self.ins(Ins::I64Const(-1));
                self.num(w::I64_EQ);
                self.ins(Ins::If);
                self.ins(Ins::LocalGet(b));
                self.ins(Ins::I64Const(i64::MIN));
                self.num(w::I64_EQ);
                self.ins(Ins::LocalSet(flag));
                self.ins(Ins::Else);
                self.ins(Ins::LocalGet(a));
                self.num(w::I64_EQZ);
                self.num(w::I32_EQZ);
                self.ins(Ins::If);
                self.ins(Ins::LocalGet(r));
                self.ins(Ins::LocalGet(a));
                self.num(w::I64_DIV_S);
                self.ins(Ins::LocalGet(b));
                self.num(w::I64_NE);
                self.ins(Ins::LocalSet(flag));
                self.ins(Ins::End);
                self.ins(Ins::End);
                self.ins(Ins::LocalGet(flag));
                self.release(VT::I32, flag);
            }
        }
        Ok(())
    }

    fn sat(&mut self, i: &Inst, op: BinOp, a: Val, b: Val) -> Result<(), String> {
        let ty = i.ty;
        let vt = class(ty).ok_or("internal error: saturating operation without a class")?;
        let (ta, tb, tr) = (self.tmp(VT::I64), self.tmp(VT::I64), self.tmp(VT::I64));
        self.get_ext64(a, ty);
        self.ins(Ins::LocalSet(ta));
        self.get_ext64(b, ty);
        self.ins(Ins::LocalSet(tb));
        self.overflow(op, ty, ta, tb, tr)?;
        // on overflow: the bound
        self.ins(Ins::If);
        let (lo, hi) = Self::range(ty);
        let (lo, hi) = if ty.bits() == 64 && !ty.signed() { (0i64, -1i64) } else { (lo, hi) };
        if !ty.signed() {
            self.ins(Ins::I64Const(if op == BinOp::Sub { lo } else { hi }));
            self.ins(Ins::LocalSet(tr));
        } else {
            // Which bound: Add -> sign of a; Sub -> sign of b (negative b
            // means the true difference is larger); Mul -> sign of a ^ b.
            self.ins(Ins::I64Const(if op == BinOp::Sub { hi } else { lo }));
            self.ins(Ins::I64Const(if op == BinOp::Sub { lo } else { hi }));
            match op {
                BinOp::Add => self.ins(Ins::LocalGet(ta)),
                BinOp::Sub => self.ins(Ins::LocalGet(tb)),
                _ => {
                    self.ins(Ins::LocalGet(ta));
                    self.ins(Ins::LocalGet(tb));
                    self.num(w::I64_XOR);
                }
            }
            self.ins(Ins::I64Const(0));
            self.num(w::I64_LT_S);
            self.ins(Ins::Select);
            self.ins(Ins::LocalSet(tr));
        }
        self.ins(Ins::End);
        self.ins(Ins::LocalGet(tr));
        if vt == VT::I32 {
            self.num(w::I32_WRAP_I64);
            self.normalize(ty);
        }
        self.put(i, vt);
        self.release(VT::I64, ta);
        self.release(VT::I64, tb);
        self.release(VT::I64, tr);
        Ok(())
    }

    fn checked_bin(&mut self, i: &Inst, op: BinOp, a: Val, b: Val, msg: &str) -> Result<(), String> {
        let ty = i.ty;
        let vt = class(ty).ok_or("internal error: checked operation without a class")?;
        let (ta, tb, tr) = (self.tmp(VT::I64), self.tmp(VT::I64), self.tmp(VT::I64));
        self.get_ext64(a, ty);
        self.ins(Ins::LocalSet(ta));
        self.get_ext64(b, ty);
        self.ins(Ins::LocalSet(tb));
        self.overflow(op, ty, ta, tb, tr)?;
        self.ins(Ins::If);
        self.ins(Ins::I32Const(0));
        self.msg(msg)?;
        self.ins(Ins::LocalGet(ta));
        self.ins(Ins::LocalGet(tb));
        let code = match op {
            BinOp::Add => crate::panic_rt::PANIC_ADD,
            BinOp::Sub => crate::panic_rt::PANIC_SUB,
            _ => crate::panic_rt::PANIC_MUL,
        };
        self.ins(Ins::I64Const(code as i64));
        self.ins(Ins::I32Const(if ty.signed() { 0 } else { 1 }));
        self.ins(Ins::Call(self.g.rt.panic));
        self.ins(Ins::End);
        self.ins(Ins::LocalGet(tr));
        if vt == VT::I32 {
            self.num(w::I32_WRAP_I64);
            self.normalize(ty);
        }
        self.put(i, vt);
        self.release(VT::I64, ta);
        self.release(VT::I64, tb);
        self.release(VT::I64, tr);
        Ok(())
    }

    fn checked_div(&mut self, i: &Inst, op: BinOp, a: Val, b: Val, msg_zero: &str, msg_range: &str) -> Result<(), String> {
        let ty = i.ty;
        let vt = class(ty).ok_or("internal error: checked division without a class")?;
        let (ta, tb) = (self.tmp(VT::I64), self.tmp(VT::I64));
        self.get_ext64(a, ty);
        self.ins(Ins::LocalSet(ta));
        self.get_ext64(b, ty);
        self.ins(Ins::LocalSet(tb));
        // b == 0
        self.ins(Ins::LocalGet(tb));
        self.num(w::I64_EQZ);
        self.ins(Ins::If);
        self.ins(Ins::I32Const(0));
        self.msg(msg_zero)?;
        self.ins(Ins::LocalGet(ta));
        self.ins(Ins::LocalGet(tb));
        self.ins(Ins::I64Const(crate::panic_rt::PANIC_DIV0 as i64));
        self.ins(Ins::I32Const(if ty.signed() { 0 } else { 1 }));
        self.ins(Ins::Call(self.g.rt.panic));
        self.ins(Ins::End);
        if ty.signed() {
            let (lo, _) = Self::range(ty);
            self.ins(Ins::LocalGet(ta));
            self.ins(Ins::I64Const(lo));
            self.num(w::I64_EQ);
            self.ins(Ins::LocalGet(tb));
            self.ins(Ins::I64Const(-1));
            self.num(w::I64_EQ);
            self.num(w::I32_AND);
            self.ins(Ins::If);
            self.ins(Ins::I32Const(0));
            self.msg(msg_range)?;
            self.ins(Ins::LocalGet(ta));
            self.ins(Ins::LocalGet(tb));
            self.ins(Ins::I64Const(crate::panic_rt::PANIC_DIV_OVERFLOW as i64));
            self.ins(Ins::I32Const(0));
            self.ins(Ins::Call(self.g.rt.panic));
            self.ins(Ins::End);
        }
        // The division itself, at the type's own class.
        let wide = vt == VT::I64;
        self.ins(Ins::LocalGet(ta));
        if !wide {
            self.num(w::I32_WRAP_I64);
        }
        self.ins(Ins::LocalGet(tb));
        if !wide {
            self.num(w::I32_WRAP_I64);
        }
        let n = match (op, ty.signed(), wide) {
            (BinOp::Div, true, false) => w::I32_DIV_S,
            (BinOp::Div, false, false) => w::I32_DIV_U,
            (BinOp::Div, true, true) => w::I64_DIV_S,
            (BinOp::Div, false, true) => w::I64_DIV_U,
            (_, true, false) => w::I32_REM_S,
            (_, false, false) => w::I32_REM_U,
            (_, true, true) => w::I64_REM_S,
            (_, false, true) => w::I64_REM_U,
        };
        self.num(n);
        if !wide {
            self.normalize(ty);
        }
        self.put(i, vt);
        self.release(VT::I64, ta);
        self.release(VT::I64, tb);
        Ok(())
    }

    /// `v` (an i64 on the stack) narrowed to `bits` and extended back by
    /// `signed` -- the round trip `panic_rt.rs::emit_checked_cast` does.
    fn narrow_extend(&mut self, bits: u32, signed: bool) {
        match (bits, signed) {
            (8, true) => self.num(w::I64_EXTEND8_S),
            (16, true) => self.num(w::I64_EXTEND16_S),
            (32, true) => self.num(w::I64_EXTEND32_S),
            (8, false) => {
                self.ins(Ins::I64Const(0xFF));
                self.num(w::I64_AND);
            }
            (16, false) => {
                self.ins(Ins::I64Const(0xFFFF));
                self.num(w::I64_AND);
            }
            (32, false) => {
                self.ins(Ins::I64Const(0xFFFF_FFFF));
                self.num(w::I64_AND);
            }
            _ => {}
        }
    }

    fn checked_cast(&mut self, i: &Inst, src: Val, from: FTy, msg: &str) -> Result<(), String> {
        let to = i.ty;
        let vt = class(to).ok_or("internal error: checked cast without a class")?;
        if from.is_float() || to.is_float() {
            return Err("internal error: a checked cast involving a float".into());
        }
        let v = self.tmp(VT::I64);
        self.get_ext64(src, from);
        self.ins(Ins::LocalTee(v));
        // narrow to `to`, widen back to `from`, compare
        let tb = if to == FTy::Bool { 8 } else { to.bits() };
        self.narrow_extend(tb, to.signed());
        let fb = if from == FTy::Bool { 8 } else { from.bits() };
        if fb > tb && fb < 64 {
            self.narrow_extend(fb, from.signed());
        }
        self.ins(Ins::LocalGet(v));
        self.num(w::I64_NE);
        self.ins(Ins::If);
        self.ins(Ins::I32Const(0));
        self.msg(msg)?;
        self.ins(Ins::LocalGet(v));
        self.ins(Ins::LocalGet(v));
        self.ins(Ins::I64Const(crate::panic_rt::PANIC_CAST as i64));
        self.ins(Ins::I32Const(if from.signed() { 0 } else { 1 }));
        self.ins(Ins::Call(self.g.rt.panic));
        self.ins(Ins::End);
        self.ins(Ins::LocalGet(v));
        if vt == VT::I32 {
            self.num(w::I32_WRAP_I64);
            self.normalize(to);
        }
        self.put(i, vt);
        self.release(VT::I64, v);
        Ok(())
    }

    fn cast(&mut self, i: &Inst, src: Val, from: FTy) -> Result<(), String> {
        let to = i.ty;
        let tv = class(to).ok_or("internal error: conversion into a type without a class")?;
        let fv = class(from).ok_or("internal error: conversion from a type without a class")?;
        if to.is_float() && from.is_float() {
            self.get(src, fv);
            if to != from {
                self.num(if to == FTy::F64 { w::F64_PROMOTE_F32 } else { w::F32_DEMOTE_F64 });
            }
            self.put(i, tv);
            return Ok(());
        }
        if to.is_float() {
            // Integer -> float: widened to 64 bits FIRST, then converted as
            // a signed number -- literally `cvtsi2sd`, including its
            // reservation about unsigned values above 2^63 (SPEC 14.1.f64).
            self.get_ext64(src, from);
            self.num(if to == FTy::F64 { w::F64_CONVERT_I64_S } else { w::F32_CONVERT_I64_S });
            self.put(i, tv);
            return Ok(());
        }
        if from.is_float() {
            // Float -> integer, cutting towards zero. `cvttsd2si` answers
            // 0x8000000000000000 for NaN and for everything out of range;
            // WebAssembly's `trunc_sat` saturates instead. The x86 answer is
            // the reference, so NaN and the upper side are corrected to it
            // (the lower side already saturates to exactly that value).
            let x = self.tmp(fv);
            self.get(src, fv);
            self.ins(Ins::LocalSet(x));
            // select(MIN, trunc_sat(x), x is NaN | x >= 2^63)
            self.ins(Ins::I64Const(i64::MIN));
            self.ins(Ins::LocalGet(x));
            self.ins(Ins::Fc(if fv == VT::F32 { w::I64_TRUNC_SAT_F32_S } else { w::I64_TRUNC_SAT_F64_S }));
            self.ins(Ins::LocalGet(x));
            self.ins(Ins::LocalGet(x));
            self.num(if fv == VT::F32 { w::F32_NE } else { w::F64_NE });
            self.ins(Ins::LocalGet(x));
            if fv == VT::F32 {
                self.ins(Ins::F32Const(((1u64 << 63) as f32).to_bits()));
                self.num(w::F32_GE);
            } else {
                self.ins(Ins::F64Const(((1u64 << 63) as f64).to_bits()));
                self.num(w::F64_GE);
            }
            self.num(w::I32_OR);
            self.ins(Ins::Select);
            self.release(fv, x);
            if tv == VT::I32 {
                self.num(w::I32_WRAP_I64);
                self.normalize(to);
            }
            self.put(i, tv);
            return Ok(());
        }
        if to == FTy::Bool {
            // Only the low `from` bits decide.
            self.get_norm(src, from);
            if fv == VT::I64 {
                self.ins(Ins::I64Const(0));
                self.num(w::I64_NE);
            } else {
                self.ins(Ins::I32Const(0));
                self.num(w::I32_NE);
            }
            self.put(i, VT::I32);
            return Ok(());
        }
        if tv == VT::I64 && self.dst_low(i) {
            // only the low 32 bits of the result are needed: those of the
            // canonical 32-bit form, or of the 64-bit source
            if fv == VT::I32 {
                self.get_norm(src, from);
            } else {
                self.get(src, VT::I32);
            }
            self.put(i, VT::I32);
            return Ok(());
        }
        match (fv, tv) {
            (VT::I32, VT::I32) => {
                self.get_norm(src, from);
                self.normalize(to);
            }
            (VT::I32, VT::I64) => self.get_ext64(src, from),
            (VT::I64, VT::I32) => {
                self.get(src, VT::I32);
                self.normalize(to);
            }
            _ => self.get(src, VT::I64),
        }
        self.put(i, tv);
        Ok(())
    }

    fn syscall(&mut self, i: &Inst, args: &[Val]) -> Result<(), String> {
        let nr = args
            .first()
            .and_then(|a| self.consts.get(a))
            .copied()
            .ok_or("internal error: wasm32: system call number is not a constant")? as i64;
        let (_, form) = syscalls::wasm(nr).ok_or("internal error: wasm32: system call without a translation")?;
        let arg = |fx: &mut Fx, k: usize, vt: VT| match args.get(k) {
            Some(a) => fx.get(*a, vt),
            None => fx.zero(vt),
        };
        match form {
            Sys::Write | Sys::Read => {
                arg(self, 1, VT::I32);
                arg(self, 2, VT::I32);
                arg(self, 3, VT::I32);
                self.ins(Ins::Call(if form == Sys::Write { H_WRITE } else { H_READ }));
                self.num(w::I64_EXTEND_I32_S);
                self.put(i, VT::I64);
            }
            Sys::Exit => {
                arg(self, 1, VT::I32);
                self.ins(Ins::Call(H_EXIT));
                self.ins(Ins::Unreachable);
            }
            Sys::ClockGettime => {
                arg(self, 1, VT::I64);
                arg(self, 2, VT::I64);
                self.ins(Ins::Call(self.g.rt.clock));
                self.put(i, VT::I64);
            }
            Sys::Getrandom => {
                arg(self, 1, VT::I32);
                arg(self, 2, VT::I32);
                self.ins(Ins::Call(H_RANDOM));
                self.num(w::I64_EXTEND_I32_S);
                self.put(i, VT::I64);
            }
            Sys::Nanosleep => {
                arg(self, 1, VT::I64);
                arg(self, 2, VT::I64);
                self.ins(Ins::Call(self.g.rt.nanosleep));
                self.put(i, VT::I64);
            }
            Sys::Mmap => {
                for k in 1..=6 {
                    arg(self, k, VT::I64);
                }
                self.ins(Ins::Call(self.g.rt.mmap));
                self.put(i, VT::I64);
            }
            Sys::Munmap => {
                arg(self, 1, VT::I64);
                arg(self, 2, VT::I64);
                self.ins(Ins::Call(self.g.rt.munmap));
                self.put(i, VT::I64);
            }
            Sys::Futex => {
                arg(self, 1, VT::I64);
                arg(self, 2, VT::I64);
                arg(self, 3, VT::I64);
                self.ins(Ins::Call(self.g.rt.futex));
                self.put(i, VT::I64);
            }
            Sys::Constant(c) => {
                self.ins(Ins::I64Const(c));
                self.put(i, VT::I64);
            }
            Sys::Missing(_) => return Err("internal error: wasm32: a refused system call reached the translation".into()),
        }
        Ok(())
    }
}
