//! x86_64-Codegenerator: FIR -> GNU-Assembler-Text (AT&T-Syntax) fuer `as`/`ld`.
//! Kein LLVM, kein Cranelift, kein C.
//!
//! SCHNITTSTELLE (fest):
//!   `pub fn emit(m: &fir::Module) -> String`
//! Erzeugt ein vollstaendiges Assemblermodul inklusive `_start`, das `main`
//! aufruft und dessen Rueckgabewert an den `exit`-Syscall gibt (freistehend,
//! ohne libc). System-V-AMD64-ABI, 16-Byte-Stackausrichtung an jeder Aufrufstelle.

use crate::fir::Module;

/// STUB — wird von Modul "codegen" implementiert.
pub fn emit(m: &Module) -> String {
    let _ = m;
    String::from("# Codegen ist in diesem Baustand noch nicht implementiert\n")
}
