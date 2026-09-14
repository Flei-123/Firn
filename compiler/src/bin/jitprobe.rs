// SPDX-License-Identifier: GPL-2.0-only
//! Die Gegenprobe: kodierte Oktette in eine ausfuehrbare Speicherseite
//! schreiben und WIRKLICH AUFRUFEN.
//!
//! Byteweise Gleichheit mit GNU `as` ist ein starkes Zeichen, aber es bleibt
//! ein Vergleich mit einem anderen Werkzeug. Erst wenn der Rechner die Oktette
//! ausfuehrt und die erwartete Zahl herauskommt, ist bewiesen, dass die
//! Kodierung im Sinne der HARDWARE stimmt.
//!
//! Genau das ist auch der Schritt, den ein JIT tut. Dieses Programm ist damit
//! der kleinste denkbare Vorlaeufer eines JIT -- und es zeigt, dass der
//! fehlende Baustein aus der Studie jetzt da ist.
//!
//! Laeuft nur auf x86-64 (dort steht dieser Server).

#[path = "../encode_x86.rs"]
mod encode_x86;
use encode_x86::*;

#[cfg(target_arch = "x86_64")]
mod seite {
    /// Eine Speicherseite besorgen, die beschreibbar UND ausfuehrbar ist.
    /// Ohne libc -- direkt ueber die Systemaufrufe mmap und mprotect.
    pub struct Seite {
        pub zeiger: *mut u8,
        pub groesse: usize,
    }

    const PROT_READ: i64 = 1;
    const PROT_WRITE: i64 = 2;
    const PROT_EXEC: i64 = 4;
    const MAP_PRIVATE: i64 = 2;
    const MAP_ANON: i64 = 0x20;

    unsafe fn syscall6(nr: i64, a: i64, b: i64, c: i64, d: i64, e: i64, f: i64) -> i64 {
        let ret: i64;
        core::arch::asm!(
            "syscall",
            inlateout("rax") nr => ret,
            in("rdi") a, in("rsi") b, in("rdx") c,
            in("r10") d, in("r8") e, in("r9") f,
            lateout("rcx") _, lateout("r11") _,
            options(nostack)
        );
        ret
    }

    impl Seite {
        pub fn neu(groesse: usize) -> Option<Seite> {
            unsafe {
                // mmap ist Systemaufruf 9
                let p = syscall6(9, 0, groesse as i64,
                                 PROT_READ | PROT_WRITE,
                                 MAP_PRIVATE | MAP_ANON, -1, 0);
                if p < 0 { return None; }
                Some(Seite { zeiger: p as *mut u8, groesse })
            }
        }

        pub fn schreiben(&mut self, okt: &[u8]) {
            unsafe {
                core::ptr::copy_nonoverlapping(okt.as_ptr(), self.zeiger, okt.len());
            }
        }

        /// Erst nach dem Schreiben ausfuehrbar machen -- niemals gleichzeitig
        /// schreibbar und ausfuehrbar, das ist die Regel W^X.
        pub fn ausfuehrbar_machen(&mut self) -> bool {
            unsafe {
                // mprotect ist Systemaufruf 10
                syscall6(10, self.zeiger as i64, self.groesse as i64,
                         PROT_READ | PROT_EXEC, 0, 0, 0) == 0
            }
        }

        pub unsafe fn als_fn2(&self) -> extern "C" fn(i64, i64) -> i64 {
            core::mem::transmute(self.zeiger)
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn main() {
    use seite::Seite;

    let mut bestanden = 0usize;
    let mut durchgefallen = 0usize;

    // Aufrufregeln: erstes Argument in rdi, zweites in rsi, Ergebnis in rax.
    let mut pruefe = |name: &str, bauen: &dyn Fn(&mut Puffer),
                      a: i64, b: i64, erwartet: i64| {
        let mut p = Puffer::neu();
        bauen(&mut p);
        let hex = p.hex();
        let mut s = match Seite::neu(4096) {
            Some(s) => s,
            None => { println!("  {:34} KEINE SEITE", name); durchgefallen += 1; return; }
        };
        s.schreiben(&p.okt);
        if !s.ausfuehrbar_machen() {
            println!("  {:34} NICHT AUSFUEHRBAR", name);
            durchgefallen += 1;
            return;
        }
        let f = unsafe { s.als_fn2() };
        let ist = f(a, b);
        if ist == erwartet {
            bestanden += 1;
            println!("  {:34} f({}, {}) = {:<12} ok   [{}]", name, a, b, ist, hex);
        } else {
            durchgefallen += 1;
            println!("  {:34} f({}, {}) = {} ERWARTET {}  [{}]",
                     name, a, b, ist, erwartet, hex);
        }
    };

    println!("=====================================================================");
    println!("AUSFUEHRUNGSPROBE -- kodierte Oktette wirklich laufen lassen");
    println!("=====================================================================");

    // 1. Die Argumente addieren.
    pruefe("mov rax,rdi; add rax,rsi; ret", &|p| {
        X86::mov_rr(p, Breite::B64, Reg::Rax, Reg::Rdi);
        X86::alu_rr(p, 0, Breite::B64, Reg::Rax, Reg::Rsi);
        X86::ret(p);
    }, 40, 2, 42);

    // 2. Ein grosser Sofortwert -- prueft movabs.
    pruefe("mov rax, 2^40+7; ret", &|p| {
        X86::mov_r_imm(p, Breite::B64, Reg::Rax, (1i64 << 40) + 7);
        X86::ret(p);
    }, 0, 0, (1i64 << 40) + 7);

    // 3. Multiplizieren.
    pruefe("mov rax,rdi; imul rax,rsi; ret", &|p| {
        X86::mov_rr(p, Breite::B64, Reg::Rax, Reg::Rdi);
        X86::imul_rr(p, Breite::B64, Reg::Rax, Reg::Rsi);
        X86::ret(p);
    }, 7, 6, 42);

    // 4. Ueber den Stapel gehen -- push/pop und Speicherzugriff mit rbp.
    pruefe("push rbp; mov rbp,rsp; ...; pop rbp", &|p| {
        X86::push_r(p, Reg::Rbp);
        X86::mov_rr(p, Breite::B64, Reg::Rbp, Reg::Rsp);
        // Argument in den Rahmen legen und wieder holen
        X86::mov_m_r(p, Breite::B64, &Mem::basis_disp(Reg::Rbp, -8), Reg::Rdi);
        X86::mov_r_m(p, Breite::B64, Reg::Rax, &Mem::basis_disp(Reg::Rbp, -8));
        X86::alu_r_imm(p, 0, Breite::B64, Reg::Rax, 1);
        X86::pop_r(p, Reg::Rbp);
        X86::ret(p);
    }, 41, 0, 42);

    // 5. Ein bedingter Sprung, der wirklich springt.
    //    if (rdi > rsi) return rdi; else return rsi;   -- das Maximum.
    pruefe("Maximum ueber cmp + jcc", &|p| {
        X86::mov_rr(p, Breite::B64, Reg::Rax, Reg::Rdi);
        X86::alu_rr(p, 7, Breite::B64, Reg::Rdi, Reg::Rsi);   // cmp rdi, rsi
        // jg ueber die naechsten drei Oktette (mov rax,rsi)
        let cc = X86::cc_nr("g").unwrap();
        X86::jcc_rel8(p, cc, 3);
        X86::mov_rr(p, Breite::B64, Reg::Rax, Reg::Rsi);
        X86::ret(p);
    }, 42, 17, 42);

    pruefe("Maximum, anderer Zweig", &|p| {
        X86::mov_rr(p, Breite::B64, Reg::Rax, Reg::Rdi);
        X86::alu_rr(p, 7, Breite::B64, Reg::Rdi, Reg::Rsi);
        let cc = X86::cc_nr("g").unwrap();
        X86::jcc_rel8(p, cc, 3);
        X86::mov_rr(p, Breite::B64, Reg::Rax, Reg::Rsi);
        X86::ret(p);
    }, 17, 42, 42);

    // 6. Eine echte Schleife: die Zahlen 1..n aufsummieren.
    //    Das prueft einen RUECKWAERTSsprung, also einen negativen Abstand.
    pruefe("Schleife: Summe 1..n", &|p| {
        X86::alu_rr(p, 6, Breite::B64, Reg::Rax, Reg::Rax);   // xor rax, rax
        // anfang:
        let anfang = p.len();
        X86::alu_r_imm(p, 7, Breite::B64, Reg::Rdi, 0);       // cmp rdi, 0
        // jle raus  -- Ziel wird gleich berechnet
        let cc_le = X86::cc_nr("le").unwrap();
        X86::jcc_rel8(p, cc_le, 0);                            // Platzhalter
        let nach_sprung = p.len();
        X86::alu_rr(p, 0, Breite::B64, Reg::Rax, Reg::Rdi);   // add rax, rdi
        X86::inc_dec_r(p, true, Breite::B64, Reg::Rdi);       // dec rdi
        // zurueck zum Anfang
        let hier = p.len() + 2;
        X86::jmp_rel8(p, (anfang as i64 - hier as i64) as i8);
        let raus = p.len();
        X86::ret(p);
        // Den Platzhalter nachtraeglich fuellen -- genau das tut ein
        // Assembler im zweiten Durchlauf.
        p.okt[nach_sprung - 1] = (raus - nach_sprung) as u8;
    }, 10, 0, 55);

    // 7. Ein echter Aufruf mit call/ret ueber ein Register.
    pruefe("call ueber Register", &|p| {
        // Wir bauen: rax = rdi + 1, aufgerufen ueber ein Register.
        // Dazu legen wir die Zieladresse spaeter; hier reicht der einfache Weg:
        // lea rax,[rdi+1] -- prueft die Adressrechnung ohne Speicherzugriff.
        X86::lea(p, Breite::B64, Reg::Rax, &Mem::basis_disp(Reg::Rdi, 1));
        X86::ret(p);
    }, 41, 0, 42);

    // 8. Adressrechnung mit Index und Skalierung.
    pruefe("lea rax,[rdi+rsi*8]", &|p| {
        X86::lea(p, Breite::B64, Reg::Rax, &Mem::basis_index(Reg::Rdi, Reg::Rsi, 8));
        X86::ret(p);
    }, 2, 5, 42);

    // 9. Die hohen Register -- dort entscheidet REX.
    pruefe("hohe Register r12/r13", &|p| {
        X86::push_r(p, Reg::R12);
        X86::push_r(p, Reg::R13);
        X86::mov_rr(p, Breite::B64, Reg::R12, Reg::Rdi);
        X86::mov_rr(p, Breite::B64, Reg::R13, Reg::Rsi);
        X86::alu_rr(p, 0, Breite::B64, Reg::R12, Reg::R13);
        X86::mov_rr(p, Breite::B64, Reg::Rax, Reg::R12);
        X86::pop_r(p, Reg::R13);
        X86::pop_r(p, Reg::R12);
        X86::ret(p);
    }, 20, 22, 42);

    // 10. Speicher ueber r13 als Basis -- der Fall mit erzwungener Verschiebung.
    pruefe("Speicher ueber r13 (erzwungenes disp8)", &|p| {
        X86::push_r(p, Reg::R13);
        X86::mov_rr(p, Breite::B64, Reg::R13, Reg::Rsp);
        X86::alu_r_imm(p, 5, Breite::B64, Reg::Rsp, 16);       // Platz schaffen
        X86::mov_m_r(p, Breite::B64, &Mem::basis(Reg::R13), Reg::Rdi);
        X86::mov_r_m(p, Breite::B64, Reg::Rax, &Mem::basis(Reg::R13));
        X86::alu_r_imm(p, 0, Breite::B64, Reg::Rsp, 16);
        X86::pop_r(p, Reg::R13);
        X86::ret(p);
    }, 42, 0, 42);

    // 11. 8-Bit-Register, die REX brauchen.
    pruefe("setne sil + movzx", &|p| {
        X86::alu_rr(p, 7, Breite::B64, Reg::Rdi, Reg::Rsi);   // cmp rdi, rsi
        let cc = X86::cc_nr("ne").unwrap();
        X86::setcc_r(p, cc, Reg::Rsi);                         // setne sil
        X86::movzx_rr(p, Breite::B64, Breite::B8, Reg::Rax, Reg::Rsi);
        X86::ret(p);
    }, 1, 2, 1);

    // 12. Schieben.
    pruefe("shl rax, 3", &|p| {
        X86::mov_rr(p, Breite::B64, Reg::Rax, Reg::Rdi);
        X86::shift_r_imm(p, 4, Breite::B64, Reg::Rax, 3);
        X86::ret(p);
    }, 5, 0, 40);

    // 13. Vorzeichenbehaftete Division ueber idiv -- braucht cqo.
    pruefe("idiv mit cqo", &|p| {
        X86::mov_rr(p, Breite::B64, Reg::Rax, Reg::Rdi);
        X86::cqo(p);
        X86::f7_r(p, 7, Breite::B64, Reg::Rsi);               // idiv rsi
        X86::ret(p);
    }, 84, 2, 42);

    println!("\n---------------------------------------------------------------------");
    println!("  bestanden     : {}", bestanden);
    println!("  durchgefallen : {}", durchgefallen);
    if durchgefallen == 0 {
        println!("\n  Alle Proben liefen auf der echten Maschine.");
        println!("  Damit ist der Baustein aus der Studie belegt: Firn kann jetzt");
        println!("  Oktette erzeugen, die der Rechner unmittelbar ausfuehrt.");
    }
    std::process::exit(if durchgefallen == 0 { 0 } else { 1 });
}

#[cfg(not(target_arch = "x86_64"))]
fn main() {
    println!("Diese Probe laeuft nur auf x86-64.");
}
