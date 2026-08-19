//! **Runde 49 — Faeden.** Die drei Primitive, die Nebenlaeufigkeit ueberhaupt
//! erst moeglich machen. Alles andere (Stapel, Join, Mutex, Kanal, die
//! Fadensicherheit des Sammlers) steht als lesbares Firn in `lib/gc/gc.fi`.
//!
//! ```firn
//! __faden_starten(arg: u64, stapel: u64, ctid: *mut u8) -> i64
//! __faden_selbst() -> *mut u8
//! __atomar_tauschen(p: *mut u64, erwartet: u64, neu: u64) -> u64
//! ```
//!
//! ## Warum `clone(2)` und nicht pthreads
//!
//! Ein Firn-Programm ist **freistehend**: eigener `_start`, keine libc, kein
//! dynamischer Linker, jeder Systemdienst ueber `syscall` (SPEC §11).
//! `pthread_create` haette die ganze glibc hereingezogen — Initialisierung,
//! TLS-Modell, Signalbehandlung, `__libc_start_main` — und damit genau das
//! aufgegeben, was die Sprache ausmacht. `clone(2)` ist ein Systemaufruf wie
//! `mmap`; er kostet nichts ausser dieser Datei.
//!
//! ## Warum das ein eigenes Primitiv braucht und nicht `syscall(56, …)` reicht
//!
//! Das ist der Kern: `clone` kehrt **zweimal** zurueck — im Erzeuger mit der
//! Fadenkennung, im Kind mit 0. Das Kind kehrt aber mit einem **neuen `rsp`**
//! zurueck, waehrend `rbp` und alle callee-saved Register Kopien des Erzeugers
//! sind. Der vom Codegenerator erzeugte Code spricht seine Rahmenplaetze ueber
//! `[rbp-off]` an — das Kind wuerde also in den Rahmen des ERZEUGERS schreiben,
//! auf einem Stapel, der ihm nicht gehoert. Es gibt keine Formulierung in der
//! Sprache, die das vermeidet; der Uebergang muss in derselben
//! Instruktionsfolge stattfinden wie der Systemaufruf. Genau das ist
//! `Op::ThreadSpawn`: Systemaufruf, Verzweigung nach Rueckgabewert, im Kind
//! Argument vom neuen Stapel holen, Einstieg rufen, danach `exit(2)` — **nicht**
//! `exit_group(2)`, sonst nimmt ein endender Faden den ganzen Prozess mit.
//!
//! ## Warum ein TLS-Selbstzeiger
//!
//! Der Sammler muss an jeder Allokationsstelle wissen, welcher Faden gerade
//! alloziert (eigene Freiliste, eigener Graupuffer, eigener Stapelbereich).
//! `gettid(2)` waere ein Systemaufruf je Allokation. Der Kern kann dem Faden
//! stattdessen ein `fs`-Basisregister geben (`arch_prctl(ARCH_SET_FS)`); der
//! Blockkopf des Fadens traegt an Offset 0 seine eigene Adresse, und
//! `mov rax, fs:0` ist **eine** Instruktion ohne Speicherzugriff ausserhalb
//! der Cachezeile des Fadens.
//!
//! ## Warum ein Vergleichs-Tausch dazukommt
//!
//! `docs/RUNDE47.md` §3.2 nennt die Luecke beim Namen: mit `lock xadd` allein
//! laesst sich weder eine Sperre bauen (der Uebergang „frei -> belegt" muss
//! bedingt sein) noch `aufwerten_atomar` schliessen. `lock cmpxchg` ist die
//! kleinste Ergaenzung, die beides erledigt.

use crate::ast::Expr;
use crate::diag::Span;
use crate::fir::{FTy, Op, Val};
use crate::lower::Lower;
use crate::sema::Checker;
use crate::types::Type;

/// Faden erzeugen.
pub(crate) const START: &str = "__thread_start";
/// Eigener Fadenblock (TLS, `fs:0`).
pub(crate) const SELBST: &str = "__thread_self";
/// Atomarer Vergleichs-Tausch.
pub(crate) const CAS: &str = "__atomic_swap";

/// Name der Einstiegsfunktion, die das Kind ruft. Sie steht in der
/// Sammler-Laufzeit (`lib/gc/gc.fi`) und bekommt den Fadenblock als einziges
/// Argument. Ein Funktionszeiger waere die Alternative; Stufe 0 hat keine
/// (dieselbe Entscheidung wie beim Verteiler der Finalisierer, Runde 47).
pub(crate) const EINSTIEG: &str = "__thread_entry";

/// `clone(2)`-Merker: geteilter Adressraum, geteilte Dateien, echter Faden
/// derselben Fadengruppe, und die beiden TID-Merker, aus denen `faden_warten`
/// gebaut ist (`CLONE_CHILD_CLEARTID` laesst den Kern beim Fadenende das Wort
/// nullen und darauf aufwecken — genau der Mechanismus von `pthread_join`).
pub(crate) const CLONE_FLAGS: u64 = 0x0000_0100  // CLONE_VM
    | 0x0000_0200                                 // CLONE_FS
    | 0x0000_0400                                 // CLONE_FILES
    | 0x0000_0800                                 // CLONE_SIGHAND
    | 0x0001_0000                                 // CLONE_THREAD
    | 0x0004_0000                                 // CLONE_SYSVSEM
    | 0x0010_0000                                 // CLONE_PARENT_SETTID
    | 0x0020_0000; // CLONE_CHILD_CLEARTID

/// Ist `name` eines der drei Primitive?
pub(crate) fn is_thread_call(name: &str) -> bool {
    name == START || name == SELBST || name == CAS
}

// ------------------------------------------------------------------- Typphase

/// Hook aus `sema::call`. `None`, wenn es keines der Primitive ist oder im
/// Programm eine gleichnamige Funktion steht — die gewinnt dann.
pub(crate) fn hook_call(
    ck: &mut Checker,
    name: &str,
    args: &[Expr],
    nspan: Span,
    espan: Span,
) -> Option<Type> {
    if !is_thread_call(name) || ck.fns.contains_key(name) {
        return None;
    }
    let _ = nspan;
    match name {
        SELBST => {
            if !args.is_empty() {
                for a in args {
                    ck.type_out_expr(a);
                }
                ck.dg.error_note(
                    espan,
                    format!("'{}' erwartet keine argumente, gefunden {}", SELBST, args.len()),
                    "die form ist __faden_selbst() -> *mut u8",
                );
                return Some(Type::Error);
            }
            Some(Type::ptr(Type::U8, true))
        }
        START => {
            if args.len() != 3 {
                for a in args {
                    ck.type_out_expr(a);
                }
                ck.dg.error_note(
                    espan,
                    format!(
                        "'{}' erwartet genau drei argumente (argument, stapel, tidwort), gefunden {}",
                        START,
                        args.len()
                    ),
                    "die form ist __faden_starten(arg: u64, stapel: u64, ctid: *mut u8) -> i64",
                );
                return Some(Type::Error);
            }
            let at = ck.expr(&args[0], Some(&Type::U64));
            let st = ck.expr(&args[1], Some(&Type::U64));
            let ct = ck.expr(&args[2], Some(&Type::ptr(Type::U8, true)));
            if !at.is_error() && !fits_as_u64(&at) {
                ck.dg.error(
                    args[0].span,
                    format!("'{}' erwartet als argument einen u64, gefunden {}", START, ck.tcx.name_of(&at)),
                );
                return Some(Type::Error);
            }
            if !st.is_error() && !fits_as_u64(&st) {
                ck.dg.error_note(
                    args[1].span,
                    format!(
                        "'{}' erwartet als stapel einen u64, gefunden {}",
                        START,
                        ck.tcx.name_of(&st)
                    ),
                    "der stapel ist die OBERE adresse des fadenstapels (er waechst nach unten)",
                );
                return Some(Type::Error);
            }
            if !ct.is_error() && !is_ptr(&ct) {
                ck.dg.error_note(
                    args[2].span,
                    format!(
                        "'{}' erwartet als drittes argument einen zeiger auf das tidwort, gefunden {}",
                        START,
                        ck.tcx.name_of(&ct)
                    ),
                    "der kern schreibt dort die fadenkennung hin und nullt sie beim fadenende",
                );
                return Some(Type::Error);
            }
            Some(Type::I64)
        }
        _ => {
            // CAS
            if args.len() != 3 {
                for a in args {
                    ck.type_out_expr(a);
                }
                ck.dg.error_note(
                    espan,
                    format!(
                        "'{}' erwartet genau drei argumente (zeiger, erwartet, neu), gefunden {}",
                        CAS,
                        args.len()
                    ),
                    "die form ist __atomar_tauschen(p: *mut u64, erwartet: u64, neu: u64) -> u64",
                );
                return Some(Type::Error);
            }
            let pt = ck.expr(&args[0], Some(&Type::ptr(Type::U64, true)));
            let et = ck.expr(&args[1], Some(&Type::U64));
            let nt = ck.expr(&args[2], Some(&Type::U64));
            if !pt.is_error() && !is_u64_ptr(&pt) {
                ck.dg.error_note(
                    args[0].span,
                    format!(
                        "'{}' erwartet als erstes argument einen *mut u64, gefunden {}",
                        CAS,
                        ck.tcx.name_of(&pt)
                    ),
                    "atomar getauscht wird genau ein 64-bit-wort",
                );
                return Some(Type::Error);
            }
            for (t, i) in [(&et, 1usize), (&nt, 2usize)] {
                if !t.is_error() && !fits_as_u64(t) {
                    ck.dg.error(
                        args[i].span,
                        format!("'{}' erwartet einen u64, gefunden {}", CAS, ck.tcx.name_of(t)),
                    );
                    return Some(Type::Error);
                }
            }
            Some(Type::U64)
        }
    }
}

fn is_u64_ptr(t: &Type) -> bool {
    match t {
        Type::Ptr { inner, .. } => **inner == Type::U64,
        _ => false,
    }
}

fn is_ptr(t: &Type) -> bool {
    matches!(t, Type::Ptr { .. })
}

fn fits_as_u64(t: &Type) -> bool {
    matches!(t, Type::U64 | Type::UntypedInt | Type::I64 | Type::Usize)
}

// ---------------------------------------------------------------- Lowerphase

/// Hook aus `lower::lower_call`.
pub(crate) fn lower_thread_call(
    lo: &mut Lower,
    name: &str,
    args: &[Expr],
    span: Span,
) -> Option<Option<Val>> {
    match name {
        SELBST => Some(Some(lo.push(FTy::Ptr, Op::ThreadSelf))),
        START => {
            if args.len() != 3 {
                return lo.ice(span, "faden-primitiv mit falscher stellenzahl");
            }
            let a = lo.lower_expr(&args[0])?;
            let s = lo.lower_expr(&args[1])?;
            let c = lo.lower_expr(&args[2])?;
            Some(Some(lo.push(FTy::I64, Op::ThreadSpawn { arg: a, stack: s, ctid: c })))
        }
        _ => {
            if args.len() != 3 {
                return lo.ice(span, "atomar-tausch mit falscher stellenzahl");
            }
            let p = lo.lower_expr(&args[0])?;
            let e = lo.lower_expr(&args[1])?;
            let n = lo.lower_expr(&args[2])?;
            Some(Some(lo.push(FTy::U64, Op::AtomicCas { addr: p, erw: e, new: n })))
        }
    }
}

// -------------------------------------------------------------- Codegenerator

/// Die Instruktionsfolge fuer `Op::ThreadSpawn`. Vorbedingung: `rdi` = Argument,
/// `rsi` = obere Stapeladresse, `rdx` = Zeiger auf das TID-Wort. Nachbedingung:
/// `rax` = Fadenkennung (> 0) bzw. negativer Fehlerwert.
///
/// Die lokale Marke `1:` ist eine **numerische** Marke des Assemblers: `jnz 1f`
/// springt zur naechsten `1:` nach vorn. Damit braucht diese Folge keinen
/// Zaehler und kann beliebig oft im selben Modul stehen.
pub(crate) fn spawn_sequence(e: &mut crate::codegen_x86::Emitter) {
    // Argument auf den KINDstapel legen — im Kind ist `rdi` mit den Merkern
    // ueberschrieben, und einen anderen Weg an den Wert gibt es nicht.
    // 16 Byte, damit der Stapel ausgerichtet bleibt (SysV: an der `call`-Stelle
    // 16-fach ausgerichtet).
    e.line("sub rsi, 16");
    e.line("mov qword ptr [rsi], rdi");
    e.line("mov r10, rdx");
    e.line(&format!("mov rdi, {}", CLONE_FLAGS));
    e.line("xor r8d, r8d");
    e.line("mov eax, 56");
    e.line("syscall");
    e.line("test rax, rax");
    e.line("jnz 1f");
    // ---- Kind: eigener Stapel, `rbp` neu, Argument zurueckholen ----------
    e.line("mov rdi, qword ptr [rsp]");
    e.line("add rsp, 16");
    e.line("xor ebp, ebp");
    e.line(&format!("call {}", crate::codegen_x86::label(EINSTIEG)));
    // exit(2), NICHT exit_group(2): nur dieser Faden endet.
    e.line("mov edi, eax");
    e.line("mov eax, 60");
    e.line("syscall");
    e.line("ud2");
    e.raw("1:");
}

/// Die Instruktionsfolge fuer `Op::AtomicCas`. Vorbedingung: `rcx` = Adresse,
/// `rax` = erwarteter Wert, `rdx` = neuer Wert. Nachbedingung: `rax` = der
/// vorgefundene Wert (gleich `erwartet`, wenn der Tausch stattfand).
pub(crate) fn cas_sequence(e: &mut crate::codegen_x86::Emitter) {
    e.line("lock cmpxchg qword ptr [rcx], rdx");
}

/// Die Instruktionsfolge fuer `Op::ThreadSelf`: der Selbstzeiger aus dem
/// Fadenblock. Vorbedingung: keine. Nachbedingung: `rax` = Fadenblock.
pub(crate) fn self_sequence(e: &mut crate::codegen_x86::Emitter) {
    e.line("mov rax, qword ptr fs:0");
}
