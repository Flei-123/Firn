//! **RUNDE KODIERER — der ELF-Schreiber.**
//!
//! Nimmt das Ergebnis von `asm_x86.rs` bzw. `asm_a64.rs` und schreibt eine
//! verschiebbare Objektdatei (ELF64, `ET_REL`), wie `as` sie liefert. Damit
//! endet die Abhängigkeit von `as`; `ld` bleibt vorerst.
//!
//! Aufbau, in dieser Reihenfolge:
//!
//! ```text
//!   ELF-Kopf
//!   Inhalte:  .text .data .rodata .note.GNU-stack .symtab .strtab
//!             .rela.text .rela.data .rela.rodata .shstrtab
//!   Abschnittstabelle
//! ```
//!
//! `.bss` steht als leerer `NOBITS`-Abschnitt drin, weil `as` ihn auch
//! anlegt und manche Binder ihn erwarten.
//!
//! Die Symboltabelle hält die Reihenfolge ein, die die ABI verlangt: erst
//! alle **lokalen** Symbole (dazu gehören die Abschnittssymbole), dann die
//! **globalen**; `sh_info` des `.symtab` zeigt auf das erste globale. Wer
//! das vertauscht, bekommt von `ld` keinen Fehler, sondern falsch
//! aufgelöste Symbole.

#![allow(dead_code)]

/// Ein Abschnitt, wie ihn der Schreiber annimmt.
pub struct OutSec {
    pub name: String,
    pub bytes: Vec<u8>,
    pub align: u64,
    /// sh_type
    pub kind: u32,
    /// sh_flags
    pub flags: u64,
    pub relocs: Vec<OutReloc>,
}

pub struct OutReloc {
    pub offset: u64,
    /// Nummer in `syms`.
    pub sym: usize,
    pub kind: u32,
    pub addend: i64,
}

pub struct OutSym {
    pub name: String,
    /// Nummer des Abschnitts in `secs`, oder `None` für undefiniert.
    pub section: Option<usize>,
    pub value: u64,
    pub global: bool,
    pub is_section: bool,
}

const SHT_PROGBITS: u32 = 1;
const SHT_SYMTAB: u32 = 2;
const SHT_STRTAB: u32 = 3;
const SHT_RELA: u32 = 4;
const SHT_NOBITS: u32 = 8;

const SHF_WRITE: u64 = 0x1;
const SHF_ALLOC: u64 = 0x2;
const SHF_EXECINSTR: u64 = 0x4;

pub const EM_X86_64: u16 = 62;
pub const EM_AARCH64: u16 = 183;

struct StrTab {
    buf: Vec<u8>,
}
impl StrTab {
    fn new() -> StrTab {
        StrTab { buf: vec![0] }
    }
    fn add(&mut self, s: &str) -> u32 {
        if s.is_empty() {
            return 0;
        }
        let at = self.buf.len() as u32;
        self.buf.extend_from_slice(s.as_bytes());
        self.buf.push(0);
        at
    }
}

fn align_to(v: usize, a: usize) -> usize {
    if a <= 1 {
        v
    } else {
        v.div_ceil(a) * a
    }
}

/// Schreibt die Objektdatei.
///
/// `secs` sind die Inhaltsabschnitte in der Reihenfolge, in der sie in der
/// Datei stehen sollen; `syms[0]` muss **nicht** das Nullsymbol sein, das
/// setzt der Schreiber selbst davor.
pub fn write(machine: u16, secs: &[OutSec], syms: &[OutSym]) -> Vec<u8> {
    // --- Symbole ordnen: lokal zuerst -------------------------------
    let mut order: Vec<usize> = (0..syms.len()).collect();
    order.sort_by_key(|&i| syms[i].global as u8);
    // Rückabbildung: alte Nummer -> Platz in der Tabelle (+1 für das
    // Nullsymbol am Anfang).
    let mut place = vec![0usize; syms.len()];
    for (newpos, &old) in order.iter().enumerate() {
        place[old] = newpos + 1;
    }
    let first_global = order.iter().position(|&i| syms[i].global).unwrap_or(order.len()) + 1;

    // --- Abschnittsnummern ------------------------------------------
    // 0 = null, dann die Inhaltsabschnitte, dann .symtab .strtab und die
    // .rela.* und zuletzt .shstrtab.
    let n_content = secs.len();
    let idx_symtab = 1 + n_content;
    let idx_strtab = idx_symtab + 1;
    let mut rela_of: Vec<Option<usize>> = vec![None; n_content];
    let mut next = idx_strtab + 1;
    for (i, s) in secs.iter().enumerate() {
        if !s.relocs.is_empty() {
            rela_of[i] = Some(next);
            next += 1;
        }
    }
    let idx_shstrtab = next;
    let n_sections = idx_shstrtab + 1;

    // --- Zeichenketten ----------------------------------------------
    let mut shstr = StrTab::new();
    let mut sec_name_off = Vec::new();
    for s in secs {
        sec_name_off.push(shstr.add(&s.name));
    }
    let off_symtab_name = shstr.add(".symtab");
    let off_strtab_name = shstr.add(".strtab");
    let mut rela_name_off: Vec<u32> = vec![0; n_content];
    for (i, s) in secs.iter().enumerate() {
        if rela_of[i].is_some() {
            rela_name_off[i] = shstr.add(&format!(".rela{}", s.name));
        }
    }
    let off_shstrtab_name = shstr.add(".shstrtab");

    let mut strtab = StrTab::new();
    let mut sym_name_off = vec![0u32; syms.len()];
    for &old in &order {
        sym_name_off[old] = if syms[old].is_section {
            0
        } else {
            strtab.add(&syms[old].name)
        };
    }

    // --- symtab bauen -----------------------------------------------
    let mut symtab = Vec::new();
    symtab.extend_from_slice(&[0u8; 24]); // Nullsymbol
    for &old in &order {
        let s = &syms[old];
        let info = ((if s.global { 1u8 } else { 0 }) << 4)
            | (if s.is_section { 3u8 } else { 0 });
        let shndx: u16 = match s.section {
            Some(i) => (i + 1) as u16,
            None => 0,
        };
        symtab.extend_from_slice(&sym_name_off[old].to_le_bytes());
        symtab.push(info);
        symtab.push(0); // st_other
        symtab.extend_from_slice(&shndx.to_le_bytes());
        symtab.extend_from_slice(&s.value.to_le_bytes());
        symtab.extend_from_slice(&0u64.to_le_bytes()); // st_size
    }

    // --- rela-Tabellen ----------------------------------------------
    let mut rela_bytes: Vec<Vec<u8>> = Vec::new();
    for s in secs.iter() {
        let mut b = Vec::new();
        for r in &s.relocs {
            let info = ((place[r.sym] as u64) << 32) | (r.kind as u64);
            b.extend_from_slice(&r.offset.to_le_bytes());
            b.extend_from_slice(&info.to_le_bytes());
            b.extend_from_slice(&(r.addend as u64).to_le_bytes());
        }
        rela_bytes.push(b);
    }

    // --- Dateiaufbau ------------------------------------------------
    let mut file: Vec<u8> = Vec::new();
    file.resize(64, 0); // ELF-Kopf

    let mut sec_off = vec![0usize; n_content];
    for (i, s) in secs.iter().enumerate() {
        if s.kind == SHT_NOBITS {
            sec_off[i] = file.len();
            continue;
        }
        let a = align_to(file.len(), s.align.max(1) as usize);
        file.resize(a, 0);
        sec_off[i] = file.len();
        file.extend_from_slice(&s.bytes);
    }
    let a = align_to(file.len(), 8);
    file.resize(a, 0);
    let symtab_off = file.len();
    file.extend_from_slice(&symtab);
    let strtab_off = file.len();
    file.extend_from_slice(&strtab.buf);
    let mut rela_off = vec![0usize; n_content];
    for (i, b) in rela_bytes.iter().enumerate() {
        if rela_of[i].is_none() {
            continue;
        }
        let a = align_to(file.len(), 8);
        file.resize(a, 0);
        rela_off[i] = file.len();
        file.extend_from_slice(b);
    }
    let shstrtab_off = file.len();
    file.extend_from_slice(&shstr.buf);
    let a = align_to(file.len(), 8);
    file.resize(a, 0);
    let shoff = file.len();

    // --- Abschnittstabelle ------------------------------------------
    let mut sh = Vec::new();
    let push_sh = |sh: &mut Vec<u8>,
                   name: u32,
                   typ: u32,
                   flags: u64,
                   addr: u64,
                   off: u64,
                   size: u64,
                   link: u32,
                   info: u32,
                   align: u64,
                   entsize: u64| {
        sh.extend_from_slice(&name.to_le_bytes());
        sh.extend_from_slice(&typ.to_le_bytes());
        sh.extend_from_slice(&flags.to_le_bytes());
        sh.extend_from_slice(&addr.to_le_bytes());
        sh.extend_from_slice(&off.to_le_bytes());
        sh.extend_from_slice(&size.to_le_bytes());
        sh.extend_from_slice(&link.to_le_bytes());
        sh.extend_from_slice(&info.to_le_bytes());
        sh.extend_from_slice(&align.to_le_bytes());
        sh.extend_from_slice(&entsize.to_le_bytes());
    };
    push_sh(&mut sh, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
    for (i, s) in secs.iter().enumerate() {
        push_sh(
            &mut sh,
            sec_name_off[i],
            s.kind,
            s.flags,
            0,
            sec_off[i] as u64,
            s.bytes.len() as u64,
            0,
            0,
            s.align.max(1),
            0,
        );
    }
    push_sh(
        &mut sh,
        off_symtab_name,
        SHT_SYMTAB,
        0,
        0,
        symtab_off as u64,
        symtab.len() as u64,
        idx_strtab as u32,
        first_global as u32,
        8,
        24,
    );
    push_sh(
        &mut sh,
        off_strtab_name,
        SHT_STRTAB,
        0,
        0,
        strtab_off as u64,
        strtab.buf.len() as u64,
        0,
        0,
        1,
        0,
    );
    for (i, _) in secs.iter().enumerate() {
        if rela_of[i].is_none() {
            continue;
        }
        push_sh(
            &mut sh,
            rela_name_off[i],
            SHT_RELA,
            0,
            0,
            rela_off[i] as u64,
            rela_bytes[i].len() as u64,
            idx_symtab as u32,
            (i + 1) as u32,
            8,
            24,
        );
    }
    push_sh(
        &mut sh,
        off_shstrtab_name,
        SHT_STRTAB,
        0,
        0,
        shstrtab_off as u64,
        shstr.buf.len() as u64,
        0,
        0,
        1,
        0,
    );
    file.extend_from_slice(&sh);

    // --- ELF-Kopf ----------------------------------------------------
    let hdr = &mut file[..64];
    hdr[0..4].copy_from_slice(&[0x7F, b'E', b'L', b'F']);
    hdr[4] = 2; // ELFCLASS64
    hdr[5] = 1; // ELFDATA2LSB
    hdr[6] = 1; // EV_CURRENT
    hdr[7] = 0; // ELFOSABI_NONE
    hdr[16..18].copy_from_slice(&1u16.to_le_bytes()); // ET_REL
    hdr[18..20].copy_from_slice(&machine.to_le_bytes());
    hdr[20..24].copy_from_slice(&1u32.to_le_bytes()); // EV_CURRENT
    hdr[40..48].copy_from_slice(&(shoff as u64).to_le_bytes());
    hdr[52..54].copy_from_slice(&64u16.to_le_bytes()); // e_ehsize
    hdr[58..60].copy_from_slice(&64u16.to_le_bytes()); // e_shentsize
    hdr[60..62].copy_from_slice(&(n_sections as u16).to_le_bytes());
    hdr[62..64].copy_from_slice(&(idx_shstrtab as u16).to_le_bytes());

    file
}

/// Die vier Abschnitte, die Firn erzeugt, mit den Merkmalen, die `as` ihnen
/// gibt. `bss` ist immer leer, steht aber drin.
pub fn standard_sections(
    text: Vec<u8>,
    data: Vec<u8>,
    rodata: Vec<u8>,
    aligns: [u64; 3],
) -> Vec<OutSec> {
    standard_sections_bss(text, data, rodata, 0, aligns)
}

/// Wie [`standard_sections`], aber mit einer `.bss` von `bss_size` Oktetten.
pub fn standard_sections_bss(
    text: Vec<u8>,
    data: Vec<u8>,
    rodata: Vec<u8>,
    bss_size: usize,
    aligns: [u64; 3],
) -> Vec<OutSec> {
    vec![
        OutSec {
            name: ".text".into(),
            bytes: text,
            align: aligns[0].max(1),
            kind: SHT_PROGBITS,
            flags: SHF_ALLOC | SHF_EXECINSTR,
            relocs: Vec::new(),
        },
        OutSec {
            name: ".data".into(),
            bytes: data,
            align: aligns[1].max(1),
            kind: SHT_PROGBITS,
            flags: SHF_ALLOC | SHF_WRITE,
            relocs: Vec::new(),
        },
        OutSec {
            name: ".bss".into(),
            bytes: vec![0u8; bss_size],
            align: 1,
            kind: SHT_NOBITS,
            flags: SHF_ALLOC | SHF_WRITE,
            relocs: Vec::new(),
        },
        OutSec {
            name: ".rodata".into(),
            bytes: rodata,
            align: aligns[2].max(1),
            kind: SHT_PROGBITS,
            flags: SHF_ALLOC,
            relocs: Vec::new(),
        },
        OutSec {
            name: ".note.GNU-stack".into(),
            bytes: Vec::new(),
            align: 1,
            kind: SHT_PROGBITS,
            flags: 0,
            relocs: Vec::new(),
        },
    ]
}
