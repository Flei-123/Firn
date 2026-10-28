use crate::elfobj::{OutReloc, OutSec};

const LINE_BASE: i64 = -5;
const LINE_RANGE: i64 = 14;
const OPCODE_BASE: i64 = 13;
const MAX_SPECIAL_ADDR_DELTA: u64 = ((255 - OPCODE_BASE) / LINE_RANGE) as u64;

const DW_LNS_COPY: u8 = 1;
const DW_LNS_ADVANCE_PC: u8 = 2;
const DW_LNS_ADVANCE_LINE: u8 = 3;
const DW_LNS_SET_FILE: u8 = 4;
const DW_LNS_SET_COLUMN: u8 = 5;
const DW_LNS_CONST_ADD_PC: u8 = 8;

const R_X86_64_64: u32 = 1;
const R_X86_64_32: u32 = 10;

const SHT_PROGBITS: u32 = 1;

const ABBREV: [u8; 20] = [
    0x01, 0x11, 0x00, 0x10, 0x06, 0x11, 0x01, 0x12, 0x01, 0x03, 0x0e, 0x1b, 0x0e, 0x25, 0x0e,
    0x13, 0x05, 0x00, 0x00, 0x00,
];

pub struct LineEntry {
    pub addr: u64,
    pub file: u32,
    pub line: u32,
    pub col: u32,
}

pub struct SourceFile {
    pub number: u32,
    pub path: String,
}

pub struct DebugSyms {
    pub text: usize,
    pub line: usize,
    pub info: usize,
    pub abbrev: usize,
    pub strings: usize,
}

pub struct DebugParams<'a> {
    pub files: &'a [SourceFile],
    pub entries: &'a [LineEntry],
    pub text_len: u64,
    pub comp_dir: &'a str,
    pub producer: &'a str,
    pub syms: DebugSyms,
}

fn put_uleb(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

fn put_sleb(out: &mut Vec<u8>, mut v: i64) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        let done = (v == 0 && b & 0x40 == 0) || (v == -1 && b & 0x40 != 0);
        if done {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

fn advance(out: &mut Vec<u8>, line_delta: i64, addr_delta: u64) {
    let mut line_delta = line_delta;
    let mut need_copy = false;
    let mut tmp = line_delta - LINE_BASE;
    if !(0..LINE_RANGE).contains(&tmp) {
        out.push(DW_LNS_ADVANCE_LINE);
        put_sleb(out, line_delta);
        line_delta = 0;
        tmp = -LINE_BASE;
        need_copy = true;
    }
    if line_delta == 0 && addr_delta == 0 {
        out.push(DW_LNS_COPY);
        return;
    }
    tmp += OPCODE_BASE;
    if addr_delta < 256 + MAX_SPECIAL_ADDR_DELTA {
        let opcode = tmp + addr_delta as i64 * LINE_RANGE;
        if opcode <= 255 {
            out.push(opcode as u8);
            return;
        }
        let opcode = tmp + (addr_delta - MAX_SPECIAL_ADDR_DELTA) as i64 * LINE_RANGE;
        if opcode <= 255 {
            out.push(DW_LNS_CONST_ADD_PC);
            out.push(opcode as u8);
            return;
        }
    }
    out.push(DW_LNS_ADVANCE_PC);
    put_uleb(out, addr_delta);
    if need_copy {
        out.push(DW_LNS_COPY);
    } else {
        out.push(tmp as u8);
    }
}

fn end_sequence(out: &mut Vec<u8>, addr_delta: u64) {
    if addr_delta == MAX_SPECIAL_ADDR_DELTA {
        out.push(DW_LNS_CONST_ADD_PC);
    } else if addr_delta != 0 {
        out.push(DW_LNS_ADVANCE_PC);
        put_uleb(out, addr_delta);
    }
    out.extend_from_slice(&[0, 1, 1]);
}

fn split_path(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        Some(i) => (&path[..i], &path[i + 1..]),
        None => ("", path),
    }
}

fn line_program(p: &DebugParams) -> (Vec<u8>, Vec<OutReloc>) {
    let mut dirs: Vec<&str> = Vec::new();
    let mut files: Vec<(&str, usize, u32)> = Vec::new();
    for f in p.files {
        let (dir, name) = split_path(&f.path);
        let dir_index = if dir.is_empty() {
            0
        } else {
            match dirs.iter().position(|d| *d == dir) {
                Some(i) => i + 1,
                None => {
                    dirs.push(dir);
                    dirs.len()
                }
            }
        };
        files.push((name, dir_index, f.number));
    }
    files.sort_by_key(|f| f.2);

    let mut header = vec![1u8, 1, LINE_BASE as i8 as u8, LINE_RANGE as u8, OPCODE_BASE as u8];
    header.extend_from_slice(&[0, 1, 1, 1, 1, 0, 0, 0, 1, 0, 0, 1]);
    for d in &dirs {
        header.extend_from_slice(d.as_bytes());
        header.push(0);
    }
    header.push(0);
    for (name, dir, _) in &files {
        header.extend_from_slice(name.as_bytes());
        header.push(0);
        put_uleb(&mut header, *dir as u64);
        header.push(0);
        header.push(0);
    }
    header.push(0);

    let mut prog: Vec<u8> = Vec::new();
    let mut relocs = Vec::new();
    let mut file = 1u32;
    let mut line = 1i64;
    let mut col = 0u32;
    let mut last: Option<u64> = None;
    let base = 4 + 2 + 4 + header.len();
    for e in p.entries {
        if e.file != file {
            prog.push(DW_LNS_SET_FILE);
            put_uleb(&mut prog, e.file as u64);
            file = e.file;
        }
        if e.col != col {
            prog.push(DW_LNS_SET_COLUMN);
            put_uleb(&mut prog, e.col as u64);
            col = e.col;
        }
        let delta = e.line as i64 - line;
        match last {
            None => {
                prog.extend_from_slice(&[0, 9, 2]);
                relocs.push(OutReloc {
                    offset: (base + prog.len()) as u64,
                    sym: p.syms.text,
                    kind: R_X86_64_64,
                    addend: e.addr as i64,
                });
                prog.extend_from_slice(&[0u8; 8]);
                advance(&mut prog, delta, 0);
            }
            Some(a) => advance(&mut prog, delta, e.addr - a),
        }
        line = e.line as i64;
        last = Some(e.addr);
    }
    end_sequence(&mut prog, p.text_len - last.unwrap_or(0));

    let mut out = Vec::with_capacity(base + prog.len());
    out.extend_from_slice(&((2 + 4 + header.len() + prog.len()) as u32).to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes());
    out.extend_from_slice(&((header.len()) as u32).to_le_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(&prog);
    (out, relocs)
}

fn section(name: &str, bytes: Vec<u8>, align: u64, relocs: Vec<OutReloc>) -> OutSec {
    OutSec { name: name.into(), bytes, align, kind: SHT_PROGBITS, flags: 0, relocs }
}

pub fn plain_section(name: &str, bytes: Vec<u8>) -> OutSec {
    section(name, bytes, 1, Vec::new())
}

pub fn line_section(p: &DebugParams) -> OutSec {
    let (bytes, relocs) = line_program(p);
    section(".debug_line", bytes, 1, relocs)
}

fn rel(offset: u64, sym: usize, kind: u32, addend: i64) -> OutReloc {
    OutReloc { offset, sym, kind, addend }
}

pub fn sections(p: &DebugParams) -> Vec<OutSec> {
    let (line_bytes, line_relocs) = line_program(p);
    let name = p.files.iter().find(|f| f.number == 1).map_or("", |f| f.path.as_str());
    let mut strings = Vec::new();
    let name_off = strings.len() as i64;
    strings.extend_from_slice(name.as_bytes());
    strings.push(0);
    let dir_off = strings.len() as i64;
    strings.extend_from_slice(p.comp_dir.as_bytes());
    strings.push(0);
    let producer_off = strings.len() as i64;
    strings.extend_from_slice(p.producer.as_bytes());
    strings.push(0);

    let mut info = Vec::new();
    info.extend_from_slice(&42u32.to_le_bytes());
    info.extend_from_slice(&3u16.to_le_bytes());
    info.extend_from_slice(&0u32.to_le_bytes());
    info.push(8);
    info.push(1);
    info.extend_from_slice(&[0u8; 4 + 8 + 8 + 4 + 4 + 4]);
    info.extend_from_slice(&0x8001u16.to_le_bytes());
    let s = &p.syms;
    let info_relocs = vec![
        rel(6, s.abbrev, R_X86_64_32, 0),
        rel(12, s.line, R_X86_64_32, 0),
        rel(16, s.text, R_X86_64_64, 0),
        rel(24, s.text, R_X86_64_64, p.text_len as i64),
        rel(32, s.strings, R_X86_64_32, name_off),
        rel(36, s.strings, R_X86_64_32, dir_off),
        rel(40, s.strings, R_X86_64_32, producer_off),
    ];

    let mut aranges = Vec::new();
    aranges.extend_from_slice(&0x2cu32.to_le_bytes());
    aranges.extend_from_slice(&2u16.to_le_bytes());
    aranges.extend_from_slice(&0u32.to_le_bytes());
    aranges.extend_from_slice(&[8, 0, 0, 0, 0, 0]);
    aranges.extend_from_slice(&0u64.to_le_bytes());
    aranges.extend_from_slice(&p.text_len.to_le_bytes());
    aranges.extend_from_slice(&[0u8; 16]);
    let aranges_relocs = vec![
        rel(6, s.info, R_X86_64_32, 0),
        rel(16, s.text, R_X86_64_64, 0),
    ];

    vec![
        section(".debug_line", line_bytes, 1, line_relocs),
        section(".debug_info", info, 1, info_relocs),
        section(".debug_abbrev", ABBREV.to_vec(), 1, Vec::new()),
        section(".debug_aranges", aranges, 16, aranges_relocs),
        section(".debug_str", strings, 1, Vec::new()),
    ]
}
