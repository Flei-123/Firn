// SPDX-License-Identifier: GPL-2.0-only
//! Der Differenzpruefer: meine Kodierung gegen GNU `as`, Oktett fuer Oktett.
//!
//! Liest wahrheit_x86.json (was `as` erzeugt hat) und kodiert jeden Fall selbst.
//! Ein Unterschied von einem Oktett ist ein Fehler -- es gibt kein "fast".
//!
//! Aufruf:  asmdiff <wahrheit.json> [--zeige N] [--form "mov r64, r64"]

use std::collections::BTreeMap;
use std::env;
use std::fs;

#[path = "../encode_x86.rs"]
mod encode_x86;
use encode_x86::text::*;
use encode_x86::*;

/// Ein Eintrag aus der Wahrheitsdatei.
struct Fall {
    form: String,
    text: String,
    hex: Option<String>,
    anm: Option<String>,
}

fn json_entziffern(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(x) => out.push(x),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Einfacher Leser fuer genau die Form, die gnu_wahrheit.py schreibt.
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
            if !rest.starts_with('"') { return None; }
            let inhalt = &rest[1..];
            let mut ende = 0;
            let bb = inhalt.as_bytes();
            let mut e = false;
            while ende < bb.len() {
                if e { e = false; }
                else if bb[ende] == b'\\' { e = true; }
                else if bb[ende] == b'"' { break; }
                ende += 1;
            }
            Some(json_entziffern(&inhalt[..ende]))
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

/// Einen Befehlstext in Oktette verwandeln.
/// None heisst "Form noch nicht abgedeckt" -- das ist etwas anderes als FALSCH.
fn kodieren(text: &str) -> Option<Vec<u8>> {
    let t = text.trim();
    let (mnem, rest) = match t.find(' ') {
        Some(k) => (&t[..k], t[k..].trim()),
        None => (t, ""),
    };
    let (lock, mnem, rest) = if mnem == "lock" {
        let (m2, r2) = match rest.find(' ') {
            Some(k) => (&rest[..k], rest[k..].trim()),
            None => (rest, ""),
        };
        (true, m2, r2)
    } else {
        (false, mnem, rest)
    };

    let mut ops: Vec<String> = Vec::new();
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

    let mut p = Puffer::neu();
    let ist_mem = |s: &str| s.contains('[');
    let reg = |s: &str| reg_aus_name(s);
    let mem = |s: &str| mem_aus_text(s);

    match (mnem, ops.len()) {
        ("ret", 0) => X86::ret(&mut p),
        ("syscall", 0) => X86::syscall(&mut p),
        ("hlt", 0) => X86::hlt(&mut p),
        ("ud2", 0) => X86::ud2(&mut p),
        ("cld", 0) => X86::cld(&mut p),
        ("cdq", 0) => X86::cdq(&mut p),
        ("cqo", 0) => X86::cqo(&mut p),

        ("push", 1) => { let (r, _) = reg(&ops[0])?; X86::push_r(&mut p, r); }
        ("pop", 1) => { let (r, _) = reg(&ops[0])?; X86::pop_r(&mut p, r); }

        ("mov", 2) => {
            if ist_mem(&ops[0]) {
                let (m, mbr) = mem(&ops[0])?;
                if let Some((r, br)) = reg(&ops[1]) {
                    X86::mov_m_r(&mut p, br, &m, r);
                } else {
                    let v = zahl(&ops[1])?;
                    X86::mov_m_imm(&mut p, mbr?, &m, v);
                }
            } else if ist_mem(&ops[1]) {
                let (r, br) = reg(&ops[0])?;
                let (m, _) = mem(&ops[1])?;
                X86::mov_r_m(&mut p, br, r, &m);
            } else if let Some((r2, _)) = reg(&ops[1]) {
                let (r1, br) = reg(&ops[0])?;
                X86::mov_rr(&mut p, br, r1, r2);
            } else {
                let (r, br) = reg(&ops[0])?;
                let v = zahl(&ops[1])?;
                X86::mov_r_imm(&mut p, br, r, v);
            }
        }

        ("lea", 2) => {
            let (r, br) = reg(&ops[0])?;
            let (m, _) = mem(&ops[1])?;
            if m.rip { return None; }
            X86::lea(&mut p, br, r, &m);
        }

        ("add", 2) | ("or", 2) | ("adc", 2) | ("sbb", 2)
        | ("and", 2) | ("sub", 2) | ("xor", 2) | ("cmp", 2) => {
            let nr: u8 = match mnem {
                "add" => 0, "or" => 1, "adc" => 2, "sbb" => 3,
                "and" => 4, "sub" => 5, "xor" => 6, _ => 7,
            };
            if ist_mem(&ops[0]) {
                let (m, mbr) = mem(&ops[0])?;
                if let Some((r, br)) = reg(&ops[1]) {
                    X86::alu_m_r(&mut p, nr, br, &m, r);
                } else {
                    let v = zahl(&ops[1])?;
                    X86::alu_m_imm(&mut p, nr, mbr?, &m, v);
                }
            } else if ist_mem(&ops[1]) {
                let (r, br) = reg(&ops[0])?;
                let (m, _) = mem(&ops[1])?;
                X86::alu_r_m(&mut p, nr, br, r, &m);
            } else if let Some((r2, _)) = reg(&ops[1]) {
                let (r1, br) = reg(&ops[0])?;
                X86::alu_rr(&mut p, nr, br, r1, r2);
            } else {
                let (r, br) = reg(&ops[0])?;
                let v = zahl(&ops[1])?;
                X86::alu_r_imm(&mut p, nr, br, r, v);
            }
        }

        ("test", 2) => {
            if ist_mem(&ops[0]) { return None; }
            let (r1, br) = reg(&ops[0])?;
            let (r2, _) = reg(&ops[1])?;
            X86::test_rr(&mut p, br, r1, r2);
        }

        ("shl", 2) | ("shr", 2) | ("sar", 2) | ("sal", 2)
        | ("rol", 2) | ("ror", 2) => {
            let nr: u8 = match mnem {
                "rol" => 0, "ror" => 1, "shl" | "sal" => 4, "shr" => 5, _ => 7,
            };
            let (r, br) = reg(&ops[0])?;
            if ops[1] == "cl" {
                X86::shift_r_cl(&mut p, nr, br, r);
            } else {
                let v = zahl(&ops[1])?;
                if !(0..=255).contains(&v) { return None; }
                X86::shift_r_imm(&mut p, nr, br, r, v as u8);
            }
        }

        ("mul", 1) | ("div", 1) | ("idiv", 1) | ("neg", 1) | ("not", 1) => {
            let nr: u8 = match mnem {
                "not" => 2, "neg" => 3, "mul" => 4, "div" => 6, _ => 7,
            };
            if ist_mem(&ops[0]) { return None; }
            let (r, br) = reg(&ops[0])?;
            X86::f7_r(&mut p, nr, br, r);
        }
        ("imul", 1) => {
            let (r, br) = reg(&ops[0])?;
            X86::f7_r(&mut p, 5, br, r);
        }
        ("inc", 1) | ("dec", 1) => {
            let dec = mnem == "dec";
            if ist_mem(&ops[0]) {
                let (m, mbr) = mem(&ops[0])?;
                X86::inc_dec_m(&mut p, dec, mbr.unwrap_or(Breite::B64), &m);
            } else {
                let (r, br) = reg(&ops[0])?;
                X86::inc_dec_r(&mut p, dec, br, r);
            }
        }

        ("imul", 2) => {
            let (r, br) = reg(&ops[0])?;
            if ist_mem(&ops[1]) {
                let (m, _) = mem(&ops[1])?;
                X86::imul_r_m(&mut p, br, r, &m);
            } else {
                let (r2, _) = reg(&ops[1])?;
                X86::imul_rr(&mut p, br, r, r2);
            }
        }
        ("imul", 3) => {
            let (r, br) = reg(&ops[0])?;
            let v = zahl(&ops[2])?;
            if ist_mem(&ops[1]) {
                let (m, _) = mem(&ops[1])?;
                X86::imul_rmi(&mut p, br, r, &m, v);
            } else {
                let (r2, _) = reg(&ops[1])?;
                X86::imul_rri(&mut p, br, r, r2, v);
            }
        }

        ("movzx", 2) | ("movsx", 2) => {
            let (r, zbr) = reg(&ops[0])?;
            let zx = mnem == "movzx";
            if ist_mem(&ops[1]) {
                let (m, mbr) = mem(&ops[1])?;
                let qbr = mbr?;
                if zx { X86::movzx_r_m(&mut p, zbr, qbr, r, &m); }
                else { X86::movsx_r_m(&mut p, zbr, qbr, r, &m); }
            } else {
                let (r2, qbr) = reg(&ops[1])?;
                if zx { X86::movzx_rr(&mut p, zbr, qbr, r, r2); }
                else { X86::movsx_rr(&mut p, zbr, qbr, r, r2); }
            }
        }
        ("movsxd", 2) => {
            let (r, _) = reg(&ops[0])?;
            if ist_mem(&ops[1]) {
                let (m, _) = mem(&ops[1])?;
                X86::movsxd_r_m(&mut p, r, &m);
            } else {
                let (r2, _) = reg(&ops[1])?;
                X86::movsxd_rr(&mut p, r, r2);
            }
        }

        (m, 1) if m.starts_with("set") => {
            let cc = X86::cc_nr(&m[3..])?;
            let (r, _) = reg(&ops[0])?;
            X86::setcc_r(&mut p, cc, r);
        }
        (m, 2) if m.starts_with("cmov") => {
            let cc = X86::cc_nr(&m[4..])?;
            let (r1, br) = reg(&ops[0])?;
            let (r2, _) = reg(&ops[1])?;
            X86::cmovcc_rr(&mut p, cc, br, r1, r2);
        }

        ("call", 1) => {
            if let Some((r, _)) = reg(&ops[0]) { X86::call_r(&mut p, r); }
            else { return None; }
        }
        ("jmp", 1) => {
            if ist_mem(&ops[0]) {
                let (m, _) = mem(&ops[0])?;
                X86::jmp_m(&mut p, &m);
            } else { return None; }
        }

        ("addsd", 2) => { let a = xmm_aus_name(&ops[0])?; let b = xmm_aus_name(&ops[1])?; X86::addsd(&mut p, a, b); }
        ("subsd", 2) => { let a = xmm_aus_name(&ops[0])?; let b = xmm_aus_name(&ops[1])?; X86::subsd(&mut p, a, b); }
        ("mulsd", 2) => { let a = xmm_aus_name(&ops[0])?; let b = xmm_aus_name(&ops[1])?; X86::mulsd(&mut p, a, b); }
        ("divsd", 2) => { let a = xmm_aus_name(&ops[0])?; let b = xmm_aus_name(&ops[1])?; X86::divsd(&mut p, a, b); }
        ("addss", 2) => { let a = xmm_aus_name(&ops[0])?; let b = xmm_aus_name(&ops[1])?; X86::addss(&mut p, a, b); }
        ("subss", 2) => { let a = xmm_aus_name(&ops[0])?; let b = xmm_aus_name(&ops[1])?; X86::subss(&mut p, a, b); }
        ("mulss", 2) => { let a = xmm_aus_name(&ops[0])?; let b = xmm_aus_name(&ops[1])?; X86::mulss(&mut p, a, b); }
        ("divss", 2) => { let a = xmm_aus_name(&ops[0])?; let b = xmm_aus_name(&ops[1])?; X86::divss(&mut p, a, b); }
        ("ucomisd", 2) => { let a = xmm_aus_name(&ops[0])?; let b = xmm_aus_name(&ops[1])?; X86::ucomisd(&mut p, a, b); }
        ("ucomiss", 2) => { let a = xmm_aus_name(&ops[0])?; let b = xmm_aus_name(&ops[1])?; X86::ucomiss(&mut p, a, b); }
        ("cvtss2sd", 2) => { let a = xmm_aus_name(&ops[0])?; let b = xmm_aus_name(&ops[1])?; X86::cvtss2sd(&mut p, a, b); }
        ("cvtsd2ss", 2) => { let a = xmm_aus_name(&ops[0])?; let b = xmm_aus_name(&ops[1])?; X86::cvtsd2ss(&mut p, a, b); }
        ("movq", 2) => {
            if let Some(x) = xmm_aus_name(&ops[0]) {
                let (r, _) = reg(&ops[1])?; X86::movq_x_r(&mut p, x, r);
            } else {
                let (r, _) = reg(&ops[0])?; let x = xmm_aus_name(&ops[1])?;
                X86::movq_r_x(&mut p, r, x);
            }
        }
        ("movd", 2) => {
            if let Some(x) = xmm_aus_name(&ops[0]) {
                let (r, _) = reg(&ops[1])?; X86::movd_x_r(&mut p, x, r);
            } else {
                let (r, _) = reg(&ops[0])?; let x = xmm_aus_name(&ops[1])?;
                X86::movd_r_x(&mut p, r, x);
            }
        }
        ("cvtsi2sd", 2) => { let x = xmm_aus_name(&ops[0])?; let (r, br) = reg(&ops[1])?; X86::cvtsi2sd(&mut p, br, x, r); }
        ("cvtsi2ss", 2) => { let x = xmm_aus_name(&ops[0])?; let (r, br) = reg(&ops[1])?; X86::cvtsi2ss(&mut p, br, x, r); }
        ("cvttsd2si", 2) => { let (r, br) = reg(&ops[0])?; let x = xmm_aus_name(&ops[1])?; X86::cvttsd2si(&mut p, br, r, x); }
        ("cvttss2si", 2) => { let (r, br) = reg(&ops[0])?; let x = xmm_aus_name(&ops[1])?; X86::cvttss2si(&mut p, br, r, x); }

        ("xadd", 2) if lock => {
            let (m, _) = mem(&ops[0])?;
            let (r, br) = reg(&ops[1])?;
            X86::lock_xadd_m_r(&mut p, br, &m, r);
        }
        ("cmpxchg", 2) if lock => {
            let (m, _) = mem(&ops[0])?;
            let (r, br) = reg(&ops[1])?;
            X86::lock_cmpxchg_m_r(&mut p, br, &m, r);
        }

        _ => return None,
    }
    Some(p.okt)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let pfad = args.get(1).cloned()
        .unwrap_or_else(|| "tools/asmstudie/wahrheit_x86.json".to_string());
    let zeige: usize = args.iter().position(|a| a == "--zeige")
        .and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok()).unwrap_or(25);
    let nur_form = args.iter().position(|a| a == "--form")
        .and_then(|i| args.get(i + 1)).cloned();

    let faelle = faelle_lesen(&pfad);

    let mut gleich = 0usize;
    let mut falsch = 0usize;
    let mut offen = 0usize;
    let mut uebersprungen = 0usize;

    let mut je_form: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
    let mut beispiele: Vec<(String, String, String)> = Vec::new();

    for f in &faelle {
        if let Some(ref nf) = nur_form {
            if &f.form != nf { continue; }
        }
        let soll = match &f.hex {
            Some(h) => h,
            None => { uebersprungen += 1; continue; }
        };
        if f.anm.as_deref() == Some("relok") { uebersprungen += 1; continue; }
        let e = je_form.entry(f.form.clone()).or_insert((0, 0, 0));
        match kodieren(&f.text) {
            None => { offen += 1; e.2 += 1; }
            Some(okt) => {
                let ist: String = okt.iter().map(|x| format!("{:02x}", x)).collect();
                if &ist == soll {
                    gleich += 1; e.0 += 1;
                } else {
                    falsch += 1; e.1 += 1;
                    if beispiele.len() < zeige {
                        beispiele.push((f.text.clone(), soll.clone(), ist));
                    }
                }
            }
        }
    }

    println!("=====================================================================");
    println!("DIFFERENZPRUEFUNG gegen GNU as  --  {}", pfad);
    println!("=====================================================================");
    println!("  byteweise GLEICH : {}", gleich);
    println!("  FALSCH           : {}", falsch);
    println!("  nicht abgedeckt  : {}", offen);
    println!("  uebersprungen    : {}  (von as abgelehnt oder mit Relokation)", uebersprungen);
    let pruefbar = gleich + falsch + offen;
    if pruefbar > 0 {
        println!("  Quote            : {:.1} % von {} pruefbaren Faellen",
                 100.0 * gleich as f64 / pruefbar as f64, pruefbar);
    }

    let formen_gruen: Vec<&String> = je_form.iter()
        .filter(|(_, v)| v.1 == 0 && v.2 == 0 && v.0 > 0).map(|(k, _)| k).collect();
    let formen_rot: Vec<(&String, &(usize, usize, usize))> = je_form.iter()
        .filter(|(_, v)| v.1 > 0).map(|(k, v)| (k, v)).collect();
    let formen_offen: Vec<(&String, &(usize, usize, usize))> = je_form.iter()
        .filter(|(_, v)| v.1 == 0 && v.2 > 0).map(|(k, v)| (k, v)).collect();

    println!();
    println!("  FORMEN vollstaendig gruen : {} von {}", formen_gruen.len(), je_form.len());
    println!("  FORMEN mit Fehlern        : {}", formen_rot.len());
    println!("  FORMEN nicht abgedeckt    : {}", formen_offen.len());

    if !formen_rot.is_empty() {
        println!("\n--- Formen mit falschen Oktetten ---");
        for (k, v) in formen_rot.iter().take(40) {
            println!("  {:38} gleich {:5}  falsch {:5}", k, v.0, v.1);
        }
    }
    if !formen_offen.is_empty() {
        println!("\n--- Formen ohne Kodierung (noch nicht gebaut) ---");
        for (k, v) in formen_offen.iter().take(40) {
            println!("  {:38} offen {:5}", k, v.2);
        }
    }
    if !beispiele.is_empty() {
        println!("\n--- Beispiele falscher Oktette (soll = GNU as) ---");
        for (t, soll, ist) in &beispiele {
            println!("  {:38}\n      soll {}\n      ist  {}", t, soll, ist);
        }
    }
}
