// SPDX-License-Identifier: GPL-2.0-only
//! Differenzpruefer ARM64: meine Kodierung gegen GNU `as`, Wort fuer Wort.

use std::collections::BTreeMap;
use std::env;
use std::fs;

#[path = "../encode_a64.rs"]
mod encode_a64;
use encode_a64::*;

struct Fall {
    form: String,
    text: String,
    hex: Option<String>,
    anm: Option<String>,
}

fn faelle_lesen(pfad: &str) -> Vec<Fall> {
    let roh = fs::read_to_string(pfad).expect("Wahrheitsdatei nicht lesbar");
    let mut faelle = Vec::new();
    let b = roh.as_bytes();
    let mut i = 0usize;
    while i < b.len() {
        if b[i] != b'{' { i += 1; continue; }
        let start = i;
        let mut tiefe = 0;
        let mut in_str = false;
        let mut esc = false;
        while i < b.len() {
            let c = b[i];
            if in_str {
                if esc { esc = false; }
                else if c == b'\\' { esc = true; }
                else if c == b'"' { in_str = false; }
            } else if c == b'"' { in_str = true; }
            else if c == b'{' { tiefe += 1; }
            else if c == b'}' { tiefe -= 1; if tiefe == 0 { i += 1; break; } }
            i += 1;
        }
        let obj = &roh[start..i];
        let hol = |schl: &str| -> Option<String> {
            let muster = format!("\"{}\":", schl);
            let p = obj.find(&muster)? + muster.len();
            let rest = obj[p..].trim_start();
            if rest.starts_with("null") { return None; }
            let rest = rest.strip_prefix('"')?;
            let mut ende = 0;
            let bb = rest.as_bytes();
            let mut e = false;
            while ende < bb.len() {
                if e { e = false; }
                else if bb[ende] == b'\\' { e = true; }
                else if bb[ende] == b'"' { break; }
                ende += 1;
            }
            Some(rest[..ende].replace("\\\"", "\""))
        };
        faelle.push(Fall {
            form: hol("form").unwrap_or_default(),
            text: hol("text").unwrap_or_default(),
            hex: hol("hex"),
            anm: hol("anm"),
        });
    }
    faelle
}

/// Registername lesen. Gibt (Nummer, Groesse) -- sp/xzr sind beide 31,
/// unterscheiden sich aber im Befehl.
fn reg(s: &str) -> Option<(R, Grosse)> {
    let t = s.trim();
    if t == "xzr" { return Some((R(31), Grosse::X)); }
    if t == "wzr" { return Some((R(31), Grosse::W)); }
    if t == "sp" { return Some((R(31), Grosse::X)); }
    if t == "wsp" { return Some((R(31), Grosse::W)); }
    if let Some(n) = t.strip_prefix('x') {
        return n.parse::<u32>().ok().filter(|v| *v < 31).map(|v| (R(v), Grosse::X));
    }
    if let Some(n) = t.strip_prefix('w') {
        return n.parse::<u32>().ok().filter(|v| *v < 31).map(|v| (R(v), Grosse::W));
    }
    None
}

/// Gleitkommaregister: d0..d31 oder s0..s31. typ 01 = doppelt, 00 = einfach.
fn vreg(s: &str) -> Option<(V, u32)> {
    let t = s.trim();
    if let Some(n) = t.strip_prefix('d') {
        return n.parse::<u32>().ok().filter(|v| *v < 32).map(|v| (V(v), 1));
    }
    if let Some(n) = t.strip_prefix('s') {
        return n.parse::<u32>().ok().filter(|v| *v < 32).map(|v| (V(v), 0));
    }
    None
}

fn zahl(s: &str) -> Option<i64> {
    let t = s.trim().trim_start_matches('#');
    if let Some(h) = t.strip_prefix("0x") { return i64::from_str_radix(h, 16).ok(); }
    t.parse::<i64>().ok()
}

/// Einen Speicheroperanden `[x0, #8]` / `[sp]` / `[x0, x1, lsl #3]` zerlegen.
/// Erkennt `[x0], #8` -- Anpassung nach dem Zugriff.
fn mem_post(s: &str) -> Option<(R, i64)> {
    let t = s.trim();
    let zu = t.find(']')?;
    let rest = t[zu + 1..].trim();
    let rest = rest.strip_prefix(',')?.trim();
    let basis = reg(t[1..zu].trim())?.0;
    Some((basis, zahl(rest)?))
}

fn mem(s: &str) -> Option<(R, Option<i64>, Option<(R, u32, bool)>)> {
    let t = s.trim().strip_prefix('[')?.trim_end_matches('!').trim_end().strip_suffix(']')?;
    let teile: Vec<&str> = t.split(',').map(|x| x.trim()).collect();
    let (basis, _) = reg(teile[0])?;
    if teile.len() == 1 { return Some((basis, Some(0), None)); }
    if teile[1].starts_with('#') {
        return Some((basis, Some(zahl(teile[1])?), None));
    }
    // Registerversatz
    let (idx, _) = reg(teile[1])?;
    let mut betrag = 0u32;
    let mut hat_shift = false;
    if teile.len() > 2 {
        let sh = teile[2];
        if let Some(v) = sh.strip_prefix("lsl") {
            betrag = zahl(v)? as u32;
            hat_shift = true;
        }
    }
    let _ = betrag;
    Some((basis, None, Some((idx, 3, hat_shift))))
}

fn ops_trennen(rest: &str) -> Vec<String> {
    let mut ops = Vec::new();
    let mut akt = String::new();
    let mut tiefe = 0;
    for c in rest.chars() {
        match c {
            '[' => { tiefe += 1; akt.push(c); }
            ']' => { tiefe -= 1; akt.push(c); }
            ',' if tiefe == 0 => { ops.push(akt.trim().to_string()); akt.clear(); }
            _ => akt.push(c),
        }
    }
    if !akt.trim().is_empty() { ops.push(akt.trim().to_string()); }
    ops
}

fn kodieren(text: &str) -> Option<u32> {
    let t = text.trim();
    let (mnem, rest) = match t.find(' ') {
        Some(k) => (&t[..k], t[k..].trim()),
        None => (t, ""),
    };
    let ops = ops_trennen(rest);

    // Schiebeanhaengsel wie "lsl #16" sind ein eigener Operand
    let shift_von = |s: &str| -> Option<(u32, u32)> {
        let t = s.trim();
        for (n, a) in [("lsl", 0u32), ("lsr", 1), ("asr", 2)] {
            if let Some(v) = t.strip_prefix(n) {
                return Some((a, zahl(v)? as u32));
            }
        }
        None
    };

    match (mnem, ops.len()) {
        ("ret", 0) => Some(A64::ret_reg(R(30))),
        ("ret", 1) => { let (r, _) = reg(&ops[0])?; Some(A64::ret_reg(r)) }
        ("clrex", 0) => Some(A64::clrex()),
        ("svc", 1) => Some(A64::svc(zahl(&ops[0])? as u32)),
        ("brk", 1) => Some(A64::brk(zahl(&ops[0])? as u32)),
        ("br", 1) => { let (r, _) = reg(&ops[0])?; Some(A64::br(0b0000, r)) }
        ("blr", 1) => { let (r, _) = reg(&ops[0])?; Some(A64::br(0b0001, r)) }

        // mov Xd, Xn  ist orr Xd, xzr, Xn;  mov Xd, sp ist add Xd, sp, #0
        ("mov", 2) => {
            let (rd, g) = reg(&ops[0])?;
            if let Some((rn, _)) = reg(&ops[1]) {
                if ops[0].trim() == "sp" || ops[1].trim() == "sp" {
                    A64::addsub_imm(g, false, false, rd, rn, 0, false)
                } else {
                    Some(A64::logisch_reg(g, 0b01, rd, R(31), rn, 0, 0, false))
                }
            } else {
                // mov Xd, #imm -- as waehlt movz/movn/orr, je nachdem was passt
                let v = zahl(&ops[1])?;
                let uv = if g == Grosse::X { v as u64 } else { (v as u32) as u64 };
                // Einstueckige Konstante -> movz
                let folge = A64::konstante(g, rd, uv);
                if folge.len() == 1 { return Some(folge[0]); }
                // sonst versucht as ein Bitmuster
                A64::logisch_imm(g, 0b01, rd, R(31), uv)
            }
        }
        ("movz", _) | ("movn", _) | ("movk", _) => {
            let (rd, g) = reg(&ops[0])?;
            let imm = zahl(&ops[1])? as u32;
            let sh = if ops.len() > 2 { shift_von(&ops[2])?.1 } else { 0 };
            let opc = match mnem { "movn" => 0b00, "movz" => 0b10, _ => 0b11 };
            A64::mov_wide(g, opc, rd, imm, sh)
        }

        ("add", _) | ("sub", _) | ("adds", _) | ("subs", _) | ("cmp", _) | ("cmn", _) => {
            let sub = mnem.starts_with("sub") || mnem == "cmp";
            let flags = mnem.ends_with('s') || mnem == "cmp" || mnem == "cmn";
            if mnem == "cmp" || mnem == "cmn" {
                let (rn, g) = reg(&ops[0])?;
                if let Some((rm, _)) = reg(&ops[1]) {
                    let (art, bet) = if ops.len() > 2 { shift_von(&ops[2])? } else { (0, 0) };
                    return A64::addsub_reg(g, sub, true, R(31), rn, rm, art, bet);
                }
                let v = zahl(&ops[1])?;
                return A64::addsub_imm(g, sub, true, R(31), rn, v as u32, false);
            }
            let (rd, g) = reg(&ops[0])?;
            let (rn, _) = reg(&ops[1])?;
            if let Some((rm, _)) = reg(&ops[2]) {
                let (art, bet) = if ops.len() > 3 { shift_von(&ops[3])? } else { (0, 0) };
                A64::addsub_reg(g, sub, flags, rd, rn, rm, art, bet)
            } else {
                let v = zahl(&ops[2])?;
                let sh12 = ops.len() > 3 && shift_von(&ops[3])?.1 == 12;
                A64::addsub_imm(g, sub, flags, rd, rn, v as u32, sh12)
            }
        }

        ("neg", 2) => {
            let (rd, g) = reg(&ops[0])?;
            let (rm, _) = reg(&ops[1])?;
            A64::addsub_reg(g, true, false, rd, R(31), rm, 0, 0)
        }
        ("mvn", 2) => {
            let (rd, g) = reg(&ops[0])?;
            let (rm, _) = reg(&ops[1])?;
            Some(A64::logisch_reg(g, 0b01, rd, R(31), rm, 0, 0, true))
        }

        ("and", _) | ("orr", _) | ("eor", _) | ("ands", _) | ("tst", _) => {
            let opc = match mnem {
                "and" => 0b00, "orr" => 0b01, "eor" => 0b10, _ => 0b11,
            };
            if mnem == "tst" {
                let (rn, g) = reg(&ops[0])?;
                if let Some((rm, _)) = reg(&ops[1]) {
                    return Some(A64::logisch_reg(g, 0b11, R(31), rn, rm, 0, 0, false));
                }
                let v = zahl(&ops[1])?;
                let uv = if g == Grosse::X { v as u64 } else { (v as u32) as u64 };
                return A64::logisch_imm(g, 0b11, R(31), rn, uv);
            }
            let (rd, g) = reg(&ops[0])?;
            let (rn, _) = reg(&ops[1])?;
            if let Some((rm, _)) = reg(&ops[2]) {
                Some(A64::logisch_reg(g, opc, rd, rn, rm, 0, 0, false))
            } else {
                let v = zahl(&ops[2])?;
                let uv = if g == Grosse::X { v as u64 } else { (v as u32) as u64 };
                A64::logisch_imm(g, opc, rd, rn, uv)
            }
        }

        ("mul", 3) => {
            let (rd, g) = reg(&ops[0])?;
            let (rn, _) = reg(&ops[1])?;
            let (rm, _) = reg(&ops[2])?;
            Some(A64::madd(g, false, rd, rn, rm, R(31)))
        }
        ("madd", 4) | ("msub", 4) => {
            let (rd, g) = reg(&ops[0])?;
            let (rn, _) = reg(&ops[1])?;
            let (rm, _) = reg(&ops[2])?;
            let (ra, _) = reg(&ops[3])?;
            Some(A64::madd(g, mnem == "msub", rd, rn, rm, ra))
        }
        ("smulh", 3) | ("umulh", 3) => {
            let (rd, _) = reg(&ops[0])?;
            let (rn, _) = reg(&ops[1])?;
            let (rm, _) = reg(&ops[2])?;
            Some(A64::mulh(mnem == "smulh", rd, rn, rm))
        }
        ("sdiv", 3) | ("udiv", 3) => {
            let (rd, g) = reg(&ops[0])?;
            let (rn, _) = reg(&ops[1])?;
            let (rm, _) = reg(&ops[2])?;
            Some(A64::div(g, mnem == "sdiv", rd, rn, rm))
        }

        ("lsl", 3) | ("lsr", 3) | ("asr", 3) => {
            let (rd, g) = reg(&ops[0])?;
            let (rn, _) = reg(&ops[1])?;
            if let Some((rm, _)) = reg(&ops[2]) {
                let op2 = match mnem { "lsl" => 0b00, "lsr" => 0b01, _ => 0b10 };
                Some(A64::schieben_reg(g, op2, rd, rn, rm))
            } else {
                // Sofortwert: ueber ubfm/sbfm
                let v = zahl(&ops[2])? as u32;
                let breite = if g == Grosse::X { 64u32 } else { 32 };
                match mnem {
                    "lsl" => Some(A64::bfm(g, 0b10, rd, rn, (breite - v) % breite, breite - 1 - v)),
                    "lsr" => Some(A64::bfm(g, 0b10, rd, rn, v, breite - 1)),
                    _ => Some(A64::bfm(g, 0b00, rd, rn, v, breite - 1)),
                }
            }
        }

        ("sxtw", 2) => {
            let (rd, _) = reg(&ops[0])?;
            let (rn, _) = reg(&ops[1])?;
            Some(A64::bfm(Grosse::X, 0b00, rd, rn, 0, 31))
        }
        ("sxtb", 2) => {
            let (rd, g) = reg(&ops[0])?;
            let (rn, _) = reg(&ops[1])?;
            Some(A64::bfm(g, 0b00, rd, rn, 0, 7))
        }
        ("sxth", 2) => {
            let (rd, g) = reg(&ops[0])?;
            let (rn, _) = reg(&ops[1])?;
            Some(A64::bfm(g, 0b00, rd, rn, 0, 15))
        }
        ("uxtb", 2) => {
            let (rd, _) = reg(&ops[0])?;
            let (rn, _) = reg(&ops[1])?;
            Some(A64::bfm(Grosse::W, 0b10, rd, rn, 0, 7))
        }
        ("uxth", 2) => {
            let (rd, _) = reg(&ops[0])?;
            let (rn, _) = reg(&ops[1])?;
            Some(A64::bfm(Grosse::W, 0b10, rd, rn, 0, 15))
        }

        ("cset", 2) => {
            let (rd, g) = reg(&ops[0])?;
            let c = cond_nr(ops[1].trim())?;
            Some(A64::csinc(g, rd, R(31), R(31), c ^ 1))
        }
        ("csel", 4) => {
            let (rd, g) = reg(&ops[0])?;
            let (rn, _) = reg(&ops[1])?;
            let (rm, _) = reg(&ops[2])?;
            let c = cond_nr(ops[3].trim())?;
            Some(A64::csel(g, rd, rn, rm, c))
        }

        // ---- Laden/Speichern ----
        ("ldr", 2) | ("str", 2) | ("ldrb", 2) | ("strb", 2)
        | ("ldrh", 2) | ("strh", 2) | ("ldrsw", 2) | ("ldrsh", 2) | ("ldrsb", 2)
        | ("ldr", 3) | ("str", 3) | ("ldrb", 3) | ("strb", 3)
        | ("ldrh", 3) | ("strh", 3) | ("ldrsw", 3) | ("ldrsh", 3) | ("ldrsb", 3) => {
            let laden = mnem.starts_with("ldr");
            if let Some((vr, typ)) = vreg(&ops[0]) {
                // Gleitkomma laden/speichern
                let (basis, ab, _) = mem(&ops[1])?;
                let gl = if typ == 1 { 3u32 } else { 2 };
                let mut w = A64::ldst_imm(gl, laden, R(vr.0), basis, ab?)?;
                w |= 1 << 26; // V-Bit: Gleitkommaregister
                return Some(w);
            }
            let (rt, g) = reg(&ops[0])?;
            let adr = if ops.len() == 3 {
                format!("{}, {}", ops[1].trim(), ops[2].trim())
            } else { ops[1].trim().to_string() };
            if adr.contains("],") {
                let (basis, v) = mem_post(&adr)?;
                let gl = match mnem {
                    "ldrb" | "strb" | "ldrsb" => 0u32,
                    "ldrh" | "strh" | "ldrsh" => 1,
                    "ldrsw" => 2,
                    _ => if g == Grosse::X { 3 } else { 2 },
                };
                return A64::ldst_post(gl, laden, rt, basis, v);
            }
            let (basis, ab, idx) = mem(&adr)?;
            let gl = match mnem {
                "ldrb" | "strb" | "ldrsb" => 0u32,
                "ldrh" | "strh" | "ldrsh" => 1,
                "ldrsw" => 2,
                _ => if g == Grosse::X { 3 } else { 2 },
            };
            if let Some((rm, _, hat_shift)) = idx {
                return Some(A64::ldst_reg(gl, laden, rt, basis, rm, 0b011, hat_shift));
            }
            let ab = ab?;
            if mnem.starts_with("ldrs") {
                A64::ldst_imm_sign(gl, g == Grosse::X, rt, basis, ab)
            } else {
                A64::ldst_imm(gl, laden, rt, basis, ab)
            }
        }

        ("ldp", 3) | ("stp", 3) | ("ldp", 4) | ("stp", 4) => {
            let (rt1, g) = reg(&ops[0])?;
            let (rt2, _) = reg(&ops[1])?;
            let roh_s = if ops.len() == 4 {
                format!("{}, {}", ops[2].trim(), ops[3].trim())
            } else { ops[2].trim().to_string() };
            let roh = roh_s.as_str();
            let vorab = roh.ends_with('!');
            if roh.contains("],") {
                let (basis, v) = mem_post(roh)?;
                return A64::ldstp(g, mnem == "ldp", 0b01, rt1, rt2, basis, v);
            }
            let (basis, ab, _) = mem(roh)?;
            let art = if vorab { 0b11 } else { 0b10 };
            A64::ldstp(g, mnem == "ldp", art, rt1, rt2, basis, ab?)
        }

        ("ldaxr", 2) => {
            let (rt, g) = reg(&ops[0])?;
            let (basis, _, _) = mem(&ops[1])?;
            Some(A64::ldaxr(g, rt, basis))
        }
        ("stlxr", 3) => {
            let (rs, _) = reg(&ops[0])?;
            let (rt, g) = reg(&ops[1])?;
            let (basis, _, _) = mem(&ops[2])?;
            Some(A64::stlxr(g, rs, rt, basis))
        }

        // ---- Gleitkomma ----
        ("fadd", 3) | ("fsub", 3) | ("fmul", 3) | ("fdiv", 3) => {
            let (rd, typ) = vreg(&ops[0])?;
            let (rn, _) = vreg(&ops[1])?;
            let (rm, _) = vreg(&ops[2])?;
            let opc = match mnem {
                "fmul" => 0b0000, "fdiv" => 0b0001,
                "fadd" => 0b0010, _ => 0b0011,
            };
            Some(A64::f_rechnen(typ, opc, rd, rn, rm))
        }
        ("fcmp", 2) => {
            let (rn, typ) = vreg(&ops[0])?;
            let (rm, _) = vreg(&ops[1])?;
            Some(A64::fcmp(typ, rn, rm))
        }
        ("fcvt", 2) => {
            let (rd, nach) = vreg(&ops[0])?;
            let (rn, von) = vreg(&ops[1])?;
            Some(A64::fcvt(von, nach, rd, rn))
        }
        ("scvtf", 2) => {
            let (rd, typ) = vreg(&ops[0])?;
            let (rn, g) = reg(&ops[1])?;
            Some(A64::cvt_ganz(g, typ, true, rd.0, rn.0))
        }
        ("fcvtzs", 2) => {
            let (rd, g) = reg(&ops[0])?;
            let (rn, typ) = vreg(&ops[1])?;
            Some(A64::cvt_ganz(g, typ, false, rd.0, rn.0))
        }
        ("fmov", 2) => {
            if let Some((rd, typ)) = vreg(&ops[0]) {
                let (rn, g) = reg(&ops[1])?;
                Some(A64::fmov_ganz(g, typ, true, rd.0, rn.0))
            } else {
                let (rd, g) = reg(&ops[0])?;
                let (rn, typ) = vreg(&ops[1])?;
                Some(A64::fmov_ganz(g, typ, false, rd.0, rn.0))
            }
        }

        _ => None,
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let pfad = args.get(1).cloned()
        .unwrap_or_else(|| "tools/asmstudie/wahrheit_a64.json".to_string());
    let zeige: usize = args.iter().position(|a| a == "--zeige")
        .and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok()).unwrap_or(25);

    let faelle = faelle_lesen(&pfad);
    let mut gleich = 0usize;
    let mut falsch = 0usize;
    let mut offen = 0usize;
    let mut uebersprungen = 0usize;
    let mut je_form: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
    let mut beispiele = Vec::new();

    for f in &faelle {
        let soll = match &f.hex {
            Some(h) => h,
            None => { uebersprungen += 1; continue; }
        };
        if f.anm.as_deref() == Some("relok") { uebersprungen += 1; continue; }
        let e = je_form.entry(f.form.clone()).or_insert((0, 0, 0));
        match kodieren(&f.text) {
            None => { offen += 1; e.2 += 1; }
            Some(w) => {
                // ARM64 ist klein-endig: das Wort steht als vier Oktette da.
                let ist: String = w.to_le_bytes().iter()
                    .map(|x| format!("{:02x}", x)).collect();
                if &ist == soll { gleich += 1; e.0 += 1; }
                else {
                    falsch += 1; e.1 += 1;
                    if beispiele.len() < zeige {
                        beispiele.push((f.text.clone(), soll.clone(), ist));
                    }
                }
            }
        }
    }

    println!("=====================================================================");
    println!("DIFFERENZPRUEFUNG ARM64 gegen GNU as  --  {}", pfad);
    println!("=====================================================================");
    println!("  wortweise GLEICH : {}", gleich);
    println!("  FALSCH           : {}", falsch);
    println!("  nicht abgedeckt  : {}", offen);
    println!("  uebersprungen    : {}", uebersprungen);
    let pruefbar = gleich + falsch + offen;
    if pruefbar > 0 {
        println!("  Quote            : {:.1} % von {} pruefbaren Faellen",
                 100.0 * gleich as f64 / pruefbar as f64, pruefbar);
    }
    let gruen: Vec<&String> = je_form.iter()
        .filter(|(_, v)| v.1 == 0 && v.2 == 0 && v.0 > 0).map(|(k, _)| k).collect();
    let rot: Vec<(&String, &(usize, usize, usize))> = je_form.iter()
        .filter(|(_, v)| v.1 > 0).map(|(k, v)| (k, v)).collect();
    let off: Vec<(&String, &(usize, usize, usize))> = je_form.iter()
        .filter(|(_, v)| v.1 == 0 && v.2 > 0).map(|(k, v)| (k, v)).collect();
    println!();
    println!("  FORMEN vollstaendig gruen : {} von {}", gruen.len(), je_form.len());
    println!("  FORMEN mit Fehlern        : {}", rot.len());
    println!("  FORMEN nicht abgedeckt    : {}", off.len());

    if !rot.is_empty() {
        println!("\n--- Formen mit falschen Worten ---");
        for (k, v) in rot.iter().take(40) {
            println!("  {:38} gleich {:5}  falsch {:5}", k, v.0, v.1);
        }
    }
    if !off.is_empty() {
        println!("\n--- Formen ohne Kodierung ---");
        for (k, v) in off.iter().take(40) {
            println!("  {:38} offen {:5}", k, v.2);
        }
    }
    if !beispiele.is_empty() {
        println!("\n--- Beispiele ---");
        for (t, soll, ist) in &beispiele {
            println!("  {:34}\n      soll {}\n      ist  {}", t, soll, ist);
        }
    }
}
